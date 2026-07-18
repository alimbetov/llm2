# v011/fix486d — Child/Parent Runtime Proof

## 1. Document status

```text
DOCUMENT_TYPE=IMPLEMENTATION_AND_PROOF_SPECIFICATION
PHASE=FIX486D_CHILD_PARENT_RUNTIME_PROOF
BASE_BRANCH=main
WORK_BRANCH=codex/fix486d-child-parent-runtime-proof
APPROVED_BASE_SHA=9de9383d6cfef3b1ed32637688907a55429b3cf3
BANK_ID=fix486-hierarchical-bank
BANK_VERSION=1.0.0
BANK_STATUS=FROZEN
BANK_AGGREGATE_SHA256=cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff
PREVIOUS_VERDICT=FIX486_FROZEN_EXECUTABLE_BANK_PASS
PRODUCTION_STATUS=BLOCKED
```

The actual branch and source identities must be resolved at execution time. Any source, bank, model, tokenizer or resolved-config drift blocks the official proof.

## 2. Preconditions

The phase starts only after the following facts are independently verified:

- PR #23 is merged into `main`;
- merge SHA is `9de9383d6cfef3b1ed32637688907a55429b3cf3` or an explicitly approved descendant;
- bank `1.0.0` is `FROZEN`;
- aggregate SHA-256 equals `cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff`;
- `fix486c` executable ingestion and all 11 query artifacts passed;
- no unresolved `fix486c` P0/P1 defect exists;
- the worktree is clean before official evidence generation.

## 3. Objective

Prove, through the real production ingestion and retrieval paths, that AstraVector preserves the distinction between a directly matched child chunk and its hydrated parent context.

The phase must prove all of the following:

1. the retriever can match the expected `SUB_180` or `SUB_260` child;
2. the exact matched-child evidence remains visible in the response and ranking trace;
3. the canonical parent is hydrated from PostgreSQL in the same access zone, document and version;
4. the returned parent text is the expected parent content and is not substituted from another logical parent;
5. multiple child candidates sharing one parent are deduplicated to one final parent context;
6. `Search` and `RetrieveContext` expose the same normalized logical child/parent identity;
7. exact technical identifiers survive ranking, hydration and final assembly;
8. every assertion is evaluated against the immutable bank qrels rather than runtime-derived expectations.

The phase answers:

> Does the production runtime return the correct matched child and the correct canonical parent, without losing child evidence or duplicating the parent?

## 4. Allowed final verdicts

Exactly one:

```text
FIX486_CHILD_PARENT_RUNTIME_PROOF_PASS
```

or:

```text
FIX486_CHILD_PARENT_RUNTIME_PROOF_BLOCKED
```

A PASS proves only the positive child/parent path for the declared Phase D cases. It does not prove:

- cross-zone isolation;
- inactive/deleted/expired lifecycle behavior;
- stale Qdrant child handling;
- hydration timeout/degradation behavior;
- Graph parent expansion;
- MMR or token-budget selection;
- load, latency SLO or production readiness.

## 5. Frozen cases in scope

Only the following immutable bank cases are authoritative for Phase D.

### FIX486-01 — Exact child match and parent hydration

Query:

```text
q-child-parent-exact
```

Required result:

- expected zone: `zone-a`;
- expected document: `doc-hierarchy`;
- expected version: `1`;
- expected parent: `parent-a1`;
- matched child: one of `child-a1-180`, `child-a1-260`;
- matched text contains `ORA-00904` and `content_chunks_v004`;
- parent text contains `ASTRA_CANONICAL_STATE_A1`;
- forbidden anchors are absent.

### FIX486-02 — Parent deduplication

Query:

```text
q-parent-dedup
```

Required result:

- expected parent: `parent-a1`;
- at least two eligible child-level candidates for the same parent must be observable before final parent deduplication, when the production trace exposes them;
- final unique parent count is `1`;
- final occurrences of the parent are at most `1`;
- no unrelated or cross-zone parent is returned.

### FIX486-07 — Exact technical evidence preservation

Query:

```text
q-exact-identifier
```

Required result:

- expected parent: `parent-a1`;
- expected child: one of `child-a1-180`, `child-a1-260`;
- matched text contains `/api/v1/search` and `parent_chunk_id`;
- ranking diagnostics contain `exact_technical_match`;
- at least one sparse or lexical score is present;
- matched-child evidence is not replaced by parent-only text.

## 6. Explicitly out of scope

Do not execute Phase D verdict logic against:

- `FIX486-03` zone isolation;
- `FIX486-04` active-version filtering;
- `FIX486-05` stale/orphan child;
- `FIX486-06` hydration timeout;
- `FIX486-08` Graph parent expansion;
- `FIX486-09` multi-intent token budget;
- `FIX486-10` large-parent pressure.

The frozen files may be read and hash-verified, but their expectations must not be changed and their runtime outcomes must not affect the Phase D verdict.

