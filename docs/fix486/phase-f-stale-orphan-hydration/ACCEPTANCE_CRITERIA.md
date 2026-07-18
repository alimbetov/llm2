# FIX486F Acceptance Criteria

## Lineage and source integrity

- [ ] Branch is `codex/fix486f-stale-orphan-hydration-proof`.
- [ ] Base is current `main` containing merged Phase E.
- [ ] Source SHA is recorded in bootstrap, manifest and final result.
- [ ] Official proof runs from a clean worktree.
- [ ] Runtime binary SHA-256 matches the tested build.
- [ ] Configuration, model and tokenizer hashes are recorded.

## Frozen bank

- [ ] Bank version is `1.0.0`.
- [ ] Bank status is `FROZEN`.
- [ ] Aggregate SHA-256 equals `cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff`.
- [ ] Frozen corpus is unchanged.
- [ ] Frozen queries are unchanged.
- [ ] Frozen qrels are unchanged.
- [ ] Frozen graph is unchanged.
- [ ] Frozen lifecycle payload is unchanged.
- [ ] Runtime output is not used to derive qrels.

## Documentation review gate

- [ ] Technical specification reviewed.
- [ ] Stale/orphan contract reviewed.
- [ ] Hydration degradation contract reviewed.
- [ ] Semantic/observability contract reviewed.
- [ ] Execution/evidence contract reviewed.
- [ ] Acceptance criteria reviewed.
- [ ] Codex execution task reviewed.
- [ ] Result template reviewed.
- [ ] No runtime implementation committed before document review completion.

## Capability audit

- [ ] Current response schema documented.
- [ ] Status enum documented.
- [ ] Warning/degradation fields documented.
- [ ] Hydration service/repository boundary documented.
- [ ] Request deadline documented.
- [ ] Hydration deadline documented.
- [ ] Retry policy documented.
- [ ] Concurrency behavior documented.
- [ ] Negative cache behavior documented.
- [ ] Circuit-breaker behavior documented.
- [ ] Metrics naming documented.
- [ ] Blank-content invariant documented.
- [ ] Production deletion path documented.
- [ ] Qdrant payload identity documented.

## Future runner contracts

- [ ] Canonical Make target exists.
- [ ] Compatibility alias resolves to canonical target.
- [ ] `--verify-identities` exists.
- [ ] `--verify-contracts` exists.
- [ ] `--execute-all` exists.
- [ ] Bootstrap evidence is created before preflight.
- [ ] EXIT/INT/TERM/HUP terminal evidence exists.
- [ ] Original exit code survives cleanup.
- [ ] Missing mandatory artifact blocks PASS.
- [ ] Active failpoint after cleanup blocks PASS.
- [ ] Runner does not hardcode user-specific paths.

## Static gates

- [ ] Python proof script compiles.
- [ ] Shell runner syntax passes.
- [ ] `cargo fmt --all --check` passes.
- [ ] `cargo check --locked --all-targets --all-features` passes.
- [ ] `cargo clippy --locked --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo test --locked --all-targets --all-features` passes.
- [ ] Phase A contracts pass.
- [ ] Phase C contracts pass.
- [ ] Phase D contracts pass.
- [ ] Phase E contracts pass.
- [ ] Phase F focused contracts pass.

## Phase-owned environment

- [ ] Unique Docker Compose project.
- [ ] Unique PostgreSQL database/schema.
- [ ] Unique Qdrant collection.
- [ ] Unique network and volumes.
- [ ] gRPC port ownership verified.
- [ ] Metrics port ownership verified.
- [ ] No foreign AstraVector runtime contributes evidence.
- [ ] Container image digests recorded.
- [ ] Health endpoint belongs to Phase F runtime.
- [ ] Metrics endpoint belongs to Phase F runtime.

## Production baseline

- [ ] Frozen fixtures ingested through production path.
- [ ] Required versions reach expected canonical states.
- [ ] Active bindings reach `SYNCED`.
- [ ] Required outbox effects reach `COMPLETED`.
- [ ] Failed outbox events are zero.
- [ ] Dead letters are zero.
- [ ] Baseline Qdrant points are present.
- [ ] Canonical audit baseline passes.
- [ ] Qdrant audit baseline passes.

## Healthy query baselines

