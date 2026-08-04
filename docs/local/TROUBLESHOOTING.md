# Local Demo Troubleshooting

Each entry gives a symptom, diagnostic command and fix.

## Docker daemon unavailable

Symptom: `docker compose up` fails.

Diagnostic:

```bash
docker version
```

Fix: start Docker Desktop or the Linux Docker service.

## Port Already In Use

Symptom: PostgreSQL, Qdrant, gRPC or metrics cannot bind.

Diagnostic:

```bash
lsof -nP -iTCP:55432 -sTCP:LISTEN
lsof -nP -iTCP:6333 -sTCP:LISTEN
lsof -nP -iTCP:50051 -sTCP:LISTEN
lsof -nP -iTCP:9090 -sTCP:LISTEN
```

Fix: stop the conflicting service or change the local-demo profile.

## PostgreSQL Authentication Failed

Diagnostic:

```bash
PGPASSWORD=astravector psql -h 127.0.0.1 -p 55432 -U astravector -d astravector -c 'SELECT 1;'
```

Fix: recreate the local demo volume with `make local-demo-reset`, or correct `DATABASE_URL`.

## Migration Failed

Diagnostic:

```bash
DATABASE_URL='postgres://astravector:astravector@127.0.0.1:55432/astravector' cargo sqlx migrate run
```

Fix: inspect the first SQL error. Do not continue to ingestion while migrations are failed.

## Qdrant Unavailable

Diagnostic:

```bash
curl -sS http://127.0.0.1:6333/collections | jq .
```

Fix: start Qdrant with `make local-demo-infra-up`.

## Model Or Tokenizer Missing

Diagnostic:

```bash
scripts/local-demo/check-model.sh
```

Fix: set `ASTRAVECTOR_MODEL_PATH` and `ASTRAVECTOR_TOKENIZER_PATH` in `.env.local-demo`.

## Wrong ONNX Output Name

Symptom: runtime startup log contains provider initialization or self-test errors.

Diagnostic:

```bash
tail -120 .local-demo/runtime.log
```

Fix: verify `ASTRAVECTOR_DENSE_OUTPUT_NAME`, `ASTRAVECTOR_TOKEN_OUTPUT_NAME` and `ASTRAVECTOR_SPARSE_OUTPUT_NAME` against the model artifact.

## Dense Dimension Mismatch

Symptom: Qdrant collection validation fails.

Diagnostic:

```bash
curl -sS http://127.0.0.1:6333/collections/astravector_local_demo | jq .
```

Fix: reset the local demo collection after confirming the runtime dense dimension is 1024.

## Sparse Output Unavailable

FIX488 uses dense-only indexing/search for the canonical tutorial. Sparse/hybrid capabilities are validated by separate quality profiles.

## Runtime Readiness False

Diagnostic:

```bash
grpcurl -plaintext 127.0.0.1:50051 grpc.health.v1.Health/Check
tail -120 .local-demo/runtime.log
```

Fix: ensure PostgreSQL and Qdrant are reachable and model self-test passed.

## grpcurl Connection Refused

Diagnostic:

```bash
cat .local-demo/runtime.pid
ps -p "$(cat .local-demo/runtime.pid)"
```

Fix: run `make local-demo-runtime-start` and inspect `.local-demo/runtime.log`.

## Reflection Disabled

FIX488 expects reflection enabled. Fallback:

```bash
grpcurl -plaintext -import-path proto -proto astravector_embedding.proto 127.0.0.1:50051 list
```

## Invalid UUID

The demo helper creates deterministic UUIDv5 values. If a hand-written request fails, check every `documentId`, `accessZoneId` and chunk ID.

## Access Zone Not Registered

FIX488 uses access-zone code `0488` and sets `ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true`. Search does not auto-create zones.

## Outbox Pending Or Dead Letter

Diagnostic:

```bash
scripts/local-demo/wait-vector-sync.sh
scripts/local-demo/inspect-postgres.sh
```

Fix: inspect `vector_outbox` status and Qdrant logs.

## Activation Rejected

Diagnostic:

```bash
scripts/local-demo/wait-vector-sync.sh
scripts/local-demo/inspect-postgres.sh
```

Fix: activation is allowed only after chunks, embeddings, bindings, completed outbox and Qdrant points exist.

## Search Returns Zero Results

Diagnostic:

```bash
scripts/local-demo/search.sh 'ASTRAVECTOR_LOCAL_DEMO_2026'
scripts/local-demo/inspect-qdrant.sh
```

Fix: confirm document is ACTIVE, access zone matches and Qdrant points exist.

## Wrong Access Level

The sample is indexed as `PUBLIC`; search uses `callerAccessLevel=PUBLIC`.

## Document Not Active

Run:

```bash
scripts/local-demo/activate-document.sh
```

## Qdrant Collection Schema Mismatch

Reset local demo after confirming no important data is stored in local volumes:

```bash
make local-demo-reset
```

## Mac Apple Silicon ONNX Runtime Issue

Use the CPU provider first. Check `.local-demo/runtime.log` for provider self-test failures.

