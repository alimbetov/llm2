# FIX490 Codex Execution Task

Implement the approved FIX490 scope from:

```text
docs/fix490/TECHNICAL_SPECIFICATION.md
docs/fix490/REST_BOUNDARY_HARDENING_ADDENDUM.md
```

on branch:

```text
agent/rest-boundary-readiness-sync
```

The hardening addendum is **normative** for REST security, deadlines, cancellation, configuration, probes, HTTP protocol behavior, error mapping and verification details.

## Objective

Add a minimal REST retrieval boundary and synchronize top-level readiness/evidence documentation with the current repository state without changing established AstraVector retrieval semantics or previously proven invariants.

## Baseline

```text
repository=alimbetov/llm2
branch=agent/rest-boundary-readiness-sync
base_main_sha=2a34b65fd24bde11e1fc01dd4ff86ee04a5cd42b
```

Before implementation, verify whether `main` moved. If it moved, record the delta and currentize the working branch before claiming current-head parity/evidence.

## Mandatory implementation

Implement only:

```http
POST /api/v1/retrieve
GET /health
GET /ready
```

REST MUST reuse the same in-process retrieval application/core path behind `AstraVectorRetrievalFacade::RetrieveContext`. Do not call AstraVector's own localhost gRPC endpoint.

REST v1 request fields are limited to direct projections of the existing facade contract:

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

Use the STANDARD public retrieval representation. Do not create REST-only ranking/search controls and do not expose debug/admin surfaces.

## Security requirements

Preserve the complete current security model:

```text
x-api-key
security.enabled
security.protect_health
security.trust_forwarded_identity_headers
security.gateway_trust_header
security.gateway_trust_token
x-astravector-role
trusted caller access level / identity
```

Prefer a minimal transport-neutral extraction shared by gRPC and REST rather than duplicated REST security logic.

`caller_access_level`, caller identity and service identity are trusted security context. Do not trust them merely because the JSON body supplies them.

Fail closed for invalid auth, invalid gateway trust and insufficient access.

## HTTP configuration

Add typed configuration using the canonical FIX490 section name:

```yaml
http:
  enabled: true
  required_on_startup: true
  host: 0.0.0.0
  port: 8080
  max_request_body_bytes: 65536
```

Follow existing environment-override conventions.

Validation must reject collisions with:

```text
gRPC    50051
metrics 9090
```

If HTTP is enabled and required, bind/start failure is fatal.

## Probe semantics

Use the existing shared `Readiness` object.

```text
GET /health -> 200 while process/HTTP adapter is alive
GET /ready  -> 200 when ready=true
GET /ready  -> 503 when ready=false
```

Honor existing `security.protect_health` semantics for HTTP probes.

Do not introduce an HTTP-specific PostgreSQL/Qdrant readiness calculation.

## HTTP protocol behavior

`POST /api/v1/retrieve` accepts `application/json` only.

Required protocol results:

```text
malformed JSON           -> 400
unsupported Content-Type -> 415
oversized body           -> 413
```

Use a bounded JSON body.

Propagate `X-Correlation-Id` into the existing request context/logging path; generate a project-standard correlation id when absent.

Minimal error JSON:

```json
{
  "code": "RESOURCE_EXHAUSTED",
  "message": "...",
  "correlationId": "..."
}
```

Do not expose stack traces, SQL, credentials, gateway tokens or sensitive backend payloads.

## Complete error mapping

Implement and test every current `AstraError` variant:

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

Do not collapse known variants into catch-all 500.

## Deadline and cancellation parity

REST must remain bounded by the existing AstraVector query/retrieval deadline model and feed the same downstream cancellation-aware retrieval path.

Required semantics:

```text
gRPC deadline exhaustion -> DEADLINE_EXCEEDED
REST equivalent          -> 504
resource exhaustion      -> 429
backend unavailable      -> 503
```

Do not convert deadline/backend failures to `200` semantic no-answer.

Bridge HTTP request cancellation/disconnect into per-request cancellation where the selected framework permits it. Global shutdown cancellation alone is not sufficient.

## Server lifecycle

Run REST and gRPC in the same process and reuse the existing global shutdown/drain lifecycle.

Required behavior:

- normal shutdown drains both listeners;
- unexpected required HTTP termination causes global shutdown/non-zero exit;
- required gRPC termination must not leave HTTP appearing healthy indefinitely;
- no new REST microservice or reverse proxy container.

