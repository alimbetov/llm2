# fix486d Child/Parent Proof Contract

## 1. Contract identity

```text
contract_id=fix486d-child-parent-proof
contract_version=1
bank_id=fix486-hierarchical-bank
bank_version=1.0.0
bank_aggregate_sha256=cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff
mandatory_query_count=3
mandatory_entry_points=Search,RetrieveContext
```

## 2. Authoritative query set

The runner must load the frozen query and qrel files, then select exactly:

```text
q-child-parent-exact
q-parent-dedup
q-exact-identifier
```

Hard-coded copies of question text or qrels outside the frozen bank are forbidden. The phase-owned runner may hard-code only the immutable query IDs used to select records from the verified bank.

The phase blocks if:

- any required query is absent;
- a required query has no qrel;
- more than one qrel exists for a query;
- a selected query or qrel differs from its frozen hash-verified bytes;
- any additional query affects the aggregate Phase D verdict.

## 3. Runtime identity-map schema

The proof must produce a machine-readable identity map with one row per logical parent and child used by Phase D.

Required fields:

```json
{
  "logical_zone_id": "zone-a",
  "runtime_access_zone_id": "uuid",
  "logical_document_id": "doc-hierarchy",
  "runtime_document_id": "uuid",
  "logical_version": 1,
  "runtime_document_version_id": "uuid",
  "logical_chunk_id": "child-a1-180",
  "runtime_chunk_id": "uuid",
  "chunk_role": "CHILD",
  "granularity": "SUB_180",
  "logical_parent_id": "parent-a1",
  "runtime_parent_chunk_id": "uuid",
  "source_block_id": "string",
  "content_sha256": "hex"
}
```

Parent rows use:

```text
chunk_role=PARENT
runtime_parent_chunk_id=null
```

Required logical entries:

```text
parent-a1
child-a1-180
child-a1-260
```

If tokenizer-aware segmentation creates additional physical children, they must retain source-block provenance and be represented in the identity map without changing the frozen logical expectations.

## 4. Canonical binding contract

For every returned `(matched_chunk_id, parent_chunk_id)` pair, read-only PostgreSQL audit must prove:

```text
matched chunk exists
matched chunk granularity is SUB_180 or SUB_260, or an explicitly documented model-safe descendant preserving the same logical child provenance
parent chunk exists
parent chunk granularity is PARENT
matched.parent_chunk_id = parent.id
matched.access_zone_id = parent.access_zone_id
matched.document_id = parent.document_id
matched.document_version_id = parent.document_version_id
version number = 1
document logical identity = doc-hierarchy
zone logical identity = zone-a
both rows satisfy production visibility rules
```

The proof blocks if hierarchy validity is inferred only from response fields without canonical audit.

## 5. Qdrant projection contract

The matched child must have a corresponding searchable Qdrant point or an explicitly documented production projection record.

Required evidence:

```text
point exists for matched child binding
payload access zone matches canonical zone
payload document identity matches canonical document
payload document version matches canonical version
payload chunk identity maps to canonical child
projection status is synchronized
```

The parent does not need to be selected as the original Qdrant hit. Parent ownership remains canonical in PostgreSQL.

## 6. Query-result schema

Each entry-point execution must write one result object:

```json
{
  "schema_version": 1,
  "phase": "fix486d",
  "query_id": "q-child-parent-exact",
  "case_id": "FIX486-01",
  "entry_point": "Search",
  "status": "PASS",
  "reason": null,
  "runtime_identity": {
    "access_zone_id": "uuid",
    "document_id": "uuid",
    "document_version": 1,
    "matched_chunk_id": "uuid",
    "parent_chunk_id": "uuid",
    "matched_source_block_id": "string",
    "parent_source_block_id": "string"
  },
  "logical_identity": {
    "zone": "zone-a",
    "document": "doc-hierarchy",
    "version": 1,
    "matched_child": "child-a1-180",
    "parent": "parent-a1"
  },
  "assertions": {},
  "artifact_refs": {},
  "failure_codes": []
}
```

Allowed statuses:

```text
PASS
FAIL
BLOCKED
SKIPPED
```

For the six mandatory query/entry-point combinations, only `PASS` is accepted by the final gate.

## 7. FIX486-01 assertions

Required assertions:

```text
expected_status_found=true
expected_zone_match=true
expected_document_match=true
expected_version_match=true
expected_parent_match=true
expected_child_any_match=true
matched_chunk_id_present=true
parent_chunk_id_present=true
matched_parent_ids_distinct=true
canonical_child_parent_binding=true
matched_text_contains_ORA_00904=true
matched_text_contains_content_chunks_v004=true
parent_text_contains_ASTRA_CANONICAL_STATE_A1=true
forbidden_ZONE_B_SECRET_PARENT_A1_absent=true
forbidden_ASTRA_INACTIVE_VERSION_TRAP_absent=true
```

