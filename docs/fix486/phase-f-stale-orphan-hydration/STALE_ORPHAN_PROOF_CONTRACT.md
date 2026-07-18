# FIX486F Stale/Orphan Proof Contract

## 1. Contract objective

This contract proves that a searchable Qdrant child candidate cannot become a final context unless PostgreSQL confirms a valid, visible, zone-scoped canonical parent and child-to-parent binding.

The contract covers:

- stale child with deleted or invisible parent;
- orphan child with missing parent identity;
- ranking non-interference;
- explicit rejection provenance;
- Search/RetrieveContext parity;
- cleanup and repeatability.

## 2. Frozen case

Primary frozen case:

```text
FIX486-05 / q-orphan-child
```

Frozen expectations:

```text
expected_zone = zone-a
expected_final_context_count = 0
forbidden_version = 3
forbidden_anchor = ASTRA_DELETED_PARENT_TRAP
required_drop_reason_any =
  HYDRATION_MISSING
  VISIBILITY_REJECTED
  DELETED_PARENT
stale_child_contexts = 0
```

The frozen qrel remains an independent oracle. It must not be used to manufacture actual runtime reason codes or logical identities.

## 3. Canonical authority invariant

A final context requires all of the following:

1. request access zone matches candidate zone;
2. candidate document identity resolves canonically;
3. candidate document version is visible;
4. child binding exists and is valid;
5. parent exists;
6. parent belongs to the same zone/document/version;
7. parent visibility is allowed;
8. hydrated content is non-empty or satisfies a proven content invariant;
9. candidate survives final context selection.

Failure of any canonical condition prevents promotion to final context.

## 4. Candidate state taxonomy

Every stale/orphan candidate observed by the runner must receive exactly one terminal classification:

```text
NOT_RETURNED_BY_QDRANT_FILTER
RETURNED_BUT_BINDING_INVALID
RETURNED_BUT_PARENT_MISSING
RETURNED_BUT_PARENT_DELETED
RETURNED_BUT_PARENT_INVISIBLE
RETURNED_BUT_VERSION_INVISIBLE
RETURNED_BUT_EMPTY_CONTENT
REJECTED_AT_FINAL_CONTEXT_ASSEMBLY
```

`UNKNOWN`, `NOT_CHECKED` and empty reason values are forbidden.

## 5. Scenario A — stale child with deleted parent

### 5.1 Setup

The stale point must originate from a production-ingested and successfully projected child.

Required setup sequence:

1. ingest frozen document through production path;
2. wait for canonical and Qdrant projection completion;
3. capture child point and canonical provenance;
4. transition parent/version to deleted or invisible state through production lifecycle path;
5. verify canonical deletion/invisibility;
6. reinsert the captured child point into the phase-owned Qdrant collection;
7. run frozen query through Search and RetrieveContext;
8. remove the injected point during cleanup.

### 5.2 Required provenance

Before mutation capture:

```text
point_id
vector_hash
payload_hash
child_physical_id
parent_physical_id
logical_child_id
logical_parent_id
access_zone_code
logical_document_id
document_version
content_hash
```

The injected stale point must preserve its original vector and payload except for fields explicitly documented by the fault plan.

### 5.3 Expected result

```text
final_context_count = 0
stale_child_contexts = 0
retryable = false
reason = DELETED_PARENT or VISIBILITY_REJECTED
```

Forbidden:

- deleted parent text in final contexts;
- `ASTRA_DELETED_PARENT_TRAP` in final contexts;
- status `FOUND`;
- empty context object;
- silent drop without reason;
- classification as ordinary semantic no-answer without rejection diagnostics.

### 5.4 Required trace

The evidence must prove:

```text
raw_qdrant_candidate_present = true
canonical_parent_state = DELETED or INVISIBLE
candidate_promoted = false
final_context_count = 0
```

## 6. Scenario B — orphan child with missing parent

### 6.1 Preferred setup

The orphan point is derived from a production point but references a phase-owned, non-existent parent identity.

Required sequence:

1. capture valid production point;
2. preserve zone, document, version, child and content provenance;
3. replace only the parent identity with a deterministic non-existent phase-owned identity;
4. insert the point into the phase-owned Qdrant collection;
5. verify no corresponding canonical parent exists;
6. execute Search and RetrieveContext;
7. remove the point during cleanup.

Direct deletion of canonical PostgreSQL rows is not preferred because it may violate foreign-key invariants and distort the proof surface.

### 6.2 Expected result

```text
final_context_count = 0
orphan_final_contexts = 0
retryable = true
reason = HYDRATION_MISSING
```

Missing parent is a consistency/degradation condition. It must not become a normal semantic no-answer.

### 6.3 Forbidden behavior

```text
FOUND
SUCCESS_NO_EVIDENCE
FOUND_WITH_EMPTY_CONTEXT
HYDRATED_FROM_CHILD_TEXT
PARENT_ID_GUESSED
```

