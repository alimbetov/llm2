# AstraVector Mac Micro Smoke Runbook

This runbook executes the narrow Mac smoke for:

```text
registry.astrabase.asia/astravector:sha-1cb6065
```

Expected digest:

```text
sha256:b0567810b5ea3df752ff8ba559fcf16bc46b245878e798b8888dcf93426ee6ad
```

Do not paste passwords into shell commands. Enter the `astra-reader` password only at interactive prompts.

## 1. Preflight

```bash
cd /Users/ruslanalimbetov/Documents/llm2/astravector
git checkout agent/astravector-image-contract
git pull --ff-only

df -h /
docker system df
uname -m
docker info --format '{{.Architecture}}'
```

Continue only if the host is Apple Silicon (`arm64`) and Docker reports arm64/aarch64. Keep at least 12 GiB free before the model download; 15 GiB or more is preferable because the bootstrap uses temporary partial downloads.

Safe cleanup, if needed:

```bash
docker container prune -f
docker image prune -f
docker builder prune -f
docker system df
```

More aggressive cleanup only with explicit operator approval:

```bash
docker image prune -a -f
docker builder prune -a -f
docker system df
```

Do not run `docker volume prune`.

## 2. Registry Login And Exact Pull

```bash
docker logout registry.astrabase.asia 2>/dev/null || true
docker login registry.astrabase.asia -u astra-reader
docker pull registry.astrabase.asia/astravector:sha-1cb6065
docker image inspect registry.astrabase.asia/astravector:sha-1cb6065 --format 'ID={{.Id}} ARCH={{.Architecture}} OS={{.Os}} DIGESTS={{json .RepoDigests}}'
```

Required:

```text
ARCH=arm64
registry.astrabase.asia/astravector@sha256:b0567810b5ea3df752ff8ba559fcf16bc46b245878e798b8888dcf93426ee6ad
```

Stop with `ASTRAVECTOR_MAC_MICRO_SMOKE_BLOCKED` if a different digest is pulled, if the image is not arm64, or if Docker uses emulation.

## 3. Disposable Dependencies

```bash
docker network create astravector-smoke 2>/dev/null || true

docker run -d --name astravector-smoke-postgres \
  --network astravector-smoke --network-alias postgres \
  -e POSTGRES_USER=astravector \
  -e POSTGRES_PASSWORD=astravector \
  -e POSTGRES_DB=astravector \
  pgvector/pgvector:pg16

docker run -d --name astravector-smoke-qdrant \
  --network astravector-smoke --network-alias qdrant \
  qdrant/qdrant:v1.14.1

docker run --rm --network astravector-smoke postgres:16-alpine \
  sh -c 'until pg_isready -h postgres -U astravector -d astravector; do sleep 1; done'

docker run --rm --network astravector-smoke curlimages/curl:8.10.1 \
  sh -c 'until curl -fsS http://qdrant:6333/collections >/dev/null; do sleep 1; done'
```

## 4. Empty Model Cache

```bash
docker volume create astravector-bge-m3-cache
docker run --rm -v astravector-bge-m3-cache:/models:ro alpine:3.22 sh -c 'find /models -mindepth 1 -maxdepth 1 -print'
```

The `find` command must print nothing before the first start.

## 5. First AstraVector Start

```bash
read -s -p "Astra reader password: " ASTRAVECTOR_NEXUS_PASSWORD
echo
export ASTRAVECTOR_NEXUS_PASSWORD

docker run -d --name astravector-smoke-runtime \
  --network astravector-smoke --network-alias astravector \
  -p 127.0.0.1:50051:50051 \
  -p 127.0.0.1:9090:9090 \
  -v astravector-bge-m3-cache:/models/bge-m3 \
  -e ASTRAVECTOR_DB_URL=postgres://astravector:astravector@postgres:5432/astravector \
  -e DATABASE_URL=postgres://astravector:astravector@postgres:5432/astravector \
  -e ASTRAVECTOR_QDRANT_URL=http://qdrant:6333 \
  -e ASTRAVECTOR_QDRANT_COLLECTION=astravector_v004 \
  -e ASTRAVECTOR_SPARSE_REQUIRED=false \
  -e ASTRAVECTOR_SPARSE_REQUIRED=false \
  -e ASTRAVECTOR_NEXUS_USERNAME=astra-reader \
  -e ASTRAVECTOR_NEXUS_PASSWORD \
  -e ASTRAVECTOR_MODEL_REPOSITORY_URL=https://nexus.astrabase.asia/repository/astra-models/astravector/bge-m3/baseline-v1 \
  -e ASTRAVECTOR_MODEL_DIR=/models/bge-m3 \
  -e ASTRAVECTOR_MODEL_PATH=/models/bge-m3/model.onnx \
  -e ASTRAVECTOR_TOKENIZER_PATH=/models/bge-m3/tokenizer.json \
  -e ASTRAVECTOR_MODEL_SHA256=f84251230831afb359ab26d9fd37d5936d4d9bb5d1d5410e66442f630f24435b \
  -e ASTRAVECTOR_MODEL_DATA_SHA256=1eebfb28493f67bba03ce0ef64bfdc7fc5a3bd9d7493f818bb1d78cd798416b4 \
  -e ASTRAVECTOR_TOKENIZER_SHA256=21106b6d7dab2952c1d496fb21d5dc9db75c28ed361a05f5020bbba27810dd08 \
  -e ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true \
  -e ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_SEARCH=false \
  -e RUST_LOG=info \
  registry.astrabase.asia/astravector:sha-1cb6065

unset ASTRAVECTOR_NEXUS_PASSWORD
```

