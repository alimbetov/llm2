# FIX486G Execution and Evidence Contract

## 1. Purpose

This contract defines the future Phase G capability audit, contract-test sequence, phase-owned runtime runner and fail-closed evidence bundle.

The initial branch is documentation-only. Runtime Graph changes and proof tooling are prohibited until document review approves the capability audit stage.

## 2. Expected implementation surface

Future files may include:

```text
scripts/fix486g-graph-parent-runtime-proof.sh
scripts/fix486g_proof.py
scripts/fix486g-audit.sql
docker-compose.fix486g.yml
config/application-fix486g.yaml
tests/fix486g_graph_parent_contracts.rs
```

Canonical Make target:

```text
verify-fix486g-graph-parent-runtime
```

Compatibility alias:

```text
verify-fix486g-graph-parent-runtime-proof
```

Both targets must execute the same canonical runner.

## 3. Runner modes

Mandatory modes:

```text
--verify-identities
--verify-contracts
--execute-all
--cleanup-only
--verify-evidence <run-dir>
```

Recommended focused modes:

```text
--execute-healthy
--execute-wrong-parent
--execute-zone-isolation
--execute-lifecycle
--execute-hop-controls
--execute-parity
--execute-repeatability
```

Official PASS is valid only for `--execute-all` from a clean tested SHA.

## 4. Run identity

Environment variable:

```text
FIX486G_RUN_ID
```

Default:

```text
fix486g-<UTC timestamp>
```

Portable evidence root:

```text
${ASTRAVECTOR_EVIDENCE_ROOT}/fix486g/<run-id>
```

No user-specific absolute path may be hardcoded.

## 5. Bootstrap and terminal evidence

Before preflight, create:

```text
bootstrap.json
stage-results.json
terminal-result.json
runner.stdout.log
runner.stderr.log
```

`bootstrap.json` records:

```text
run_id
branch
source_sha
remote_sha
worktree_clean
start_time_utc
host
os
architecture
command
mode
frozen_bank_identity
base_phase_verdict
```

The runner must handle:

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

Missing terminal evidence, active phase infrastructure after cleanup, hash mismatch or signal termination blocks PASS.

## 6. Source identity

Official execution requires:

```text
branch = codex/fix486g-graph-parent-proof or approved runtime sub-branch
local SHA = remote SHA = tested SHA
worktree clean = true
```

The runner must verify that the tested source contains the prerequisite Phase F result lineage.

Frozen bank:

```text
version = 1.0.0
status = FROZEN
aggregate SHA-256 = cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff
```

Any frozen payload mutation blocks the run.

## 7. Mandatory static gates

```bash
python3 -m py_compile scripts/fix486g_proof.py
bash -n scripts/fix486g-graph-parent-runtime-proof.sh
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo sqlx prepare --check -- --all-targets --all-features
```

Required focused suites must include prior FIX486 contracts and Phase G contracts.

## 8. Phase-owned environment

Phase G must own unique:

```text
Compose project
PostgreSQL database or schema
Qdrant collection
network
volumes
gRPC port
metrics port
```

No foreign runtime may contribute evidence.

Record:

```text
container image references and digests
runtime binary SHA-256
resolved configuration SHA-256
model SHA-256
tokenizer SHA-256
migration head
health response
metrics endpoint ownership
```

## 9. Capability audit stage

Before runtime edits, publish:

```text
docs/fix486/phase-g-graph-parent-runtime/capability-audit.md
```

It must map the actual production path:

```text
request
-> direct retrieval
-> seed normalization
-> graph relation lookup
-> endpoint filtering
-> related-child candidate construction
-> canonical binding validation
-> related-parent hydration
-> direct/Graph dedup
-> MMR admission
-> token-budget admission
-> final visibility
-> response/trace/metrics
```

The audit must answer:

- where seed and related identities originate;
- whether relation rows are zone/document/version scoped;
- how the related parent is selected;
- where canonical binding validation occurs;
- whether seed parent reuse is currently possible;
- how direct and Graph results are deduplicated;
- how provenance is represented and possibly lost;
- where hop limits and cycle controls are enforced;
- whether Graph-disabled requests execute any Graph work;
- how Search and RetrieveContext differ;
- how Graph deadlines, retries, caches and concurrency are implemented;
- how rejected Graph candidates affect candidate windows;
- which metrics already exist.

`UNKNOWN_MATERIAL_CAPABILITIES` must equal zero before design approval.