The system must not reconstruct parent content from stale child text.

## 7. Ranking non-interference

### 7.1 Clean control

Run the relevant query when the stale/orphan point is absent.

Capture:

- final logical parent IDs;
- content hashes;
- required intent coverage;
- final context count;
- relative order of surviving contexts;
- normalized scores where available.

### 7.2 Faulted control

Run the same query under identical canonical state with the stale/orphan point present.

After excluding the rejected point from comparison, valid result semantics must remain equivalent.

### 7.3 Hard gates

```text
valid_contexts_displaced_by_stale = 0
required_intents_lost_by_stale = 0
surviving_parent_set_changed = 0
stale_candidate_promoted = 0
```

Score comparison may use a documented tolerance. Parent set, content hashes and required intent coverage are exact gates.

If the stale point consumes top-k capacity and removes a valid result, Phase F is blocked even when the stale point itself is later rejected.

## 8. Candidate refill requirement

The implementation must prove one of the following:

1. canonical visibility filtering occurs before final top-k selection;
2. retrieval requests enough candidate surplus that rejected candidates cannot starve required valid results;
3. the system refills from remaining candidates after canonical rejection.

The proof must state which strategy production uses.

Artifact:

```text
candidate-selection-strategy.json
```

## 9. Zone, document and version isolation

Injected points remain subject to Phase E invariants.

Required hard gates:

```text
cross_zone_results = 0
cross_document_results = 0
wrong_version_results = 0
foreign_parent_hydrations = 0
```

An orphan candidate must not resolve to an identically named parent in another zone, document or version.

Canonical parent lookup key must include sufficient composite identity to prevent collision.

## 10. Drop-reason provenance

For every rejected candidate, response-normalized evidence and internal trace must agree on:

```text
reason_code
rejection_stage
retryable
logical_parent_id
access_zone_code
logical_document_id
document_version
```

Internal trace additionally records physical IDs and timing.

Required artifact:

```text
diagnostic-propagation-audit.json
```

Hard gate:

```text
response_trace_reason_mismatches = 0
```

## 11. Metrics requirements

The proof must map actual project metrics to these semantic counters:

```text
stale_candidate_rejections_total{entry_point,reason}
candidate_rejections_total{entry_point,reason}
degraded_requests_total{entry_point,reason}
```

Required deltas:

- one stale rejection per executed stale scenario and entry point, adjusted only for documented retries;
- one orphan rejection per executed orphan scenario and entry point;
- no success counter increment for rejected-only results;
- no high-cardinality identity labels.

## 12. Search/RetrieveContext parity

For stale and orphan scenarios, Search and RetrieveContext must agree on:

- no final context;
- reason class;
- retryable class;
- zone/document/version identity;
- forbidden anchor count;
- stale candidate promotion count;
- semantic versus infrastructure classification.

Hard gate:

```text
stale_orphan_entry_point_mismatches = 0
```

## 13. Warm repeat

Without re-ingestion, repeat stale and orphan scenarios.

Required stability:

- same candidate origin;
- same rejection reason class;
- no final contexts;
- no duplicate canonical state;
- no Qdrant point growth;
- no unexpected outbox changes;
- deterministic metric deltas.

## 14. Restart repeat

Restart only AstraVector runtime while preserving PostgreSQL and Qdrant state.

Required after restart:

- failpoint/injection state is explicitly re-established or confirmed;
- stale/orphan rejection remains unchanged;
- no foreign-zone resolution;
- no duplicate state;
- cleanup remains possible.

## 15. Cleanup

Cleanup must:

- remove injected stale/orphan points;
- verify original production projection is not corrupted;
- remove phase-owned collection/database resources according to policy;
- release ports;
- disable all failpoints;
- preserve evidence and model files.

Required artifact:

```text
fault-cleanup.json
```

PASS is forbidden when injected points remain.

## 16. Evidence artifacts

Minimum stale/orphan evidence:

```text
fault-point-origin.json
fault-plan.json
stale-point-injection.json
orphan-point-injection.json
stale-candidate-trace.json
deleted-parent-rejection.json
orphan-candidate-trace.json
missing-parent-rejection.json
candidate-selection-strategy.json
ranking-non-interference.json
diagnostic-propagation-audit.json
metrics-delta.json
fault-cleanup.json
```

## 17. Hard-gate summary

```text
stale_final_contexts = 0
orphan_final_contexts = 0
deleted_parent_contexts = 0
missing_parent_contexts = 0
stale_candidate_promoted = 0
unclassified_stale_drops = 0
valid_contexts_displaced_by_stale = 0
required_intents_lost_by_stale = 0
cross_zone_results = 0
wrong_version_results = 0
response_trace_reason_mismatches = 0
cleanup_leaks = 0
```

Any non-zero value blocks Phase F PASS.