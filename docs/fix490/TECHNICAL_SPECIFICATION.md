# FIX490 — Minimal REST Retrieval Boundary and Current-HEAD Readiness Synchronization

## 1. Purpose

Close the missing HTTP/REST retrieval boundary around the existing AstraVector retrieval core, synchronize top-level documentation and readiness/evidence references with the current `main` baseline, and preserve all already-proven retrieval, ingestion, consistency, security, lifecycle, ranking, and degradation invariants.

This phase is intentionally narrow. It is **not** an architectural rewrite and must not introduce new retrieval semantics.

## 2. Baseline

```text
repository: alimbetov/llm2
base_branch: main
base_sha: 2a34b65fd24bde11e1fc01dd4ff86ee04a5cd42b
baseline_merge: PR #38 — Agent/fix489r3 local stable floor
```

The baseline already contains the completed FIX489-R3 local stable-floor and 60-minute soak evidence.

Known current evidence:

```text
FIX489_R3_LOCAL_STABLE_FLOOR_PASS
FIX489_R3_SOAK_60M_PASS
FIX489_R3_PASS
maximum_stable_concurrency=2
recommended_operating_concurrency=1
capacity_scope=LOCAL_MAC_CPU
production_capacity_claim=false
```

Official 60-minute soak evidence records:

```text
measurement_duration_seconds=3600
completed_operations=7381
success_rate=1.0
grpc_statuses.OK=7381
UNKNOWN=0
cross_zone_leakage_count=0
access_level_violation_count=0
wrong_version_count=0
deleted_context_count=0
expired_context_count=0
indexing_context_count=0
lifecycle_invalid_context_count=0
missing_active_qdrant_points_after_cooldown=0
duplicate_canonical_identity_count=0
failed_outbox=0
dead_letters=0
orphan_binding_count=0
orphan_outbox_count=0
unexpected_INTERNAL=0
unclassified_timeout=0
panic=0
crash=0
deadlock=0
memory_behavior_stable=true
queues_bounded=true
```

This is local Mac CPU evidence only. FIX490 must not convert it into a production-capacity claim.

## 3. Existing architecture that must be preserved

AstraVector already has the following authoritative boundaries:

```text
AstraVectorIngestionFacade
  -> logical document / LogicalBlock input
  -> canonical tokenizer
  -> token-aware hierarchical chunking
  -> BGE-M3 / ONNX representations
  -> PostgreSQL canonical state
  -> outbox
  -> Qdrant projection

AstraVectorRetrievalFacade::RetrieveContext
  -> authoritative Search pipeline
  -> Dense / Sparse / Lexical retrieval
  -> fusion
  -> canonical PostgreSQL parent hydration
  -> Graph expansion
  -> MMR
  -> hard token budget
  -> final PostgreSQL visibility recheck
  -> final intent/coverage recomputation
  -> response
```

The REST boundary must reuse the same application/retrieval implementation. It must not create a second retrieval pipeline.

## 4. Hard invariants — MUST NOT CHANGE

FIX490 is prohibited from changing the semantics or tuning of:

- `CanonicalTokenizer` behavior or tokenizer/model version identity;
- BGE-M3/ONNX inference ownership;
- ingestion chunking ownership inside AstraVector;
- `SOURCE -> PARENT -> SUB_180/SUB_260` hierarchy;
- logical-block trace/provenance semantics;
- dense/sparse representation generation;
- Dense/Sparse/Lexical/Hybrid branch semantics;
- fusion/RRF weights, thresholds, candidate limits, or ordering semantics;
- no-answer policy and thresholds;
- canonical child-to-parent hydration;
- GraphRAG seed, expansion, relation, survivor, or provenance semantics;
- MMR relevance/redundancy behavior or weights;
- hard token-budget behavior;
- final visibility recheck;
- access-zone/access-level isolation;
- document/version identity rules;
- ACTIVE/INDEXING/DELETED/EXPIRED lifecycle semantics;
- TTL/legal-hold/deletion semantics;
- PostgreSQL as canonical state;
- Qdrant as rebuildable projection;
- outbox/publisher/reconciliation behavior;
- retry, deadline, cancellation, backpressure, or degradation semantics;
- frozen quality banks, qrels, query sets, evidence thresholds, or fixture identities.

