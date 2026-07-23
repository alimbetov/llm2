# FIX486G — Graph Parent Runtime Proof

## 1. Purpose

Phase G proves that GraphRAG expansion returns the canonical parent of the **related Graph child**, not the parent of the direct seed, while preserving complete, zone-scoped and lifecycle-valid provenance through the production retrieval path.

Frozen case:

```text
FIX486-08 / q-graph-repair
```

Allowed verdicts:

```text
FIX486_GRAPH_PARENT_RUNTIME_PROOF_PASS
FIX486_GRAPH_PARENT_RUNTIME_PROOF_BLOCKED
```

This initial specification is contract-first. Production Graph code, proof runners, Compose profiles and evidence tooling must not be changed until document review and the production capability audit are complete.

## 2. Lineage

Planning branch:

```text
codex/fix486g-graph-parent-proof
```

Base branch and tested anchor:

```text
codex/fix486f-runtime-proof
c5fa4cb41cf9cd57ddf914562723bbe9758110cd
FIX486_STALE_ORPHAN_HYDRATION_RUNTIME_PROOF_PASS
```

The official Phase G runner must record the final tested source SHA and must reject a dirty worktree or a remote/local SHA mismatch.

Frozen bank identity:

```text
version: 1.0.0
status: FROZEN
aggregate SHA-256:
cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff
```

Phase G must not modify frozen corpus, queries, qrels, graph relations or lifecycle payloads.

## 3. Architectural statement

GraphRAG is an evidence expansion mechanism. It is not an authorization mechanism and it is not a substitute for canonical hydration.

The required identity chain is:

```text
direct seed candidate
  -> seed matched child
  -> graph edge
  -> related child
  -> canonical binding validation
  -> related child's own canonical parent
  -> final Graph context and citation
```

The following shortcut is forbidden:

```text
related child -> reuse seed parent
```

For the frozen positive relation:

```text
seed child:     child-a1-180
seed parent:    parent-a1
relation:       REPAIRED_BY
related child:  child-a3-180
related parent: parent-a3
```

PostgreSQL remains canonical for access zone, document/version identity, lifecycle, binding validity, child-to-parent relation and parent content. Qdrant and graph relation rows are projections or retrieval inputs and cannot independently authorize final evidence.

## 4. Scope

### 4.1 Positive own-parent proof

The production path must prove:

1. direct retrieval identifies the expected A1 seed evidence;
2. one-hop Graph expansion traverses the frozen `REPAIRED_BY` edge;
3. the related A3 child is identified explicitly;
4. canonical hydration validates the related child's binding;
5. the final Graph-derived context is `parent-a3`;
6. the Graph result is not normalized to `parent-a1`;
7. Graph provenance survives final deduplication, MMR admission and token-budget admission;
8. Search and RetrieveContext expose equivalent normalized Graph semantics.

### 4.2 Security and lifecycle proof

The phase must prove:

- zone-A seeds cannot traverse zone-B relations;
- Graph relation endpoints are zone-scoped;
- inactive, deleted, expired or invisible related versions are rejected;
- missing or binding-invalid related children/parents are rejected;
- a stale Graph candidate cannot become final context;
- Graph expansion cannot weaken direct hydration or final visibility checks.

### 4.3 Provenance proof

Every accepted Graph context must retain, in response diagnostics or protected trace/evidence:

```text
seed_access_zone_id
seed_document_id
seed_document_version
seed_matched_chunk_id
seed_parent_chunk_id
relation_id or stable relation identity
relation_type
relation_score
related_access_zone_id
related_document_id
related_document_version
related_chunk_id
related_parent_chunk_id
hop_index
origin = GRAPH
```

Public response fields may use safe logical or opaque identifiers. Protected traces must retain physical identities required for audit.

### 4.4 Ranking non-interference

A malformed or rejected Graph candidate must not displace a valid Graph or direct candidate merely because it had a higher pre-hydration score.

Phase G may add a bounded candidate reserve or refill only if current behavior cannot preserve valid survivors. It must not tune Graph weights, RRF constants or MMR policy to force the frozen answer.

### 4.5 Hop-limit proof

For `q-graph-repair`:

```text
enable_graph_expansion = true
graph_max_hops = 1
```

The runtime must prove exactly one admitted Graph hop. Second-hop evidence, cycles and repeated edge traversal are forbidden in this phase.

### 4.6 Controls

Mandatory controls:

1. Graph disabled: no Graph-derived result or provenance;
2. Graph enabled with the frozen edge: `parent-a3` is returned;
3. wrong-zone relation present: no traversal or leakage;
4. invalid related binding: explicit rejection and valid-survivor preservation;
5. stale/inactive related target: no final context;
6. warm repeat: normalized result stable;
7. restart repeat: normalized result stable;
8. Search/RetrieveContext parity.

