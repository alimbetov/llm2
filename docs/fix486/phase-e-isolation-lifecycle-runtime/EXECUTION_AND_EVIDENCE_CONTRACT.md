# FIX486E Execution and Evidence Contract

## 1. Official execution principle

An official Phase E run is a foreground, fail-closed, source-bound execution against phase-owned infrastructure.

The runner must never return PASS when:

```text
a mandatory stage is absent
an artifact is missing
a result cannot be normalized
a foreign-zone identity is ambiguous
a lifecycle state is unknown
the terminal process exits non-zero
the evidence manifest is incomplete
```

## 2. Proposed implementation surface

Expected phase-owned implementation files:

```text
scripts/fix486e-isolation-lifecycle-runtime-proof.sh
scripts/fix486e_proof.py
scripts/fix486e-isolation-lifecycle-audit.sql
docker-compose.fix486e.yml
config/application-fix486e.yaml
tests/fix486e_isolation_lifecycle_contracts.rs
Makefile
```

Names may differ only when an existing project convention requires it. The final Make target must be explicit and portable.

Canonical target:

```text
verify-fix486e-isolation-lifecycle-runtime
```

Compatibility alias may be added:

```text
verify-fix486e-isolation-lifecycle-runtime-proof
```

Both must execute the same official mode.

## 3. Runner modes

The runner should support:

```text
--verify-identities
--verify-bank
--dry-run
--execute-all
--cleanup-only
```

Only `--execute-all` can produce an official PASS.

A dry run must be clearly marked non-official.

## 4. Clean-source gate

Before official infrastructure startup:

```text
branch identity verified
source SHA recorded
worktree clean
no untracked files affecting execution
frozen bank hashes verified
```

A dirty tree produces:

```text
failure_code = DIRTY_WORKTREE
verdict = FIX486_ISOLATION_LIFECYCLE_RUNTIME_PROOF_BLOCKED
```

Bootstrap evidence must still be created.

## 5. Early bootstrap evidence

Before the first potentially failing command, create:

```text
bootstrap.json
stage-results.json
terminal-result.json or initialized placeholder
runner.stdout.log
runner.stderr.log
```

`bootstrap.json` contains:

```text
run_id
phase
mode
source_sha
branch
start_time_utc
hostname
operating system
shell version
Make implementation
Docker version
Compose version
```

The runner installs traps for:

```text
EXIT
ERR
INT
TERM
HUP
```

The original exit code or signal must survive evidence finalization and cleanup.

## 6. Run identity

Support explicit override:

```text
FIX486E_RUN_ID
```

Default format:

```text
fix486e-<mode>-<short-sha>-<UTC timestamp>
```

External evidence root:

```text
/Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix486e/<run-id>
```

The implementation must not hardcode the user's absolute path. Resolve repository parent or accept an environment override.

## 7. Phase-owned infrastructure

Use distinct identities for:

```text
Docker Compose project
PostgreSQL database or schema
Qdrant collection
network
volumes
runtime process
ports
temporary configuration
```

Recommended prefix:

```text
fix486e_<run-id-safe-suffix>
```

Do not reuse Phase D databases, collections, or volumes.

## 8. Port-ownership preflight

Before startup, verify ownership or availability for:

```text
gRPC port
metrics port
PostgreSQL port if host-exposed
Qdrant HTTP port
Qdrant gRPC port if used
```

A listening process not owned by the current Phase E run is a hard blocker:

```text
failure_code = FOREIGN_RUNTIME_PORT_OWNER
```

Evidence includes PID, command line, and port.

## 9. Binary and configuration identity

Record SHA-256 for:

```text
release AstraVector binary
application-fix486e.yaml
resolved runtime configuration
model ONNX files
tokenizer files
frozen bank manifest
all five frozen payload files
```

Also record:

```text
container image digests
Rust compiler version
Cargo.lock SHA-256
migration set SHA-256
```

## 10. Official stage order

Recommended stage order:

```text
runner-bootstrap
clean-worktree
source-identity
frozen-bank-verification
static-gates
port-ownership-preflight
phase-environment-start
migrations
runtime-build-or-select
runtime-start
health-check
metrics-check
zone-setup
fixture-ingestion
lifecycle-setup
activation
projection-completion
canonical-audit
qdrant-audit
identity-map
mandatory-search
mandatory-retrieve-context
opposite-zone-controls
lifecycle-anchor-probes
search-retrieve-parity
warm-repeat
runtime-restart
post-restart-health
post-restart-proof
legal-hold-audit
evidence-integrity
cleanup
terminal-result
```

Each stage must have:

```text
name
status
start_time
end_time
duration_ms
exit_code
failure_code
artifact references
```

## 11. Static gates

At minimum:

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

Also execute focused contracts:

```text
fix486_hierarchical_bank_contracts
fix486c_frozen_bank_contracts
fix486d_child_parent_contracts
fix486e_isolation_lifecycle_contracts
```

Python and shell artifacts require:

```bash
python3 -m py_compile scripts/fix486e_proof.py
bash -n scripts/fix486e-isolation-lifecycle-runtime-proof.sh
```

## 12. Fixture execution policy

The runner consumes the frozen fixture without mutation.

Production ingestion must create Zone A and Zone B through supported APIs.

Required logical setup:

```text
zone-a runtime code 4862
zone-b runtime code 4863
```

The actual mapping must be captured from setup responses or canonical rows.

Lifecycle versions must be created so that canonical state reflects:

```text
v1 ACTIVE
v2 INDEXING
v3 DELETED
v4 EXPIRED relative to the recorded test clock
```

Legal hold must be present for the intended active v1 state.

## 13. Lifecycle preparation rules

Prefer supported production APIs for registration, ingestion, activation, deletion, expiry, and hold state.

When the product lacks an API for a required deterministic state, a phase-owned setup adapter may use direct SQL only when:

```text
the mutation is documented
the exact rows are recorded before and after
the mutation does not bypass the retrieval predicate under test
the mutation is scoped to the phase database
the official evidence marks it as controlled setup
```

Do not inject a stale Qdrant child for v3 in Phase E.

## 14. Projection completion

Wait for canonical vector bindings and outbox effects to reach terminal success.

Record:

```text
expected binding count
synced binding count
pending binding count
failed binding count
completed outbox count
failed outbox count
dead-letter count
Qdrant point count
```

Do not begin query proof while required active projections are incomplete.

## 15. Canonical audit artifact

`canonical-audit.json` must include:

```text
zone mapping
document mapping
all runtime version rows
state and visibility fields
expires_at and test clock
legal hold fields
chunk counts by zone/version/role
binding counts by zone/version/state
outbox counts
orphan counts
duplicate counts
cross-zone binding anomalies
```

The audit must use canonical `content_hash` and existing schema semantics. Do not introduce an implicit `pgcrypto` dependency.

## 16. Qdrant audit artifact

`qdrant-audit.json` must include:

```text
collection identity
collection configuration
point count
point count by zone
point count by version when available
payload zone fields
payload document/version fields
foreign-zone collision checks
unexpected inactive/deleted/expired points
```

Presence of a non-searchable point is not automatically a failure if production design permits it, but final retrieval must reject it and the path must be classified.

## 17. Runtime identity map

`logical-runtime-map.json` must map:

```text
logical zone -> runtime zone
logical document -> runtime document per zone
logical version -> runtime version per zone
logical parent/child -> runtime chunks per zone/version
runtime bindings -> Qdrant point IDs
```

The map must prove same logical labels across zones resolve to distinct physical identities.

## 18. Request artifacts

For every request preserve:

```text
request.json
response.raw.json or protobuf-normalized JSON
result.json
telemetry.json
```

Normalize protobuf `int64` values consistently as strings or lossless integers. Do not compare JavaScript-rounded values.

Distinguish:

```text
model token counts
source-backed offsets
```

No-answer classification must be multilingual-safe and must not treat transport failure as a valid no-answer.

## 19. Mandatory primary artifacts

Required directories:

```text
queries/q-zone-a/search/
queries/q-zone-a/retrieve-context/
queries/q-zone-b/search/
queries/q-zone-b/retrieve-context/
queries/q-active-version/search/
queries/q-active-version/retrieve-context/
```

Each contains request, response, normalized result, and telemetry.

Aggregate:

```text
query-results.jsonl
```

must contain exactly six primary rows for the initial pass.

