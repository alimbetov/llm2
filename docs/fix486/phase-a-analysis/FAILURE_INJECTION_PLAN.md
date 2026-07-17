# FIX486A failure injection plan

## Rules

- Failpoints compile only under `smoke-failpoints` and require the existing enable flag.
- They are stage names, never fixture/query IDs.
- Every scenario runs through public tonic `RetrieveContext` or `Search`.
- A timeout/backend failure cannot be asserted as ordinary `INSUFFICIENT` evidence.
- Partial and total policies are tested separately with unchanged queries/qrels.

## Matrix

| Failpoint | Injection location | Expected result | Forbidden result | Target |
|---|---|---|---|---|
| `parent_hydration_before_query_delay` | before `fetch_hydrated_search_contexts_multi` SQL | deadline exceeded or unavailable | success with empty ordinary no-answer | fix486f |
| `parent_hydration_after_query_error` | after SQL, before row mapping | unavailable/degraded error | FOUND or silent empty | fix486f |
| `parent_hydration_omit_one` | test-controlled repository result boundary | surviving contexts plus explicit degraded warning and missing identity trace | full coverage claim | fix486f |
| `graph_context_hydration_delay` | Graph related-context batch fetch | direct-only degraded response when policy permits | Graph candidate with wrong parent | fix486g |
| `graph_context_hydration_omit_one` | Graph context map construction | candidate dropped with reason | child substituted as parent | fix486g |
| existing `qdrant_dense_search` | Qdrant client | sparse/FTS fallback or explicit failure by mode | false backend success | existing smoke |
| existing `qdrant_sparse_search` | Qdrant client | dense/FTS fallback or explicit failure by mode | false backend success | existing smoke |

## Orphan and stale projection setup

1. Ingest and activate the bank document through production gRPC.
2. Capture child, parent, binding and point identities.
3. Leave a child point in Qdrant while making the canonical parent deleted or absent in a
   transaction controlled by the test.
4. Query the unchanged `q-orphan-child` request.
5. Assert zero stale contexts, no forbidden text, explicit drop trace and no cross-zone result.
6. Restore or destroy the isolated Testcontainers environment.

## Cancellation and deadlines

Each delay failpoint must be longer than the request stage budget. Tests assert bounded completion,
cancelled SQL/future work, released semaphore permits, no task leak and correct tonic status. The
partial omission failpoint is not a timeout simulation; it exists to define policy for a subset of
unhydratable candidates in a successful batch.