## 5. Frozen expectations

Frozen query:

```text
q-graph-repair
Как canonical state помогает восстановить отсутствующие Qdrant points?
```

Expected direct parent:

```text
parent-a1
```

Expected Graph parent:

```text
parent-a3
```

Expected related child:

```text
child-a3-180 or child-a3-260
```

Required relation:

```text
REPAIRED_BY or frozen compatible RELATED_TO
```

Frozen hard gates:

```text
graph_wrong_parent = 0
graph_cross_zone_results = 0
graph_provenance_missing = 0
inactive_or_deleted_graph_results = 0
```

Phase-owned additional hard gates:

```text
graph_seed_parent_reuse = 0
graph_binding_invalid_contexts = 0
graph_hop_limit_violations = 0
graph_disabled_origin_count = 0
graph_false_success = 0
```

## 6. Required result semantics

### 6.1 Healthy Graph result

```text
status = FOUND
origin includes GRAPH
direct parent includes parent-a1
graph parent includes parent-a3
graph relation includes REPAIRED_BY
hop_count = 1
warnings contain no Graph integrity warning
```

### 6.2 Rejected Graph target with valid survivor

```text
status = FOUND or DEGRADED according to surviving coverage
valid contexts > 0
rejected Graph target is absent
rejection reason is explicit
retryable reflects the actual cause
false full coverage = 0
```

### 6.3 Total Graph-only failure

When direct evidence remains valid, Graph failure must not erase the direct result. It may return direct evidence with explicit Graph degradation.

When the request semantically requires Graph-only evidence and no valid Graph target survives, the runtime must not report full Graph coverage. It must return an explicit degraded/insufficient result or a transport failure according to the production error class.

## 7. Required failure classification

At minimum, evidence must distinguish:

```text
GRAPH_EDGE_MISSING
GRAPH_ENDPOINT_ZONE_MISMATCH
GRAPH_ENDPOINT_VERSION_MISMATCH
GRAPH_BINDING_INVALID
GRAPH_TARGET_VISIBILITY_REJECTED
GRAPH_TARGET_HYDRATION_MISSING
GRAPH_TARGET_EMPTY_CONTEXT
GRAPH_HOP_LIMIT_REJECTED
GRAPH_CYCLE_REJECTED
GRAPH_DEADLINE_EXCEEDED
```

Equivalent existing production reason codes may be used if the mapping is documented and unambiguous.

## 8. Production capability audit

Before runtime changes, document the current production behavior for:

- Graph relation storage schema and identity;
- relation ingestion path;
- seed candidate identity;
- edge filtering by zone/document/version;
- graph expansion query and deadline;
- related child hydration path;
- related parent resolution;
- direct/Graph dedup identity;
- provenance fields in response and trace;
- cycle and hop-limit handling;
- Graph candidate limits and admission order;
- Graph interaction with hydration rejection reserve;
- Graph interaction with MMR and token budget;
- metrics and bounded labels;
- Search/RetrieveContext parity;
- Graph-disabled behavior;
- cache, retry and concurrency behavior.

Unknown material behavior blocks implementation approval.

## 9. Contract-first gate

Before production edits, focused tests must demonstrate current behavior for:

1. related child hydrates its own parent;
2. seed parent cannot be reused as related parent;
3. canonical binding mismatch is rejected;
4. zone-B edge cannot be traversed by zone A;
5. stale/inactive related target is rejected;
6. provenance is complete;
7. Graph disabled produces no Graph origin;
8. one-hop limit is enforced;
9. invalid Graph candidate does not displace a healthy survivor;
10. Search/RetrieveContext semantics are equivalent;
11. direct/Graph dedup does not erase distinct provenance;
12. no N+1 parent hydration is introduced.

A test may be green against current behavior only when the capability is already correctly implemented. Any uncovered correctness gap must first have a reproducing red contract.

## 10. Out of scope

The following are explicitly deferred:

- FIX486-09 MMR/token-budget proof;
- FIX486-10 large-parent pressure;
- Graph weight or relation-score tuning;
- multi-hop GraphRAG beyond the one-hop rejection control;
- entity extraction and automatic relation discovery quality;
- global graph algorithms;
- generated-answer faithfulness;
- Mac load/capacity certification;
- aggregate FIX486 verdict.

Phase G may verify that Graph provenance survives existing MMR and token-budget stages, but it must not claim Phase H coverage.

## 11. Phase completion

Phase G is complete only when:

- documentation review is approved;
- capability audit has no unknown material behavior;
- all focused contracts pass;
- any in-scope P0/P1 defect is repaired and rerun;
- the phase-owned runtime runner completes from a clean tested SHA;
- external evidence passes hash verification;
- repository result index is published;
- unresolved in-scope P0/P1 count is zero;
- final verdict is one of the two allowed verdicts.
