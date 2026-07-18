# FIX486F Production Capability Audit

## Audit Identity

```text
branch: codex/fix486f-stale-orphan-hydration-proof
document-review commit: 81390a9
frozen bank: 1.0.0 / FROZEN
frozen aggregate: cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff
audit mode: read-only production code inspection
```

## Verdict

```text
CAPABILITY_AUDIT_COMPLETE
RUNTIME_IMPLEMENTATION_REQUIRED
P0_DESIGN_REVIEW_REQUIRED
```

No runtime code was changed during this audit.

## Current API Surface

### Search

`SearchResponseV004` contains results, diagnostics and warnings. It has no
response-level evidence status, retryable flag, degradation object, dropped-parent
identities or rejection reasons. A PostgreSQL hydration timeout is propagated as a
gRPC `DEADLINE_EXCEEDED` error, so no normal Search response body is produced.

Evidence:

- `proto/astravector_embedding.proto`: `SearchResponseV004`,
  `SearchDiagnosticsV004`;
- `src/grpc/mod.rs`: `AstraVectorV004Control::search`.

### RetrieveContext

`RetrieveContextResponse` contains `RetrievalSummary` with `EvidenceStatus`,
`degraded` and string degradation codes. It has no retryable flag, coverage class,
dropped-parent identities, per-parent reasons or rejection stages.

RetrieveContext delegates to Search and maps the Search response. This is a useful
single semantic core for parity, but Search transport errors currently propagate
without a RetrieveContext response body.

Evidence:

- `proto/astravector_embedding.proto`: `EvidenceStatus`, `RetrievalSummary`,
  `RetrieveContextResponse`;
- `src/grpc/mod.rs`: `AstraVectorRetrievalFacade::retrieve_context`.

## Hydration Boundary

Production retrieval performs one PostgreSQL batch fetch. Candidate triples are
passed as parallel UUID arrays and joined through `unnest(... WITH ORDINALITY)`.
The query applies zone, document, version, access-level, lifecycle, deletion and
expiry predicates. PostgreSQL `statement_timeout` is bounded by both configured
timeout and remaining request budget.

This confirms:

```text
batch SQL hydration = AVAILABLE
per-parent SQL hydration = NOT PRESENT
per-parent retries = NOT PRESENT
partial PostgreSQL row timeout = NOT A REAL DATABASE CAPABILITY
total batch statement timeout = AVAILABLE
```

The selected-parent timeout proof must therefore use a test-only orchestration
fault after the batch boundary. It must not split the query into N parent calls or
claim a PostgreSQL row-level timeout.

Evidence:

- `src/grpc/mod.rs`: hydration-key construction and
  `fetch_hydrated_search_contexts_multi` call;
- `src/persistence/mod.rs`: `fetch_hydrated_search_contexts_multi`.

## P0 Finding: Parent Identity Is Not Binding-Backed

### Risk

The batch query joins the matched chunk and parent using IDs derived from Qdrant.
It verifies that both chunks have the same zone, document and version, but it does
not prove that the matched child canonically belongs to that parent. It neither
joins `vector_bindings_v004` nor requires `m.parent_chunk_id = p.id` for child
granularities.

A tampered or stale Qdrant payload can therefore pair a valid child with a
different existing parent in the same zone/document/version. That parent can pass
all current visibility predicates and become final evidence.

Classification:

```text
FIX486F-P0-001
QDRANT_PARENT_ID_NOT_VERIFIED_AGAINST_CANONICAL_BINDING
```

### Required Fix

Keep one batch SQL query, but make candidate identity binding-backed:

1. Extract `binding_id`, `chunk_id`, `parent_chunk_id` and zone from each Qdrant
   hit.
2. Pass all four identities to the batch query with ordinality.
3. Join `vector_bindings_v004` by `(access_zone_id, binding_id)`.
4. Require binding chunk/document/version/parent identities to agree with canonical
   chunks and candidate payload.
5. For PARENT candidates, require matched and parent identity to be the same
   canonical PARENT chunk.
6. For SUB candidates, require canonical `m.parent_chunk_id = p.id` and binding
   `parent_chunk_id = p.id`.
7. Require binding lifecycle/search synchronization state suitable for retrieval.
8. Emit `BINDING_INVALID` when payload and canonical binding disagree.

This fix preserves batch hydration and zone isolation and introduces no N+1 query.

## Missing and Rejected Parent Semantics

Hydration currently returns only successful rows. Missing rows are counted by
`astravector_parent_hydration_missing_total` and silently skipped while assembling
results. There is no per-candidate terminal reason, no retryable classification,
and no response/trace propagation.

Consequences:

- a stale or orphan candidate can disappear without client-visible reason;
- missing canonical parent can be misreported as ordinary empty retrieval;
- partial loss can still look like a normal successful response;
- the Phase F reason-consistency gate cannot pass.

