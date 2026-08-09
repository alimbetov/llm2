# FIX490 Codex Execution / Smoke Task — Internal REST

Repository/branch:

```text
alimbetov/llm2
agent/rest-boundary-readiness-sync
```

Read first:

```text
docs/fix490/TECHNICAL_SPECIFICATION.md
docs/fix490/REST_BOUNDARY_HARDENING_ADDENDUM.md
docs/fix490/ACCEPTANCE_CRITERIA.md
```

The addendum is normative and supersedes any old wording that treats REST as a public/security boundary.

## Objective

Complete and validate the already-started FIX490 implementation:

```text
POST /api/v1/retrieve
GET  /health
GET  /ready
```

The REST API is **internal-only**. Do not add API keys, JWT/OAuth, gateway trust, roles, forwarded-identity auth, or REST authorization middleware.

`callerAccessLevel` remains a retrieval visibility parameter only.

## Architecture rule

REST and gRPC must converge on the existing AstraVector Search/retrieval core.

```text
gRPC RetrieveContext ----\
                          -> existing Search / retrieval core
REST /api/v1/retrieve ---/
```

Forbidden:

```text
REST -> localhost/self gRPC
new REST-specific dense/sparse pipeline
REST-specific ranking/fusion/Graph/MMR logic
```

Review any mapping duplicated in `src/http.rs` against the current gRPC `RetrieveContext` implementation. If exact semantic parity can be improved with a small transport-neutral extraction without changing retrieval semantics, do so. Do not start a refactor campaign.

## Required REST contract

Request fields:

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
callerAccessLevel=INTERNAL
profile=BALANCED
maxContexts=existing facade default
```

Profile mappings and candidate limits must match `AstraVectorRetrievalFacade::RetrieveContext` exactly.

## Internal visibility semantics

Validate, but do not reinterpret:

```text
PUBLIC
INTERNAL
CONFIDENTIAL
RESTRICTED
```

These values control retrieval filtering only.

Also preserve existing:

```text
access-zone resolver
multi-zone limits
ACTIVE visibility
DELETED/EXPIRED/INDEXING exclusion
final PostgreSQL visibility recheck
```

There are **no REST authentication tests** in FIX490.

## HTTP behavior

Required:

```text
valid application/json -> normal retrieval
malformed JSON          -> 400
unsupported media type  -> 415
oversized body          -> 413
backpressure             -> 429
backend unavailable      -> 503
deadline                 -> 504
```

Stable errors:

```json
{
  "code": "...",
  "message": "...",
  "correlationId": "..."
}
```

`X-Correlation-Id` should be propagated when supplied; otherwise generate one.

## Probes

```text
GET /health -> 200 while HTTP listener is alive
GET /ready  -> 200 if shared Readiness=true
GET /ready  -> 503 if shared Readiness=false
```

No auth on probes. Do not create another readiness algorithm.

## Runtime/config

Current FIX490 implementation uses:

```text
ASTRAVECTOR_HTTP_ENABLED
ASTRAVECTOR_HTTP_HOST
ASTRAVECTOR_HTTP_PORT
ASTRAVECTOR_HTTP_MAX_REQUEST_BODY_BYTES
```

Default port: `8080`.

Validate no collision with:

```text
gRPC    50051
metrics 9090
```

HTTP and gRPC remain in one process and share shutdown/readiness.

## Critical build step

The branch added `axum`; therefore inspect `Cargo.lock` immediately.

If lockfile does not contain the dependency graph required by current `Cargo.toml`, regenerate/update `Cargo.lock` normally, review the lockfile diff, and commit it. Then all final gates must run with `--locked`.

Do not change unrelated dependency versions intentionally.

## Mandatory semantic parity smoke

For the same running AstraVector state, execute equivalent REST and gRPC retrievals and compare:

```text
context count
ordering
accessZoneId
documentId
documentVersion
sourceBlockId
matchedChunkId
parentChunkId
matchedText
parentText
denseScore
sparseScore
fusionScore
finalScore
evidenceStatus
degraded/degradationCodes
Graph provenance/presence when enabled
```

Cover at least:

```text
BALANCED
TECHNICAL
SEMANTIC
LEXICAL_STRICT
Graph off
Graph on
no-answer/insufficient
```

Where fixtures allow, cover multi-zone and a degraded-but-successful response.

## Visibility/lifecycle smoke

These are retrieval semantics, not authentication tests:

```text
callerAccessLevel PUBLIC vs INTERNAL
wrong/inactive/missing zone
multi-zone disabled / too many zones
DELETED excluded
EXPIRED excluded
INDEXING/non-active excluded
```

## Failure smoke

Where existing harness/failpoints support it:

```text
Qdrant unavailable/timeout
PostgreSQL hydration timeout/failure
query deadline
backpressure/resource exhaustion
```

A degraded result with valid contexts stays 200. Total backend failure does not become a semantic empty 200.

## Frozen invariants

Do not modify/tune:

```text
CanonicalTokenizer
BGE-M3/ONNX
chunking ownership
SOURCE/PARENT/SUB_180/SUB_260
dense/sparse/lexical/hybrid
RRF/fusion weights/thresholds
no-answer
parent hydration
GraphRAG
MMR
token budget
final visibility
access/version/lifecycle/TTL semantics
PostgreSQL canonical state
outbox/reconciliation
Qdrant projection
retry/backpressure/degradation
frozen qrels/banks/thresholds
```

If a REST fix requires changing one of these, stop and report:

```text
FIX490_BLOCKED_BY_ARCHITECTURE_CHANGE
```

## Gates

Run:

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

Do not weaken existing tests/evidence.

## Curl smoke

At minimum execute and record:

```bash
curl -i http://127.0.0.1:8080/health
curl -i http://127.0.0.1:8080/ready
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

Use actual repository fixtures/environment values rather than inventing production data.

Run a short transport concurrency smoke at 1 and 2 if practical. This is not capacity evidence.

## Documentation/evidence

Add/finalize:

```text
docs/api/rest-api.md
docs/fix490/CURRENT_HEAD_STATUS.md
docs/fix490/RESULT.md
```

Reconcile stale top-level status against authoritative later FIX486/FIX489 evidence without rewriting historical evidence files.

Keep:

```text
capacity_scope=LOCAL_MAC_CPU
production_capacity_claim=false
maximum_stable_concurrency=2
recommended_operating_concurrency=1
```

Do not claim PRODUCTION_READY without independent evidence for every remaining gate.

## Final output

Record:

```text
base_sha
implementation_sha
tested_sha
current_main_sha
Cargo.lock update status
commands executed
REST/gRPC parity results
visibility/lifecycle results
HTTP protocol results
failure/degradation results
documentation sync status
```

Allowed implementation verdicts:

```text
FIX490_REST_BOUNDARY_AND_READINESS_SYNC_PASS
FIX490_REST_BOUNDARY_AND_READINESS_SYNC_BLOCKED
```

Then execute the independent review instructions in:

```text
docs/fix490/CODEX_REST_VERIFICATION_PROMPT.md
```
