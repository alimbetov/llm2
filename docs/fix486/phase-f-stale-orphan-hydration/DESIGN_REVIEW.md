# FIX486F Design Review Checkpoint

## Input

This checkpoint consumes:

- `DOCUMENT_REVIEW.md` at verdict `APPROVED_FOR_CAPABILITY_AUDIT`;
- `capability-audit.md` with `UNKNOWN_MATERIAL_CAPABILITIES = 0`;
- frozen bank `1.0.0 / FROZEN` at aggregate
  `cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff`.

## Design Verdict

```text
APPROVED_FOR_CONTRACT_TEST_IMPLEMENTATION
RUNTIME_CODE_NOT_YET_APPROVED
```

Focused contracts must be written and fail against the current behavior before
production runtime changes begin.

## Canonical Hydration Design

Introduce an internal candidate key containing:

```text
access_zone_id
binding_id
matched_chunk_id
parent_chunk_id
granularity
raw_rank
```

Pass all identities through one batch SQL query with ordinality. Join the canonical
binding, matched chunk, parent chunk and document version. Require the binding and
payload identities to agree. Child candidates require
`matched.parent_chunk_id = parent.id`; PARENT candidates require matched and parent
identity to be the same canonical PARENT.

Return one terminal internal outcome per input ordinal:

```text
HYDRATED
BINDING_INVALID
VISIBILITY_REJECTED
HYDRATION_MISSING
PARENT_HYDRATION_TIMEOUT
EMPTY_CONTEXT
```

The query remains batch-based. No per-parent SQL loop is permitted.

## Failpoint Boundary

Use an immutable, bounded plan loaded at startup only when an explicit
non-production configuration flag is enabled. The plan matches caller
`correlation_id`, entry point, zone and selected parent identities and includes
`max_activations`.

This approach provides:

- no public activation API;
- no unauthenticated mutable control plane;
- no global sleep;
- deterministic concurrent faulted/healthy requests;
- recovery without restart after the bounded matching activation is consumed;
- failpoint-disabled-by-default behavior after a normal startup.

`TIMEOUT_SELECTED_PARENTS` is applied to orchestration outcomes after the one batch
fetch. `TIMEOUT_ALL_PARENTS` may additionally exercise the real batch statement
deadline. Evidence must distinguish these mechanisms.

## Response Compatibility

Add optional, new-numbered protobuf messages for retrieval degradation and dropped
parent summaries. Existing healthy fields and field numbers remain unchanged.

Search and RetrieveContext are normalized from the same internal hydration outcome.
Partial failure returns successful transport with explicit degraded/partial
semantics and surviving contexts. Total failure returns gRPC
`DEADLINE_EXCEEDED`/`UNAVAILABLE` with structured status details and no normal
response body or content.

No failpoint field is added to a public request.

## Candidate Rejection Reserve

Do not change ranking weights, RRF, Graph or MMR. Add a bounded hydration rejection
reserve before final context selection. The internal fetch window must exceed the
requested final parent count by a configured, capped reserve. If the implementation
uses a second fetch, it must be bounded by the existing candidate maximum and
overall request deadline.

Contract tests must prove that one high-ranked rejected candidate cannot displace a
known-valid survivor.

## Blank Parent Strategy

Immediately reject empty/whitespace PARENT content during hydration with
`EMPTY_CONTEXT`. Add ingestion regression coverage. A future granularity-aware
database constraint may harden storage, but must continue to allow metadata-only or
disabled SOURCE storage modes.

Phase F PASS requires either that migration-backed invariant or a runtime scenario
proving the retrieval guard. The retrieval guard is required in both cases for
defense in depth against legacy rows.

## Metrics and Trace

Add bounded metric labels only:

```text
entry_point
outcome
reason
scope
```

Correlation, zone, document, binding, chunk, parent and point identities remain in
structured protected traces. Metric deltas are process-epoch scoped across restart.

## Concurrency and Cleanup

Retain the existing request admission semaphore and one batch hydration call per
request. Do not introduce parent single-flight, negative cache or circuit breaker in
Phase F. The immutable failpoint plan owns only bounded atomic activation counters;
dropping/restarting the phase runtime clears all state.

Runner cleanup must additionally remove injected Qdrant points and phase-owned
infrastructure while preserving evidence and model files.

## Contract-First Gate

Before runtime edits, add failing focused contracts for:

1. binding/payload parent mismatch rejection;
2. exhaustive outcome for every candidate ordinal;
3. missing-parent versus binding-invalid distinction;
4. partial and total timeout normalization;
5. non-vacuous candidate non-interference;
6. request-scoped bounded failpoint matching;
7. blank PARENT rejection;
8. bounded metric label schema;
9. Search/RetrieveContext semantic parity;
10. no N+1 parent hydration.

Only after these contracts demonstrate the current gaps may the corresponding
production fixes be implemented.
