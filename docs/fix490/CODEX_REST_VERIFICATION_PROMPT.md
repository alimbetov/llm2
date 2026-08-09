# Codex Prompt — Verify FIX490 REST API Against Specification

Use this prompt **after the FIX490 REST implementation exists**. The task is verification-first. Do not redesign AstraVector and do not widen scope unless a failing test proves a real defect in the REST boundary itself.

---

You are verifying the implementation of FIX490 in repository:

```text
https://github.com/alimbetov/llm2
```

Expected branch:

```text
agent/rest-boundary-readiness-sync
```

Before changing anything, read these files completely:

```text
docs/fix490/TECHNICAL_SPECIFICATION.md
docs/fix490/REST_BOUNDARY_HARDENING_ADDENDUM.md
docs/fix490/ACCEPTANCE_CRITERIA.md
docs/fix490/CODEX_EXECUTION_TASK.md
```

Also inspect the current implementation of:

```text
src/main.rs
src/grpc/mod.rs
src/security/mod.rs
src/error/mod.rs
src/config/mod.rs
src/health/*
proto/astravector_embedding.proto
Cargo.toml
config/application.yaml
config/application-*.yaml
```

Then inspect every REST/HTTP file added by FIX490 and all tests/docs changed by the implementation.

## Mission

Prove whether FIX490 closes the REST boundary without changing any existing AstraVector retrieval invariant.

The authoritative rule is:

```text
REST is only a transport adapter over the same in-process retrieval core used by gRPC RetrieveContext.
```

The following architecture is forbidden:

```text
REST -> localhost/self gRPC -> AstraVector
```

Do not accept implementation presence as proof. Execute tests and compare behavior.

## Step 1 — Baseline and diff audit

Run and record:

```bash
git status --short
git branch --show-current
git rev-parse HEAD
git rev-parse main
git merge-base main HEAD
git diff --stat main...HEAD
git diff --name-status main...HEAD
```

If `main` moved since FIX490 began, record the exact delta. Do not claim current-main evidence without currentizing/revalidating as required by the specification.

Review the complete diff and explicitly classify every changed file as one of:

```text
REST_TRANSPORT
SHARED_SECURITY_EXTRACTION
SHARED_RETRIEVAL_ENTRY_EXTRACTION
CONFIG
TEST
DOCUMENTATION
EVIDENCE
UNRELATED
```

Any `UNRELATED` production-code change is a review failure unless independently justified by FIX490.

## Step 2 — Architecture invariant audit

Prove from the code that REST and gRPC share the same retrieval implementation.

Verify that REST does NOT introduce independent implementations of:

```text
query planning
dense retrieval
sparse retrieval
lexical retrieval
fusion/RRF
no-answer
parent hydration
GraphRAG
MMR
token budget
visibility recheck
intent/coverage recomputation
access-zone resolution
lifecycle filtering
```

Search for suspicious duplication and report exact files/functions.

Verify no semantic/tuning changes to:

```text
CanonicalTokenizer
BGE-M3/ONNX
SOURCE/PARENT/SUB_180/SUB_260 chunking
Dense/Sparse/Lexical/Hybrid
fusion/RRF weights/thresholds
GraphRAG
MMR
no-answer
token budget
PostgreSQL canonical state
outbox/reconciliation
Qdrant projection
access/version/lifecycle/TTL
retry/deadline/backpressure/degradation
frozen quality banks/qrels/thresholds
```

If such a change exists, stop PASS classification and report:

```text
FIX490_BLOCKED_BY_ARCHITECTURE_CHANGE
```

unless the change is demonstrably behavior-preserving extraction required only to share security/retrieval entry logic and parity tests prove equivalence.

## Step 3 — REST surface audit

Required endpoints only:

```text
POST /api/v1/retrieve
GET  /health
GET  /ready
```

Fail verification if FIX490 adds public REST ingestion/admin/control-plane APIs outside the approved scope.

Verify typed HTTP configuration exists and covers at least:

```text
enabled
required_on_startup
host
port
max_request_body_bytes
```

Verify default HTTP port does not collide with:

```text
gRPC    50051
metrics 9090
```

Verify configuration validation rejects collisions.

## Step 4 — Security parity audit

Inspect both gRPC and REST security paths.

REST must preserve the existing semantics for:

```text
x-api-key
security.enabled
security.protect_health
security.trust_forwarded_identity_headers
security.gateway_trust_header
security.gateway_trust_token
x-astravector-role
caller access level
trusted forwarded identity
```