The matched anchors must be evaluated against matched-child evidence. Parent anchors must be evaluated against hydrated-parent evidence.

## 8. FIX486-02 assertions

Required assertions:

```text
expected_status_found=true
expected_parent_match=true
pre_dedup_children_for_parent_a1_at_least_two=true
final_unique_parent_count=1
final_parent_a1_occurrences=1
final_duplicate_parent_contexts=0
forbidden_ZONE_B_SECRET_PARENT_A1_absent=true
```

If the runtime has fewer than two eligible child candidates because the configured candidate limit is too small, the phase is `BLOCKED`. Do not change ranking weights or the frozen query. A narrowly scoped candidate/trace depth configuration may be used only if it is already part of the supported production diagnostic contract and does not alter final ranking semantics.

## 9. FIX486-07 assertions

Required assertions:

```text
expected_status_found=true
expected_parent_match=true
expected_child_any_match=true
matched_text_contains_/api/v1/search=true
matched_text_contains_parent_chunk_id=true
trace_exact_technical_match=true
sparse_or_lexical_score_present=true
matched_child_evidence_lost=0
```

A result fails if exact identifiers are present only in the parent text or only in the query echo.

## 10. Search/RetrieveContext parity schema

For each query, normalize both entry-point results to:

```json
{
  "query_id": "string",
  "logical_zone": "string",
  "logical_document": "string",
  "document_version": 1,
  "logical_matched_child": "string",
  "logical_parent": "string",
  "matched_required_anchors": [],
  "parent_required_anchors": [],
  "forbidden_anchors_found": [],
  "status": "PASS"
}
```

Required comparison:

```text
same logical zone=true
same logical document=true
same version=true
same logical matched child=true, or both select children explicitly allowed by the same qrel and both map to the same expected parent
same logical parent=true
same required-anchor result=true
same forbidden-anchor result=true
```

If the qrel allows either `child-a1-180` or `child-a1-260`, entry points may choose different allowed children only when both preserve the required child anchors, map to `parent-a1`, and the difference is explicitly recorded. The preferred strict result is identical logical child selection.

## 11. Dedup trace contract

The runtime evidence must expose enough information to derive:

```text
candidate child IDs before parent dedup
candidate parent IDs before parent dedup
final parent IDs
candidate drop/dedup reason
```

Allowed implementation:

- use existing ranking trace fields;
- extend diagnostics with identifier-only parent-dedup stages;
- emit phase-owned read-only audit correlated by trace/request ID.

Forbidden implementation:

- changing final ranking;
- changing candidate scores;
- returning hidden content solely for the test;
- logging raw secrets or unrestricted document text.

## 12. Repeatability contract

Run identifiers may differ. The following fields must remain stable across warm repeat and runtime restart:

```text
logical zone
logical document
version
logical parent
allowed logical child
required-anchor satisfaction
forbidden-anchor absence
dedup outcome
query status
```

Physical IDs must remain stable when production deterministic identity inputs are unchanged. Any physical-ID drift must be classified and investigated rather than normalized away.

## 13. Fail-closed contract

The aggregate proof must be blocked by any of:

```text
BANK_HASH_MISMATCH
SOURCE_IDENTITY_MISMATCH
MODEL_IDENTITY_MISMATCH
TOKENIZER_IDENTITY_MISMATCH
CONFIG_IDENTITY_MISMATCH
INGESTION_FAILED
PROJECTION_INCOMPLETE
IDENTITY_MAP_INCOMPLETE
CANONICAL_BINDING_INVALID
MATCHED_CHILD_NOT_PRESERVED
PARENT_HYDRATION_INVALID
PARENT_DEDUP_FAILED
EXACT_IDENTIFIER_EVIDENCE_LOST
SEARCH_RETRIEVE_MISMATCH
MANDATORY_QUERY_SKIPPED
EVIDENCE_INCOMPLETE
UNRESOLVED_P0_P1
```

An infrastructure or evidence error must never become a normal `INSUFFICIENT` or no-answer result for the positive Phase D cases.

## 14. Immutability contract

Throughout Phase D:

```text
bank version remains 1.0.0
bank status remains FROZEN
aggregate SHA-256 remains cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff
queries unchanged
qrels unchanged
corpus unchanged
graph unchanged
lifecycle unchanged
```

Any required fixture correction must create a new bank version and blocks Phase D against `1.0.0`.
