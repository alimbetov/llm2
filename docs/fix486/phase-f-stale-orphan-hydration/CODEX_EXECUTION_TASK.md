# Codex Execution Task — FIX486F

## 1. Current phase

```text
FIX486F — stale/orphan and hydration degradation runtime proof
```

Branch:

```text
codex/fix486f-stale-orphan-hydration-proof
```

Allowed final verdicts:

```text
FIX486_STALE_ORPHAN_HYDRATION_RUNTIME_PROOF_PASS
FIX486_STALE_ORPHAN_HYDRATION_RUNTIME_PROOF_BLOCKED
```

## 2. Current PR boundary

The initial PR is documentation-only.

Before document review completion, do not add:

- runtime failpoint implementation;
- Phase F runner;
- Docker Compose/config;
- audit SQL;
- focused Rust contracts;
- production API changes;
- official evidence claims.

The first Codex action after document approval is a production capability audit, not immediate implementation.

## 3. Read the complete specification

Read in this order:

```text
TECHNICAL_SPECIFICATION.md
STALE_ORPHAN_PROOF_CONTRACT.md
HYDRATION_DEGRADATION_CONTRACT.md
SEMANTIC_AND_OBSERVABILITY_CONTRACT.md
EXECUTION_AND_EVIDENCE_CONTRACT.md
ACCEPTANCE_CRITERIA.md
RESULT_TEMPLATE.md
```

Do not silently weaken a hard gate. If the current production API cannot represent a required semantic, document the gap as a defect or a versioned/backward-compatible design decision.

## 4. Step 0 — confirm repository identity

```bash
cd /Users/ruslanalimbetov/Documents/llm2/astravector

git fetch origin
git switch codex/fix486f-stale-orphan-hydration-proof
git pull --ff-only

git branch --show-current
git rev-parse HEAD
git status --porcelain
git log -1 --oneline
```

Require:

```text
branch = codex/fix486f-stale-orphan-hydration-proof
worktree = clean
```

Record current `main` and merge-base SHA.

## 5. Step 1 — frozen-bank verification

Verify:

```text
version = 1.0.0
status = FROZEN
aggregate SHA-256 = cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff
```

Do not edit frozen corpus, queries, qrels, graph or lifecycle payloads.

If the aggregate differs, stop with BLOCKED.

## 6. Step 2 — production capability audit

Create:

```text
docs/fix486/phase-f-stale-orphan-hydration/capability-audit.md
```

Inspect actual production code and document:

1. Search response schema.
2. RetrieveContext response schema.
3. Status enums.
4. Warning/degradation enums and fields.
5. Parent hydration service/repository boundary.
6. Candidate-selection and final top-k order.
7. Candidate refill/surplus behavior.
8. Request deadline.
9. Per-parent hydration deadline.
10. Retry policy.
11. Concurrent hydration model.
12. Single-flight/shared-future behavior.
13. Negative caches.
14. Circuit breakers.
15. Existing metrics and label conventions.
16. Structured tracing fields.
17. Blank parent content invariant.
18. Production document deletion path.
19. Qdrant payload identity and filters.
20. Current stale/missing/deleted parent behavior.

The audit must separate:

```text
ALREADY_SUPPORTED
REQUIRES_RUNNER_ONLY
REQUIRES_TEST_ONLY
REQUIRES_PRODUCTION_CHANGE
UNKNOWN
```

No implementation should start while material items remain `UNKNOWN`.

## 7. Step 3 — design review checkpoint

Before code changes, summarize:

- proposed failpoint boundary;
- response-schema changes, if any;
- backward compatibility;
- metrics additions;
- candidate refill strategy;
- empty-parent strategy;
- concurrency strategy;
- cleanup strategy.

Do not modify frozen bank to fit current production behavior.

## 8. Step 4 — focused contracts first

Future focused suite:

```text
tests/fix486f_failure_semantics_contracts.rs
```

