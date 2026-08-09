# FIX490 — Internal REST Boundary Addendum

Status: **NORMATIVE** for FIX490.

This document supersedes any earlier FIX490 wording that treats REST as a public security boundary.

## 1. Architectural decision

AstraVector REST is an **internal service transport**.

Therefore FIX490 MUST NOT introduce REST authentication or authorization infrastructure:

```text
NO x-api-key requirement
NO JWT/OAuth
NO gateway trust token
NO forwarded-identity authentication
NO REST roles
NO REST security middleware
NO independent HTTP authorization policy
```

Network/service-mesh/Kubernetes infrastructure may restrict access to AstraVector outside this codebase. That is not part of FIX490.

`callerAccessLevel` is retained because it is an input to AstraVector retrieval visibility semantics (`PUBLIC`, `INTERNAL`, `CONFIDENTIAL`, `RESTRICTED`). It is **not** proof of caller identity and must not be described as HTTP authentication.

Likewise, access-zone selection is retrieval scope, not REST authentication.

## 2. Scope

Required internal HTTP surface:

```text
POST /api/v1/retrieve
GET  /health
GET  /ready
```

No REST ingestion, admin, control-plane, Qdrant, lifecycle, embedding-preview, model-management, or document-management endpoints are added by FIX490.

The REST handler MUST execute the same authoritative AstraVector retrieval core used by gRPC `RetrieveContext`:

```text
REST request
  -> RetrieveContext-equivalent mapping
  -> existing Search/retrieval core
  -> REST response mapping
```

Forbidden:

```text
REST -> localhost/self gRPC -> AstraVector
```

REST MUST NOT implement a second dense/sparse/hybrid/Graph/MMR pipeline.

## 3. Exact REST request contract

FIX490 REST v1 accepts:

```text
question
accessZoneId
accessZoneIds
accessZoneCode
accessZoneCodes
callerAccessLevel
profile
maxContexts
filters
enableGraphExpansion
graphMaxHops
graphMaxRelatedContexts
correlationId
```

Defaults:

```text
callerAccessLevel = INTERNAL
profile           = BALANCED
maxContexts       = same facade default when zero/absent
```

Profiles MUST map exactly like the current gRPC retrieval facade:

```text
SEMANTIC       -> DENSE + DENSE_ONLY
LEXICAL_STRICT -> SPARSE + DENSE_SPARSE_REQUIRED
LEGAL          -> HYBRID + DENSE_SPARSE_REQUIRED
TECHNICAL      -> HYBRID + DENSE_SPARSE_IF_AVAILABLE
BALANCED       -> HYBRID + DENSE_SPARSE_IF_AVAILABLE
```

Candidate limits MUST preserve existing facade values:

```text
LEGAL          120
TECHNICAL      100
LEXICAL_STRICT 80
SEMANTIC       60
BALANCED       80
```

Multi-zone requests MUST reuse the existing access-zone resolver and limits. No REST-specific zone logic may change retrieval semantics.

## 4. Response parity

REST must expose the same retrieval outcome, not merely a similar search result.

For semantically equivalent REST and gRPC requests, compare at least:

```text
context count
context order
access_zone_id
document_id
document_version
source_block_id
matched_chunk_id
parent_chunk_id
matched_text
parent_text
dense_score
sparse_score
fusion_score
final_score
evidence status
degraded flag
degradation codes
Graph-derived context presence/provenance
```

REST MUST NOT recompute ranking, scores, coverage, Graph decisions, MMR decisions, token-budget decisions, or visibility.

## 5. Retrieval invariants remain frozen

FIX490 MUST NOT change:

```text
CanonicalTokenizer
BGE-M3/ONNX inference
chunking ownership inside AstraVector
SOURCE/PARENT/SUB_180/SUB_260 hierarchy
dense/sparse/lexical/hybrid retrieval
fusion/RRF tuning
no-answer policy
parent hydration
GraphRAG
MMR
token budget
final visibility recheck
access-zone/access-level filtering
document/version/lifecycle/TTL semantics
PostgreSQL canonical state
outbox/reconciliation
Qdrant projection
retry/deadline/backpressure/degradation semantics
frozen quality banks/qrels/thresholds
```

If REST requires one of these to change, stop with:

```text
FIX490_BLOCKED_BY_ARCHITECTURE_CHANGE
```

## 6. HTTP protocol

`POST /api/v1/retrieve` accepts JSON.

Required behavior:

```text
malformed JSON           -> 400
unsupported Content-Type -> 415
oversized request body   -> 413
```

Use a bounded body. Default target: `65536` bytes.

Stable error body:

