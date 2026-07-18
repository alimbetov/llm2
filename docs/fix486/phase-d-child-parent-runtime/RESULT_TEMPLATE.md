# FIX486D Child/Parent Runtime Proof Result

## Identity

| Field | Value |
|---|---|
| Repository | `alimbetov/llm2` |
| Branch | `<branch>` |
| Tested source SHA | `<sha>` |
| origin/main SHA | `<sha>` |
| Bank ID | `fix486-hierarchical-bank` |
| Bank version/status | `1.0.0 / FROZEN` |
| Bank aggregate SHA-256 | `cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff` |
| Cargo.lock SHA-256 | `<sha256>` |
| Runtime binary SHA-256 | `<sha256>` |
| Resolved config SHA-256 | `<sha256>` |
| Model SHA-256 | `<sha256>` |
| Tokenizer SHA-256 | `<sha256>` |
| PostgreSQL image identity | `<reference/digest>` |
| Qdrant image identity | `<reference/digest>` |
| Run ID | `<run-id>` |

## Scope

Phase D proves only:

```text
FIX486-01 — exact matched child and canonical parent hydration
FIX486-02 — parent deduplication
FIX486-07 — exact technical matched-child evidence preservation
```

Each case is executed through both `Search` and `RetrieveContext`.

The phase does not certify isolation, lifecycle failure cases, Graph expansion, MMR/token budgets, load or production readiness.

## Mandatory gates

| Gate | Result | Evidence |
|---|---|---|
| `cargo fmt --all --check` | `<PASS/BLOCKED>` | `<artifact>` |
| locked check | `<PASS/BLOCKED>` | `<artifact>` |
| locked clippy | `<PASS/BLOCKED>` | `<artifact>` |
| locked all-target tests | `<PASS/BLOCKED>` | `<artifact>` |
| SQLx prepare check | `<PASS/BLOCKED>` | `<artifact>` |
| Phase A bank contracts | `<PASS/BLOCKED>` | `<artifact>` |
| Phase C frozen-bank contracts | `<PASS/BLOCKED>` | `<artifact>` |
| Phase D focused contracts | `<PASS/BLOCKED>` | `<artifact>` |

## Ingestion and integrity

| Assertion | Value |
|---|---:|
| Production ingestion | `<PASS/BLOCKED>` |
| Active documents | `<count>` |
| Active versions | `<count>` |
| Parent chunks | `<count>` |
| Child chunks | `<count>` |
| Synchronized bindings | `<count>` |
| Completed outbox effects | `<count>` |
| Failed/dead-letter outbox effects | `<count>` |
| Qdrant points | `<count>` |
| Orphan children | `<count>` |
| Cross-zone bindings | `<count>` |
| Cross-document bindings | `<count>` |
| Cross-version bindings | `<count>` |

## Identity-map summary

| Logical object | Runtime identity | Granularity | Parent | Source block | Result |
|---|---|---|---|---|---|
| `parent-a1` | `<uuid>` | `PARENT` | — | `<id>` | `<PASS>` |
| `child-a1-180` or descendant | `<uuid>` | `SUB_180` | `<uuid>` | `<id>` | `<PASS>` |
| `child-a1-260` or descendant | `<uuid>` | `SUB_260` | `<uuid>` | `<id>` | `<PASS>` |

Document any tokenizer-aware physical descendants while preserving the frozen logical child provenance.

## Primary query matrix

| Case | Query | Entry point | Matched child | Parent | Status |
|---|---|---|---|---|---|
| `FIX486-01` | `q-child-parent-exact` | Search | `<logical/runtime>` | `parent-a1 / <uuid>` | `<PASS>` |
| `FIX486-01` | `q-child-parent-exact` | RetrieveContext | `<logical/runtime>` | `parent-a1 / <uuid>` | `<PASS>` |
| `FIX486-02` | `q-parent-dedup` | Search | `<children>` | `parent-a1 / <uuid>` | `<PASS>` |
| `FIX486-02` | `q-parent-dedup` | RetrieveContext | `<children>` | `parent-a1 / <uuid>` | `<PASS>` |
| `FIX486-07` | `q-exact-identifier` | Search | `<logical/runtime>` | `parent-a1 / <uuid>` | `<PASS>` |
| `FIX486-07` | `q-exact-identifier` | RetrieveContext | `<logical/runtime>` | `parent-a1 / <uuid>` | `<PASS>` |