## 10. Required stage order

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
graph-relation-ingestion
canonical-audit-baseline
qdrant-audit-baseline
graph-audit-baseline
graph-disabled-control
healthy-direct-baseline
healthy-graph-search
healthy-graph-retrieve-context
entry-point-parity
wrong-parent-fault-setup
wrong-parent-proof
wrong-parent-cleanup
zone-isolation-control
lifecycle-invalid-target-control
binding-invalid-target-control
candidate-non-interference
hop-limit-control
cycle-control
warm-repeat
runtime-restart
post-restart-repeat
normalized-comparison
metrics-and-trace-validation
cleanup
evidence-manifest
repository-result-materialization
terminal-verdict
```

Stages may be split but must not be reordered in a way that allows a faulted state to contaminate the healthy baseline.

## 11. Production ingestion and graph relation setup

Frozen documents must be ingested through the production path. Graph relations must be created through the production-supported relation ingestion path.

Direct database inserts are allowed only for explicit fault injection after baseline provenance has been captured, and only in the phase-owned environment.

Evidence must distinguish:

```text
production-created relation
fault-injected relation
production-created Qdrant point
fault-injected projection mutation
```

## 12. Canonical audit

SQL audit must report at minimum:

```text
seed child identity
seed parent identity
related child identity
related parent identity
binding validity
zone/document/version equality
lifecycle visibility
relation row identity and type
duplicate relations
orphan relation endpoints
cross-zone relations
cross-version relations
```

Healthy baseline requires zero unauthorized/orphan relation endpoints.

## 13. Qdrant and graph projection audit

Record:

```text
point IDs
binding IDs
payload child IDs
payload parent IDs
zone/document/version payload
vector schema
relation endpoint physical IDs
relation type and score
```

A Graph proof may not derive expected qrels from runtime output.

## 14. Healthy proof artifacts

Required normalized artifacts:

```text
healthy-direct-search.json
healthy-direct-retrieve-context.json
healthy-graph-search.json
healthy-graph-retrieve-context.json
graph-identity-chain.json
entry-point-parity.json
graph-provenance.json
```

`graph-identity-chain.json` must contain the full seed→edge→related-child→related-parent chain.

## 15. Fault controls

### 15.1 Wrong parent

Construct a phase-owned invalid Graph candidate where the related A3 child is paired with seed parent A1 while retaining traceable provenance from a valid production baseline.

Expected:

```text
classification = GRAPH_BINDING_INVALID or equivalent
invalid final contexts = 0
valid survivor preserved = true
```

### 15.2 Cross-zone relation

Ensure the frozen zone-B relation is present and more attractive if necessary, without modifying frozen data.

Expected:

```text
zone-B edge used = 0
zone-B parent returned = 0
zone-B anchors leaked = 0
```

### 15.3 Lifecycle-invalid target

Use a phase-owned relation endpoint whose canonical version is inactive/deleted/expired.

Expected:

```text
invalid final contexts = 0
explicit rejection reason = true
```

### 15.4 Hop and cycle controls

Add phase-owned second-hop/cycle relations without modifying frozen graph payload.

Expected:

```text
hop > 1 admitted = 0
cycle evidence credit inflation = 0
```

## 16. Search/RetrieveContext parity

The runner must compare normalized fields rather than raw protobuf ordering alone.

Required equality or documented compatible equivalence:

```text
seed logical identity
related logical identity
related parent logical identity
relation type
hop index
origin
required anchors
forbidden anchors
rejection classification
```

## 17. Repeatability

Warm and restart comparisons must normalize:

- timestamps;
- process IDs;
- trace IDs;
- ports;
- runtime-generated correlation IDs.

They must preserve:

- source/binary/config/model/tokenizer hashes;
- logical and physical canonical object identities;
- relation identity/type;
- content hashes;
- normalized result order;
- hard-gate counters.

## 18. Evidence manifest

The final manifest must include every mandatory artifact with:

```text
relative path
size
SHA-256
stage owner
required/optional flag
```

Manifest verification must detect:

```text
missing files
extra unexpected executable-bank mutations
hash mismatches
empty mandatory files
active failpoints
incomplete cleanup
```

## 19. Repository result package

On successful or blocked official execution, publish compact repository artifacts:

```text
RESULT.md
MANIFEST_POINTER.json
STAGE_RESULTS_SUMMARY.json
DEFECT_REGISTER.json
NORMALIZED_COMPARISON_SUMMARY.json
```

The full generated evidence bundle remains outside Git.

## 20. Verdict rules

PASS requires:

- all mandatory stages PASS;
- evidence manifest verification PASS;
- frozen bank unchanged;
- unresolved in-scope P0/P1 = 0;
- all Graph hard gates = 0;
- cleanup PASS;
- exact tested SHA recorded.

Otherwise:

```text
FIX486_GRAPH_PARENT_RUNTIME_PROOF_BLOCKED
blocking_stage=<stage>
failure_code=<code>
evidence_preserved=true
```