- [ ] Clean stale-query baseline executes through Search.
- [ ] Clean stale-query baseline executes through RetrieveContext.
- [ ] Healthy hydration baseline executes through Search.
- [ ] Healthy hydration baseline executes through RetrieveContext.
- [ ] Healthy hydration status is `FOUND`.
- [ ] Healthy hydration contexts are non-empty.
- [ ] Healthy coverage class is `FULL`.
- [ ] Healthy warning list is empty.

## Fault-point provenance

- [ ] Every injected point originated from production projection.
- [ ] Original point ID recorded.
- [ ] Vector hash recorded.
- [ ] Payload hash recorded.
- [ ] Content hash recorded.
- [ ] Zone/document/version recorded.
- [ ] Logical and physical child identity recorded.
- [ ] Logical and physical parent identity recorded.
- [ ] Arbitrary fabricated vector is not used.

## Stale deleted-parent scenario

- [ ] Parent/version transitioned through production lifecycle path.
- [ ] Canonical deleted/invisible state verified.
- [ ] Captured child point reinserted only in phase-owned collection.
- [ ] Raw Qdrant stale candidate is present.
- [ ] Search executes.
- [ ] RetrieveContext executes.
- [ ] Final context count is zero.
- [ ] Status is not `FOUND`.
- [ ] Retryable is false.
- [ ] Reason is `DELETED_PARENT` or `VISIBILITY_REJECTED`.
- [ ] `ASTRA_DELETED_PARENT_TRAP` is absent.
- [ ] Deleted parent content is absent.
- [ ] Candidate is explicitly classified.

## Orphan missing-parent scenario

- [ ] Orphan point is derived from production point.
- [ ] Orphan parent identity is deterministic and phase-owned.
- [ ] No canonical parent exists for injected identity.
- [ ] Search executes.
- [ ] RetrieveContext executes.
- [ ] Final context count is zero.
- [ ] Retryable is true.
- [ ] Reason is `HYDRATION_MISSING`.
- [ ] Result is not ordinary semantic no-answer.
- [ ] Child text is not used as parent substitute.
- [ ] Parent identity is not guessed.

## Ranking non-interference

- [ ] Clean control captured.
- [ ] Faulted control captured.
- [ ] Valid logical parent set unchanged.
- [ ] Valid content hashes unchanged.
- [ ] Required intent coverage unchanged.
- [ ] Valid final context count unchanged.
- [ ] Relative order of valid contexts unchanged.
- [ ] Stale/orphan candidate does not displace valid result.
- [ ] Candidate selection/refill strategy documented.

## Partial hydration timeout

- [ ] Failpoint is request-scoped.
- [ ] Failpoint is parent-scoped.
- [ ] `parent-a1` succeeds.
- [ ] `parent-a3` times out.
- [ ] Search executes.
- [ ] RetrieveContext executes.
- [ ] Status is `DEGRADED`.
- [ ] At least one context survives.
- [ ] Retryable is true.
- [ ] Coverage class is `PARTIAL`.
- [ ] `parent-a1` is listed as surviving.
- [ ] `parent-a3` is listed as dropped.
- [ ] Warning contains `PARENT_HYDRATION_TIMEOUT` or mapped equivalent.
- [ ] Surviving intents are explicit.
- [ ] Dropped intents are explicit.
- [ ] Full coverage is not claimed.
- [ ] Dropped-parent-only anchors are absent.

## Total hydration timeout

- [ ] All required parents are targeted.
- [ ] Search executes.
- [ ] RetrieveContext executes.
- [ ] Status is `UNAVAILABLE`, `DEADLINE_EXCEEDED`, or strictly compatible infrastructure `DEGRADED`.
- [ ] Context count is zero.
- [ ] Retryable is true.
- [ ] Infrastructure failure is explicit.
- [ ] Full hydration failure is explicit.
- [ ] No parent content returned.
- [ ] No child text returned as substitute.
- [ ] No generated/metadata-derived answer returned.
- [ ] No context placeholder returned.
- [ ] No empty context object returned as success.
- [ ] No semantic no-answer classification returned.
- [ ] `FOUND` is not returned.
- [ ] `SUCCESS_NO_EVIDENCE` is not returned.

## Deadline boundedness

- [ ] Request deadline recorded.
- [ ] Hydration deadline recorded.
- [ ] Failpoint delay recorded.
- [ ] Fixed timeout margin recorded.
- [ ] Actual elapsed time recorded.
- [ ] Retry count recorded.
- [ ] Observed latency is within request deadline plus jitter.
- [ ] Per-parent retries do not multiply overall deadline.

## Concurrency isolation

