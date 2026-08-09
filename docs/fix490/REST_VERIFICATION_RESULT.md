# FIX490 REST Verification Result

## Identity

- Repository: `alimbetov/llm2`
- Local path: `/Users/ruslanalimbetov/Documents/llm2/astravector`
- Branch: `agent/rest-boundary-readiness-sync`
- Initial tested HEAD: `fd0dfab4c2f30de74ff39cf85aa39b85c0b16138`
- Base reference used for review: `origin/main`
- Scope reviewed: `Cargo.toml`, `Cargo.lock`, `src/http.rs`, `src/main.rs`, `src/lib.rs`, `src/grpc/mod.rs`, `config/application.yaml`, `proto/astravector_embedding.proto`

## Architecture Review

REST is implemented as an internal-only transport boundary:

- `POST /api/v1/retrieve` is implemented in `src/http.rs`.
- The HTTP server is started in the same Rust process as gRPC from `src/main.rs`.
- REST holds the same in-process `AstraVectorV004ControlService`.
- No localhost gRPC client, `Channel`, REST-specific ranking, REST-specific Graph, REST-specific MMR, JWT, x-api-key, or gateway-auth trust path was found in `src/http.rs`.
- `callerAccessLevel` remains a retrieval visibility input, not an authentication credential.
- `/health` and `/ready` share the process readiness state.
- `ASTRAVECTOR_HTTP_ENABLED=false` suppresses the HTTP listener while gRPC remains available.

One parity hardening was applied:

- REST `/api/v1/retrieve` now marks the internal search request with `RetrievalEntryPoint("RetrieveContext")`.
- This aligns hydration failpoints, rejection metrics, and diagnostics with the gRPC `RetrieveContext` facade.
- Ranking, Graph, RRF, MMR, query thresholds, frozen banks, qrels, and auth semantics were not changed.

## Cargo Lock

`cargo check --locked --all-targets --all-features` initially failed because `Cargo.lock` was stale after adding direct `axum` usage:

```text
error: cannot update the lock file ... Cargo.lock because --locked was passed to prevent this
```

The lockfile was synchronized by running non-locked Cargo once. The only new package entry was:

```text
serde_path_to_error v0.1.20
```

After synchronization, `--locked` gates passed.

## Static Gates

Final commands executed on the updated worktree:

```text
cargo fmt --all --check
PASS

cargo check --locked --all-targets --all-features
PASS

cargo clippy --locked --all-targets --all-features -- -D warnings
PASS

cargo test --locked --all-targets --all-features
PASS
```

Observed full-test evidence included:

```text
src/lib.rs unit tests: 173 passed
e2e_testcontainers: 2 passed
smoke_load_retrieve_context_testcontainers: 1 passed
quality_bench_runtime_quick: 11 passed
fix486g_graph_parent_contracts: 24 passed
fix486g_runner_hardening_contracts: 13 passed
```

## Runtime Smoke

Runtime was built and started with the real local model:

```text
cargo build --release --locked
PASS

model_path=/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/onnx/model.onnx
model_sha256=f84251230831afb359ab26d9fd37d5936d4d9bb5d1d5410e66442f630f24435b

tokenizer_path=/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/onnx/tokenizer.json
tokenizer_sha256=6710678b12670bc442b99edc952c4d996ae309a7020c1fa0096dd245c2faf790
```

gRPC reflection:

```text
astravector.embedding.v1.AstraVectorAdminFacade
astravector.embedding.v1.AstraVectorIngestionFacade
astravector.embedding.v1.AstraVectorRetrievalFacade
astravector.embedding.v1.AstraVectorRuntime
astravector.embedding.v1.AstraVectorV004Control
grpc.health.v1.Health
grpc.reflection.v1.ServerReflection
```

HTTP health/readiness:

```text
GET /health -> 200 {"status":"SERVING"}
GET /ready  -> 200 {"ready":true,"status":"READY"}
```

REST retrieval smoke:

```text
POST /api/v1/retrieve
question="Где AstraVector хранит каноническое состояние?"
accessZoneCode="0488"
callerAccessLevel="PUBLIC"
profile="SEMANTIC"
enableGraphExpansion=false

HTTP 200
summary.evidenceStatus="FOUND"
summary.returnedContexts=1
documentId="175f2d7c-a5b8-573b-903f-f64eaaea903c"
accessZoneId="b4ec78f9-70c3-5264-8b75-1b85f1905e44"
matchedChunkId="47e5cf3c-16a3-5894-8997-697b78c599af"
parentChunkId="47e5cf3c-16a3-5894-8997-697b78c599af"
sourceUri="/Users/ruslanalimbetov/Documents/llm2/astravector/examples/local-demo/sample-ru.txt"
```

Returned original PostgreSQL parent text:

```text
AstraVector хранит каноническое состояние документов в PostgreSQL. В PostgreSQL находятся версии документов, исходные chunks, bindings, lifecycle status и transactional outbox. Это источник истины, который можно проверять обычными SQL-запросами.
```

gRPC `RetrieveContext` parity smoke with the same query returned the same:

```text
summary.evidenceStatus="EVIDENCE_STATUS_FOUND"
summary.returnedContexts=1
documentId="175f2d7c-a5b8-573b-903f-f64eaaea903c"
accessZoneId="b4ec78f9-70c3-5264-8b75-1b85f1905e44"
matchedChunkId="47e5cf3c-16a3-5894-8997-697b78c599af"
parentChunkId="47e5cf3c-16a3-5894-8997-697b78c599af"
```

## HTTP Protocol Checks

```text
malformed JSON              -> 400 INVALID_JSON
unsupported Content-Type    -> 415 UNSUPPORTED_MEDIA_TYPE
empty question              -> 400 INVALID_ARGUMENT
configured body too large   -> 413 PAYLOAD_TOO_LARGE
```

The 413 path was verified by restarting the runtime with:

```text
ASTRAVECTOR_HTTP_MAX_REQUEST_BODY_BYTES=64
```

and sending a normal JSON body larger than that limit.

## Listener Lifecycle

Enabled:

```text
ASTRAVECTOR_HTTP_ENABLED=true
HTTP listener: 0.0.0.0:8080
gRPC listener: 0.0.0.0:50051
```

Disabled:

```text
ASTRAVECTOR_HTTP_ENABLED=false
gRPC reflection: PASS
HTTP 8080 listener: absent
curl /health: connection refused
```

## Concurrency Smoke

Two concurrent REST calls were executed against `/api/v1/retrieve` using separate `curl` processes.

```text
request 1: status=200 time_total=0.451162
request 2: status=200 time_total=0.399936
```

Response summaries:

```text
request 1: evidenceStatus="FOUND", returnedContexts=1, documentId="175f2d7c-a5b8-573b-903f-f64eaaea903c"
request 2: evidenceStatus="FOUND", returnedContexts=1, documentId="175f2d7c-a5b8-573b-903f-f64eaaea903c"
```

## Files Changed

```text
Cargo.lock
src/grpc/mod.rs
src/http.rs
docs/fix490/REST_VERIFICATION_RESULT.md
```

## Residual Risk

- This is a local MacBook runtime verification, not a production environment proof.
- REST parity was smoke-verified for the local-demo semantic retrieval path; it was not a full FIX489/FIX486 campaign rerun.
- Security remains intentionally out of scope for FIX490 because the REST boundary is internal-only.

## Verdict

```text
FIX490_REST_VERIFICATION_PASS
```
