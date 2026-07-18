# FIX486E Acceptance Criteria

## Lineage and source integrity

- [ ] Branch is `codex/fix486e-isolation-lifecycle-runtime-proof`.
- [ ] Base resolves to `377852cc6d7ff315b8d7eb27762672d794fd7a9c` or an approved descendant of merged Phase D.
- [ ] Official evidence is created from a clean worktree.
- [ ] Source SHA is recorded in bootstrap, manifest, and result summary.
- [ ] Runtime binary SHA-256 matches the tested source build.

## Frozen bank

- [ ] Bank ID is `fix486-hierarchical-bank`.
- [ ] Bank version is `1.0.0`.
- [ ] Bank status is `FROZEN`.
- [ ] Aggregate SHA-256 is `cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff`.
- [ ] All five payload hashes verify.
- [ ] Frozen corpus, queries, qrels, graph, and lifecycle files are unchanged.
- [ ] No qrel is derived from runtime output.

## Implementation contracts

- [ ] Canonical Make target exists: `verify-fix486e-isolation-lifecycle-runtime`.
- [ ] Compatibility alias resolves to the same execute runner if provided.
- [ ] Runner supports clean identity verification.
- [ ] Bootstrap evidence is created before preflight.
- [ ] EXIT and signal traps preserve terminal evidence.
- [ ] Original exit code survives cleanup.
- [ ] Missing mandatory artifact prevents PASS.
- [ ] Runner does not hardcode user-specific absolute paths.

## Static gates

- [ ] `python3 -m py_compile scripts/fix486e_proof.py` passes.
- [ ] `bash -n scripts/fix486e-isolation-lifecycle-runtime-proof.sh` passes.
- [ ] `cargo fmt --all --check` passes.
- [ ] `cargo check --locked --all-targets --all-features` passes.
- [ ] `cargo clippy --locked --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo test --locked --all-targets --all-features` passes.
- [ ] Phase A contracts pass.
- [ ] Phase C contracts pass.
- [ ] Phase D contracts pass.
- [ ] Phase E focused contracts pass.

## Phase-owned environment

- [ ] Docker Compose project identity is unique.
- [ ] PostgreSQL database or schema identity is unique.
- [ ] Qdrant collection identity is unique.
- [ ] Network and volume identities are unique.
- [ ] gRPC and metrics ports have verified ownership.
- [ ] No foreign AstraVector runtime contributes evidence.
- [ ] Container image digests are recorded.
- [ ] Health endpoint belongs to the phase runtime.
- [ ] Metrics endpoint belongs to the phase runtime.

## Runtime zone identity

- [ ] `zone-a` maps to runtime zone code `4862`.
- [ ] `zone-b` maps to runtime zone code `4863`.
- [ ] Zone mappings are captured from setup or canonical state.
- [ ] No runtime zone maps to both logical zones.
- [ ] Same logical document IDs across zones have distinct physical IDs.
- [ ] Same logical parent IDs across zones have distinct physical IDs.
- [ ] Same logical child IDs across zones have distinct physical IDs.
- [ ] Same logical labels cannot collide in bindings or Qdrant payload identities.

## Lifecycle setup

- [ ] Zone A document v1 is ACTIVE.
- [ ] Zone A document v2 is INDEXING and not searchable.
- [ ] Zone A document v3 is DELETED and not searchable.
- [ ] Zone A document v4 is expired relative to the recorded test clock and not searchable.
- [ ] Legal hold is represented on intended active v1 state.
- [ ] Test clock value and timezone are recorded.
- [ ] Lifecycle preparation uses production APIs where available.
- [ ] Any direct SQL setup is explicit, phase-owned, and audited.
- [ ] No stale Qdrant fault injection is introduced in Phase E.

## Projection readiness

- [ ] Required active bindings reach SYNCED.
- [ ] Required outbox effects reach COMPLETED.
- [ ] Failed outbox effects are zero.
- [ ] Dead letters are zero.
- [ ] Active Qdrant points are present.
- [ ] Projection completion is reached before query proof starts.

## Canonical audit

