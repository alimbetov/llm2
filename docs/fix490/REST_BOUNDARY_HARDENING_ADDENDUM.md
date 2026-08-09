# FIX490 — REST Boundary Hardening Addendum

Status: **NORMATIVE** for FIX490.

This addendum refines `TECHNICAL_SPECIFICATION.md` where the original specification was intentionally transport-agnostic. It does not widen FIX490 into a new architecture phase. Where this addendum is more specific about REST security, deadlines, cancellation, configuration, error mapping, probes, or verification, this document takes precedence.

## 1. Scope remains unchanged

Required public HTTP surface:

```text
POST /api/v1/retrieve
GET  /health
GET  /ready
```

No REST ingestion, admin, control-plane, Qdrant, lifecycle, model-management, Graph administration, embedding-preview, or document-management endpoints are added by FIX490.

The REST handler MUST call the same in-process retrieval application/core path used by `AstraVectorRetrievalFacade::RetrieveContext`. It MUST NOT call AstraVector through localhost/self gRPC.

## 2. Complete AstraError -> HTTP mapping

The implementation MUST cover every current `AstraError` variant, not only the subset originally listed.

Normative mapping:

```text
AstraError::InvalidArgument    -> 400 Bad Request
AstraError::OutOfRange         -> 400 Bad Request
AstraError::Unauthenticated    -> 401 Unauthorized
AstraError::PermissionDenied   -> 403 Forbidden
AstraError::NotFound           -> 404 Not Found
AstraError::AlreadyExists      -> 409 Conflict
AstraError::FailedPrecondition -> 409 Conflict
AstraError::OwnershipLost      -> 409 Conflict
AstraError::ResourceExhausted  -> 429 Too Many Requests
AstraError::Cancelled          -> 499 Client Closed Request
AstraError::Unavailable        -> 503 Service Unavailable
AstraError::DeadlineExceeded   -> 504 Gateway Timeout
AstraError::Internal           -> 500 Internal Server Error
```

`499` is intentionally used for request cancellation because it preserves the distinction between caller-side cancellation and server timeout. If the selected HTTP framework cannot produce a meaningful response after the connection is already gone, the internal classification and metrics MUST still record cancellation rather than remapping it to `500` or semantic no-answer.

Do not expose stack traces, SQL text, credentials, internal tokens, raw backend payloads, or sensitive configuration in error bodies.

Minimal stable REST error body:

```json
{
  "code": "RESOURCE_EXHAUSTED",
  "message": "request rejected by bounded admission control",
  "correlationId": "..."
}
```

Required fields:

```text
code
message
correlationId
```

## 3. Shared security semantics, not duplicated security logic

REST MUST preserve the complete existing AstraVector security model:

```text
x-api-key
security.enabled
security.protect_health
security.trust_forwarded_identity_headers
security.gateway_trust_header
security.gateway_trust_token
x-astravector-role
caller access level / forwarded identity context
```

Implementation rule:

> Extract or reuse a transport-neutral security validator only as much as needed so gRPC and REST consume the same authentication / trusted-forwarded-identity semantics. Do not create a second independently implemented REST security policy.

This limited extraction is permitted by FIX490 because it prevents security divergence; it MUST NOT become a broad security refactor.

### 3.1 Trusted caller context

`caller_access_level`, caller identity and caller service identity are security context. They MUST NOT be trusted merely because a public JSON request body supplies them.

REST body contains retrieval intent and search scope. Trusted identity/access context comes from the already-configured authentication / trusted gateway path.

Conceptually:

```text
JSON body
  -> question / zones / profile / filters / graph options

trusted request headers / validated gateway identity
  -> caller identity / role / access level / service identity
```

If security is disabled in the active non-production profile, preserve the existing development/test semantics. Do not invent a separate REST-only default privilege model.

### 3.2 Health protection parity

HTTP probes MUST honor existing `security.protect_health` semantics.

```text
protect_health=false -> HTTP health/readiness follow the existing health exemption policy
protect_health=true  -> HTTP health/readiness require configured authentication
```

REST and gRPC health exposure MUST NOT silently diverge.

## 4. Exact REST v1 request contract

FIX490 REST v1 supports the following retrieval fields because they map directly to `RetrieveContextRequest`:

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

Do not add arbitrary transport-specific search parameters.

`ResponseDetail` is deliberately NOT exposed as a free REST tuning surface in FIX490. REST v1 uses the existing STANDARD public retrieval representation. Internal/debug diagnostics remain on existing gRPC/admin surfaces unless required for test instrumentation.

Multi-zone requests MUST use the existing access-zone resolution and validation logic, including:

```text
allow_multi_zone_search
max_search_access_zones
access-zone id format constraints
access-zone registry resolution/status rules
access-zone code rules
```

No REST-local zone parser or authorization shortcut is permitted.

## 5. HTTP content contract

`POST /api/v1/retrieve` accepts:

```text
Content-Type: application/json
```

Required behavior:

```text
malformed JSON             -> 400 Bad Request
unsupported media type     -> 415 Unsupported Media Type
request body above limit   -> 413 Payload Too Large
```

REST MUST use a bounded JSON request-body size. Add typed configuration for the limit; a conservative default such as 64 KiB is sufficient for retrieval requests.

## 6. Correlation identity

REST MUST preserve correlation identity into the existing retrieval request context and logs.

Preferred external header:

```text
X-Correlation-Id
```

Rules:

- if a valid correlation id is supplied, propagate it;
- if absent, generate one using the project-standard UUID approach;
- include the correlation id in REST error responses;
- do not use query text, access-zone id or user identity as metrics labels.

## 7. Deadline parity

The existing gRPC path has bounded transport/application deadlines. REST MUST NOT become an unbounded alternative path.

