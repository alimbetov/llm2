# FIX492 — External Ingestion Contract Hardening

## Status

Proposed implementation specification for branch `agent/fix492-ingestion-contract-hardening`.

## Goal

Make AstraVector session ingestion safe for independent external clients (especially future AstraIndexator and generated Java/Spring clients) without requiring them to reverse-engineer Rust internals.

FIX492 closes four P0 contract gaps:

1. byte-precise `batch_content_hash` contract;
2. byte-precise `final_content_hash` contract;
3. typed ingestion session state;
4. typed ingestion session error/status contract.

The change must remain backward-compatible with current protobuf consumers.

## Existing wire contract that must be preserved

Current protobuf package:

```text
astravector.embedding.v1
```

Session RPC request/response messages already expose:

```text
StartLogicalDocumentIngestionRequest
StartLogicalDocumentIngestionResponse
AppendLogicalDocumentBlocksRequest
AppendLogicalDocumentBlocksResponse
FinalizeLogicalDocumentIngestionRequest
AbortLogicalDocumentIngestionRequest
AbortLogicalDocumentIngestionResponse
GetLogicalDocumentIngestionStatusRequest
GetLogicalDocumentIngestionStatusResponse
```

Existing fields such as `status` and `error_code` are strings. They MUST NOT be removed or renumbered in FIX492.

Current server behavior already requires:

```text
batch_content_hash = 64-char SHA-256 hex
final_content_hash = SHA-256 compatible hex
```

and rejects mismatches with precondition errors.

## Compatibility strategy

FIX492 is additive.

Do not:

- rename existing protobuf fields;
- reuse field numbers;
- change existing RPC names;
- replace existing `string status` fields in-place;
- replace existing `string error_code` in-place;
- change existing session persistence keys;
- change document/chunking/retrieval semantics.

Instead add typed fields at new protobuf field numbers and keep legacy string mirrors during a compatibility window.

## 1. Batch content hash contract

### Current server behavior

The Rust server computes a canonical server-side hash over `repeated LogicalBlock blocks`, compares it with `AppendLogicalDocumentBlocksRequest.batch_content_hash`, and stores the canonical hash for replay/idempotency checks.

Current implementation constructs a deterministic JSON value per `LogicalBlock`, serializes the ordered array with `serde_json::to_vec`, and hashes those bytes with SHA-256.

That implementation detail is not yet a safe cross-language contract because external clients need an exact specification of:

- field inclusion;
- field naming;
- absent/default handling;
- metadata ordering;
- source-link ordering;
- UTF-8 normalization rules;
- JSON escaping;
- whitespace;
- number representation.

### FIX492 canonical representation

Define a versioned canonical hash format:

```text
ASTRAVECTOR_LOGICAL_BLOCK_BATCH_HASH_V1
```

The canonical byte stream MUST be documented and implemented identically in Rust test helpers and golden fixtures.

Recommended contract for V1:

1. blocks are processed in request array order; no sorting by `order_index`;
2. each block is converted into canonical JSON using the exact documented JSON property names;
3. all canonical object keys are emitted in a fixed order defined below;
4. maps (`metadata`, `attributes`) are emitted with keys sorted by Unicode code-point / UTF-8 lexical order; the exact chosen rule must be fixed in tests;
5. repeated values (`source_links`) preserve request order;
6. proto scalar default values are represented explicitly in canonical JSON if and only if the existing Rust canonicalizer currently includes them; implementation and fixtures must prove this;
7. strings are encoded as UTF-8 and escaped according to JSON; no additional Unicode normalization is performed unless existing server code already does it;
8. serialization contains no insignificant whitespace;
9. the top-level value is a JSON array;
10. SHA-256 is computed over the exact serialized UTF-8 bytes;
11. wire hash is lowercase 64-character hexadecimal without `sha256:` prefix as the canonical output; server MAY accept an existing compatibility prefix if already supported.

Canonical `LogicalBlock` property order must be frozen by implementation evidence, including at least:

```text
block_id
parent_block_id
block_type
text
order_index
source_location
source_links
metadata
```

Nested canonical representations for `SourceLocation` and `SourceLink` must also be frozen.

If inspection proves current `logical_block_to_json` uses a different field order or inclusion set, preserve current server semantics and document the exact current order instead of changing hashes silently.

### Batch hash algorithm identifier

Add documentation/API capability metadata for:

```text
logical_block_batch_hash_algorithm = "ASTRAVECTOR_LOGICAL_BLOCK_BATCH_HASH_V1"
```

Do not make external clients guess that the contract is `serde_json`-specific.

### Replay semantics

For the same:

```text
ingestion_session_id + batch_index
```

- same canonical hash => idempotent replay accepted;
- different canonical hash => `BATCH_HASH_MISMATCH` typed precondition error;
- block order differences MUST produce a different hash unless the canonical algorithm explicitly states otherwise.

## 2. Final content hash contract

### Current server behavior

Finalize currently renders staged logical blocks to the same text representation used by AstraVector chunking, then computes a normalized content hash and compares it to `FinalizeLogicalDocumentIngestionRequest.final_content_hash`.