- [ ] Faulted Request A and healthy Request B execute concurrently.
- [ ] Same parent may be used for both requests.
- [ ] Request A receives timeout semantics.
- [ ] Request B receives healthy `FOUND`.
- [ ] Request B is not blocked by A's full fault delay.
- [ ] Failpoint does not leak across requests.
- [ ] Negative cache is not poisoned.
- [ ] Shared future/single-flight state is not poisoned.
- [ ] Global timeout contamination is zero.
- [ ] Search concurrency control passes.
- [ ] RetrieveContext concurrency control passes.

## Recovery without restart

- [ ] Healthy baseline established.
- [ ] Failpoint activated.
- [ ] Fault behavior observed.
- [ ] Failpoint disabled.
- [ ] Runtime not restarted.
- [ ] Identical request returns `FOUND`.
- [ ] Full context coverage restored.
- [ ] Warnings cleared.
- [ ] Dropped parent count returns to zero.
- [ ] Sticky degraded cache is zero.
- [ ] Sticky negative cache is zero.
- [ ] Circuit breaker is not stuck open.
- [ ] Residual failpoint state is zero.

## Empty-parent capability gate

- [ ] Schema/ingestion blank-content invariant audited.
- [ ] If impossible, `empty-parent-invariant.json` proves it.
- [ ] If possible, Search runtime scenario executes.
- [ ] If possible, RetrieveContext runtime scenario executes.
- [ ] Empty content classified as `EMPTY_CONTEXT` or `NO_CONTENT`.
- [ ] Empty content not classified as timeout.
- [ ] Empty parent never becomes final context.
- [ ] Empty context false success is zero.

## Semantic integrity

- [ ] `semantic-integrity.json` exists.
- [ ] Healthy required intents recorded.
- [ ] Healthy parent coverage recorded.
- [ ] Surviving intents recorded.
- [ ] Dropped intents recorded.
- [ ] Surviving parents recorded.
- [ ] Dropped parents recorded.
- [ ] Required anchor coverage recorded.
- [ ] Forbidden anchor leakage recorded.
- [ ] Coverage class recorded.
- [ ] Embedding diagnostic, if present, is non-gating.

## Dropped-parent diagnostics

- [ ] Degradation class exposed.
- [ ] Retryable exposed.
- [ ] Dropped parent count exposed.
- [ ] Dropped parent identities exposed safely.
- [ ] Drop reason exposed.
- [ ] Rejection stage exposed.
- [ ] Public response does not require internal UUIDs.
- [ ] Internal trace retains physical IDs.

## Observability

- [ ] Metrics contract mapping exists.
- [ ] Reason contract mapping exists.
- [ ] Hydration request metric exists.
- [ ] Hydration duration metric exists.
- [ ] Hydration timeout metric exists.
- [ ] Candidate rejection metric exists.
- [ ] Stale rejection metric exists.
- [ ] Degraded request metric exists.
- [ ] Metrics-before snapshot exists.
- [ ] Stale snapshot exists.
- [ ] Orphan snapshot exists.
- [ ] Partial timeout snapshot exists.
- [ ] Total timeout snapshot exists.
- [ ] Recovery snapshot exists.
- [ ] Metric delta audit passes.
- [ ] High-cardinality metric labels are zero.

## Diagnostic propagation

- [ ] Response reason equals trace reason.
- [ ] Trace reason equals metric reason category.
- [ ] Retryable values agree.
- [ ] Rejection stages agree.
- [ ] Request ID links response and trace.
- [ ] UTC timestamp recorded.
- [ ] Zone/document/version recorded.
- [ ] Foreign-zone IDs/content do not leak.

## Search/RetrieveContext parity

- [ ] Stale semantics match.
- [ ] Orphan semantics match.
- [ ] Healthy hydration semantics match.
- [ ] Partial timeout semantics match.
- [ ] Total timeout semantics match.
- [ ] Recovery semantics match.
- [ ] Coverage classes match.
- [ ] Surviving parent sets match.
- [ ] Dropped parent sets match.
- [ ] Reason classes match.
- [ ] Retryable classes match.
- [ ] Forbidden leakage is zero for both.

## Initial result completeness

- [ ] Tier 1 rows are exactly `12/12`.
- [ ] Ranking control rows are `4/4`.
- [ ] Recovery rows are `2/2`.
- [ ] Concurrency rows are `4/4`.
- [ ] Empty-parent rows are `2/2` or proven invariant exists.
- [ ] `query-results.jsonl` is non-empty.
- [ ] Every row has a verdict.