If implementation of REST appears to require changing any invariant above, the phase must stop and record `FIX490_BLOCKED_BY_ARCHITECTURE_CHANGE` rather than silently modifying production semantics.

## 5. REST scope

### 5.1 Required endpoint

Implement one public retrieval endpoint:

```http
POST /api/v1/retrieve
```

The REST endpoint must be a thin transport adapter over the same application implementation used by `AstraVectorRetrievalFacade::RetrieveContext`.

Conceptual flow:

```text
HTTP JSON request
    -> REST DTO validation/mapping
    -> existing RetrieveContext application path
    -> existing authoritative Search pipeline
    -> REST DTO mapping
    -> HTTP JSON response
```

Do **not** implement:

```text
REST -> localhost gRPC -> AstraVector
```

The REST server and gRPC server may share the same process, dependencies, repository, scheduler, inference engine, Qdrant client, cancellation infrastructure and readiness source.

### 5.2 Health endpoints

Expose minimal HTTP probes:

```http
GET /health
GET /ready
```

Required semantics:

- `/health`: process is alive and HTTP adapter can answer;
- `/ready`: reflect the existing AstraVector readiness state; do not invent an independent REST readiness calculation.

### 5.3 Explicitly out of scope

Do not add REST APIs for:

- ingestion;
- chunk creation;
- embedding preview;
- document activation;
- document deletion;
- outbox retry;
- reconciliation;
- Qdrant administration;
- Graph administration;
- runtime configuration;
- model management;
- control/admin facade operations.

Those remain gRPC/internal boundaries in FIX490.

## 6. REST contract

The REST request must be a minimal JSON projection of the already-established `RetrieveContextRequest` contract, not a new retrieval model.

Minimum fields:

```json
{
  "question": "How does PostgreSQL recover missing Qdrant points?",
  "accessZoneId": "<uuid>",
  "profile": "TECHNICAL",
  "maxContexts": 8,
  "enableGraphExpansion": true
}
```

Support only fields that can be mapped unambiguously to the existing retrieval facade contract. Optional fields may include current filters and multi-zone identifiers if mapping preserves current authorization semantics.

The REST response must preserve the existing public retrieval information model:

```text
contexts
summary
warnings
degradation
```

Each context must preserve, where present in the facade response:

```text
matchedText
parentText
documentId
documentVersion
sourceBlockId
matchedChunkId
parentChunkId
accessZoneId
citation/source location
source links
scores
metadata
```

REST must not recompute scores, ranking, coverage, evidence status, degradation or citations.

## 7. Authentication and access propagation

The REST adapter must not weaken the existing security boundary.

At minimum:

- authenticate using the same configured API-key policy or a transport-equivalent adapter to the existing `ApiKeyAuth` configuration;
- map caller access level and request context into the same retrieval request fields used by gRPC;
- fail closed when required identity/access context is missing or invalid;
- never accept an access zone from a request while dropping caller access-level enforcement;
- do not introduce a REST-only bypass around access-zone filters or final PostgreSQL visibility checks.

A REST request and a semantically equivalent gRPC `RetrieveContext` request must produce the same authorized context identities under the same runtime state.

## 8. Error and degradation mapping

Map existing AstraVector errors deterministically to HTTP status codes without changing runtime classification.

Minimum mapping policy:

```text
InvalidArgument       -> 400
Unauthenticated       -> 401
PermissionDenied      -> 403
NotFound              -> 404
FailedPrecondition    -> 409
ResourceExhausted     -> 429
Unavailable           -> 503
DeadlineExceeded      -> 504
Internal/unclassified -> 500
```