This is currently a P0 client contract gap because an independent Java service cannot safely reconstruct an undocumented Rust text renderer.

### Required FIX492 decision

The implementation MUST choose and freeze exactly one externally reproducible V1 rule.

Preferred rule:

```text
ASTRAVECTOR_LOGICAL_DOCUMENT_FINAL_HASH_V1
```

The canonical final hash must be defined from public ingestion DTO data only, not from private Rust-only state.

Before changing behavior, compare the preferred rule with current `render_logical_blocks_for_chunking` + `normalized_content_hash` behavior. If changing the algorithm would invalidate existing clients/session data, preserve the existing algorithm as V1 and document it precisely instead.

The spec must state:

- block ordering rule;
- exact separator between rendered blocks;
- whether block type/headings participate;
- whether empty text participates;
- line-ending normalization (`LF` vs `CRLF`);
- leading/trailing whitespace policy;
- Unicode normalization policy;
- exact UTF-8 bytes hashed;
- accepted wire hash forms.

### Finalize semantics

- missing hash => `INVALID_ARGUMENT`;
- malformed SHA-256 => `INVALID_ARGUMENT` if current code validates format; otherwise add compatible validation;
- mismatch => typed `FINAL_CONTENT_HASH_MISMATCH`, session must return to retryable `ACTIVE` state as current behavior intends;
- matching hash => finalize proceeds;
- retry after an ambiguous timeout must be reconcilable through `GetLogicalDocumentIngestionStatus`.

## 3. Typed ingestion session state

Add a protobuf enum, additive only:

```proto
enum IngestionSessionStateV1 {
  INGESTION_SESSION_STATE_V1_UNSPECIFIED = 0;
  INGESTION_SESSION_STATE_V1_ACTIVE = 1;
  INGESTION_SESSION_STATE_V1_FINALIZING = 2;
  INGESTION_SESSION_STATE_V1_COMPLETED = 3;
  INGESTION_SESSION_STATE_V1_ABORTED = 4;
  INGESTION_SESSION_STATE_V1_FAILED = 5;
  INGESTION_SESSION_STATE_V1_EXPIRED = 6;
}
```

Exact states must be verified against PostgreSQL/Rust state transitions before implementation. Do not publish a state that the runtime cannot produce or reconcile.

Add a new typed field (new field number) to all session responses that currently expose legacy string `status`:

```text
StartLogicalDocumentIngestionResponse
AppendLogicalDocumentBlocksResponse
AbortLogicalDocumentIngestionResponse
GetLogicalDocumentIngestionStatusResponse
```

Legacy `status` remains populated with the current uppercase string for backward compatibility.

One mapping function in Rust must own string <-> enum mapping. Avoid duplicated ad-hoc matches.

## 4. Typed ingestion error contract

Add an additive enum, final values to be verified against actual server branches:

```proto
enum IngestionErrorCodeV1 {
  INGESTION_ERROR_CODE_V1_UNSPECIFIED = 0;
  INGESTION_ERROR_CODE_V1_SESSION_NOT_FOUND = 1;
  INGESTION_ERROR_CODE_V1_SESSION_EXPIRED = 2;
  INGESTION_ERROR_CODE_V1_INVALID_SESSION_STATE = 3;
  INGESTION_ERROR_CODE_V1_BATCH_HASH_MISMATCH = 4;
  INGESTION_ERROR_CODE_V1_FINAL_CONTENT_HASH_MISMATCH = 5;
  INGESTION_ERROR_CODE_V1_STAGING_CORRUPTED = 6;
  INGESTION_ERROR_CODE_V1_LIMIT_EXCEEDED = 7;
  INGESTION_ERROR_CODE_V1_VALIDATION_FAILED = 8;
  INGESTION_ERROR_CODE_V1_INDEXING_FAILED = 9;
  INGESTION_ERROR_CODE_V1_DEPENDENCY_UNAVAILABLE = 10;
  INGESTION_ERROR_CODE_V1_INTERNAL = 11;
}
```

Add a structured message:

```proto
message IngestionErrorV1 {
  IngestionErrorCodeV1 code = 1;
  string message = 2;
  bool retryable = 3;
}
```

Add `IngestionErrorV1 error = <new field>;` to `GetLogicalDocumentIngestionStatusResponse`.

Keep legacy:

```text
error_code
error_message
```

populated for backward compatibility.

Do not expose raw PostgreSQL/SQLx errors as stable external error codes.

## 5. gRPC status mapping

Freeze the minimum transport mapping used by external clients:

```text
INVALID_ARGUMENT
  malformed UUID/hash/request fields

NOT_FOUND
  session not found

FAILED_PRECONDITION
  session expired
  invalid lifecycle transition
  BATCH_HASH_MISMATCH
  FINAL_CONTENT_HASH_MISMATCH

RESOURCE_EXHAUSTED
  configured ingestion limits exceeded

UNAVAILABLE
  transient PostgreSQL/dependency unavailability

DATA_LOSS
  staged persistence consistency/hash corruption

INTERNAL
  unexpected server defect
```

