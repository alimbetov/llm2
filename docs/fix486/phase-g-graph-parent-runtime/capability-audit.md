# FIX486G Production Capability Audit

## Scope and source identity

Audit performed on branch `codex/fix486g-graph-parent-proof` at
`d1f49c13749e19727b4369add3cf7e98fe4461e7` before any Phase G production
change.

## Production path

```text
Search / RetrieveContext
  -> direct dense/sparse/lexical retrieval
  -> PostgreSQL direct-candidate hydration
  -> Graph seed selection by (access_zone_id, matched_chunk_id)
  -> expand_chunks_1hop_by_seed_keys
  -> fetch_contexts_for_graph_related_chunks_multi
  -> SearchResultV004 construction
  -> direct/Graph merge and dedup
  -> MMR
  -> token budget
  -> final PostgreSQL visibility recheck
```

`RetrieveContext` constructs a `SearchRequestV004` and calls the same Search
implementation. Its Graph semantics therefore share the same expansion,
hydration, merge, and final visibility path.

## Capability map

| Capability | Implementation | Audit result |
|---|---|---|
| Relation storage | `rag_graph_nodes`, `rag_graph_edges`, partitioned views | Present |
| Production relation ingestion | `save_quality_fixture_relation_edges_tx`, invoked during production chunk persistence from immutable ingestion metadata | Present, fixture-scoped metadata contract |
| Seed identity | `(access_zone_id, matched_chunk_id)` from hydrated direct results | Correct composite key |
| Zone filter | Graph edge joins and node joins include `access_zone_id`; hydration and final visibility repeat it | Present |
| Lifecycle filter | active/non-quarantined/non-expired nodes, edges, chunks and active document versions | Present |
| Hop handling | repository method is physically one-hop and emits `hop_distance=1` | Bounded to one hop |
| Cycle handling | no recursive traversal; self-edge is not explicitly excluded | Correct for multi-hop cycles, incomplete for self-edge attribution |
| Related parent | `p.id=COALESCE(c.parent_chunk_id,c.id)` in one batch query | Uses related child's own canonical parent |
| Parent batch hydration | one SQL statement for all related chunk IDs | No N+1 |
| Binding validation | related hydration uses `LEFT JOIN vector_bindings_v004` | Defect: missing/unsynced/mismatched binding can be accepted |
| Relation identity | expansion returns type, score and rank | Defect: `edge_id` and stable relation identity are discarded |
| Candidate reserve | Graph SQL is limited to final `max_related` before binding hydration | Defect: invalid high-ranked endpoints can consume the candidate window |
| Graph disabled | expansion branch requires request flag and global enablement | No Graph call when disabled |
| Dedup | stable result identity with bounded secondary metadata merge | Graph origin can survive; complete edge identity is unavailable upstream |
| MMR/token budget | shared final selection; no Phase G tuning required | Present |
| Final visibility | batch PostgreSQL `filter_visible_chunk_ids_multi` | Present |
| Deadline | request deadline plus bounded Graph stage budget, cancellation token and semaphore | Present |
| Cache/retry | no Graph result cache or internal retry in request path | Deterministic, bounded |
| Metrics | expansion, candidate, relation, merge, timeout and admission metrics | Present; missing wrong-parent/binding/provenance rejection counters |

## Reproduced design defects

### FIX486G-P0-001 — Related Graph binding is not canonical-gated

Evidence: both graph hydration queries use `LEFT JOIN
astravector.vector_bindings_v004 b`. Parent selection is canonical, but no
binding is required and neither `qdrant_sync_status='SYNCED'` nor binding
document/version/parent identity is asserted.

Risk: a stale Graph node can become final evidence after its searchable binding
has disappeared or diverged.

### FIX486G-P1-001 — Stable edge identity is lost

Evidence: `RelatedChunk` contains relation type/score/rank but not `edge_id`,
relation source/properties, or related document/version. Response metadata
therefore cannot reconstruct the required immutable edge identity.

Risk: semantically similar duplicate relations cannot be independently audited,
and complete provenance cannot be proven.

### FIX486G-P1-002 — Invalid candidates can exhaust the pre-hydration window

Evidence: `expand_chunks_1hop_by_seed_keys` applies `LIMIT max_related_chunks`
before `fetch_contexts_for_graph_related_chunks_multi` rejects unhydratable
rows. No Graph-specific bounded reserve/refill is used.

Risk: a high-ranked invalid endpoint can displace a lower-ranked valid Graph
survivor without changing ranking weights.

### FIX486G-P1-003 — Self-edge may receive false Graph attribution

Evidence: the one-hop SQL does not exclude `related_chunk_id = seed_chunk_id`.

Risk: a self relation can relabel direct evidence as Graph-derived and inflate
Graph contribution.

### FIX486G-P1-004 — Relation endpoints fan out across granularities

Evidence: relation ingestion projected logical block relations to every
`PARENT`, `SUB_180` and `SUB_260` endpoint combination even when ingestion
metadata declared physical child granularities.

Risk: one logical edge can receive duplicate Graph credit, fault isolation can
mutate the wrong child and matched-child provenance becomes unstable.

### FIX486G-P0-002 — Parent deduplication removes relation-bearing child seeds

Runtime evidence from `fix486g-20260722T165646Z` showed that Qdrant returned
the expected A1 parent group, but final parent representative selection removed
its hydrated `SUB_180`/`SUB_260` identities before Graph seed selection. The
physical child-to-child `REPAIRED_BY` edge was therefore unreachable.

Risk: valid production relations silently fail whenever a PARENT point wins its
parent group, even though a canonical searchable child from the same group was
retrieved and hydrated.

## Existing correct invariants

- Related child hydration uses its own `parent_chunk_id`; seed parent reuse is
  not present in the normal construction path.
- Multi-zone identity uses `(access_zone_id, chunk_id)` throughout expansion and
  hydration lookup.
- Parent hydration and final visibility are batched.
- Graph expansion is bounded by semaphore, timeout, candidate and edge limits.
- Search and RetrieveContext share production Graph logic.
- No Phase G ranking, RRF, MMR or token-budget tuning is needed.

## Audit verdict

```text
UNKNOWN_MATERIAL_CAPABILITIES = 0
CAPABILITY_AUDIT = COMPLETE
PRODUCTION_REPAIR_ALLOWED_ONLY_AFTER_RED_CONTRACTS = true
```
