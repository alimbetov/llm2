# fix486d Acceptance Criteria

## Lineage and immutability

- [ ] Base lineage resolves to merge SHA `9de9383d6cfef3b1ed32637688907a55429b3cf3` or an explicitly approved descendant.
- [ ] Worktree is clean before official evidence generation.
- [ ] Source SHA is recorded in every final artifact manifest.
- [ ] Bank ID is `fix486-hierarchical-bank`.
- [ ] Bank version is exactly `1.0.0`.
- [ ] Bank status is exactly `FROZEN`.
- [ ] Aggregate SHA-256 equals `cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff`.
- [ ] Per-file hashes remain unchanged.
- [ ] Frozen corpus, queries, qrels, graph and lifecycle files are not modified.

## Scope selection

- [ ] Exactly three mandatory query IDs are selected from the frozen bank.
- [ ] `q-child-parent-exact` is present with exactly one qrel.
- [ ] `q-parent-dedup` is present with exactly one qrel.
- [ ] `q-exact-identifier` is present with exactly one qrel.
- [ ] No Phase E–H query affects the Phase D aggregate verdict.
- [ ] Both `Search` and `RetrieveContext` are mandatory entry points.
- [ ] Six primary query-result rows are produced.

## Production ingestion

- [ ] Phase-owned PostgreSQL and Qdrant start from a controlled clean state.
- [ ] Migrations PASS.
- [ ] Release runtime starts and becomes SERVING.
- [ ] Real model and tokenizer identities are recorded.
- [ ] Frozen fixture is ingested through the production path.
- [ ] No direct successful-fixture insert into internal chunk tables is used.
- [ ] No direct positive-case Qdrant point injection is used.
- [ ] Document operations complete within bounded deadlines.
- [ ] All required outbox effects reach terminal success.
- [ ] No dead-letter outbox effect exists.
- [ ] Qdrant projection reaches bounded consistency.

## Identity map

- [ ] Runtime access-zone identity is mapped to `zone-a`.
- [ ] Runtime document identity is mapped to `doc-hierarchy`.
- [ ] Runtime version identity maps to logical version `1`.
- [ ] `parent-a1` runtime identity is recorded.
- [ ] `child-a1-180` runtime identity or model-safe descendant provenance is recorded.
- [ ] `child-a1-260` runtime identity or model-safe descendant provenance is recorded.
- [ ] Every in-scope runtime child maps to one canonical parent.
- [ ] Every identity-map row has source-block provenance.
- [ ] Every identity-map row has content SHA-256.
- [ ] No logical ID is compared directly to a runtime UUID without mapping.

## Canonical hierarchy audit

- [ ] Matched chunk rows exist in PostgreSQL.
- [ ] Parent rows exist in PostgreSQL.
- [ ] Matched chunks are child granularity or documented model-safe descendants.
- [ ] Parent chunks are parent granularity.
- [ ] `matched.parent_chunk_id = parent.id` for every result.
- [ ] Child and parent share access zone.
- [ ] Child and parent share document.
- [ ] Child and parent share document version.
- [ ] Version number is `1`.
- [ ] Visibility predicates pass for both child and parent.
- [ ] Orphan child count is `0`.
- [ ] Cross-zone binding count is `0`.
- [ ] Cross-document binding count is `0`.
- [ ] Cross-version binding count is `0`.

## Qdrant projection audit

- [ ] Every selected matched child has a synchronized searchable point.
- [ ] Qdrant payload zone matches canonical zone.
- [ ] Qdrant payload document matches canonical document.
- [ ] Qdrant payload version matches canonical version.
- [ ] Qdrant payload chunk identity maps to canonical child.
- [ ] No stale or unrelated point is used for a positive result.

## FIX486-01 — exact child and parent

### Search

- [ ] Status is `FOUND`/PASS according to the frozen qrel.
- [ ] Zone resolves to `zone-a`.
- [ ] Document resolves to `doc-hierarchy`.
- [ ] Version equals `1`.
- [ ] Parent resolves to `parent-a1`.
- [ ] Matched child resolves to `child-a1-180` or `child-a1-260`.
- [ ] `matched_chunk_id` is present.
- [ ] `parent_chunk_id` is present.
- [ ] Matched and parent IDs are distinct.
- [ ] Matched text contains `ORA-00904`.
- [ ] Matched text contains `content_chunks_v004`.
- [ ] Parent text contains `ASTRA_CANONICAL_STATE_A1`.
- [ ] `ZONE_B_SECRET_PARENT_A1` is absent.
- [ ] `ASTRA_INACTIVE_VERSION_TRAP` is absent.

### RetrieveContext

- [ ] All corresponding Search assertions pass.
- [ ] Normalized logical child/parent identity matches the accepted Search result.

