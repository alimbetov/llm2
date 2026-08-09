# FIX490 Codex Execution Task

Implement the approved FIX490 scope from `docs/fix490/TECHNICAL_SPECIFICATION.md` on branch `agent/rest-boundary-readiness-sync`.

## Objective

Add a minimal REST retrieval boundary and synchronize top-level readiness/evidence documentation with the current repository state without changing established AstraVector retrieval semantics or previously proven invariants.

## Baseline

```text
repository=alimbetov/llm2
branch=agent/rest-boundary-readiness-sync
base_main_sha=2a34b65fd24bde11e1fc01dd4ff86ee04a5cd42b
spec_commit=5e8fd71238c12d8b5ac1803d342e01bc1abc24fb
```

Before implementation, verify whether `main` moved. If it moved, record the delta and rebase/currentize the working branch before claiming current-head parity/evidence.

## Mandatory implementation

1. Add one REST retrieval endpoint:

```http
POST /api/v1/retrieve
```

2. Add HTTP probes:

```http
GET /health
GET /ready
```

3. Reuse the existing retrieval application/core path behind `AstraVectorRetrievalFacade::RetrieveContext`. Do not call the service's own gRPC endpoint from the REST adapter.

4. Keep REST DTOs as a minimal JSON projection of existing `RetrieveContextRequest` / `RetrieveContextResponse` semantics.

5. Reuse existing readiness state and existing authentication/access policy semantics.

6. Add deterministic HTTP mapping for existing `AstraError` classes.

7. Add parity tests proving REST and gRPC agree on context identity/order/content/status under equivalent requests.

8. Add security/failure/degradation parity tests sufficient to prove the REST adapter cannot bypass existing access/lifecycle and degradation behavior.

9. Run locked static/test gates and relevant existing invariant suites without weakening them.

10. Audit and synchronize:

```text
README.md
docs/ASTRAVECTOR_READINESS_REPORT.md
docs/02-readiness-and-verdicts.md
docs/12-roadmap.md
```

against current authoritative phase results. Do not rewrite immutable historical evidence.

11. Add `docs/fix490/CURRENT_HEAD_STATUS.md` with explicit provenance categories:

```text
IMPLEMENTED
PROVEN_ON_CURRENT_OR_INHERITED_SHA
PROVEN_LOCAL_ONLY
STILL_OPEN
NOT_IN_SCOPE
```

12. Add `docs/fix490/RESULT.md` containing exact tested SHA, commands, gate results and one of:

```text
FIX490_REST_BOUNDARY_AND_READINESS_SYNC_PASS
FIX490_REST_BOUNDARY_AND_READINESS_SYNC_BLOCKED
```

## Frozen semantics

Do not change or tune:

```text
CanonicalTokenizer
BGE-M3/ONNX inference
chunking ownership
SOURCE/PARENT/SUB_180/SUB_260 hierarchy
Dense/Sparse/Lexical/Hybrid retrieval
fusion/RRF weights or thresholds
no-answer semantics
parent hydration
GraphRAG semantics/weights
MMR semantics/weights
token budget
final visibility recheck
access-zone/access-level isolation
document/version/lifecycle/TTL semantics
PostgreSQL canonical-state model
outbox/reconciliation
Qdrant projection model
retry/deadline/cancellation/backpressure semantics
frozen quality banks/qrels/thresholds
```

Any need to change one of these is a blocker, not permission to widen scope.

## Evidence constraints

Current merged FIX489-R3 evidence includes a local stable floor and a successful 60-minute soak. Preserve its scope exactly:

```text
capacity_scope=LOCAL_MAC_CPU
production_capacity_claim=false
maximum_stable_concurrency=2
recommended_operating_concurrency=1
FIX489_R3_SOAK_60M_PASS
```

Do not translate local-hardware proof into production-capacity or `PRODUCTION_READY` claims.

## Required gates

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

Also run focused REST parity and relevant existing retrieval/security/degradation integration tests. Record every executed command and exact result in `docs/fix490/RESULT.md`.

## Review policy

Prefer the smallest possible code change. The desired implementation is a transport adapter, not a new application layer or a refactor campaign. Do not perform opportunistic cleanup unrelated to FIX490.