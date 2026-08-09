# AstraVector Internal REST API — FIX490

## Purpose

This REST API is an **internal transport boundary** for AstraVector retrieval.

It does not replace gRPC ingestion or administration and it does not implement a separate retrieval engine.

```text
gRPC RetrieveContext ----\
                          -> existing AstraVector Search/retrieval core
REST /api/v1/retrieve ---/
```

AstraVector does **not** authenticate REST callers in FIX490. Network access control belongs to deployment infrastructure, not this API implementation.

`callerAccessLevel` is a retrieval visibility parameter, not authentication.

## Configuration

Environment variables:

```text
ASTRAVECTOR_HTTP_ENABLED=true
ASTRAVECTOR_HTTP_HOST=0.0.0.0
ASTRAVECTOR_HTTP_PORT=8080
ASTRAVECTOR_HTTP_MAX_REQUEST_BODY_BYTES=65536
```

Default ports:

```text
REST    8080
gRPC    50051
metrics 9090
```

REST port collision with gRPC or metrics is rejected during startup.

## POST /api/v1/retrieve

Content type:

```text
application/json
```

Example:

```json
{
  "question": "How does PostgreSQL recover missing Qdrant points?",
  "accessZoneId": "11111111-1111-1111-1111-111111111111",
  "callerAccessLevel": "INTERNAL",
  "profile": "TECHNICAL",
  "maxContexts": 5,
  "enableGraphExpansion": true
}
```

Supported request fields:

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
maxContexts       = 5
```

Access levels:

```text
PUBLIC
INTERNAL
CONFIDENTIAL
RESTRICTED
```

Profiles:

```text
BALANCED
LEGAL
TECHNICAL
SEMANTIC
LEXICAL_STRICT
```

The profile mapping is intentionally identical to the existing gRPC retrieval facade.

## Response

Successful response includes:

```text
summary
contexts
warnings
degradation
diagnostics
```

Context identity/text/score fields are derived from the same existing Search pipeline used by gRPC retrieval.

Typical shape:

```json
{
  "summary": {
    "totalCandidates": 12,
    "returnedContexts": 5,
    "profile": "TECHNICAL",
    "evidenceStatus": "FOUND",
    "degraded": false,
    "degradationCodes": []
  },
  "contexts": [
    {
      "matchedText": "...",
      "parentText": "...",
      "documentId": "...",
      "documentVersion": 1,
      "sourceBlockId": "...",
      "matchedChunkId": "...",
      "parentChunkId": "...",
      "accessZoneId": "...",
      "scores": {
        "denseScore": 0.0,
        "sparseScore": 0.0,
        "fusionScore": 0.0,
        "finalScore": 0.0
      },
      "metadata": {}
    }
  ],
  "warnings": [],
  "degradation": null,
  "diagnostics": {}
}
```

A degraded retrieval with valid surviving contexts remains HTTP `200` and reports degradation explicitly.

## Error mapping

```text
invalid argument / out of range -> 400
not found                       -> 404
failed precondition/conflict    -> 409
resource exhausted              -> 429
cancelled                       -> 499 when response is possible
unavailable                     -> 503
deadline exceeded               -> 504
internal/unclassified           -> 500
```

If an existing core path propagates `Unauthenticated` or `PermissionDenied`, they map deterministically to `401`/`403`; FIX490 REST itself adds no authentication policy.

Error body:

```json
{
  "code": "RESOURCE_EXHAUSTED",
  "message": "...",
  "correlationId": "..."
}
```

## Protocol errors

```text
malformed JSON           -> 400
unsupported Content-Type -> 415
oversized request body   -> 413
```

## Correlation

Optional header:

```text
X-Correlation-Id: <value>
```

If neither header nor `correlationId` body field is supplied, AstraVector generates a UUID.

## Health

```http
GET /health
```

Returns `200` while the internal HTTP listener is alive.

## Readiness

```http
GET /ready
```

Uses the same shared AstraVector `Readiness` state as the existing runtime:

```text
ready=true  -> 200
ready=false -> 503
```

The REST layer does not implement its own PostgreSQL/Qdrant readiness algorithm.

## Curl smoke

```bash
curl -i http://127.0.0.1:8080/health
curl -i http://127.0.0.1:8080/ready
```

Retrieval example:

```bash
curl -i -X POST http://127.0.0.1:8080/api/v1/retrieve \
  -H 'Content-Type: application/json' \
  -H 'X-Correlation-Id: fix490-smoke' \
  -d '{
    "question":"<known fixture question>",
    "accessZoneId":"<fixture zone UUID>",
    "callerAccessLevel":"INTERNAL",
    "profile":"TECHNICAL",
    "maxContexts":5,
    "enableGraphExpansion":true
  }'
```

Use repository fixture values for actual smoke/parity verification.

## Non-goals

FIX490 does not add REST endpoints for:

```text
ingestion
chunking
embedding preview
document lifecycle
outbox/reconciliation
Qdrant administration
Graph administration
runtime/model management
```

Those remain existing gRPC/internal boundaries.
