# Codex Prompt — Verify FIX490 Internal REST API

Use this prompt after pulling:

```text
repository: alimbetov/llm2
branch: agent/rest-boundary-readiness-sync
```

This is a **verification-first** task. Read completely:

```text
docs/fix490/TECHNICAL_SPECIFICATION.md
docs/fix490/REST_BOUNDARY_HARDENING_ADDENDUM.md
docs/fix490/ACCEPTANCE_CRITERIA.md
docs/fix490/CODEX_EXECUTION_TASK.md
```

The REST API is internal-only. **Do not add or test REST authentication**. No API key, JWT/OAuth, gateway trust, role, or forwarded-identity mechanism belongs to FIX490 REST.

`callerAccessLevel` and access-zone values are retrieval visibility/scope inputs, not authentication.

## Mission

Prove whether the current branch closes the internal REST retrieval boundary without changing existing AstraVector retrieval semantics.

Authoritative rule:

```text
REST must reach the same existing Search/retrieval core and produce the same retrieval outcome as gRPC RetrieveContext for equivalent inputs.
```

Forbidden architecture:

```text
REST -> localhost/self gRPC
REST-specific dense/sparse retrieval
REST-specific ranking/fusion/Graph/MMR
```

## 1. Baseline and diff

Run:

```bash
git status --short
git branch --show-current
git rev-parse HEAD
git rev-parse main
git merge-base main HEAD
git diff --stat main...HEAD
git diff --name-status main...HEAD
```

Record exact SHAs.

Inspect every code/config/test/doc change. Flag any unrelated change to tokenizer, embeddings, chunking, retrieval ranking, GraphRAG, MMR, PostgreSQL/Qdrant consistency, lifecycle, qrels, or thresholds.

## 2. Build/lockfile integrity

Inspect `Cargo.toml` and `Cargo.lock`.

The implementation added Axum. If `Cargo.lock` is stale, update it first with the minimal normal Cargo operation, inspect the lock diff, and retain only dependency-resolution changes required by current `Cargo.toml`.

Then run final gates with `--locked`:

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

Do not report PASS if any locked gate fails.

## 3. Static architecture review

Inspect:

```text
src/http.rs
src/main.rs
src/grpc/mod.rs
src/lib.rs
Cargo.toml
config/application.yaml
proto/astravector_embedding.proto
```

Prove:

```text
[ ] HTTP and gRPC are in one process
[ ] REST does not self-call gRPC
[ ] REST uses existing Search/retrieval implementation
[ ] no new ranking or retrieval algorithm exists in HTTP code
[ ] no REST auth middleware exists
[ ] existing gRPC security configuration was not changed by FIX490
[ ] shared Readiness is used by /ready
[ ] global shutdown is shared
```

Pay special attention to any mapping duplicated between `src/http.rs` and `AstraVectorRetrievalFacade::retrieve_context`:

```text
profile -> search mode
profile -> embedding mode
profile -> candidate limit
maxContexts default/cap
graph default/max
filters
access zones
caller access level
response evidence/degradation assembly
```

If duplication has drifted, report FAIL. A minimal common extraction is allowed only if it changes no retrieval semantics.

## 4. Endpoint/protocol smoke

Start AstraVector using the repository's normal local/test environment.

Verify:

```bash
curl -i http://127.0.0.1:8080/health
curl -i http://127.0.0.1:8080/ready
```

Expected:

```text
/health -> 200 while HTTP listener is alive
/ready  -> 200 iff shared Readiness=true, otherwise 503
```

Test `/api/v1/retrieve`:

```bash
curl -i -X POST http://127.0.0.1:8080/api/v1/retrieve \
  -H 'Content-Type: application/json' \
  -H 'X-Correlation-Id: fix490-rest-smoke' \
  -d '{
    "question":"<known fixture question>",
    "accessZoneId":"<fixture zone UUID>",
    "callerAccessLevel":"INTERNAL",
    "profile":"TECHNICAL",
    "maxContexts":5,
    "enableGraphExpansion":true
  }'
```

Use actual fixture values.

Protocol negatives:

```text
malformed JSON             -> 400
unsupported Content-Type   -> 415
body over configured limit -> 413
empty question             -> 400
bad callerAccessLevel      -> 400
bad profile                -> 400
```

Check stable error JSON:

```text
code
message
correlationId
```

No stack/SQL/secrets/raw backend payloads.

## 5. Direct REST-vs-gRPC parity

For each case below, send semantically equivalent requests to:

```text
gRPC AstraVectorRetrievalFacade/RetrieveContext
REST POST /api/v1/retrieve
```

At minimum:

```text
BALANCED
TECHNICAL
SEMANTIC
LEXICAL_STRICT
Graph disabled
Graph enabled
no-answer/insufficient
```

If current fixtures support them, also test:

```text
LEGAL
multi-zone
successful degraded retrieval
```

Compare normalized results field by field:

```text
context count
context ordering
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
degraded
degradationCodes
Graph-derived presence/provenance
```

Serialization formatting differences are acceptable. Semantic differences are not.

## 6. Visibility and lifecycle semantics

These are **not auth tests**.

Using existing fixtures/harness, verify retrieval behavior for:

```text
callerAccessLevel=PUBLIC
callerAccessLevel=INTERNAL
callerAccessLevel=CONFIDENTIAL
callerAccessLevel=RESTRICTED
wrong/inactive/missing zone
multi-zone disabled
too many zones
invalid zone
DELETED
EXPIRED
INDEXING/non-active
```

REST must preserve the same filtering as the existing Search/gRPC path.

## 7. Retrieval/degradation failures

Where supported by existing failpoints or test environment:

```text
Qdrant unavailable/timeout
PostgreSQL hydration timeout/failure
deadline exhaustion
backpressure/resource exhaustion
```

Expected transport mapping:

```text
ResourceExhausted -> 429
Unavailable       -> 503
DeadlineExceeded  -> 504
```

A valid degraded response with surviving contexts remains HTTP 200 and keeps degradation data.

A total backend failure must not be converted to empty/no-answer HTTP 200.

## 8. Core invariant diff audit

Confirm FIX490 did not change semantics/tuning of:

```text
CanonicalTokenizer
BGE-M3/ONNX
SOURCE/PARENT/SUB_180/SUB_260
dense/sparse/lexical/hybrid
fusion/RRF
no-answer
parent hydration
GraphRAG
MMR
token budget
final visibility
access/version/lifecycle/TTL
PostgreSQL canonical state
outbox/reconciliation
Qdrant projection
retry/backpressure/degradation
frozen qrels/banks/thresholds
```

If any changed without a separately proven requirement:

```text
FIX490_REST_VERIFICATION_FAIL
```

## 9. Runtime lifecycle

Verify:

```text
HTTP port defaults to 8080
HTTP/gRPC/metrics port collision is rejected
ASTRAVECTOR_HTTP_ENABLED=false suppresses HTTP listener
Ctrl-C/global shutdown stops both listeners
HTTP listener failure does not leave a silently healthy half-process
```

## 10. Minimal observability

Verify HTTP metrics are bounded by route/method/status class only.

Reject high-cardinality labels such as query text, document id, access zone, access level, or correlation id.

## 11. Short concurrency smoke

Run a short internal REST retrieval smoke at concurrency 1 and 2 with known valid fixture queries.

This checks transport regressions only. Do not produce a capacity claim and do not rerun the FIX489-R3 60-minute soak solely for FIX490.

## 12. Documentation/evidence

Verify/add if implementation is otherwise correct:

```text
docs/api/rest-api.md
docs/fix490/CURRENT_HEAD_STATUS.md
docs/fix490/RESULT.md
```

Reconcile top-level docs only against authoritative evidence.

Preserve:

```text
capacity_scope=LOCAL_MAC_CPU
production_capacity_claim=false
maximum_stable_concurrency=2
recommended_operating_concurrency=1
FIX489_R3_SOAK_60M_PASS
```

Do not claim `PRODUCTION_READY` from implementation presence or local capacity evidence.

## 13. Verification report

Create:

```text
docs/fix490/REST_VERIFICATION_RESULT.md
```

Record:

```text
branch
base/main/tested SHA
Cargo.lock status
all commands and exit statuses
endpoint smoke results
REST-vs-gRPC normalized comparisons
visibility/lifecycle results
failure/degradation results
core-invariant diff audit
remaining blockers
```

Use exactly one verdict:

```text
FIX490_REST_VERIFICATION_PASS
FIX490_REST_VERIFICATION_FAIL
FIX490_REST_VERIFICATION_BLOCKED
```

Do not make broad code changes during verification. If a concrete FIX490-local defect is found, fix only that defect, add/adjust a focused test, rerun affected checks, and document the change. If correction requires changing a frozen retrieval invariant, stop with `FIX490_REST_VERIFICATION_BLOCKED`.