Observe startup without exposing secrets:

```bash
docker logs -f astravector-smoke-runtime
```

Do not treat `model and dependency bootstrap complete` as readiness. It only proves bootstrap completion.

## 6. Model SHA256 From The Volume

```bash
docker run --rm -v astravector-bge-m3-cache:/models:ro alpine:3.22 \
  sh -c 'sha256sum /models/model.onnx /models/model.onnx_data /models/tokenizer.json'
```

Required values:

```text
f84251230831afb359ab26d9fd37d5936d4d9bb5d1d5410e66442f630f24435b  /models/model.onnx
1eebfb28493f67bba03ce0ef64bfdc7fc5a3bd9d7493f818bb1d78cd798416b4  /models/model.onnx_data
21106b6d7dab2952c1d496fb21d5dc9db75c28ed361a05f5020bbba27810dd08  /models/tokenizer.json
```

## 7. gRPC Health

If `grpcurl` is installed on the Mac:

```bash
grpcurl -plaintext -d '{"service":"astravector.embedding.v1.AstraVectorRuntime"}' \
  127.0.0.1:50051 grpc.health.v1.Health/Check
```

If not:

```bash
docker run --rm --network astravector-smoke fullstorydev/grpcurl:v1.9.3 \
  -plaintext -d '{"service":"astravector.embedding.v1.AstraVectorRuntime"}' \
  astravector:50051 grpc.health.v1.Health/Check
```

Required response includes:

```text
"status": "SERVING"
```

## 8. Russian Ingestion And Retrieval

Create the canonical smoke text:

```bash
mkdir -p .local-demo
cat > .local-demo/mac-micro-smoke-ru.txt <<'EOF'
AstraVector хранит каноническое состояние документов в PostgreSQL. Qdrant используется как перестраиваемая поисковая проекция. Модель BGE-M3 загружается из Nexus и используется для построения эмбеддингов.
EOF
```

Use the existing repository gRPC helper path:

```bash
ASTRAVECTOR_GRPC_ADDR=127.0.0.1:50051 \
ASTRAVECTOR_QDRANT_URL=http://qdrant:6333 \
ASTRAVECTOR_QDRANT_COLLECTION=astravector_v004 \
ASTRAVECTOR_LOCAL_DEMO_ACCESS_ZONE_CODE=0488 \
python3 scripts/local-demo/local_demo.py load-text .local-demo/mac-micro-smoke-ru.txt

ASTRAVECTOR_GRPC_ADDR=127.0.0.1:50051 \
ASTRAVECTOR_QDRANT_URL=http://qdrant:6333 \
ASTRAVECTOR_QDRANT_COLLECTION=astravector_v004 \
python3 scripts/local-demo/local_demo.py wait-vector-sync

ASTRAVECTOR_GRPC_ADDR=127.0.0.1:50051 \
ASTRAVECTOR_QDRANT_URL=http://qdrant:6333 \
ASTRAVECTOR_QDRANT_COLLECTION=astravector_v004 \
python3 scripts/local-demo/local_demo.py activate-document

ASTRAVECTOR_GRPC_ADDR=127.0.0.1:50051 \
ASTRAVECTOR_QDRANT_URL=http://qdrant:6333 \
ASTRAVECTOR_QDRANT_COLLECTION=astravector_v004 \
python3 scripts/local-demo/local_demo.py search 'Где AstraVector хранит каноническое состояние документов?' \
  | tee .local-demo/mac-micro-smoke-search.json
```

The retrieval response must return the same document and evidence text containing the PostgreSQL fact. A generated natural-language answer is not required.

## 9. Second Start Cache Proof