## Warm repeat

- [ ] Critical scenarios repeated without ingestion.
- [ ] Semantic status classes stable.
- [ ] Surviving/dropped parent sets stable.
- [ ] Canonical counts do not grow.
- [ ] Qdrant counts do not grow unexpectedly.
- [ ] Outbox counts do not grow unexpectedly.
- [ ] Metric deltas remain deterministic.
- [ ] Recovery remains healthy.

## Restart repeat

- [ ] Only AstraVector runtime restarted.
- [ ] PostgreSQL state preserved.
- [ ] Qdrant state preserved.
- [ ] Health passes after restart.
- [ ] Metrics endpoint passes after restart.
- [ ] Failpoint disabled by default after restart.
- [ ] Healthy baseline restored.
- [ ] Partial timeout reproducible after explicit activation.
- [ ] Total timeout reproducible after explicit activation.
- [ ] Stale/orphan rejection remains correct.
- [ ] Recovery works after restart.
- [ ] Duplicate state remains zero.

## Evidence integrity

- [ ] Bootstrap exists.
- [ ] Stage results exist.
- [ ] Terminal result exists.
- [ ] Manifest exists.
- [ ] Checksums exist.
- [ ] Environment identity exists.
- [ ] Runtime capability audit exists.
- [ ] Binary/config/model/tokenizer hashes exist.
- [ ] Health artifact exists.
- [ ] Canonical audits exist.
- [ ] Qdrant audits exist.
- [ ] Fault plan and activation artifacts exist.
- [ ] Fault-point origin exists.
- [ ] Stale/orphan traces exist.
- [ ] Hydration traces exist.
- [ ] Ranking comparison exists.
- [ ] Semantic integrity exists.
- [ ] Observability audits exist.
- [ ] Recovery proof exists.
- [ ] Concurrency proof exists.
- [ ] Warm comparison exists.
- [ ] Restart comparison exists.
- [ ] Search/Retrieve parity exists.
- [ ] Hard-gates artifact exists.
- [ ] Final result exists.
- [ ] Manifest file count recorded.
- [ ] Manifest aggregate recorded.
- [ ] Checksum mismatches are zero.

## Cleanup

- [ ] Failpoints disabled.
- [ ] Injected stale point removed.
- [ ] Injected orphan point removed.
- [ ] No injected points remain.
- [ ] Phase runtime stopped.
- [ ] Phase containers removed.
- [ ] Phase network removed.
- [ ] Phase database/schema removed according to policy.
- [ ] Phase Qdrant collection removed according to policy.
- [ ] Phase volumes removed when required.
- [ ] Ports released.
- [ ] Models preserved.
- [ ] Evidence preserved.
- [ ] Cleanup leaks equal zero.

## Final PASS gate

- [ ] Tier 1 `12/12 PASS`.
- [ ] Ranking controls `4/4 PASS`.
- [ ] Recovery controls `2/2 PASS`.
- [ ] Concurrency controls `4/4 PASS`.
- [ ] Empty-parent control passes or invariant proven.
- [ ] Stale final contexts equal zero.
- [ ] Orphan final contexts equal zero.
- [ ] Unclassified drops equal zero.
- [ ] Valid contexts displaced equal zero.
- [ ] Partial status is truthful `DEGRADED`.
- [ ] Partial surviving evidence is preserved.
- [ ] Partial dropped parents are explicit.
- [ ] Partial false full coverage equals zero.
- [ ] Total false `FOUND` equals zero.
- [ ] Total content returned equals zero.
- [ ] False semantic no-answer equals zero.
- [ ] Deadline multiplication equals zero.
- [ ] Cross-request failpoint leaks equal zero.
- [ ] Recovery without restart passes.
- [ ] Search/Retrieve parity passes.
- [ ] Semantic integrity passes.
- [ ] Observability passes.
- [ ] Warm repeat passes.
- [ ] Restart repeat passes.
- [ ] Evidence integrity passes.
- [ ] Cleanup leaks equal zero.
- [ ] Terminal exit code equals zero.
- [ ] Unresolved Phase F P0/P1 defects equal zero.

Final verdict:

```text
FIX486_STALE_ORPHAN_HYDRATION_RUNTIME_PROOF_PASS
```

Any unchecked mandatory item produces:

```text
FIX486_STALE_ORPHAN_HYDRATION_RUNTIME_PROOF_BLOCKED
```