If current `AstraError` names differ, document the exact code mapping actually used.

Important:

- a `DEGRADED` retrieval response with surviving valid contexts remains a successful retrieval response, not an HTTP transport failure;
- total backend failure/deadline must not be converted into a semantic empty/no-answer success;
- cancellation/deadline/backpressure signals must retain their existing meaning.

## 9. Runtime integration

Add a small HTTP server to the existing process.

Preferred constraints:

- choose a minimal Rust HTTP framework suitable for the existing Tokio runtime;
- bind host/port from typed configuration;
- default HTTP port must not collide with gRPC (`50051`) or metrics;
- participate in the existing cancellation token and graceful shutdown;
- `/ready` must use existing `Readiness` state;
- HTTP startup failure must fail startup if REST is configured as required;
- if REST is configurable/optional, document the exact startup behavior.

Do not introduce a new microservice or reverse-proxy container for FIX490.

## 10. Required parity tests

### 10.1 Contract parity

For the same runtime state and request inputs:

```text
gRPC RetrieveContext
REST POST /api/v1/retrieve
```

must agree on at least:

- returned context count;
- context ordering;
- `(access_zone_id, document_id, document_version, matched_chunk_id, parent_chunk_id)` identity;
- matched/parent text;
- evidence status;
- degradation state/codes;
- dense/sparse/fusion/final scores within serialization tolerance;
- Graph-derived result presence/provenance when Graph is enabled.

### 10.2 Security parity

Mandatory negative cases:

- wrong access zone;
- insufficient access level;
- deleted document/version;
- expired context;
- INDEXING/non-active document;
- malformed/absent auth.

Expected result: REST must not expose any context that gRPC would reject.

### 10.3 Failure/degradation parity

Exercise at least the existing testable failure classes relevant to retrieval:

- partial optional retrieval-branch degradation;
- PostgreSQL hydration timeout/failure where current test harness supports it;
- Qdrant unavailable/timeout where current harness supports it;
- request deadline exhaustion;
- overload/backpressure/`RESOURCE_EXHAUSTED` equivalent.

REST must preserve the existing semantic distinction between successful degraded retrieval and transport/request failure.

## 11. Regression gates

FIX490 must run the existing locked/static gates relevant to current main:

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

Run existing retrieval/invariant proofs that are reasonably executable in the phase environment. At minimum, do not weaken or edit their frozen assertions to obtain PASS.

The new REST tests are additive. Existing gRPC tests remain authoritative regression gates.

## 12. Evidence synchronization

The current top-level readiness documentation predates major later evidence and currently contains stale statements such as load/soak being open and recovery proof being pending.

FIX490 must perform a source-of-truth audit before updating top-level status.

Required documents to inspect and reconcile include at least:

```text
README.md
docs/ASTRAVECTOR_READINESS_REPORT.md
docs/02-readiness-and-verdicts.md
docs/12-roadmap.md
```

and the latest authoritative phase results, including:

```text
docs/fix489/r3/RESULT.md
docs/fix489/r3/LOCAL_STABLE_FLOOR_RESULT.md
docs/fix489/r3/SOAK_RESULT.md
```

Also account for the later FIX486 retrieval/isolation/lifecycle/Graph evidence already merged into `main`.

### Evidence rules

- never claim a result for current HEAD solely because an older SHA passed unless the repository's proof policy explicitly allows inherited evidence;
- distinguish `tested_sha`, `soak_tested_sha`, branch/head SHA and current main SHA;
- preserve local-hardware scope labels;
- do not convert local capacity into production capacity;
- do not call AstraVector `PRODUCTION_READY` unless all repository-defined production-ready gates are actually satisfied;
- replace stale blocker statements only when contradicted by authoritative later evidence;
- where a proof remains historical/inherited, say so explicitly;
- do not edit frozen evidence payloads to make them appear current.

## 13. Current-status report produced by FIX490

