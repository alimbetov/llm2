# FIX486G Acceptance Criteria

## Lineage and source integrity

- [ ] Branch is `codex/fix486g-graph-parent-proof` or an approved stacked runtime sub-branch.
- [ ] Base contains tested Phase F SHA `c5fa4cb41cf9cd57ddf914562723bbe9758110cd` or its reviewed merged descendant.
- [ ] Phase F prerequisite verdict is recorded.
- [ ] Local SHA equals remote SHA equals tested SHA.
- [ ] Official run starts from a clean worktree.
- [ ] Runtime binary, config, model and tokenizer hashes are recorded.

## Frozen bank

- [ ] Bank version is `1.0.0`.
- [ ] Bank status is `FROZEN`.
- [ ] Aggregate SHA-256 is `cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff`.
- [ ] Frozen corpus is unchanged.
- [ ] Frozen queries are unchanged.
- [ ] Frozen qrels are unchanged.
- [ ] Frozen graph payload is unchanged.
- [ ] Frozen lifecycle payload is unchanged.
- [ ] Runtime output is not used to derive expected identities.

## Documentation gate

- [ ] Technical specification reviewed.
- [ ] Graph parent proof contract reviewed.
- [ ] Execution/evidence contract reviewed.
- [ ] Acceptance criteria reviewed.
- [ ] Codex execution task reviewed.
- [ ] Result template reviewed.
- [ ] Documentation verdict permits capability audit.
- [ ] No production Graph edits precede review approval.

## Capability audit

- [ ] Production Graph path is mapped end to end.
- [ ] Relation storage schema is documented.
- [ ] Relation ingestion path is documented.
- [ ] Seed identity construction is documented.
- [ ] Related endpoint filtering is documented.
- [ ] Related-child candidate construction is documented.
- [ ] Canonical binding validation is documented.
- [ ] Related-parent hydration is documented.
- [ ] Direct/Graph dedup identity is documented.
- [ ] Provenance fields and loss points are documented.
- [ ] Hop-limit handling is documented.
- [ ] Cycle handling is documented.
- [ ] Graph-disabled behavior is documented.
- [ ] Deadline/retry/concurrency/cache behavior is documented.
- [ ] Metrics mapping is documented.
- [ ] Search/RetrieveContext differences are documented.
- [ ] `UNKNOWN_MATERIAL_CAPABILITIES = 0`.

## Contract-first gate

- [ ] Focused contract proves related child uses its own parent.
- [ ] Focused contract rejects seed-parent reuse.
- [ ] Focused contract rejects binding mismatch.
- [ ] Focused contract rejects cross-zone edge traversal.
- [ ] Focused contract rejects inactive/deleted/expired target.
- [ ] Focused contract requires complete provenance.
- [ ] Focused contract proves Graph-disabled control.
- [ ] Focused contract enforces one-hop maximum.
- [ ] Focused contract prevents cycle credit inflation.
- [ ] Focused contract proves candidate non-interference.
- [ ] Focused contract proves Search/RetrieveContext parity.
- [ ] Focused contract proves no N+1 hydration.
- [ ] Any current defect has a red reproducer before repair.

## Static gates

- [ ] Python proof script compiles.
- [ ] Shell runner syntax passes.
- [ ] `cargo fmt --all --check` passes.
- [ ] `cargo check --locked --all-targets --all-features` passes.
- [ ] `cargo clippy --locked --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo test --locked --all-targets --all-features` passes.
- [ ] `cargo sqlx prepare --check -- --all-targets --all-features` passes.
- [ ] Prior FIX486 contract suites pass.
- [ ] Phase G focused contracts pass.

## Phase-owned environment

- [ ] Unique Compose project is used.
- [ ] Unique PostgreSQL database/schema is used.
- [ ] Unique Qdrant collection is used.
- [ ] Unique network and volumes are used.
- [ ] gRPC port ownership is verified.
- [ ] Metrics port ownership is verified.
- [ ] No foreign runtime contributes evidence.
- [ ] Container image digests are recorded.
- [ ] Health endpoint belongs to the Phase G runtime.
- [ ] Metrics endpoint belongs to the Phase G runtime.

## Production baseline