```json
{
  "code": "RESOURCE_EXHAUSTED",
  "message": "...",
  "correlationId": "..."
}
```

Do not expose stack traces, SQL text, credentials, raw backend payloads, or configuration secrets.

## 7. Error mapping

REST maps the existing retrieval/runtime status deterministically:

```text
InvalidArgument / OutOfRange -> 400
Unauthenticated              -> 401  # only if propagated by an existing core path; REST adds no auth
PermissionDenied             -> 403  # retrieval/core semantics only
NotFound                     -> 404
AlreadyExists                -> 409
FailedPrecondition           -> 409
Aborted / OwnershipLost      -> 409
ResourceExhausted            -> 429
Cancelled                    -> 499 when a response is possible
Unavailable                  -> 503
DeadlineExceeded             -> 504
Internal/unclassified        -> 500
```

A successful degraded retrieval with valid surviving contexts remains HTTP `200`.
A total backend/deadline failure must not become `200` + empty semantic no-answer.

## 8. Correlation, deadlines and cancellation

REST propagates `X-Correlation-Id` when present and generates a UUID when absent.

REST must use the same configured query deadline and downstream bounded operation path as retrieval over gRPC.

```text
deadline exhausted  -> 504
backpressure        -> 429
backend unavailable -> 503
```

Global process shutdown is shared by gRPC and HTTP. Per-request cancellation should be preserved where supported without introducing a new cancellation subsystem.

## 9. Internal HTTP configuration

Minimal configuration may be implemented using the project configuration mechanism or environment-backed typed settings:

```text
ASTRAVECTOR_HTTP_ENABLED=true
ASTRAVECTOR_HTTP_HOST=0.0.0.0
ASTRAVECTOR_HTTP_PORT=8080
ASTRAVECTOR_HTTP_MAX_REQUEST_BODY_BYTES=65536
```

HTTP port must not collide with gRPC or metrics.

Default ports:

```text
HTTP    8080
gRPC    50051
metrics 9090
```

## 10. Probe semantics

No authentication is applied to the internal REST probes.

```text
GET /health
  200 when HTTP process is alive

GET /ready
  200 when shared Readiness=true
  503 when shared Readiness=false
```

`/ready` MUST use the existing `Readiness` object. Do not implement a second PostgreSQL/Qdrant readiness calculation.

## 11. Runtime lifecycle

HTTP and gRPC run in the same AstraVector process.

- normal shutdown drains/cancels both listeners;
- unexpected HTTP listener failure must not leave the process silently half-alive;
- gRPC termination must not leave HTTP indefinitely claiming readiness;
- no extra REST microservice or reverse proxy is introduced.

## 12. Minimal HTTP observability

Add bounded transport metrics only, for example:

```text
astravector_http_requests_total
astravector_http_request_duration_seconds
```

Allowed labels:

```text
route
method
status_class
```

Do not label by query, document, access zone, access level, or correlation id.

## 13. Required tests

Positive semantic parity:

```text
single-zone
multi-zone when enabled
BALANCED
TECHNICAL
SEMANTIC
LEXICAL_STRICT
Graph disabled
Graph enabled
no-answer
successful degradation
```

Retrieval visibility/lifecycle tests (not HTTP auth tests):

```text
PUBLIC vs INTERNAL/CONFIDENTIAL/RESTRICTED visibility
wrong/inactive/missing zone
multi-zone disabled
too many zones
invalid zone id
DELETED excluded
EXPIRED excluded
INDEXING/non-active excluded
```

HTTP boundary tests:

```text
malformed JSON -> 400
unsupported content type -> 415
oversized body -> 413
/health -> 200
/ready -> 200 or 503 from shared Readiness
correlation id propagation
status/error mapping
```

Failure/degradation parity where the existing harness supports it:

```text
Qdrant unavailable/timeout
PostgreSQL hydration timeout/failure
request deadline exhaustion
overload/backpressure
```

## 14. Smoke scope

Codex should execute:

```text
GET /health
GET /ready
POST /api/v1/retrieve
```

plus direct REST-vs-gRPC result comparison for the same retrieval inputs.

A short concurrency smoke at local concurrency 1 and 2 is sufficient for transport regression detection.

Do not rerun the full FIX489-R3 60-minute soak solely because REST was added unless core semantics changed.

## 15. Documentation

Add/update:

```text
docs/api/rest-api.md
docs/fix490/CURRENT_HEAD_STATUS.md
docs/fix490/RESULT.md
```

REST documentation must explicitly call the endpoint **internal-only** and state that AstraVector itself does not authenticate REST callers in FIX490.