## 7. Production-path boundary

Official evidence must use:

```text
frozen bank materialization
→ public/production ingestion facade
→ canonical PostgreSQL state
→ outbox projection
→ Qdrant searchable child points
→ production Search
→ production RetrieveContext
→ normalized child/parent proof
```

Forbidden shortcuts:

- direct insertion of successful child/parent fixtures into chunk tables;
- direct insertion of Qdrant points for positive cases;
- constructing expected physical UUIDs before ingestion;
- replacing model-backed retrieval with a fixture-only mock;
- asserting success solely from database shape without a live retrieval response;
- changing queries, qrels, corpus, tokenizer behavior or ranking settings to obtain PASS.

Read-only SQL and Qdrant audit queries are allowed.

## 8. Identity model

Frozen bank identities are logical expectations. Runtime identities must be captured after ingestion.

For every in-scope logical object record:

```text
logical_zone_id
runtime_access_zone_id
logical_document_id
runtime_document_id
logical_version
runtime_document_version_id
logical_parent_id
runtime_parent_chunk_id
logical_child_id
runtime_child_chunk_id
granularity
source_block_id
```

The proof must never compare a runtime UUID directly with a logical ID string. It must use an explicit identity map generated from canonical persisted metadata and phase-owned provenance.

## 9. Canonical child/parent assertions

For every positive result, prove:

```text
MATCHED_CHUNK_ID_PRESENT=true
PARENT_CHUNK_ID_PRESENT=true
MATCHED_CHUNK_ID_IS_CHILD=true
PARENT_CHUNK_ID_IS_PARENT=true
MATCHED_AND_PARENT_IDS_DISTINCT=true
CHILD_PARENT_BINDING_EXISTS=true
SAME_ACCESS_ZONE=true
SAME_DOCUMENT=true
SAME_DOCUMENT_VERSION=true
PARENT_STATUS_VISIBLE=true
CHILD_STATUS_VISIBLE=true
MATCHED_TEXT_NONEMPTY=true
PARENT_TEXT_NONEMPTY=true
```

The canonical PostgreSQL audit is authoritative for hierarchy membership and visibility. Qdrant is evidence of the matched search projection, not the source of truth for parent ownership.

## 10. Text provenance assertions

### Matched child

The final response or diagnostics must preserve:

- the physical matched child ID;
- the child source block ID;
- child granularity;
- child text or an unambiguous matched-text field;
- retrieval-source flags;
- ranking stages sufficient to show that the child was directly retrieved.

### Hydrated parent

The final response must preserve:

- the physical parent chunk ID;
- parent source block ID;
- parent text;
- hydration provenance;
- an unambiguous association to the matched child.

### No substitution

The proof fails if:

- matched child ID points to the parent row;
- parent ID points to a child row;
- child text is silently replaced by parent text;
- parent text belongs to another parent;
- parent text is assembled from unrelated fragments without explicit provenance;
- a missing parent is reported as a normal positive result.

## 11. Search and RetrieveContext parity

Execute every in-scope query through both public entry points where supported:

```text
Search
RetrieveContext
```

Normalize volatile fields and compare:

```text
access zone
document identity
document version
matched child identity
parent identity
source block identity
logical status
required anchors
forbidden anchors
```

The APIs may differ in presentation, score formatting or response envelope. They must not disagree on the selected logical child/parent pair.

Required assertion:

```text
SEARCH_RETRIEVE_LOGICAL_CHILD_PARENT_MATCH=true
```

## 12. Parent deduplication proof

For `q-parent-dedup`, produce evidence at two stages when diagnostics support it:

1. pre-parent-dedup candidate set;
2. final assembled context set.

Required metrics:

```text
eligible_child_candidates_for_parent_a1 >= 2
unique_parent_ids_before_final_assembly >= 1
final_occurrences_of_parent_a1 = 1
final_duplicate_parent_contexts = 0
```

If the production trace does not expose pre-dedup candidates, add narrowly scoped diagnostics that record identifiers and stages without changing ranking or selection behavior.

Do not weaken the requirement to merely checking that one result was returned.

## 13. Exact identifier proof

For `q-exact-identifier`, the proof must show:

```text
matched_text contains /api/v1/search
matched_text contains parent_chunk_id
exact_technical_match = true
sparse_score present OR lexical_score present
matched_child_evidence_lost = 0
```

The identifiers must be found in matched-child evidence. Their presence only in the hydrated parent is insufficient.

## 14. Execution matrix

### D1 — Clean production ingestion and canonical audit

1. Verify source, bank, model, tokenizer and config identities.
2. Start phase-owned PostgreSQL and Qdrant from clean state.
3. Apply migrations.
4. Start the release runtime.
5. Ingest the frozen fixture through the production path.
6. Wait for bounded outbox completion and projection consistency.
7. Build the logical-to-runtime identity map.
8. Record canonical hierarchy and Qdrant projection audits.