- [ ] Canonical audit records both zones.
- [ ] Canonical audit records versions 1 through 4 for Zone A.
- [ ] Canonical audit records state, deletion, expiry, and legal-hold fields.
- [ ] Canonical audit records chunk and binding counts per zone/version.
- [ ] Canonical audit uses existing `content_hash`.
- [ ] No implicit `pgcrypto` dependency is introduced.
- [ ] Orphan canonical children are zero for normal Phase E setup.
- [ ] Duplicate chunks are zero.
- [ ] Duplicate bindings are zero.
- [ ] Cross-zone binding anomalies are zero.

## Qdrant audit

- [ ] Collection identity is recorded.
- [ ] Point count is recorded.
- [ ] Point counts are broken down by zone.
- [ ] Payload zone/document/version fields are audited.
- [ ] Foreign-zone payload collisions are zero.
- [ ] Any non-searchable projected point is classified and cannot become a final context.

## FIX486-03 — Zone A positive proof

- [ ] `q-zone-a` executes through Search.
- [ ] `q-zone-a` executes through RetrieveContext.
- [ ] Request zone is Zone A / 4862.
- [ ] Status is FOUND.
- [ ] Result version is 1.
- [ ] Logical parent is `parent-a1`.
- [ ] Parent contains `ASTRA_CANONICAL_STATE_A1`.
- [ ] `ZONE_B_SECRET_PARENT_A1` is absent.
- [ ] `ZONE_B_PRIVATE_SOURCE` is absent.
- [ ] Cross-zone promoted candidates are zero.
- [ ] Cross-zone hydrations are zero.
- [ ] Cross-zone final contexts are zero.
- [ ] Cross-zone graph results are zero.

## FIX486-03 — Zone B positive proof

- [ ] `q-zone-b` executes through Search.
- [ ] `q-zone-b` executes through RetrieveContext.
- [ ] Request zone is Zone B / 4863.
- [ ] Status is FOUND.
- [ ] Result version is 1.
- [ ] Logical parent is `parent-a1` scoped to Zone B.
- [ ] Parent contains `ZONE_B_SECRET_PARENT_A1`.
- [ ] `ASTRA_CANONICAL_STATE_A1` is absent.
- [ ] `ASTRA_LEGAL_HOLD_A2` is absent.
- [ ] Cross-zone promoted candidates are zero.
- [ ] Cross-zone hydrations are zero.
- [ ] Cross-zone final contexts are zero.
- [ ] Cross-zone graph results are zero.

## Opposite-zone controls

- [ ] q-zone-a question is executed in Zone B through Search.
- [ ] q-zone-a question is executed in Zone B through RetrieveContext.
- [ ] Zone A anchors and physical identities are absent from final results.
- [ ] q-zone-b question is executed in Zone A through Search.
- [ ] q-zone-b question is executed in Zone A through RetrieveContext.
- [ ] Zone B anchors and physical identities are absent from final results.
- [ ] Explicit no-answer is distinguished from transport or normalizer failure.
- [ ] Four supplemental result rows are present.

## FIX486-04 — Active version

- [ ] `q-active-version` executes through Search.
- [ ] `q-active-version` executes through RetrieveContext.
- [ ] Request zone is Zone A / 4862.
- [ ] Status is FOUND.
- [ ] Logical document is `doc-hierarchy`.
- [ ] Actual version is 1.
- [ ] Logical parent is `parent-a1`.
- [ ] Version 2 final results are zero.
- [ ] Version 3 final results are zero.
- [ ] Version 4 final results are zero.
- [ ] `ASTRA_INACTIVE_VERSION_TRAP` is absent.
- [ ] `ASTRA_DELETED_PARENT_TRAP` is absent.
- [ ] `ASTRA_EXPIRED_PARENT_TRAP` is absent.
- [ ] `wrong_version_results` is zero.
- [ ] `inactive_version_results` is zero.
- [ ] `deleted_version_results` is zero.
- [ ] `expired_version_results` is zero.

## Lifecycle path classification

- [ ] v2 exclusion path is classified.
- [ ] v3 exclusion path is classified.
- [ ] v4 exclusion path is classified.
- [ ] Each classification is one of NOT_PROJECTED, FILTERED_AT_CANDIDATE_QUERY, REJECTED_AT_CANONICAL_HYDRATION, or REJECTED_AT_FINAL_VISIBILITY.
- [ ] No lifecycle path remains UNKNOWN or NOT_CHECKED.
- [ ] Supplemental trap-anchor probes produce zero final forbidden contexts.