For REST retrieval, derive one effective request deadline from the existing retrieval/query deadline configuration and any trusted/supported HTTP request deadline mechanism.

At minimum:

```text
REST effective deadline <= configured AstraVector retrieval/query budget
```

The same downstream `OperationBudget` / cancellation-aware retrieval semantics MUST be used.

Required parity expectation:

```text
gRPC deadline exhaustion -> DEADLINE_EXCEEDED
REST equivalent          -> 504 Gateway Timeout
```

Do not translate deadline failure into `200` + empty contexts / semantic no-answer.

## 8. Per-request cancellation parity

Global service shutdown cancellation is not sufficient.

The REST request future MUST participate in per-request cancellation semantics so that client disconnect / handler cancellation does not intentionally leave expensive embedding, Qdrant, Graph, MMR or PostgreSQL work running independently of the abandoned request.

If the framework cancels handler futures on disconnect, bridge that cancellation into the same request cancellation token used by the retrieval core where practical.

Cancellation remains bounded and observable; it MUST NOT be remapped to `Internal`.

## 9. Typed HTTP configuration

Add a minimal typed config section. Canonical name for FIX490: `http`.

Example:

```yaml
http:
  enabled: true
  required_on_startup: true
  host: 0.0.0.0
  port: 8080
  max_request_body_bytes: 65536
```

Environment overrides should follow existing AstraVector configuration conventions.

Validation MUST reject port collision:

```text
http.port != grpc.port
http.port != metrics.port
```

Default ports remain:

```text
gRPC    50051
metrics 9090
HTTP    8080
```

If `http.enabled=false`, no HTTP listener is started. If `http.enabled=true` and `required_on_startup=true`, bind/startup failure is fatal.

## 10. Probe semantics

`GET /health`:

```text
process/HTTP adapter alive -> 200 OK
```

`GET /ready` MUST use the existing shared `Readiness` object only:

```text
ready=true  -> 200 OK
ready=false -> 503 Service Unavailable
```

No REST-local PostgreSQL/Qdrant/scheduler readiness algorithm may be introduced.

Minimal JSON is sufficient, for example:

```json
{"status":"READY","ready":true}
```

## 11. Required server lifecycle behavior

HTTP and gRPC run in the same process and share the existing global shutdown token/drain lifecycle.

Required orchestration:

- normal process shutdown drains both listeners;
- if a required HTTP listener terminates unexpectedly, initiate global shutdown and exit non-zero;
- if required gRPC terminates unexpectedly, HTTP must not leave the process appearing healthy indefinitely;
- no second supervisor service/container is introduced.

## 12. Minimal HTTP observability

Do not create a new observability subsystem. Add only bounded-cardinality transport metrics needed to operate the boundary, e.g. equivalent counters/histograms for:

```text
HTTP requests total
HTTP request duration
```

Allowed labels are bounded transport dimensions such as:

```text
route
method
status_class
```

Forbidden high-cardinality labels include:

```text
query text
user id
document id
access-zone id
correlation id
```

Existing retrieval-core metrics remain authoritative for ranking/backend behavior.

## 13. REST-specific parity matrix

Positive parity must cover at least:

```text
single-zone STANDARD retrieval
multi-zone retrieval when enabled
Graph disabled
Graph enabled
no-answer / insufficient evidence response
successful degraded retrieval with surviving contexts
```

Security/validation negative parity must cover at least:

```text
missing/bad API key when auth enabled
untrusted forwarded identity headers
bad gateway trust token when forwarded identity is enabled
insufficient caller access level
wrong/inactive/missing zone
multi-zone disabled
zone count above configured maximum
invalid zone identifier
DELETED document/version excluded
EXPIRED context excluded
INDEXING/non-active context excluded
```

HTTP protocol boundary tests must cover:

```text
malformed JSON -> 400
unsupported content type -> 415
oversized body -> 413
not ready -> 503 on /ready
protect_health parity
complete AstraError mapping
correlation id propagation
```

Failure/degradation parity must cover supported harness cases for:

```text
Qdrant unavailable/timeout
PostgreSQL hydration timeout/failure
request deadline exhaustion
overload/backpressure
client/request cancellation where testable
```

## 14. REST smoke and concurrency check

Add a simple executable curl/example smoke for:

```text
GET /health
GET /ready
POST /api/v1/retrieve
```

Run a short REST concurrency smoke at the already-known local operating range (for example concurrency 1 and 2) only to detect transport regressions. This is NOT a new capacity claim.

FIX490 MUST NOT rerun the full 60-minute FIX489-R3 soak solely because a thin REST adapter was added, unless the implementation changes retrieval/runtime semantics or the test evidence reveals such a change.

Existing FIX489-R3 evidence remains inherited with its exact scope:

```text
capacity_scope=LOCAL_MAC_CPU
production_capacity_claim=false
```

## 15. REST documentation

FIX490 must add:

```text
docs/api/rest-api.md
```

and link it from the documentation index where appropriate.

The REST API document must contain at least:

```text
base URL / port configuration
POST /api/v1/retrieve request fields
response model
security headers/trusted identity rules
error mapping
health/readiness semantics
curl examples
scope/non-goals
```

OpenAPI generation and Swagger UI are explicitly NOT required by FIX490.

## 16. Hard stop remains in force

If satisfying this addendum requires changing retrieval/chunking/model/ranking/Graph/MMR/access/lifecycle semantics, stop and record:

```text
FIX490_BLOCKED_BY_ARCHITECTURE_CHANGE
```

Transport-neutral extraction needed solely to prevent gRPC/REST security or retrieval-entry divergence is allowed only when behavior remains unchanged and parity tests prove it.
