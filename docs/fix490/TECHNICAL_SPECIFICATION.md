# FIX490 — Internal REST Retrieval Boundary and Current-HEAD Readiness Synchronization

## 1. Goal

Add a minimal **internal REST transport** to AstraVector for retrieval and synchronize top-level documentation/evidence with the actual current branch state.

FIX490 is deliberately small:

```text
POST /api/v1/retrieve
GET  /health
GET  /ready
```

No new retrieval semantics are introduced.

## 2. Baseline

Initial FIX490 baseline:

```text
repository: alimbetov/llm2
base_branch: main
base_sha: 2a34b65fd24bde11e1fc01dd4ff86ee04a5cd42b
baseline_merge: PR #38 / FIX489-R3
```

The branch must record its final tested SHA and current main SHA after implementation/testing.

Inherited authoritative FIX489-R3 local evidence includes:

```text
FIX489_R3_LOCAL_STABLE_FLOOR_PASS
FIX489_R3_SOAK_60M_PASS
FIX489_R3_PASS
maximum_stable_concurrency=2
recommended_operating_concurrency=1
capacity_scope=LOCAL_MAC_CPU
production_capacity_claim=false
```

The 60-minute local soak completed 7381/7381 operations successfully with no recorded correctness/safety violations in the authoritative FIX489-R3 result files. FIX490 must preserve the `LOCAL_MAC_CPU` scope and must not convert this into a production-capacity claim.

## 3. Existing AstraVector core is authoritative

Current retrieval architecture remains:

```text
question
  -> canonical tokenizer / BGE-M3 query representation
  -> Dense / Sparse / Lexical retrieval
  -> fusion
  -> canonical PostgreSQL parent hydration
  -> Graph expansion
  -> MMR
  -> hard token budget
  -> final PostgreSQL visibility recheck
  -> evidence/degradation response
```

Current ingestion architecture remains:

```text
LogicalBlock input
  -> canonical tokenizer
  -> token-aware hierarchical chunking
  -> BGE-M3 representations
  -> PostgreSQL canonical state
  -> outbox
  -> Qdrant projection
```

FIX490 must not move tokenizer/chunking/embedding ownership out of AstraVector.

## 4. REST architectural boundary

REST is a second **internal transport adapter**, not a second retrieval engine.

Target:

```text
gRPC RetrieveContext ----\
                          -> existing Search/retrieval core
REST /api/v1/retrieve ---/
```

Forbidden:

```text
REST -> localhost/self gRPC
REST-specific Dense/Sparse/Hybrid implementation
REST-specific ranking/fusion/Graph/MMR
new REST microservice
```

The REST implementation may perform only transport DTO validation/mapping and response serialization around the existing retrieval core.

## 5. Internal-only decision

AstraVector REST is internal-only.

FIX490 adds **no REST authentication/authorization subsystem**:

```text
NO x-api-key requirement
NO JWT/OAuth
NO gateway trust token
NO REST roles
NO forwarded-identity authentication
NO REST auth middleware
```

Infrastructure outside AstraVector may restrict network access. That is not FIX490 scope.

`callerAccessLevel` is a retrieval visibility parameter used by the existing core. It is not caller authentication.

Access-zone selection is retrieval scope, not authentication.

Existing gRPC security behavior is not removed or redesigned by FIX490.

## 6. REST request contract

`POST /api/v1/retrieve` accepts a JSON projection of the existing retrieval facade inputs:

```json
{
  "question": "How does PostgreSQL recover missing Qdrant points?",
  "accessZoneId": "<uuid>",
  "accessZoneIds": [],
  "accessZoneCode": "",
  "accessZoneCodes": [],
  "callerAccessLevel": "INTERNAL",
  "profile": "TECHNICAL",
  "maxContexts": 8,
  "filters": [],
  "enableGraphExpansion": true,
  "graphMaxHops": 1,
  "graphMaxRelatedContexts": 0,
  "correlationId": "optional"
}
```

Defaults:

```text
callerAccessLevel = INTERNAL
profile           = BALANCED
maxContexts       = current RetrieveContext default
```

Allowed access-level values:

```text
PUBLIC
INTERNAL
CONFIDENTIAL
RESTRICTED
```

Allowed profiles:

```text
BALANCED
LEGAL
TECHNICAL
SEMANTIC
LEXICAL_STRICT
```

## 7. Required profile parity

REST must preserve current gRPC facade mapping exactly:

```text
Profile         Search mode   Embedding mode                 Candidate limit
BALANCED        HYBRID        DENSE_SPARSE_IF_AVAILABLE      80
LEGAL           HYBRID        DENSE_SPARSE_REQUIRED          120
TECHNICAL       HYBRID        DENSE_SPARSE_IF_AVAILABLE      100
SEMANTIC        DENSE         DENSE_ONLY                     60
LEXICAL_STRICT  SPARSE        DENSE_SPARSE_REQUIRED          80
```

`maxContexts`, Graph defaults/limits, filters, zone resolution, and multi-zone limits must follow the existing retrieval implementation.

## 8. REST response contract

REST returns the existing retrieval outcome serialized to JSON.

Required top-level information:

```text
contexts
summary
warnings
degradation
diagnostics
```

Each context must preserve at minimum:

```text
matchedText
parentText
documentId
documentVersion
sourceBlockId
matchedChunkId
parentChunkId
accessZoneId
citation
scores
metadata
```

Scores:

```text
denseScore
sparseScore
fusionScore
finalScore
```

Summary must preserve:

```text
totalCandidates
returnedContexts
profile
evidenceStatus
degraded
degradationCodes
dense/sparse/fusion execution flags and candidate counts
```

REST must not recompute ranking/scores/evidence/degradation independently.

## 9. Semantic parity requirement