## Legal hold

- [ ] Active v1 legal-hold state is present.
- [ ] Active v1 remains retrievable.
- [ ] Cleanup protection is evidenced.
- [ ] Legal hold does not make v2 searchable.
- [ ] Legal hold does not make v3 searchable.
- [ ] Legal hold does not make v4 searchable.
- [ ] Legal-hold state survives warm repeat.
- [ ] Legal-hold state survives runtime restart.

## Primary result completeness

- [ ] Initial primary result rows are exactly 6/6.
- [ ] `query-results.jsonl` is non-empty.
- [ ] Every row has request zone, actual zone, document, version, parent, anchors, and verdict.
- [ ] Protobuf int64 values are normalized losslessly.
- [ ] Model token counts are not confused with source offsets.
- [ ] Multilingual no-answer classification is explicit.

## Search/RetrieveContext parity

- [ ] q-zone-a parity passes.
- [ ] q-zone-b parity passes.
- [ ] q-active-version parity passes.
- [ ] Zone semantics are identical across entry points.
- [ ] Version semantics are identical across entry points.
- [ ] Forbidden anchor counts are identical and zero.

## Warm repeat

- [ ] All six mandatory requests are repeated without ingestion.
- [ ] Logical results are stable.
- [ ] Forbidden counts remain zero.
- [ ] Document version count does not increase.
- [ ] Chunk count does not increase.
- [ ] Binding count does not increase.
- [ ] Completed outbox count does not increase unexpectedly.
- [ ] Qdrant point count does not increase.

## Restart proof

- [ ] Only AstraVector runtime is restarted.
- [ ] PostgreSQL state is preserved.
- [ ] Qdrant state is preserved.
- [ ] Post-restart Health passes.
- [ ] Post-restart metrics endpoint passes.
- [ ] All six mandatory requests are repeated.
- [ ] All four opposite-zone controls are repeated.
- [ ] Zone isolation remains unchanged.
- [ ] Active-version filtering remains unchanged.
- [ ] Legal-hold state remains unchanged.
- [ ] Duplicate canonical or projection state is zero.

## Evidence completeness

- [ ] Bootstrap evidence exists.
- [ ] Stage results exist.
- [ ] Terminal result exists.
- [ ] Manifest exists.
- [ ] Checksums exist and verify.
- [ ] Environment and binary/config/model/tokenizer identities exist.
- [ ] Canonical audit exists.
- [ ] Qdrant audit exists.
- [ ] Runtime identity map exists.
- [ ] Six primary request/response/result artifact sets exist.
- [ ] Four opposite-zone artifact sets exist.
- [ ] Lifecycle probe artifacts exist.
- [ ] Warm comparison exists.
- [ ] Restart comparison exists.
- [ ] Search/RetrieveContext parity artifact exists.
- [ ] Health and metrics artifacts exist.
- [ ] Evidence file count is recorded.
- [ ] Manifest SHA-256 is recorded separately.

## Evidence leak scan

- [ ] Zone B secret anchors do not appear in Zone A final or hydrated evidence.
- [ ] Zone A canonical/legal-hold anchors do not appear in Zone B final or hydrated evidence.
- [ ] Foreign content is not emitted in unredacted candidate logs.
- [ ] Foreign physical IDs are not selected into wrong-zone results.
- [ ] `cross_zone_evidence_leaks` is zero.

## Cleanup

- [ ] Phase runtime is stopped.
- [ ] Phase containers are removed.
- [ ] Phase network is removed.
- [ ] Phase volumes are removed when policy requires it.
- [ ] Phase database/schema and Qdrant collection are removed.
- [ ] Ports are released.
- [ ] Models are preserved.
- [ ] `target/` is preserved.
- [ ] Frozen bank is preserved.
- [ ] Previous evidence is preserved.
- [ ] Unrelated containers and volumes are preserved.
- [ ] Cleanup leaks are zero.

## Final verdict

All conditions above must be satisfied before returning:

```text
FIX486_ISOLATION_LIFECYCLE_RUNTIME_PROOF_PASS
```

Any missing mandatory condition requires:

```text
FIX486_ISOLATION_LIFECYCLE_RUNTIME_PROOF_BLOCKED
```