Create a compact current-head status document that separates:

```text
IMPLEMENTED
PROVEN_ON_CURRENT_OR_INHERITED_SHA
PROVEN_LOCAL_ONLY
STILL_OPEN
NOT_IN_SCOPE
```

At minimum include:

- Dense/Sparse/Hybrid runtime state;
- hierarchical child/parent retrieval;
- access-zone/access-level isolation;
- lifecycle visibility;
- stale/orphan hydration handling;
- GraphRAG correctness evidence status;
- MMR/token-budget evidence status;
- PostgreSQL/Qdrant consistency/outbox/recovery state;
- local capacity stable floor;
- 60-minute soak;
- packaging/deployment/Kubernetes validation;
- backup/restore/rollback status;
- security hardening beyond current API-key/access-zone model;
- REST boundary status.

The report must be traceable to source files/SHAs and must not infer PASS from implementation presence alone.

## 14. Acceptance criteria

FIX490 passes only if all mandatory items below are true:

```text
[ ] branch baseline is current main SHA 2a34b65fd24bde11e1fc01dd4ff86ee04a5cd42b or explicitly rebased/currentized before final evidence
[ ] POST /api/v1/retrieve implemented
[ ] GET /health implemented
[ ] GET /ready implemented using existing Readiness state
[ ] REST calls same retrieval application/core path as gRPC; no localhost gRPC loopback
[ ] no retrieval/chunking/model/ranking/Graph/MMR/access/lifecycle semantics changed
[ ] authentication/access propagation is fail-closed
[ ] deterministic AstraError -> HTTP mapping documented and tested
[ ] gRPC/REST positive parity tests PASS
[ ] security negative parity tests PASS
[ ] relevant degradation/failure parity tests PASS
[ ] locked fmt/check/clippy/test gates PASS
[ ] existing invariant/proof suites are not weakened
[ ] README/readiness docs synchronized to authoritative latest evidence
[ ] current-head evidence/status report added with SHA provenance
[ ] local FIX489-R3 capacity/soak remains explicitly LOCAL_MAC_CPU, not production capacity
[ ] no PRODUCTION_READY claim without independent proof of all remaining production gates
```

Final allowed verdicts:

```text
FIX490_REST_BOUNDARY_AND_READINESS_SYNC_PASS
FIX490_REST_BOUNDARY_AND_READINESS_SYNC_BLOCKED
```

## 15. Implementation sequence

Use the following sequence to minimize regression risk:

1. audit current `main` contracts, runtime construction and latest evidence;
2. write REST request/response/error mapping contracts and red parity tests;
3. add minimal HTTP dependency/configuration;
4. extract/reuse the existing retrieval application entry point only if required to avoid transport-to-transport calls;
5. implement `/api/v1/retrieve`, `/health`, `/ready`;
6. run focused parity/security/degradation tests;
7. run locked all-target gates;
8. audit latest phase evidence and current documentation for stale claims;
9. update top-level documentation without rewriting immutable evidence history;
10. publish a current-head status/evidence index;
11. produce final FIX490 result with exact tested SHA and gate results.

## 16. Explicit non-goals

FIX490 must not:

- redesign AstraVector into multiple services;
- move chunking/tokenization/embedding into AstraIndexator;
- add AstraIndexator implementation;
- add SeaweedFS support to AstraVector;
- add REST ingestion/admin APIs;
- create a new contracts repository;
- tune retrieval quality;
- change frozen evaluation data;
- add Kafka/event infrastructure;
- redesign PostgreSQL schemas or Qdrant collections;
- claim production capacity from MacBook evidence.

The intended end state is deliberately small:

```text
AstraIndexator --gRPC LogicalBlock[]--> AstraVector
                                         |
                                         +-- existing canonical RAG core
                                         |
                                         +-- gRPC RetrieveContext
                                         +-- REST POST /api/v1/retrieve
                                         +-- HTTP health/readiness
```
