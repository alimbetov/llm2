# FIX486F Execution and Evidence Contract

## 1. Purpose

This contract defines how the future Phase F runner, fault campaign and evidence bundle must be implemented and executed.

This PR is documentation-only. The files named below are future implementation targets and must not be added before document review is complete.

Document review findings and their resolution are recorded in
`DOCUMENT_REVIEW.md`. Runtime implementation remains prohibited while its verdict
is `CHANGES_REQUIRED`.

## 2. Future implementation surface

Expected files:

```text
scripts/fix486f-stale-orphan-hydration-proof.sh
scripts/fix486f_proof.py
scripts/fix486f-audit.sql
docker-compose.fix486f.yml
config/application-fix486f.yaml
tests/fix486f_failure_semantics_contracts.rs
```

Canonical Make target:

```text
verify-fix486f-stale-orphan-hydration-runtime
```

Compatibility alias:

```text
verify-fix486f-stale-orphan-hydration-runtime-proof
```

Both targets must resolve to the same execute path.

## 3. Runner modes

The runner should support at minimum:

```text
--verify-identities
--verify-contracts
--execute-all
--cleanup-only
```

Optional focused modes may include:

```text
--execute-stale
--execute-orphan
--execute-hydration
--execute-concurrency
--verify-evidence <run-dir>
```

Official PASS is valid only for `--execute-all` from a clean source SHA.

## 4. Run identity

Environment variable:

```text
FIX486F_RUN_ID
```

Default generated form:

```text
fix486f-<UTC timestamp>
```

Evidence root:

```text
/Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix486f/<run-id>
```

The implementation must derive portable roots and must not hardcode the user's absolute path.

## 5. Bootstrap evidence

Before any potentially failing preflight action, create:

```text
bootstrap.json
stage-results.json
terminal-result.json or initialized placeholder
runner.stdout.log
runner.stderr.log
```

`bootstrap.json` records:

```text
run_id
branch
source_sha
worktree_clean
start_time_utc
host
os
architecture
command
mode
frozen_bank_identity
```

## 6. Terminal fail-closed behavior

The shell runner must install handlers for:

```text
EXIT
INT
TERM
HUP
```

Terminal evidence records:

```text
status
exit_code
signal
last_completed_stage
failing_stage
failure_code
cleanup_status
end_time_utc
```

Rules:

1. original exit code is captured before evidence finalization;
2. cleanup cannot overwrite the primary failure;
3. a cleanup failure is recorded separately;
4. missing terminal evidence prevents PASS;
5. a signal produces BLOCKED, not PASS.

## 7. Source and frozen identity

Official execution requires:

```text
branch = codex/fix486f-stale-orphan-hydration-proof
worktree = clean
source SHA = exact tested commit
```

Frozen bank:

```text
version = 1.0.0
status = FROZEN
aggregate SHA-256 = cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff
```

Any frozen payload mutation blocks the run.

## 8. Mandatory static gates

Future official runner must execute or reference exact successful results for:

```bash
python3 -m py_compile scripts/fix486f_proof.py
bash -n scripts/fix486f-stale-orphan-hydration-proof.sh
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

Focused contract suites must include Phase A, C, D, E and F contracts.

## 9. Phase-owned environment

Phase F must own unique:

```text
Docker Compose project
PostgreSQL database or schema
Qdrant collection
network
volumes
gRPC port
metrics port
```

Port ownership is verified before startup and after cleanup.

No foreign AstraVector runtime may contribute evidence.

Record:

```text
container image digests
runtime binary SHA-256
configuration SHA-256
model SHA-256
tokenizer SHA-256
health response
metrics endpoint ownership
```

## 10. Capability audit stage

Before implementing failpoints, create:

```text
capability-audit.md
```

It must identify current production behavior for:

- response schema;
- status enum;
- warning/degradation fields;
- hydration repository/service boundary;
- retry policy;
- request and hydration deadlines;
- concurrency model;
- negative caches;
- circuit breaker;
- metrics names;
- blank-content invariant;
- production deletion lifecycle;
- Qdrant payload identity.

Any new API field must be justified as backward compatible or versioned.

## 11. Official stage order

Required stage sequence:

```text
runner-bootstrap
source-identity
frozen-bank-verification
static-gates
port-ownership-preflight
phase-environment-start
migrations
release-runtime-start
health-and-metrics
production-ingestion
projection-completion
canonical-audit-baseline
qdrant-audit-baseline
healthy-query-baselines
fault-point-origin-capture
stale-deleted-parent-setup
stale-deleted-parent-proof
stale-ranking-control
stale-cleanup
orphan-missing-parent-setup
orphan-missing-parent-proof
orphan-ranking-control
orphan-cleanup
hydration-healthy-baseline
partial-timeout-setup
partial-timeout-proof
total-timeout-setup
total-timeout-proof
recovery-without-restart
concurrency-isolation
empty-parent-capability-gate
warm-repeat
runtime-restart
restart-repeat
observability-audit
evidence-integrity
fault-cleanup
phase-cleanup
terminal-result
```

Every mandatory stage has explicit `PASS`, `FAIL`, `BLOCKED` or `SKIPPED` status. `SKIPPED` requires a documented allowed reason.

## 12. Fault activation evidence

Required files:

```text
fault-plan.json
fault-activation.json
fault-state-before.json
fault-state-active.json
fault-state-after.json
fault-cleanup.json
```

Each activation records:

```text
run_id
request_id
entry_point
mode
target parents
max activations
deadline
delay
activation time
deactivation time
actual activation count
```

The activation artifact must also record the non-production capability flag,
control-channel identity, and caller `correlation_id` used for request matching.
No public production API field may activate a failpoint.

PASS requires zero active failpoints after cleanup.

## 13. Qdrant fault provenance

Required artifact:

```text
fault-point-origin.json
```

It must prove injected points derive from production projection.

Required hash material:

```text
point_id
vector_hash
payload_hash
content_hash
zone
document
version
child identity
parent identity
```

Arbitrary fabricated vectors are forbidden.

## 14. Canonical audit

Required snapshots:

```text
canonical-audit-before.json
canonical-audit-faulted.json
canonical-audit-after.json
```

Audit covers:

- document/version states;
- parent visibility;
- child bindings;
- orphan canonical children;
- cross-zone/cross-document/cross-version anomalies;
- duplicate chunks;
- duplicate bindings;
- outbox states;
- dead letters;
- deletion generation consistency.

The audit uses existing canonical `content_hash` and must not introduce an implicit `pgcrypto` dependency.

## 15. Qdrant audit

Required snapshots:

```text
qdrant-audit-before.json
qdrant-audit-faulted.json
qdrant-audit-after.json
```

Audit covers:

- collection identity;
- point count;
- point hashes;
- zone/document/version payloads;
- injected point identity;
- unexpected points;
- cleanup status;
- agreement with canonical bindings.

## 16. Request/response/result artifacts

Each execution row requires:

```text
request.json
response.json
trace.json
result.json
```

Normalized aggregate:

```text
query-results.jsonl
```

Each row contains:

```text
scenario
entry_point
request zone
status class
semantic/infrastructure classification
contexts
surviving parents
dropped parents
reasons
retryable
warnings
coverage class
anchors
forbidden leakage
verdict
```

## 17. Runtime matrix completeness

Tier 1 requires `12/12` rows:

- two clean stale-query baseline rows;
- two healthy hydration baseline rows;
- two stale deleted-parent rows;
- two orphan missing-parent rows;
- two partial timeout rows;
- two total timeout rows.

Tier 2 requires:

- four ranking-control rows;
- two recovery-without-restart rows;
- four concurrent healthy/faulted rows;
- two empty-parent rows or a proven invariant artifact.

Missing required rows block PASS.

Ranking rows are valid only when the clean control has at least one valid survivor
and the injected candidate is observed inside the raw candidate window. Orphan
`HYDRATION_MISSING` rows must use the post-binding hydration failpoint; a Qdrant
parent-ID tamper is a separate `BINDING_INVALID` diagnostic.

## 18. Evidence directory structure

Minimum top-level files:

```text
bootstrap.json
terminal-result.json
stage-results.json
manifest.json
checksums.sha256
environment.json
runtime-capabilities.json
binary-config-model-tokenizer-hashes.json
health.json
metrics-contract-map.json
reason-contract-map.json
hard-gates.json
final-result.json
```

Fault evidence:

```text
fault-plan.json
fault-activation.json
fault-point-origin.json
fault-state-before.json
fault-state-active.json
fault-state-after.json
fault-cleanup.json
```

Audit evidence:

```text
canonical-audit-before.json
canonical-audit-faulted.json
canonical-audit-after.json
qdrant-audit-before.json
qdrant-audit-faulted.json
qdrant-audit-after.json
```

Stale/orphan evidence:

```text
stale-candidate-trace.json
deleted-parent-rejection.json
orphan-candidate-trace.json
missing-parent-rejection.json
candidate-selection-strategy.json
ranking-non-interference.json
```

Hydration evidence:

```text
hydration-baseline.json
hydration-partial-timeout.json
hydration-total-timeout.json
hydration-deadline-audit.json
surviving-context-proof.json
semantic-integrity.json
semantic-similarity-diagnostic.json
```

Observability evidence:

```text
metrics-before.txt
metrics-after-stale.txt
metrics-after-orphan.txt
metrics-after-partial-timeout.txt
metrics-after-total-timeout.txt
metrics-after-recovery.txt
metrics-delta.json
structured-log-audit.json
diagnostic-propagation-audit.json
evidence-leak-scan.json
```

Recovery/concurrency:

```text
recovery-without-restart.json
restart-recovery.json
concurrency-isolation.json
deadline-boundedness.json
```

Comparisons:

```text
query-results.jsonl
search-retrieve-parity.json
baseline-vs-fault-comparison.json
warm-repeat.json
restart-repeat.json
```

## 19. Manifest

`manifest.json` records:

```text
schema_version
phase
run_id
source_sha
branch
start/end timestamps
frozen bank identity
runtime binary/config/model/tokenizer hashes
container digests
evidence file count
checksums file hash
manifest internal aggregate
final verdict
```

Manifest generation is fail-closed.

The manifest must not include itself in a recursively unstable checksum unless the scheme explicitly defines a stable two-level digest.

## 20. Checksums

`checksums.sha256` must verify all mandatory evidence files except explicitly documented self-referential manifest exclusions.

Required final verification:

```text
missing files = 0
checksum mismatches = 0
unexpected mutable artifacts = 0
```

## 21. Metrics evidence

Required snapshots are taken before and after each fault class.

`metrics-delta.json` maps actual names to semantic metrics and records exact expected deltas.

High-cardinality label scan is mandatory.

## 22. Warm repeat

Warm repeat runs without re-ingestion.

It proves:

- stable fault semantics;
- stable surviving/dropped parent sets;
- no canonical/Qdrant growth;
- deterministic metric deltas;
- no sticky state after recovery.

## 23. Restart repeat

Restart only AstraVector runtime.

Post-restart proof requires:

- Health and metrics pass;
- failpoint disabled by default;
- healthy baseline restored;
- explicit reactivation reproduces partial/total semantics;
- stale/orphan rejection remains correct;
- no duplicate state;
- cleanup remains complete.

## 24. Cleanup

Cleanup order:

1. disable failpoints;
2. remove injected Qdrant points;
3. verify no injected points remain;
4. stop runtime;
5. remove phase containers/network;
6. remove phase database/schema and collection according to policy;
7. remove phase volumes when required;
8. verify ports released;
9. preserve models, source and evidence.

Hard gate:

```text
cleanup_leaks = 0
```

## 25. PASS conditions

The runner may emit PASS only when:

```text
all mandatory stages PASS
all required rows present
all stale/orphan contexts zero
ranking non-interference PASS
partial degradation truthful
partial surviving evidence preserved
total timeout returns no content
false semantic no-answer zero
recovery without restart PASS
concurrency isolation PASS
Search/RetrieveContext parity PASS
observability consistency PASS
warm repeat PASS
restart repeat PASS
manifest and checksums PASS
cleanup leaks zero
terminal exit code zero
unresolved Phase F P0/P1 zero
```

Otherwise verdict is BLOCKED.

## 26. Publication boundary

Only after official PASS may the implementation branch be pushed as a completed proof and the PR body updated with evidence identity.

This documentation PR must remain draft until document review is complete and must not be merged as a claim of runtime PASS.
