# FIX490 Current Branch Status

Status in this document distinguishes implementation presence from test proof.

## Source identity

```text
base_main_sha=2a34b65fd24bde11e1fc01dd4ff86ee04a5cd42b
branch=agent/rest-boundary-readiness-sync
rest_implementation_status=IMPLEMENTED_NOT_YET_SMOKE_VERIFIED
```

The exact final `tested_sha` must be written by the smoke/verification pass after Cargo and runtime tests complete.

## IMPLEMENTED

Current branch contains:

```text
internal REST listener in same AstraVector process
POST /api/v1/retrieve
GET /health
GET /ready
shared Readiness usage
shared shutdown lifecycle
bounded JSON request body
correlation id handling
HTTP status/error mapping
bounded HTTP transport metrics
REST request -> existing Search/retrieval core mapping
REST response serialization of contexts/scores/evidence/degradation
internal-only REST decision with no REST auth middleware
HTTP enable/host/port/body-limit environment configuration
port collision validation
```

Implementation presence is not a PASS claim until Codex executes the required gates and smoke/parity tests.

## PROVEN_ON_CURRENT_OR_INHERITED_SHA

The branch inherits the existing AstraVector retrieval/consistency architecture from its main baseline. FIX490 is explicitly prohibited from changing these semantics:

```text
CanonicalTokenizer + BGE-M3/ONNX ownership
token-aware hierarchical chunking
SOURCE/PARENT/SUB_180/SUB_260
dense/sparse/lexical/hybrid retrieval
fusion/RRF
parent hydration
GraphRAG
MMR
token budget
final visibility recheck
PostgreSQL canonical state
outbox/reconciliation
Qdrant projection
access/version/lifecycle semantics
```

Final FIX490 verification must inspect the branch diff and confirm these invariants were not modified.

## PROVEN_LOCAL_ONLY

Authoritative FIX489-R3 result records:

```text
FIX489_R3_LOCAL_STABLE_FLOOR_PASS=PASS
FIX489_R3_SOAK_60M_PASS=PASS
FIX489_R3_PASS
capacity_scope=LOCAL_MAC_CPU
production_capacity_claim=false
maximum_stable_concurrency=2
recommended_operating_concurrency=1
first_controlled_saturation_concurrency=3
```

Source:

```text
docs/fix489/r3/RESULT.md
```

This remains local hardware evidence only.

## STILL OPEN FOR FIX490

```text
Cargo.lock synchronization for the direct Axum dependency
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
HTTP protocol smoke
REST-vs-gRPC semantic parity smoke
visibility/lifecycle parity smoke
Graph off/on parity
failure/degradation mapping smoke where harness supports it
short local REST concurrency smoke 1/2
documentation top-level reconciliation after test outcome
final docs/fix490/RESULT.md
independent docs/fix490/REST_VERIFICATION_RESULT.md
```

## NOT IN SCOPE

```text
REST authentication/authorization
REST ingestion/admin/control plane
AstraIndexator implementation
SeaweedFS integration inside AstraVector
retrieval tuning
new ranking algorithms
new Graph/MMR behavior
Kafka/event infrastructure
production capacity certification
```

## Required next evidence step

Use:

```text
docs/fix490/CODEX_EXECUTION_TASK.md
```

for build/smoke completion and then:

```text
docs/fix490/CODEX_REST_VERIFICATION_PROMPT.md
```

for independent verification.