## 20. Opposite-zone control artifacts

Required directories:

```text
controls/q-zone-a-in-zone-b/search/
controls/q-zone-a-in-zone-b/retrieve-context/
controls/q-zone-b-in-zone-a/search/
controls/q-zone-b-in-zone-a/retrieve-context/
```

Aggregate:

```text
opposite-zone-results.jsonl
```

must contain four rows.

## 21. Lifecycle probe artifacts

At minimum preserve one normalized result for each trap anchor:

```text
ASTRA_INACTIVE_VERSION_TRAP
ASTRA_DELETED_PARENT_TRAP
ASTRA_EXPIRED_PARENT_TRAP
```

Record whether the version was:

```text
not projected
filtered at candidate retrieval
rejected at hydration
rejected at final visibility
```

## 22. Search/RetrieveContext comparison

Create:

```text
comparisons/search-retrieve-parity.json
```

It compares all three mandatory queries by logical zone, document, version, parent, anchors, warnings, and hard-gate counters.

Any zone or version semantic mismatch blocks the run.

## 23. Warm-repeat evidence

Create:

```text
warm/query-results.jsonl
comparisons/warm-repeat.json
warm/canonical-counts.json
warm/qdrant-counts.json
```

Warm repeat executes the same six primary requests without ingestion.

## 24. Restart evidence

Create:

```text
restart/pre-stop-state.json
restart/stop-result.json
restart/start-result.json
restart/health.json
restart/query-results.jsonl
restart/opposite-zone-results.jsonl
comparisons/restart-repeat.json
```

PostgreSQL and Qdrant state must remain intact.

## 25. Health and metrics artifacts

Required:

```text
health/pre-query.json
health/post-restart.json
metrics/pre-query.prom
metrics/post-query.prom
metrics/post-restart.prom
```

Health and metrics endpoints must belong to the phase-owned runtime.

## 26. Log handling

Preserve:

```text
logs/runner.stdout.log
logs/runner.stderr.log
logs/runtime.log
logs/postgresql.log
logs/qdrant.log
logs/docker-compose.log
```

Scan logs for foreign-zone anchor leakage. If full foreign text appears in the wrong-zone request trace, the proof is blocked even when the final result is clean.

## 27. Manifest

`manifest.json` includes:

```text
run_id
phase
mode
source_sha
branch
start/end timestamps
verdict
frozen bank identity
binary/config/model/tokenizer hashes
container image digests
runtime zone mapping
artifact count
checksums file hash
manifest format version
```

The manifest must not include its own digest recursively. Publish a separate manifest SHA-256.

## 28. Checksums

`checksums.sha256` covers every evidence file except itself and temporary lock files.

The evidence-integrity stage verifies:

```text
all manifest-listed files present
all digests match
no unexpected mutable files
mandatory directories complete
primary rows = 6
opposite-zone rows = 4
```

## 29. Terminal result

`terminal-result.json` records:

```text
status
exit_code
signal
primary_failure_stage
primary_failure_code
cleanup_status
end_time_utc
```

Successful terminal result:

```json
{
  "status": "PASS",
  "exit_code": 0,
  "signal": null
}
```

## 30. Cleanup semantics

Cleanup always runs after evidence finalization begins.

Order:

```text
capture primary exit state
stop phase runtime
collect final logs
remove phase containers/network/volumes
verify ports released
verify no cleanup leaks
write cleanup result
write terminal result
return original exit code
```

A cleanup error is secondary unless no earlier error exists.

## 31. PASS requirements

An official PASS requires:

```text
source clean and bound to one SHA
frozen bank unchanged
all static gates pass
phase infrastructure isolated
health and metrics pass
canonical and Qdrant audits pass
six primary rows pass
four opposite-zone controls pass
lifecycle probes pass
Search/RetrieveContext parity passes
warm repeat passes
restart proof passes
legal-hold audit passes
all isolation/lifecycle hard gates are zero
evidence integrity passes
cleanup leaks are zero
terminal exit code is zero
```

## 32. Publication rule

Only after official PASS:

```text
push branch
update draft PR
add evidence comment
commit compact result summary if required
```

Do not merge Phase E in the same task that first creates evidence. Merge requires a separate head-SHA and evidence review.