## Minimal observability

Add only bounded-cardinality HTTP transport metrics needed to operate the boundary, e.g. request count and duration.

Allowed labels: route, method, status class.

Do not label metrics with query text, user id, document id, access-zone id or correlation id.

## Required tests

### Positive REST/gRPC parity

Prove equivalence for:

```text
single-zone retrieval
multi-zone retrieval when enabled
Graph disabled
Graph enabled
insufficient/no-answer response
successful degraded response with surviving contexts
```

Compare at least:

```text
context count/order
access_zone_id
document_id
document_version
matched_chunk_id
parent_chunk_id
source_block_id
matched_text
parent_text
dense/sparse/fusion/final scores
evidence status
degradation state/codes
Graph-derived presence/provenance
```

### Security/lifecycle negative parity

Cover, where supported by current harness:

```text
missing/bad API key
untrusted forwarded identity
bad gateway trust token
insufficient access level
wrong/inactive/missing zone
multi-zone disabled
too many zones
invalid zone id
DELETED
EXPIRED
INDEXING/non-active
```

REST must expose no context that the authoritative gRPC/security path would reject.

### HTTP boundary tests

Cover:

```text
malformed JSON -> 400
unsupported media type -> 415
oversized body -> 413
health/readiness status codes
protect_health parity
complete AstraError mapping
correlation propagation
```

### Failure/degradation parity

Use supported failpoints/harness for:

```text
partial retrieval-branch degradation
PostgreSQL hydration timeout/failure
Qdrant unavailable/timeout
request deadline exhaustion
overload/backpressure
request cancellation where testable
```

A valid degraded response with surviving contexts remains successful; total required-backend failure remains an HTTP failure.

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

## Regression gates

Run exactly:

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

Also run focused REST parity/security/protocol/degradation tests and relevant existing retrieval/security/invariant suites. Do not weaken frozen assertions.

## REST smoke

Document and execute examples for:

```text
GET /health
GET /ready
POST /api/v1/retrieve
```

Run a short REST transport smoke at local concurrency 1 and 2 where practical. This is transport regression evidence, not capacity proof.

Do **not** rerun the full FIX489-R3 60-minute soak solely because the thin REST adapter was added, unless core runtime semantics changed or new testing invalidates inherited evidence.

## Documentation/evidence synchronization

Audit and reconcile:

```text
README.md
docs/README.md
docs/ASTRAVECTOR_READINESS_REPORT.md
docs/02-readiness-and-verdicts.md
docs/12-roadmap.md
```

Add:

```text
docs/api/rest-api.md
docs/fix490/CURRENT_HEAD_STATUS.md
docs/fix490/RESULT.md
```

`docs/api/rest-api.md` must document request fields, response model, auth/trusted identity, errors, probes, configuration and curl examples.

`CURRENT_HEAD_STATUS.md` must use explicit provenance categories:

```text
IMPLEMENTED
PROVEN_ON_CURRENT_OR_INHERITED_SHA
PROVEN_LOCAL_ONLY
STILL_OPEN
NOT_IN_SCOPE
```

Preserve FIX489-R3 evidence scope exactly:

```text
capacity_scope=LOCAL_MAC_CPU
production_capacity_claim=false
maximum_stable_concurrency=2
recommended_operating_concurrency=1
FIX489_R3_SOAK_60M_PASS
```

Do not claim `PRODUCTION_READY` without independent proof of every repository-defined production gate.

## Final result

`docs/fix490/RESULT.md` must contain exact tested SHA, current main SHA, executed commands, gate outcomes, REST/gRPC parity evidence, security negative evidence, HTTP boundary evidence, deadline/backpressure evidence and documentation/evidence audit result.

Allowed implementation verdicts:

```text
FIX490_REST_BOUNDARY_AND_READINESS_SYNC_PASS
FIX490_REST_BOUNDARY_AND_READINESS_SYNC_BLOCKED
```

## Independent verification

After implementation, use:

```text
docs/fix490/CODEX_REST_VERIFICATION_PROMPT.md
```

for a separate verification pass. The verifier should review/test first and only make minimal FIX490-local fixes when a test proves a boundary defect.

## Review policy

Prefer the smallest possible code change. The desired implementation is a transport adapter, not a new application layer or refactor campaign. Do not perform opportunistic cleanup unrelated to FIX490.