Add contracts before runtime implementation.

Minimum contract groups:

### Runner fail-closed

- bootstrap before preflight;
- terminal result on error/signal;
- original exit code preserved;
- missing artifact blocks PASS;
- active failpoint after cleanup blocks PASS;
- phase-owned collection restriction.

### Stale/orphan

- stale child cannot form final context;
- deleted parent reason explicit;
- missing parent reason explicit;
- missing parent is not semantic no-answer;
- child text cannot replace missing parent;
- stale point cannot displace valid evidence;
- qrels do not create runtime reason codes.

### Hydration

- partial timeout preserves unaffected contexts;
- partial status is `DEGRADED`;
- dropped parent is explicit;
- partial coverage is not falsely full;
- total timeout is not `FOUND`;
- total timeout returns no content;
- infrastructure failure is not semantic no-answer;
- deadline is bounded;
- retries do not multiply deadline;
- empty context object prohibited.

### Observability

- response reason equals trace reason;
- trace reason maps to metric reason;
- no high-cardinality metric labels;
- rejection trace has zone/document/version;
- no foreign-zone leakage.

### Recovery

- failpoint removal restores healthy behavior without restart;
- no sticky negative/degraded cache;
- restart disables failpoint by default.

### Concurrency

- request-scoped failpoint;
- timeout does not leak to healthy request;
- healthy request is not serialized behind fault delay;
- same-parent shared state is not poisoned.

## 9. Step 5 — expected implementation files

Only after document/capability review:

```text
scripts/fix486f-stale-orphan-hydration-proof.sh
scripts/fix486f_proof.py
scripts/fix486f-audit.sql
docker-compose.fix486f.yml
config/application-fix486f.yaml
tests/fix486f_failure_semantics_contracts.rs
```

Make targets:

```text
verify-fix486f-stale-orphan-hydration-runtime
verify-fix486f-stale-orphan-hydration-runtime-proof
```

The alias must invoke the same canonical target.

## 10. Step 6 — failpoint implementation

The failpoint belongs at canonical parent hydration boundary after candidate selection.

Preserve production batch hydration. Do not introduce per-parent SQL calls solely
to make `TIMEOUT_SELECTED_PARENTS` possible. Record whether the selected-parent
fault is a real independently bounded hydration unit or a post-batch orchestration
fault.

Required modes:

```text
NONE
RETURN_NOT_FOUND_SELECTED
TIMEOUT_SELECTED_PARENTS
TIMEOUT_ALL_PARENTS
```

`EMPTY_CONTENT_SELECTED` is added only if capability audit proves blank parent content can exist.

Required matching:

```text
run_id
request_id
entry_point
access_zone_code
logical_parent_ids
physical_parent_ids
max_activations
```

Use caller `correlation_id` (or a documented equivalent available before
hydration) as the activation request identity. Enable failpoints only with an
explicit non-production startup capability and a local phase-owned control
mechanism; never add public unauthenticated activation.

Deterministic delay:

```text
failpoint_delay_ms = hydration_deadline_ms + fixed_margin_ms
```

Forbidden:

- global sleep;
- public production endpoint;
- detached fault process;
- persistent activation;
- unlimited activations;
- whole-service PostgreSQL outage substitution.

## 11. Step 7 — response semantics

Implement only if current API lacks equivalent fields.

### Partial timeout

Require equivalent to:

```text
status = DEGRADED
retryable = true
coverage = PARTIAL
surviving parents explicit
dropped parents explicit
warning = PARENT_HYDRATION_TIMEOUT
```

### Total timeout

Require equivalent to:

```text
status = UNAVAILABLE or DEADLINE_EXCEEDED
contexts = 0
retryable = true
infrastructure failure explicit
```

Never return:

```text
FOUND
SUCCESS_NO_EVIDENCE
semantic no-answer
child text as parent content
```

Prefer backward-compatible optional protobuf fields or a versioned API change. Document generated-code impact if protobuf changes.