```bash
docker stop --time 45 astravector-smoke-runtime
docker rm astravector-smoke-runtime

read -s -p "Astra reader password: " ASTRAVECTOR_NEXUS_PASSWORD
echo
export ASTRAVECTOR_NEXUS_PASSWORD

docker run -d --name astravector-smoke-runtime \
  --network astravector-smoke --network-alias astravector \
  -p 127.0.0.1:50051:50051 \
  -p 127.0.0.1:9090:9090 \
  -v astravector-bge-m3-cache:/models/bge-m3 \
  -e ASTRAVECTOR_DB_URL=postgres://astravector:astravector@postgres:5432/astravector \
  -e DATABASE_URL=postgres://astravector:astravector@postgres:5432/astravector \
  -e ASTRAVECTOR_QDRANT_URL=http://qdrant:6333 \
  -e ASTRAVECTOR_QDRANT_COLLECTION=astravector_v004 \
  -e ASTRAVECTOR_NEXUS_USERNAME=astra-reader \
  -e ASTRAVECTOR_NEXUS_PASSWORD \
  -e ASTRAVECTOR_MODEL_REPOSITORY_URL=https://nexus.astrabase.asia/repository/astra-models/astravector/bge-m3/baseline-v1 \
  -e ASTRAVECTOR_MODEL_DIR=/models/bge-m3 \
  -e ASTRAVECTOR_MODEL_PATH=/models/bge-m3/model.onnx \
  -e ASTRAVECTOR_TOKENIZER_PATH=/models/bge-m3/tokenizer.json \
  -e ASTRAVECTOR_MODEL_SHA256=f84251230831afb359ab26d9fd37d5936d4d9bb5d1d5410e66442f630f24435b \
  -e ASTRAVECTOR_MODEL_DATA_SHA256=1eebfb28493f67bba03ce0ef64bfdc7fc5a3bd9d7493f818bb1d78cd798416b4 \
  -e ASTRAVECTOR_TOKENIZER_SHA256=21106b6d7dab2952c1d496fb21d5dc9db75c28ed361a05f5020bbba27810dd08 \
  -e ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true \
  -e ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_SEARCH=false \
  -e RUST_LOG=info \
  registry.astrabase.asia/astravector:sha-1cb6065

unset ASTRAVECTOR_NEXUS_PASSWORD
```

Re-run the health check and the same search. The second-start logs must show valid cached files or checksum verification, with no fresh large model download.

## 10. Cheap Negative Auth Gate

Use a fresh temporary empty model volume and an intentionally invalid password. Remove it immediately after the test.

```bash
docker volume create astravector-bge-m3-cache-bad-auth
docker run --name astravector-smoke-bad-auth \
  --network astravector-smoke \
  -v astravector-bge-m3-cache-bad-auth:/models/bge-m3 \
  -e ASTRAVECTOR_DB_URL=postgres://astravector:astravector@postgres:5432/astravector \
  -e DATABASE_URL=postgres://astravector:astravector@postgres:5432/astravector \
  -e ASTRAVECTOR_QDRANT_URL=http://qdrant:6333 \
  -e ASTRAVECTOR_QDRANT_COLLECTION=astravector_v004 \
  -e ASTRAVECTOR_NEXUS_USERNAME=astra-reader \
  -e ASTRAVECTOR_NEXUS_PASSWORD=__intentionally_invalid__ \
  -e ASTRAVECTOR_MODEL_REPOSITORY_URL=https://nexus.astrabase.asia/repository/astra-models/astravector/bge-m3/baseline-v1 \
  -e ASTRAVECTOR_MODEL_DIR=/models/bge-m3 \
  -e ASTRAVECTOR_MODEL_PATH=/models/bge-m3/model.onnx \
  -e ASTRAVECTOR_TOKENIZER_PATH=/models/bge-m3/tokenizer.json \
  -e ASTRAVECTOR_MODEL_SHA256=f84251230831afb359ab26d9fd37d5936d4d9bb5d1d5410e66442f630f24435b \
  -e ASTRAVECTOR_MODEL_DATA_SHA256=1eebfb28493f67bba03ce0ef64bfdc7fc5a3bd9d7493f818bb1d78cd798416b4 \
  -e ASTRAVECTOR_TOKENIZER_SHA256=21106b6d7dab2952c1d496fb21d5dc9db75c28ed361a05f5020bbba27810dd08 \
  registry.astrabase.asia/astravector:sha-1cb6065

docker logs astravector-smoke-bad-auth
docker rm astravector-smoke-bad-auth
docker volume rm astravector-bge-m3-cache-bad-auth
```

Required: startup fails closed during model download/authentication and never reaches gRPC readiness.

## 11. SIGTERM

```bash
docker stop --time 45 astravector-smoke-runtime
docker inspect astravector-smoke-runtime --format 'State={{.State.Status}} ExitCode={{.State.ExitCode}} OOMKilled={{.State.OOMKilled}}'
```

Required: stopped container, no OOM kill, and no evidence that Docker had to force-kill the process.

## 12. Cleanup

Recommended cleanup keeps the model cache:

```bash
docker rm astravector-smoke-runtime 2>/dev/null || true
docker rm -f astravector-smoke-postgres astravector-smoke-qdrant 2>/dev/null || true
docker network rm astravector-smoke 2>/dev/null || true
docker system df
```

Full cleanup only on explicit operator request:

```bash
docker rm astravector-smoke-runtime 2>/dev/null || true
docker rm -f astravector-smoke-postgres astravector-smoke-qdrant 2>/dev/null || true
docker network rm astravector-smoke 2>/dev/null || true
docker volume rm astravector-bge-m3-cache
docker system df
```

Never use broad `docker volume prune` for this smoke.