- [ ] Frozen fixtures are ingested through production ingestion.
- [ ] Required active document/version states are reached.
- [ ] Required bindings are `SYNCED`.
- [ ] Required outbox effects are `COMPLETED`.
- [ ] Failed outbox events are zero.
- [ ] Dead letters are zero.
- [ ] Qdrant points match canonical searchable bindings.
- [ ] Frozen Graph relation is ingested through the supported production path.
- [ ] Canonical SQL audit passes.
- [ ] Qdrant audit passes.
- [ ] Graph relation audit passes.

## Frozen identity map

- [ ] Logical `child-a1-180` maps to a runtime seed child or approved tokenizer descendant.
- [ ] Logical `parent-a1` maps to the seed canonical parent.
- [ ] Logical `child-a3-180` maps to a runtime related child or approved tokenizer descendant.
- [ ] Logical `parent-a3` maps to the related canonical parent.
- [ ] Frozen `REPAIRED_BY` relation maps to physical endpoints.
- [ ] Physical mapping is zone/document/version scoped.

## Graph-disabled control

- [ ] Graph expansion is disabled in the control request.
- [ ] No Graph query is credited as executed.
- [ ] No Graph origin appears in final evidence.
- [ ] No relation provenance appears.
- [ ] Direct result remains healthy.
- [ ] `graph_disabled_origin_count = 0`.

## Healthy direct baseline

- [ ] Search direct baseline executes.
- [ ] RetrieveContext direct baseline executes.
- [ ] Direct seed child belongs to A1.
- [ ] Direct canonical parent is `parent-a1`.
- [ ] Direct result has no false Graph provenance.

## Healthy Graph Search

- [ ] Graph expansion executes.
- [ ] Frozen edge is in the raw Graph window.
- [ ] Seed child is A1.
- [ ] Seed parent is `parent-a1`.
- [ ] Relation type is `REPAIRED_BY` or exact frozen-compatible type.
- [ ] Related child is A3.
- [ ] Related parent is `parent-a3`.
- [ ] Related parent differs from seed parent.
- [ ] Hop index is `1`.
- [ ] Final Graph origin is retained.
- [ ] Required A3 reconciliation anchors are present.

## Healthy Graph RetrieveContext

- [ ] Graph expansion executes.
- [ ] Frozen edge is in the raw Graph window.
- [ ] Seed child is A1.
- [ ] Seed parent is `parent-a1`.
- [ ] Relation type is retained.
- [ ] Related child is A3.
- [ ] Related parent is `parent-a3`.
- [ ] Related parent differs from seed parent.
- [ ] Hop index is `1`.
- [ ] Final Graph origin is retained.
- [ ] Required A3 reconciliation anchors are present.

## Entry-point parity

- [ ] Search and RetrieveContext use the same zone/document/version.
- [ ] Seed logical identity is equivalent.
- [ ] Relation identity/type is equivalent.
- [ ] Related child identity is equivalent.
- [ ] Related parent identity is equivalent.
- [ ] Hop index is equivalent.
- [ ] Origin is equivalent.
- [ ] Required anchor coverage is equivalent.
- [ ] Forbidden-anchor leakage is zero for both.

## Wrong-parent fault control

- [ ] Fault candidate originates from captured production identities.
- [ ] Related A3 child is paired with invalid seed parent A1.
- [ ] Invalid candidate is inside the raw Graph window.
- [ ] Canonical binding mismatch is detected.
- [ ] Rejection reason is explicit.
- [ ] Invalid candidate never becomes final context.
- [ ] `graph_wrong_parent = 0` in final healthy result counters.
- [ ] `graph_seed_parent_reuse = 0` in final contexts.

## Candidate non-interference

- [ ] At least one valid survivor exists.
- [ ] At least one invalid higher-ranked Graph candidate exists.
- [ ] Valid logical parent set is unchanged after invalid-candidate injection.
- [ ] Valid content hashes are unchanged.
- [ ] Valid survivor remains in final results.
- [ ] Invalid candidate does not consume final capacity.
- [ ] Reserve/refill, if used, remains bounded.
- [ ] Ranking weights are unchanged.

## Zone isolation

- [ ] Frozen zone-B relation exists in the environment.
- [ ] Zone-A seed cannot traverse the zone-B relation.
- [ ] Zone-B child is absent from final results.
- [ ] Zone-B parent is absent from final results.
- [ ] Zone-B anchors are absent from public output.
- [ ] `graph_cross_zone_results = 0`.