## 12. Step 8 — observability

Map or add metrics equivalent to:

```text
parent_hydration_requests_total
parent_hydration_duration_seconds
hydration_timeouts_total
candidate_rejections_total
stale_candidate_rejections_total
degraded_requests_total
```

Labels must be bounded enums only.

Structured trace must include parent identity internally, without exposing internal UUIDs as metric labels.

## 13. Step 9 — runner implementation

The runner must:

1. create bootstrap evidence;
2. verify source/frozen identities;
3. run static gates;
4. verify phase-owned ports;
5. start phase-owned PostgreSQL/Qdrant;
6. run migrations;
7. build/start release runtime;
8. verify Health and metrics ownership;
9. ingest frozen fixtures through production API;
10. wait for projection completion;
11. capture baseline audits;
12. execute healthy baselines;
13. capture production point provenance;
14. run stale deleted-parent scenario;
15. run stale ranking control;
16. clean stale injection;
17. run orphan missing-parent scenario;
18. run orphan ranking control;
19. clean orphan injection;
20. run healthy hydration baseline;
21. run partial timeout;
22. run total timeout;
23. disable failpoint and prove recovery without restart;
24. run concurrency isolation;
25. prove empty-parent invariant or scenario;
26. run warm repeat;
27. restart only AstraVector;
28. repeat critical semantics;
29. audit metrics/logs/diagnostics;
30. build manifest/checksums;
31. clean fault and phase resources;
32. emit terminal result.

## 14. Step 10 — mandatory runtime rows

Tier 1:

```text
12/12
```

- Search/RetrieveContext clean stale-query baseline;
- Search/RetrieveContext healthy hydration baseline;
- Search/RetrieveContext stale deleted-parent;
- Search/RetrieveContext orphan missing-parent;
- Search/RetrieveContext partial timeout;
- Search/RetrieveContext total timeout.

Tier 2:

```text
ranking controls = 4/4
recovery controls = 2/2
concurrency controls = 4/4
empty parent = 2/2 or proven invariant
```

Every row has request, response, trace and normalized result.

## 15. Step 11 — official evidence

Use unique run ID:

```bash
export FIX486F_RUN_ID="fix486f-$(date -u +%Y%m%dT%H%M%SZ)"
```

Execute canonical target in foreground:

```bash
make verify-fix486f-stale-orphan-hydration-runtime
```

Do not use:

```text
nohup
&
disown
background terminal task
```

Previous BLOCKED evidence must remain immutable.

## 16. Step 12 — official static gates

Before official proof on final clean SHA:

```bash
python3 -m py_compile scripts/fix486f_proof.py
bash -n scripts/fix486f-stale-orphan-hydration-proof.sh
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

Also run Phase A/C/D/E/F focused contracts.

If code changes after these gates, rerun all gates.

## 17. Step 13 — hard gates

Require all values zero:

```text
stale_final_contexts
orphan_final_contexts
deleted_parent_contexts
missing_parent_contexts
stale_candidate_promoted
unclassified_stale_drops
valid_contexts_displaced_by_stale
required_intents_lost_by_stale
partial_surviving_contexts_lost
partial_false_no_answer
partial_false_full_coverage
total_false_found
found_with_empty_context
success_no_evidence
false_semantic_no_answer
content_returned_during_total_timeout
deadline_multiplication
cross_request_failpoint_leaks
healthy_request_blocked_by_faulted_request
negative_cache_poisoning
shared_future_poisoning
global_timeout_contamination
post_fault_recovery_failures
sticky_degraded_cache
sticky_negative_cache
faultpoint_residual_state
entry_point_semantic_mismatches
response_trace_reason_mismatches
trace_metric_reason_mismatches
retryable_mismatches
rejection_stage_mismatches
high_cardinality_metric_labels
evidence_leaks
cleanup_leaks
```

The implementation must use the canonical hard-gate names from
`TECHNICAL_SPECIFICATION.md`. Aggregate aliases may be reported additionally but
must not replace the four detailed propagation counters.

## 18. Step 14 — defect handling

For any failure:

1. emit `FIX486_STALE_ORPHAN_HYDRATION_RUNTIME_PROOF_BLOCKED`;
2. preserve evidence directory;
3. record exact failing stage;
4. record exit code/signal;
5. classify defect;
6. do not mutate frozen bank;
7. implement minimal fix;
8. add focused regression;
9. create separate commit;
10. rerun all gates and full proof on new clean SHA.

Suggested defect taxonomy:

```text
FIX486F-STALE-001 stale candidate reached final context
FIX486F-STALE-002 stale candidate displaced valid result
FIX486F-ORPHAN-001 missing parent not explicitly classified
FIX486F-VIS-001 deleted parent passed visibility
FIX486F-HYDR-001 partial timeout lost surviving context
FIX486F-HYDR-002 total timeout returned false success
FIX486F-HYDR-003 dropped-parent diagnostics absent
FIX486F-SEM-001 infrastructure failure became semantic no-answer
FIX486F-SEM-002 partial result claimed full coverage
FIX486F-CONTENT-001 empty parent produced final context
FIX486F-DEADLINE-001 deadline unbounded or multiplied
FIX486F-CONC-001 timeout leaked across requests
FIX486F-RECOVERY-001 degraded state remained after recovery
FIX486F-PARITY-001 entry-point semantic mismatch
FIX486F-OBS-001 response/trace/metric reasons disagree
FIX486F-EXEC-001 runner/evidence false positive
```

## 19. Step 15 — commit discipline

Use focused commits. Do not combine unrelated refactors.

Suggested progression:

```text
docs(fix486f): audit failure semantics capabilities
test(fix486f): define failure semantics contracts
feat(fix486f): add scoped hydration failpoints
fix(fix486f): preserve truthful degraded responses
obs(fix486f): expose hydration rejection telemetry
test(fix486f): add stale orphan runtime runner
```

Actual messages should reflect actual changes.

## 20. Step 16 — publication

Only after full PASS:

```bash
git status --porcelain
git rev-parse HEAD
git push -u origin codex/fix486f-stale-orphan-hydration-proof
```

Update draft PR with:

- tested source SHA;
- evidence run ID;
- frozen bank identity;
- manifest SHA-256;
- row counts;
- stale/orphan hard gates;
- partial/total hydration results;
- ranking non-interference;
- concurrency;
- recovery;
- observability;
- warm/restart proof;
- final verdict.

Do not merge without explicit approval.

## 21. Expected PASS report

Use `RESULT_TEMPLATE.md`.

Minimum summary:

```text
FIX486F official runtime proof completed

Tested source SHA: <sha>
Evidence run: <run-id>
Manifest SHA-256: <sha>
Frozen bank: 1.0.0 / FROZEN
Tier 1 rows: 12/12 PASS
Ranking controls: 4/4 PASS
Recovery controls: 2/2 PASS
Concurrency controls: 4/4 PASS
Stale contexts: 0
Orphan contexts: 0
Partial hydration: PASS
Total hydration: PASS
Search/Retrieve parity: PASS
Observability: PASS
Warm repeat: PASS
Restart repeat: PASS
Cleanup leaks: 0
Verdict: FIX486_STALE_ORPHAN_HYDRATION_RUNTIME_PROOF_PASS
```

## 22. Expected BLOCKED report

```text
FIX486F official runtime proof blocked

Current SHA: <sha>
Evidence run: <run-id>
Last completed stage: <stage>
Failing stage: <stage>
Failure code: <code>
Exit code/signal: <value>
Rows completed: <n>/<expected>
Evidence preserved: true
Branch pushed as PASS: false
Verdict: FIX486_STALE_ORPHAN_HYDRATION_RUNTIME_PROOF_BLOCKED
```
