# FIX486G Graph Parent Proof Contract

## 1. Contract identity

This contract owns the frozen runtime case:

```text
FIX486-08 / q-graph-repair
```

It proves Graph expansion identity, canonical parent resolution, provenance, isolation and lifecycle safety. It does not prove MMR/token-budget quality or load capacity.

## 2. Canonical object model

A Graph-expanded candidate must be represented internally by identities equivalent to:

```text
seed_zone_id
seed_document_id
seed_document_version
seed_matched_chunk_id
seed_parent_chunk_id
relation_id
relation_type
relation_score
related_zone_id
related_document_id
related_document_version
related_chunk_id
related_parent_chunk_id
hop_index
raw_graph_rank
```

The related parent is derived from the canonical binding of `related_chunk_id`. It must not be copied from the seed result.

## 3. Frozen positive chain

The proof must establish the following normalized chain:

```text
zone-a / doc-hierarchy / v1
child-a1-180
parent-a1
REPAIRED_BY
child-a3-180 or physical tokenizer descendant
parent-a3
```

Allowed physical child descendants must preserve frozen logical provenance and must be mapped in the runtime identity map.

## 4. Direct and Graph result distinction

The final normalized evidence must distinguish:

```text
DIRECT:
  matched child = A1 child
  parent = parent-a1

GRAPH:
  seed child = A1 child
  relation = REPAIRED_BY
  related child = A3 child
  parent = parent-a3
```

Deduplication may merge duplicate occurrences of the same final parent, but it must not collapse `parent-a3` into `parent-a1`, erase Graph origin or attach the edge to the wrong parent.

## 5. Positive assertions

For both Search and RetrieveContext:

- Graph expansion is actually executed;
- the frozen edge is present in the raw graph expansion window;
- `seed_chunk_id` maps to the expected A1 child;
- `seed_parent_chunk_id` maps to `parent-a1`;
- `related_chunk_id` maps to the expected A3 child;
- `related_parent_chunk_id` maps to `parent-a3`;
- the A3 binding is canonical and visible;
- final Graph content contains the A3 reconciliation evidence;
- relation type and score are preserved;
- `hop_index = 1`;
- Graph origin survives final selection;
- normalized Search/RetrieveContext identities are equivalent.

## 6. Wrong-parent rejection

The following candidate is invalid:

```text
seed child = child-a1-180
related child = child-a3-180
related parent = parent-a1
```

It must be classified as a Graph binding/parent identity failure and excluded before final context construction.

Required counters:

```text
graph_wrong_parent += 1
graph_seed_parent_reuse += 1
```

Official healthy proof requires both final counters to be zero. Fault-control scenarios may increment diagnostic counters but must still produce zero invalid final contexts.

## 7. Zone isolation

A zone-A request must not traverse or return the frozen zone-B self-relation.

Required controls:

- raw graph query is zone-scoped;
- endpoint hydration validates zone identity again;
- final visibility validates zone identity again;
- no zone-B anchor appears in matched, parent, citation, warning or public diagnostic text;
- protected traces may record rejected foreign identities only in the phase-owned evidence environment.

Hard gate:

```text
graph_cross_zone_results = 0
```

## 8. Document and version isolation

Seed and related endpoints must satisfy the production relation policy. For the frozen case they are in the same document/version.

A relation with a wrong document or version must be rejected unless an explicitly supported production relation type permits that boundary and canonical authorization is proven. Phase G must not introduce such a new policy.

Hard gates:

```text
graph_cross_document_unauthorized = 0
graph_wrong_version_results = 0
```

## 9. Lifecycle safety

The related child and its parent must both satisfy canonical lifecycle and visibility checks.

Forbidden final targets:

```text
INDEXING
DELETED
EXPIRED
missing document version
missing parent
binding-invalid child/parent pair
empty or whitespace parent
```

Hard gate:

```text
inactive_or_deleted_graph_results = 0
```

## 10. Provenance completeness

An accepted Graph result requires all of:

```text
seed identity
relation identity or stable relation tuple
relation type
relation score
related child identity
related parent identity
hop index
origin GRAPH
```

If any mandatory provenance component is absent, the result must not be presented as fully proven Graph evidence.