## FIX486-01 evidence

### Search

```text
matched_chunk_id_present=<true|false>
parent_chunk_id_present=<true|false>
matched_parent_ids_distinct=<true|false>
canonical_binding_valid=<true|false>
matched_text_contains_ORA-00904=<true|false>
matched_text_contains_content_chunks_v004=<true|false>
parent_text_contains_ASTRA_CANONICAL_STATE_A1=<true|false>
forbidden_anchors_found=<list>
```

### RetrieveContext

```text
matched_chunk_id_present=<true|false>
parent_chunk_id_present=<true|false>
matched_parent_ids_distinct=<true|false>
canonical_binding_valid=<true|false>
matched_text_contains_ORA-00904=<true|false>
matched_text_contains_content_chunks_v004=<true|false>
parent_text_contains_ASTRA_CANONICAL_STATE_A1=<true|false>
forbidden_anchors_found=<list>
```

## FIX486-02 dedup evidence

### Search

```text
eligible_children_for_parent_a1=<count>
pre_dedup_parent_occurrences=<count>
final_parent_a1_occurrences=<count>
final_duplicate_parent_contexts=<count>
dedup_reasons=<list>
```

### RetrieveContext

```text
eligible_children_for_parent_a1=<count>
pre_dedup_parent_occurrences=<count>
final_parent_a1_occurrences=<count>
final_duplicate_parent_contexts=<count>
dedup_reasons=<list>
```

## FIX486-07 exact-child evidence

### Search

```text
matched_text_contains_/api/v1/search=<true|false>
matched_text_contains_parent_chunk_id=<true|false>
exact_technical_match=<true|false>
sparse_score_present=<true|false>
lexical_score_present=<true|false>
matched_child_evidence_lost=<0|count>
```

### RetrieveContext

```text
matched_text_contains_/api/v1/search=<true|false>
matched_text_contains_parent_chunk_id=<true|false>
exact_technical_match=<true|false>
sparse_score_present=<true|false>
lexical_score_present=<true|false>
matched_child_evidence_lost=<0|count>
```

## Entry-point parity

| Query | Zone/document/version | Child comparison | Parent comparison | Anchor comparison | Result |
|---|---|---|---|---|---|
| `q-child-parent-exact` | `<same/different>` | `<same/allowed-equivalent/different>` | `<same/different>` | `<same/different>` | `<PASS>` |
| `q-parent-dedup` | `<same/different>` | `<same/allowed-equivalent/different>` | `<same/different>` | `<same/different>` | `<PASS>` |
| `q-exact-identifier` | `<same/different>` | `<same/allowed-equivalent/different>` | `<same/different>` | `<same/different>` | `<PASS>` |

## Repeatability

| Stage | Result | Notes |
|---|---|---|
| Warm Search repeat | `<PASS/BLOCKED>` | `<notes>` |
| Warm RetrieveContext repeat | `<PASS/BLOCKED>` | `<notes>` |
| Runtime restart | `<PASS/BLOCKED>` | `<notes>` |
| Post-restart Search | `<PASS/BLOCKED>` | `<notes>` |
| Post-restart RetrieveContext | `<PASS/BLOCKED>` | `<notes>` |
| Physical identity stability | `<PASS/BLOCKED>` | `<notes>` |

## Repairs

| Defect | Severity | Root cause | Regression | Fix commit | Rerun |
|---|---|---|---|---|---|
| `<id>` | `<P0/P1/P2>` | `<cause>` | `<test/evidence>` | `<sha>` | `<PASS>` |

Unresolved in-scope defects:

```text
P0=<count>
P1=<count>
```

## Evidence

```text
external evidence root: <path>
manifest SHA-256: <sha256>
evidence aggregate SHA-256: <sha256>
evidence file count: <count>
mandatory missing files: <count>
hash mismatches: <count>
```

## Scope boundary

Confirm:

```text
frozen bank changed=false
qrels changed=false
ranking tuned=false
Graph tuned=false
MMR/token-budget tuned=false
access-zone filters weakened=false
production readiness claimed=false
```

## Verdict

Successful form:

```text
FIX486_CHILD_PARENT_RUNTIME_PROOF_PASS
```

Blocked form:

```text
FIX486_CHILD_PARENT_RUNTIME_PROOF_BLOCKED
blocking_stage=<stage>
failure_code=<code>
evidence_preserved=true
```