Every typed error must have an explicitly documented retry recommendation. Transport code alone must not be the only semantic signal for persisted session failures.

## 6. Golden vectors

Create checked-in language-neutral fixtures under:

```text
tests/fixtures/ingestion-contract-v1/
```

Required fixtures:

```text
batch-minimal.json
batch-unicode-ru.json
batch-metadata-order-a.json
batch-metadata-order-b.json
batch-source-links.json
batch-empty-default-fields.json
final-document-basic.json
final-document-unicode-ru.json
manifest.json
```

`manifest.json` records for each fixture:

```json
{
  "algorithm": "...V1",
  "canonicalSha256": "...",
  "canonicalUtf8Hex": "...",
  "notes": "..."
}
```

At minimum prove:

- Russian UTF-8 text;
- metadata map insertion order does not change the hash;
- block array order does change the hash;
- source-link order behavior is explicit;
- optional/default fields behavior is explicit;
- Java-compatible expected hashes are published as fixed constants.

## 7. Rust implementation structure

Do not leave the public canonicalization algorithm buried inside the 700k+ line `src/grpc/mod.rs`.

Extract a focused module, suggested path:

```text
src/ingestion_contract/
├── mod.rs
├── canonical_hash.rs
├── session_state.rs
└── error.rs
```

Responsibilities:

```text
canonical_hash.rs
  canonical LogicalBlock representation
  batch hash V1
  final document hash V1
  SHA-256 normalization

session_state.rs
  DB string -> typed enum
  typed enum -> legacy string
  lifecycle mapping invariants

error.rs
  internal/session failure -> typed external error
  retryability mapping
```

`src/grpc/mod.rs` should consume these helpers.

## 8. Persistence compatibility

Do not require destructive schema changes.

Current persisted string `status`, `error_code`, hashes and staged JSON may remain canonical DB representation in FIX492 if that avoids migration risk.

Typed protobuf state/error can be derived at the boundary.

If a DB migration is introduced, it must be additive and rollback-safe.

## 9. External client contract documentation

Add:

```text
docs/contracts/INGESTION_EXTERNAL_CONTRACT_V1.md
```

It must be sufficient for a Java developer to implement AstraIndexator without reading Rust.

Required sections:

- RPC sequence;
- DTO field tables;
- `batch_content_hash` exact algorithm;
- `final_content_hash` exact algorithm;
- Java type mapping;
- session state machine;
- typed error table;
- retry matrix;
- idempotency rules;
- golden vector table;
- examples for Start/Append/Finalize/GetStatus/Abort;
- compatibility/versioning rules.

## 10. Java reference algorithm

Documentation must include a Java 17+ reference implementation or pseudocode that reproduces every golden hash exactly.

Preferred implementation should use Jackson only if the canonical JSON contract can be configured deterministically. If that is fragile, specify an explicit canonical writer instead.

Do not require Rust-specific serialization behavior from Java clients without documenting it byte-for-byte.

## 11. Tests

Add contract tests that fail on any accidental change to canonical bytes or enum mappings.

Required test groups:

```text
batch_hash_golden_vectors_v1
final_hash_golden_vectors_v1
metadata_order_is_stable
unicode_is_stable
block_order_is_significant
batch_replay_same_hash_is_idempotent
batch_replay_different_hash_is_rejected
typed_state_matches_legacy_status
typed_error_matches_legacy_error
grpc_status_mapping_contract
final_hash_mismatch_returns_session_to_active
```

Where practical include an integration/Testcontainers test over PostgreSQL for session replay/finalize behavior.

## 12. Non-goals

FIX492 must NOT:

- redesign chunking;
- change BGE-M3 inference;
- redesign access zones;
- change TTL semantics;
- change retrieval;
- change PostgreSQL canonical-state invariant;
- change Qdrant rebuildability;
- implement AstraIndexator;
- remove legacy fields;
- redesign the entire proto package.

## Definition of Done

PASS only when all are true:

1. exact batch hash V1 documented;
2. exact final hash V1 documented;
3. Rust implementation is centralized and deterministic;
4. golden vectors committed;
5. Java reference reproduces vectors;
6. typed session state is additive and backward-compatible;
7. typed error contract is additive and backward-compatible;
8. legacy status/error strings remain correct;
9. gRPC semantic/status mapping documented and tested;
10. existing ingestion/session tests still pass;
11. `cargo fmt --all --check` passes;
12. `cargo check --locked --all-targets --all-features` passes;
13. `cargo clippy --locked --all-targets --all-features -- -D warnings` passes;
14. `cargo test --locked --all-targets --all-features` passes;
15. no access-zone/TTL/retrieval regression;
16. external contract doc is sufficient to implement a Java client without Rust source inspection.

Final verdict token for later independent validation:

```text
FIX492_EXTERNAL_INGESTION_CONTRACT_PASS
FIX492_EXTERNAL_INGESTION_CONTRACT_FAIL
FIX492_EXTERNAL_INGESTION_CONTRACT_BLOCKED
```