The preferred implementation is shared transport-neutral validation, not duplicated policies.

Explicitly prove:

```text
callerAccessLevel is not trusted merely because a JSON body supplies it
caller identity/access comes from validated security context
missing/bad API key fails closed when auth is enabled
untrusted forwarded identity fails closed
bad gateway trust token fails closed
access-zone filters remain active
final PostgreSQL visibility checks remain active
```

Verify `/health` and `/ready` follow the configured `protect_health` policy rather than inventing REST-only behavior.

## Step 5 — Request/response contract audit

REST v1 retrieval fields must map directly to existing `RetrieveContextRequest` semantics:

```text
question
accessZoneId
accessZoneIds
accessZoneCode
accessZoneCodes
profile
maxContexts
filters
enableGraphExpansion
graphMaxHops
graphMaxRelatedContexts
```

REST must not create new ranking controls.

Verify REST response is a JSON projection of the existing public retrieval result and does not recompute:

```text
scores
ranking
coverage
evidence status
degradation
citations
Graph provenance
```

Verify FIX490 uses the STANDARD public retrieval representation and does not expose internal/debug control surfaces unless the specification was explicitly updated.

## Step 6 — HTTP protocol tests

Execute automated tests and/or direct HTTP checks for:

```text
valid application/json request
malformed JSON -> 400
unsupported Content-Type -> 415
oversized request body -> 413
/health alive -> 200
/ready ready -> 200
/ready not-ready -> 503
protect_health=false behavior
protect_health=true behavior
```

Verify a bounded request-body limit is enforced.

Verify error body contains at least:

```text
code
message
correlationId
```

and does not expose sensitive internal data.

## Step 7 — Complete error mapping verification

Verify every current `AstraError` variant is mapped and tested:

```text
InvalidArgument    -> 400
OutOfRange         -> 400
Unauthenticated    -> 401
PermissionDenied   -> 403
NotFound           -> 404
AlreadyExists      -> 409
FailedPrecondition -> 409
OwnershipLost      -> 409
ResourceExhausted  -> 429
Cancelled          -> 499 classification where response is possible
Unavailable        -> 503
DeadlineExceeded   -> 504
Internal           -> 500
```

Do not accept catch-all `500` for known variants.

## Step 8 — Deadline and cancellation parity

Prove REST cannot be used as an unbounded retrieval path.

Verify REST uses the existing configured query/retrieval budget and reaches the same downstream cancellation-aware retrieval logic as gRPC.

Test, where harness permits:

```text
deadline exhausted -> REST 504
backpressure/resource exhausted -> REST 429
backend unavailable -> REST 503
client/request cancellation remains cancellation, not Internal/no-answer
```

Verify total backend/deadline failure is never returned as `200` with empty contexts.

## Step 9 — gRPC vs REST positive parity

Using identical runtime state and semantically equivalent requests, compare gRPC `RetrieveContext` and REST `/api/v1/retrieve`.

At minimum compare:

```text
returned context count
ordering
access_zone_id
document_id
document_version
matched_chunk_id
parent_chunk_id
source_block_id
matched_text
parent_text
dense_score
sparse_score
fusion_score
final_score
evidence status
degradation state/codes
Graph-derived presence/provenance when enabled
```

Use explicit float tolerance only for JSON serialization; do not allow ranking/order divergence.

Required positive scenarios:

```text
single-zone retrieval
multi-zone retrieval when enabled
Graph disabled
Graph enabled
insufficient/no-answer response
successful degraded response with surviving contexts
```

## Step 10 — Security/lifecycle negative parity

Prove REST leaks nothing gRPC would reject.

Required scenarios where supported by current fixtures/harness:

```text
wrong access zone
inactive/missing zone
multi-zone disabled
too many zones
invalid zone id
insufficient access level
DELETED document/version
EXPIRED context
INDEXING/non-active document
missing/bad auth
untrusted forwarded identity
bad gateway trust token
```

For every scenario, compare REST behavior with semantically equivalent gRPC behavior or the same authoritative underlying policy.

## Step 11 — Failure/degradation parity

Exercise existing supported failure injection for:

```text
partial dense/sparse/lexical degradation
PostgreSQL hydration timeout/failure
Qdrant unavailable/timeout
request deadline exhaustion
overload/backpressure
```

Verify the key semantic distinction:

```text
valid partial degradation with surviving contexts -> successful REST retrieval
complete required backend failure              -> HTTP failure
```