Hard gate:

```text
graph_provenance_missing = 0
```

Public response compatibility may expose a reduced safe subset, but protected trace plus normalized evidence must reconstruct the complete chain.

## 11. Graph-disabled control

With Graph expansion disabled:

- no graph repository/query call is counted as executed;
- no Graph origin appears;
- no relation provenance appears;
- `parent-a3` must not be credited as Graph-derived;
- direct retrieval behavior remains valid.

Hard gate:

```text
graph_disabled_origin_count = 0
```

## 12. Hop and cycle controls

For `graph_max_hops = 1`:

- only first-hop edges may be admitted;
- no second-hop relation may reach final selection;
- repeated traversal of the same relation is forbidden;
- a self-cycle or A→B→A cycle cannot increase evidence credit;
- duplicate edges cannot multiply intent coverage.

Hard gates:

```text
graph_hop_limit_violations = 0
graph_cycle_credit_inflation = 0
```

## 13. Candidate non-interference

The fault campaign must contain a non-vacuous control with:

- at least one invalid high-ranked Graph candidate inside the raw graph window;
- at least one valid lower-ranked Graph or direct survivor;
- identical valid logical parent set before and after fault injection;
- identical valid content hashes;
- no invalid candidate in final results;
- no loss of the valid survivor caused solely by rejection.

The implementation may use bounded reserve/refill, but must remain within the configured Graph candidate maximum and request deadline.

## 14. Deadline and degradation semantics

A Graph timeout is distinct from semantic no-answer.

Partial Graph failure with valid direct evidence may return:

```text
status = FOUND or DEGRADED
contexts > 0
Graph warning present
retryable according to cause
false full Graph coverage = 0
```

Total request failure may return `DEADLINE_EXCEEDED` or `UNAVAILABLE` only when the overall request cannot truthfully return valid evidence.

Forbidden classifications:

```text
SUCCESS_NO_EVIDENCE for infrastructure failure
FOUND with fabricated Graph provenance
FULL Graph coverage when required Graph evidence was dropped
```

## 15. Entry-point parity

Search and RetrieveContext must use the same normalized Graph candidate identity and canonical validation semantics.

Allowed differences are limited to response-envelope concerns documented by the capability audit. The following must be equivalent:

- seed identity;
- relation identity/type;
- related child;
- related parent;
- accepted/rejected classification;
- hop index;
- Graph origin;
- required anchor coverage;
- forbidden leakage counters.

## 16. Repeatability

Official proof requires:

1. healthy first run;
2. warm repeat without re-ingestion;
3. runtime restart;
4. post-restart repeat;
5. normalized identity comparison.

Timestamps, trace IDs, physical ports and process IDs may differ. Canonical object identities, content hashes, relation type and final verdict must match.

## 17. Observability

Bounded metric labels may include only stable enums such as:

```text
entry_point
outcome
reason
relation_type
hop_class
```

Zone, document, relation, binding and chunk identities must not become metric labels. They belong in protected structured trace/evidence.

Required phase counters:

```text
graph_candidates_total
graph_candidates_hydrated_total
graph_candidates_rejected_total
graph_wrong_parent_total
graph_cross_zone_rejected_total
graph_provenance_missing_total
graph_hop_limit_rejected_total
graph_final_contexts_total
```

Existing equivalent metrics may be mapped instead of duplicated.

## 18. No proof-only architecture degradation

Phase G must not introduce:

- N+1 SQL hydration;
- unbounded Graph fan-out;
- public mutable failpoint APIs;
- global sleeps;
- user-specific paths;
- fixture-ID checks in production ranking;
- relation-type hardcoding solely for `FIX486-08`;
- weakened access-zone or lifecycle filters;
- MMR or token-budget tuning.

## 19. Completion conditions

The contract passes only when:

```text
graph_wrong_parent = 0
graph_seed_parent_reuse = 0
graph_cross_zone_results = 0
graph_provenance_missing = 0
inactive_or_deleted_graph_results = 0
graph_hop_limit_violations = 0
graph_binding_invalid_contexts = 0
graph_false_success = 0
unresolved_P0 = 0
unresolved_P1 = 0
```