### D2 — Search proof

Execute the three in-scope queries through `Search`.

Capture:

- raw request and response;
- normalized result;
- ranking trace;
- child/parent identity audit;
- anchor assertions;
- dedup metrics.

### D3 — RetrieveContext proof

Execute the same three queries through `RetrieveContext`.

Capture the same artifacts and compare with D2.

### D4 — Warm repeatability

Repeat D2 and D3 without reingestion.

Required stable values:

- logical child/parent identity;
- expected anchors;
- forbidden-anchor absence;
- parent dedup outcome;
- query verdict.

Scores and latency may vary within ordinary floating-point and runtime variance. A different logical parent or lost child evidence is not allowed.

### D5 — Restart repeatability

Restart the runtime without reingestion, wait for readiness, and repeat the three queries through both entry points.

Required assertion:

```text
POST_RESTART_CHILD_PARENT_LOGICAL_RESULT_STABLE=true
```

This is a narrow persistence check for child/parent proof, not a repetition of the full Phase B recovery campaign.

## 15. Required machine-readable statuses

Every query execution must be classified as exactly one:

```text
PASS
FAIL
BLOCKED
SKIPPED
```

`SKIPPED` is not acceptable for an in-scope mandatory query.

Infrastructure, model, tokenizer, deadline, ingestion, projection and evidence failures must not be converted into a query-level no-answer PASS.

## 16. Mandatory static and contract gates

Run with locked dependencies:

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo sqlx prepare --check -- --all-targets --all-features
cargo test --locked --test fix486_hierarchical_bank_contracts -- --nocapture
cargo test --locked --test fix486c_frozen_bank_contracts -- --nocapture
```

Phase D implementation must add focused contracts for:

- immutable in-scope query selection;
- identity-map completeness;
- child/parent canonical binding validation;
- Search/RetrieveContext normalized parity;
- exact-child anchor preservation;
- parent deduplication;
- fail-closed verdict aggregation;
- evidence completeness.

## 17. Evidence root

Use an external path:

```text
<ASTRAVECTOR_EVIDENCE_ROOT>/fix486d/<run-id>/
```

Recommended structure:

```text
environment/
source/
bank/
config/
model-tokenizer/
infrastructure/
ingestion/
identity-map/
canonical-audit/
qdrant-audit/
search/
retrieve-context/
comparisons/
restart/
logs/
metrics/
stage-results.json
query-results.jsonl
manifest.json
defect-register.json
FIX486D-CHILD-PARENT-RUNTIME-RESULT.md
```

Large logs remain outside Git. Commit only compact schemas, scripts, summaries, manifests and hashes.

## 18. Defect policy

For every reproducible in-scope P0/P1 defect:

1. preserve failing evidence;
2. record source and identity hashes;
3. add a failing regression test;
4. document the root cause;
5. implement the smallest production-safe fix in a separate commit;
6. do not change frozen bank payload or qrels;
7. rerun the same failed stage;
8. rerun D1–D5 and all mandatory gates;
9. publish before/after evidence.

Examples of in-scope defects:

- matched child ID is replaced by parent ID;
- child belongs to a different parent than returned;
- parent comes from wrong document/version/zone;
- parent appears twice after deduplication;
- exact child anchors disappear after hydration;
- Search and RetrieveContext select different logical child/parent pairs;
- infrastructure failure is reported as a successful no-answer.

Ranking tuning, Graph tuning and broad API redesign are prohibited in this phase.

## 19. Definition of Done

```text
[ ] Approved base lineage recorded
[ ] Frozen bank hash verified unchanged
[ ] Clean production ingestion completed
[ ] Identity map complete for in-scope parents and children
[ ] Canonical hierarchy audit PASS
[ ] Qdrant projection audit PASS
[ ] FIX486-01 Search PASS
[ ] FIX486-01 RetrieveContext PASS
[ ] FIX486-02 Search PASS
[ ] FIX486-02 RetrieveContext PASS
[ ] FIX486-07 Search PASS
[ ] FIX486-07 RetrieveContext PASS
[ ] Search/RetrieveContext normalized parity PASS
[ ] Parent deduplication proof PASS
[ ] Exact matched-child evidence proof PASS
[ ] Warm repeatability PASS
[ ] Restart repeatability PASS
[ ] All mandatory static and contract gates PASS
[ ] No mandatory stage skipped
[ ] No unresolved in-scope P0/P1
[ ] External evidence completeness PASS
[ ] Frozen payload and aggregate SHA unchanged
```

## 20. Final gate

`FIX486_CHILD_PARENT_RUNTIME_PROOF_PASS` requires every mandatory assertion and evidence item to pass.

Any mandatory failure, skipped query, missing trace, identity mismatch, bank drift, incomplete evidence or unresolved in-scope P0/P1 produces:

```text
FIX486_CHILD_PARENT_RUNTIME_PROOF_BLOCKED
```