## Step 12 — Correlation and observability

Verify `X-Correlation-Id` or the implemented equivalent is propagated into the existing request context/logging path; if absent, a correlation id is generated.

Verify transport metrics are bounded-cardinality. Reject metrics labels containing:

```text
query text
user id
document id
access-zone id
correlation id
```

## Step 13 — Server lifecycle

Verify HTTP and gRPC share the existing process shutdown lifecycle.

Test or inspect proof that:

```text
normal shutdown drains both servers
required HTTP bind/start failure is fatal
unexpected required HTTP termination triggers global failure/shutdown
required gRPC termination cannot leave HTTP reporting healthy indefinitely
```

No separate REST microservice/container should have been introduced.

## Step 14 — Regression gates

Run exactly:

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

Run all focused REST tests plus relevant existing retrieval/security/degradation/invariant tests.

Do not edit or weaken existing frozen assertions, quality banks, qrels, thresholds or fixtures to obtain PASS.

## Step 15 — REST smoke

Run documented executable checks equivalent to:

```bash
curl http://127.0.0.1:8080/health
curl http://127.0.0.1:8080/ready
curl -X POST http://127.0.0.1:8080/api/v1/retrieve \
  -H 'Content-Type: application/json' \
  -H 'X-Correlation-Id: fix490-curl-smoke' \
  -d '{...}'
```

If auth is enabled, use the configured test API key and trusted identity headers according to the specification.

Run a short REST transport concurrency smoke at local concurrency 1 and 2 if the harness supports it. This is regression evidence only, NOT a capacity claim.

Do NOT rerun the 60-minute FIX489-R3 soak solely because REST was added unless core runtime semantics changed or REST testing reveals a reason to invalidate inherited evidence.

## Step 16 — Documentation/evidence verification

Verify FIX490 updates or correctly reconciles:

```text
README.md
docs/README.md
docs/ASTRAVECTOR_READINESS_REPORT.md
docs/02-readiness-and-verdicts.md
docs/12-roadmap.md
docs/api/rest-api.md
docs/fix490/CURRENT_HEAD_STATUS.md
docs/fix490/RESULT.md
```

The new status report must distinguish:

```text
IMPLEMENTED
PROVEN_ON_CURRENT_OR_INHERITED_SHA
PROVEN_LOCAL_ONLY
STILL_OPEN
NOT_IN_SCOPE
```

Preserve exact FIX489-R3 scope:

```text
capacity_scope=LOCAL_MAC_CPU
production_capacity_claim=false
maximum_stable_concurrency=2
recommended_operating_concurrency=1
FIX489_R3_SOAK_60M_PASS
```

Do not claim `PRODUCTION_READY` unless every repository-defined production gate is independently proven.

## Step 17 — Required verification report

Create/update:

```text
docs/fix490/REST_VERIFICATION_RESULT.md
```

It must contain:

```text
base_sha
tested_sha
main_sha
model identity where runtime-tested
tokenizer identity where runtime-tested
config/effective config identity where available
exact commands executed
PASS/FAIL/SKIP for every verification section
REST/gRPC parity evidence
security negative evidence
error mapping evidence
deadline/backpressure evidence
health/readiness evidence
regression gate results
documentation/evidence audit result
```

Every SKIP must include a reason and whether it blocks FIX490.

Allowed final verdicts only:

```text
FIX490_REST_VERIFICATION_PASS
FIX490_REST_VERIFICATION_FAIL
FIX490_REST_VERIFICATION_BLOCKED
```

PASS is allowed only when every mandatory acceptance criterion is satisfied and there is no unresolved invariant/security parity violation.

## Modification policy during verification

Default behavior is review/test/report, not implementation.

You MAY fix a defect only when all conditions are true:

1. the defect is inside the approved FIX490 REST boundary;
2. the fix is minimal;
3. it does not change frozen retrieval semantics;
4. you add or strengthen a test reproducing the defect;
5. you record the fix in `REST_VERIFICATION_RESULT.md`.

If the required fix would change a frozen invariant, do not implement it. Report:

```text
FIX490_BLOCKED_BY_ARCHITECTURE_CHANGE
```

At the end, provide a concise summary containing:

```text
VERDICT
TESTED_SHA
FILES_CHANGED_BY_VERIFICATION
MANDATORY_GATES
REST_GRPC_PARITY
SECURITY_PARITY
REMAINING_BLOCKERS
```