For the same AstraVector state and equivalent inputs, gRPC `RetrieveContext` and REST `/api/v1/retrieve` must match on:

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
degraded flag/degradation codes
Graph-derived presence/provenance
```

Serialization naming/format differences are allowed; semantic differences are not.

## 10. Visibility/lifecycle parity

REST must preserve current retrieval behavior for:

```text
caller access level filtering
single-zone/multi-zone resolution
inactive/missing/invalid zones
ACTIVE visibility
DELETED exclusion
EXPIRED exclusion
INDEXING/non-active exclusion
final PostgreSQL visibility recheck
```

These are retrieval semantics, not REST security rules.

## 11. Frozen invariants

FIX490 MUST NOT change semantics/tuning of:

```text
CanonicalTokenizer
BGE-M3/ONNX
chunking ownership
SOURCE/PARENT/SUB_180/SUB_260
dense/sparse/lexical/hybrid retrieval
fusion/RRF weights and thresholds
no-answer policy
parent hydration
GraphRAG
MMR
hard token budget
final visibility recheck
access/version/lifecycle/TTL semantics
PostgreSQL canonical state
outbox/reconciliation
Qdrant projection
retry/deadline/backpressure/degradation
frozen quality banks/qrels/thresholds
```

If REST implementation requires changing one of these, stop and record:

```text
FIX490_BLOCKED_BY_ARCHITECTURE_CHANGE
```

## 12. HTTP protocol

Required behavior:

```text
valid JSON               -> normal retrieval result
malformed JSON           -> 400
unsupported Content-Type -> 415
oversized body           -> 413
empty/invalid input       -> 400
backpressure              -> 429
backend unavailable       -> 503
deadline exhausted       -> 504
```

Error body:

```json
{
  "code": "RESOURCE_EXHAUSTED",
  "message": "...",
  "correlationId": "..."
}
```

Do not return stack traces, SQL text, secrets, or raw backend payloads.

## 13. Correlation/deadline/degradation

Use `X-Correlation-Id` when supplied, otherwise generate a UUID.

REST must use the existing configured query deadline and same bounded downstream Search/retrieval operations.

A valid degraded retrieval with surviving contexts remains HTTP 200 and preserves degradation data.

A total retrieval-backend/deadline failure must not be converted to a semantic empty/no-answer 200 response.

## 14. HTTP configuration

Minimal internal HTTP configuration:

```text
ASTRAVECTOR_HTTP_ENABLED=true
ASTRAVECTOR_HTTP_HOST=0.0.0.0
ASTRAVECTOR_HTTP_PORT=8080
ASTRAVECTOR_HTTP_MAX_REQUEST_BODY_BYTES=65536
```

Default ports:

```text
HTTP    8080
gRPC    50051
metrics 9090
```

Port collisions must fail validation.

## 15. Health/readiness

```text
GET /health -> 200 while HTTP listener is alive
GET /ready  -> 200 when shared Readiness=true
GET /ready  -> 503 when shared Readiness=false
```

No REST authentication is applied to probes.

`/ready` must reuse the existing `Readiness` object and must not independently query/reimplement PostgreSQL/Qdrant/scheduler readiness.

## 16. Runtime lifecycle

HTTP and gRPC remain in the same process and share the existing shutdown token/lifecycle.

Requirements:

- HTTP listener can be disabled;
- unexpected HTTP listener failure must trigger/participate in global shutdown rather than silently leaving a partial runtime;
- normal shutdown terminates both listeners;
- no additional deployment unit is introduced.

## 17. Minimal observability

Add bounded HTTP transport metrics only, such as:

```text
astravector_http_requests_total
astravector_http_request_duration_seconds
```

Allowed dimensions:

```text
route
method
status_class
```

No query/document/access-zone/access-level/correlation values as metric labels.

## 18. Build/regression gates

Final branch must pass:

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

The REST dependency addition requires `Cargo.lock` to be current before these gates.

Existing tests/evidence must not be weakened.

## 19. Required smoke/parity tests

At minimum:

```text
GET /health
GET /ready
POST /api/v1/retrieve
REST vs gRPC BALANCED
REST vs gRPC TECHNICAL
REST vs gRPC SEMANTIC
REST vs gRPC LEXICAL_STRICT
Graph off/on
no-answer
PUBLIC vs INTERNAL visibility where fixture supports it
lifecycle exclusion where fixture supports it
```

Where harness permits:

```text
multi-zone
successful degraded response
Qdrant unavailable/timeout
PostgreSQL hydration timeout/failure
backpressure
deadline
```

A short local REST concurrency smoke at concurrency 1 and 2 is sufficient for FIX490 transport regression detection.

Do not rerun the full FIX489-R3 60-minute soak solely because the thin REST adapter was added, unless core semantics changed.

## 20. Documentation/evidence synchronization

Audit and reconcile current top-level documentation against authoritative later phase evidence, including FIX486 and FIX489-R3.

At minimum inspect/update as needed:

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

Do not rewrite historical evidence files.

Preserve exact scope distinctions:

```text
IMPLEMENTED
PROVEN_ON_CURRENT_OR_INHERITED_SHA
PROVEN_LOCAL_ONLY
STILL_OPEN
NOT_IN_SCOPE
```

Never infer PASS from implementation presence alone.

## 21. Production-readiness claims

FIX490 does not itself establish production readiness.

Keep local FIX489-R3 evidence labelled:

```text
capacity_scope=LOCAL_MAC_CPU
production_capacity_claim=false
```

No production-capacity or `PRODUCTION_READY` claim without separate evidence satisfying repository-defined gates.

## 22. Acceptance/result

Implementation result file:

```text
docs/fix490/RESULT.md
```

must record exact tested SHA, main SHA, commands, gate results, REST/gRPC parity results, visibility/lifecycle results, HTTP protocol results, and remaining blockers.

Allowed implementation verdicts:

```text
FIX490_REST_BOUNDARY_AND_READINESS_SYNC_PASS
FIX490_REST_BOUNDARY_AND_READINESS_SYNC_BLOCKED
```

Independent verification uses:

```text
docs/fix490/CODEX_REST_VERIFICATION_PROMPT.md
```

and writes:

```text
docs/fix490/REST_VERIFICATION_RESULT.md
```