## Lifecycle and binding controls

- [ ] Inactive related version is rejected.
- [ ] Deleted related version is rejected.
- [ ] Expired related version is rejected.
- [ ] Missing related parent is rejected.
- [ ] Binding-invalid related endpoint is rejected.
- [ ] Empty related parent is rejected.
- [ ] No invalid target becomes final context.
- [ ] `inactive_or_deleted_graph_results = 0`.
- [ ] `graph_binding_invalid_contexts = 0`.

## Hop and cycle controls

- [ ] Request uses `graph_max_hops = 1`.
- [ ] First-hop frozen relation is admitted.
- [ ] Second-hop relation is not admitted.
- [ ] Self-cycle does not multiply evidence.
- [ ] A→B→A cycle does not multiply evidence.
- [ ] Duplicate edge does not multiply intent credit.
- [ ] `graph_hop_limit_violations = 0`.
- [ ] `graph_cycle_credit_inflation = 0`.

## Provenance

- [ ] Seed zone/document/version are recorded.
- [ ] Seed child is recorded.
- [ ] Seed parent is recorded.
- [ ] Relation identity is recorded.
- [ ] Relation type is recorded.
- [ ] Relation score is recorded.
- [ ] Related zone/document/version are recorded.
- [ ] Related child is recorded.
- [ ] Related parent is recorded.
- [ ] Hop index is recorded.
- [ ] Graph origin is recorded.
- [ ] `graph_provenance_missing = 0`.

## Observability

- [ ] Existing/new metric mapping is published.
- [ ] Metric labels are bounded enums.
- [ ] Zone/document/chunk IDs are not metric labels.
- [ ] Protected trace retains physical identities.
- [ ] Response, trace and metric outcomes are consistent.
- [ ] Graph-disabled metrics do not claim execution.
- [ ] Rejection reasons are countable without high cardinality.

## Repeatability

- [ ] Healthy run completes.
- [ ] Warm repeat completes without re-ingestion.
- [ ] Runtime restart completes.
- [ ] Post-restart Search completes.
- [ ] Post-restart RetrieveContext completes.
- [ ] Normalized identity chain matches.
- [ ] Content hashes match.
- [ ] Relation type and hop index match.
- [ ] Hard-gate counters match.

## Evidence integrity

- [ ] Bootstrap evidence exists before preflight.
- [ ] Stage results exist.
- [ ] Terminal evidence exists.
- [ ] EXIT/INT/TERM/HUP handling is implemented.
- [ ] Original failure code survives cleanup.
- [ ] Manifest lists every mandatory artifact.
- [ ] Missing mandatory artifacts block PASS.
- [ ] Hash mismatches block PASS.
- [ ] Frozen payload mutation blocks PASS.
- [ ] Active phase infrastructure after cleanup blocks PASS.
- [ ] Evidence verification passes.

## Defects and scope

- [ ] Every reproducible in-scope P0/P1 is registered.
- [ ] Every repaired P0/P1 has before/after evidence.
- [ ] Every repaired P0/P1 has a regression test.
- [ ] Unresolved in-scope P0 count is zero.
- [ ] Unresolved in-scope P1 count is zero.
- [ ] No Graph/RRF/MMR weight tuning is used to force PASS.
- [ ] No MMR/token-budget proof is claimed.
- [ ] No load/capacity proof is claimed.
- [ ] No production-ready claim is made.

## Repository result package

- [ ] `RESULT.md` is published.
- [ ] `MANIFEST_POINTER.json` is published.
- [ ] `STAGE_RESULTS_SUMMARY.json` is published.
- [ ] `DEFECT_REGISTER.json` is published.
- [ ] `NORMALIZED_COMPARISON_SUMMARY.json` is published.
- [ ] Tested SHA and evidence hashes are exact.

## Verdict

Successful form:

```text
FIX486_GRAPH_PARENT_RUNTIME_PROOF_PASS
```

Blocked form:

```text
FIX486_GRAPH_PARENT_RUNTIME_PROOF_BLOCKED
blocking_stage=<stage>
failure_code=<code>
evidence_preserved=true
```