## FIX486-02 — parent deduplication

### Search

- [ ] Expected parent resolves to `parent-a1`.
- [ ] At least two eligible child candidates for `parent-a1` are proven before final deduplication.
- [ ] Candidate-to-parent mapping is visible in trace/audit evidence.
- [ ] Final unique parent count is `1`.
- [ ] Final occurrences of `parent-a1` equal `1`.
- [ ] Final duplicate parent contexts equal `0`.
- [ ] Dedup/drop reason is recorded.
- [ ] `ZONE_B_SECRET_PARENT_A1` is absent.

### RetrieveContext

- [ ] All corresponding Search assertions pass.
- [ ] Normalized dedup outcome matches Search.

## FIX486-07 — exact technical child evidence

### Search

- [ ] Expected parent resolves to `parent-a1`.
- [ ] Matched child resolves to an allowed child.
- [ ] Matched text contains `/api/v1/search`.
- [ ] Matched text contains `parent_chunk_id`.
- [ ] `exact_technical_match=true` is present in trace evidence.
- [ ] Sparse score or lexical score is present.
- [ ] Exact identifiers are proven in matched-child evidence, not only parent text.
- [ ] `matched_child_evidence_lost=0`.

### RetrieveContext

- [ ] All corresponding Search assertions pass.
- [ ] Normalized logical child/parent identity matches the accepted Search result.

## Entry-point parity

- [ ] Search and RetrieveContext use the same logical zone.
- [ ] Search and RetrieveContext use the same logical document.
- [ ] Search and RetrieveContext use version `1`.
- [ ] Search and RetrieveContext select the same logical parent.
- [ ] Selected children are identical, or both are explicitly allowed by the same qrel and preserve all child anchors.
- [ ] Required-anchor outcomes match.
- [ ] Forbidden-anchor outcomes match.
- [ ] No entry point reports a normal no-answer for an infrastructure/model/evidence failure.

## Repeatability

- [ ] Warm Search repeat preserves logical child/parent result.
- [ ] Warm RetrieveContext repeat preserves logical child/parent result.
- [ ] Warm repeat preserves required anchors.
- [ ] Warm repeat preserves parent dedup outcome.
- [ ] Runtime restart without reingestion succeeds.
- [ ] Post-restart Search preserves logical child/parent result.
- [ ] Post-restart RetrieveContext preserves logical child/parent result.
- [ ] Physical identity drift is absent or explicitly blocks the phase.

## Static and contract gates

- [ ] `cargo fmt --all --check` PASS.
- [ ] `cargo check --locked --all-targets --all-features` PASS.
- [ ] `cargo clippy --locked --all-targets --all-features -- -D warnings` PASS.
- [ ] `cargo test --locked --all-targets --all-features` PASS.
- [ ] `cargo sqlx prepare --check -- --all-targets --all-features` PASS.
- [ ] `fix486_hierarchical_bank_contracts` PASS.
- [ ] `fix486c_frozen_bank_contracts` PASS.
- [ ] Phase D focused contracts PASS.
- [ ] Controlled missing-artifact test blocks evidence completeness.
- [ ] Controlled identity mismatch test blocks verdict aggregation.
- [ ] Controlled mandatory-query skip blocks verdict aggregation.

## Evidence completeness

- [ ] External evidence run root exists.
- [ ] `stage-results.json` exists and parses.
- [ ] `query-results.jsonl` exists and has six primary rows.
- [ ] Identity map exists and is complete.
- [ ] Canonical audit exists.
- [ ] Qdrant audit exists.
- [ ] Six primary request/response artifact sets exist.
- [ ] Search/RetrieveContext comparisons exist.
- [ ] Warm-repeat comparisons exist.
- [ ] Restart comparison exists.
- [ ] Defect register exists.
- [ ] Manifest enumerates every mandatory artifact.
- [ ] Every manifest SHA-256 matches.
- [ ] No artifact path escapes the run root.
- [ ] Evidence aggregate hash is recorded.
- [ ] Cleanup evidence is recorded.

## Defects and verdict

- [ ] No unresolved in-scope P0 exists.
- [ ] No unresolved in-scope P1 exists.
- [ ] No mandatory query is `FAIL`, `BLOCKED` or `SKIPPED`.
- [ ] No mandatory stage is `FAIL`, `BLOCKED` or `SKIPPED`.
- [ ] Frozen bank aggregate remains unchanged after execution.
- [ ] Final verdict is exactly `FIX486_CHILD_PARENT_RUNTIME_PROOF_PASS`.

If any mandatory item is not satisfied, the only allowed verdict is:

```text
FIX486_CHILD_PARENT_RUNTIME_PROOF_BLOCKED
```