Required design: return a batch hydration outcome containing successful contexts
and a rejection record for every requested candidate. Every input ordinal must end
as exactly one of `HYDRATED`, `BINDING_INVALID`, `VISIBILITY_REJECTED`,
`HYDRATION_MISSING`, `PARENT_HYDRATION_TIMEOUT`, or `EMPTY_CONTEXT`.

## Candidate Selection and Refill

Search obtains up to `candidate_limit` Qdrant hits, groups them by parent, hydrates
that bounded set, silently drops missing rows, and later truncates/merges final
contexts. There is candidate surplus by default (`top_k * 4`), but there is no
explicit refill loop after canonical rejection. A caller may also set
`candidate_limit == top_k`.

Current capability:

```text
candidate surplus = DEFAULT_ONLY
canonical filtering before final merge = PRESENT
bounded refill after rejection = ABSENT
non-interference guarantee = NOT PROVEN
```

Recommended design: add a bounded internal rejection reserve or bounded refill
strategy independent of ranking weights. The Phase F control must prove a raw
stale candidate entered the window while a known-valid survivor remained in the
final set.

## Deadlines, Retry and Cancellation

- The request deadline is the minimum of transport and configured query deadline.
- PostgreSQL statement timeout is capped by remaining request budget minus a safety
  margin.
- PostgreSQL statement timeout maps to gRPC `DEADLINE_EXCEEDED`.
- Parent hydration has no retry loop, so deadline multiplication is currently
  absent.
- Request cancellation uses a request-scoped cancellation token for the broader
  search path.

The hydration SQL call itself is awaited directly. Phase F must verify cancellation
latency empirically and must not add detached timeout tasks.

## Concurrency, Cache and Circuit Breaker

Retrieval uses a bounded global semaphore. Parent hydration is one batch query per
request. No parent-hydration single-flight, negative cache or circuit breaker was
found. Existing caches concern embeddings/MMR rather than canonical parent
hydration.

This simplifies request-scoped fault isolation. A Phase F failpoint must remain
keyed by caller correlation ID and must not mutate global hydration state.

## Observability

Existing hydration metrics are limited to:

```text
astravector_parent_hydration_duration_seconds
astravector_parent_hydration_candidates_total
astravector_parent_hydration_missing_total
```

Missing capabilities:

- outcome/reason labels;
- timeout scope;
- degraded request count;
- stale/binding rejection count;
- structured per-candidate rejection trace;
- response/trace/metric reason mapping.

Metrics must use bounded labels only. Correlation, document, parent, chunk, binding
and point identities belong in protected traces, never metric labels.

## Blank Content Invariant

The public chunking path rejects blank source text and constructs PARENT chunks
from non-empty normalized input. However, the database schema declares only
`content text NOT NULL`; it does not reject empty or whitespace-only PARENT text.
The hydration query also has no blank-content predicate.

Result:

```text
EMPTY_PARENT_SCHEMA_INVARIANT = NOT_PROVEN
EMPTY_PARENT_INGESTION_INVARIANT = PARTIAL
EMPTY_PARENT_RETRIEVAL_GUARD = ABSENT
```

Phase F therefore requires the controlled runtime scenario or a new enforceable
schema plus ingestion and retrieval regression. Existing metadata-only SOURCE
chunks must remain allowed; any schema rule must be granularity-aware.

## Production Deletion and Qdrant Identity

The Phase E deletion path advances `ttl_generation`, fences DELETE_POINT effects,
and excludes legal-hold/already-deleting bindings. Qdrant payload includes zone ID
and code, binding ID, document/version, root/source/parent/chunk identities,
granularity, representation, access/lifecycle, expiry, hold state and version
metadata.

The payload is sufficiently rich for Phase F provenance. The defect is not missing
payload identity; it is failure to verify that identity against canonical binding
during hydration.

## Backward-Compatible Response Design

Preferred implementation:

1. Add optional degradation/rejection messages to Search diagnostics and
   RetrieveContext summary without renumbering existing fields.
2. Keep successful healthy responses byte/semantically backward compatible.
3. Use gRPC status plus structured status details for total timeout; do not invent
   a successful response body.
4. Normalize both entry points from one internal hydration outcome type.
5. Expose public dropped-parent identity as an opaque identifier or safe logical
   identifier; keep UUIDs in protected debug trace only.

Proto changes require regenerated code and compatibility tests. No public field may
activate failpoints.

## Implementation Gate

Implementation may begin only after approving this design checkpoint:

```text
1. binding-backed batch hydration
2. exhaustive per-candidate hydration outcomes
3. bounded rejection reserve/refill
4. test-only request-scoped failpoint capability
5. backward-compatible degradation diagnostics
6. bounded-cardinality metrics and protected traces
7. blank-parent runtime protection
```

Ranking weights, RRF, Graph, MMR, tokenizer behavior, frozen queries and qrels
remain unchanged.
