# AstraVector Mac Runtime Proof Runbook

This runbook is operator-executable on a Mac Docker host. It intentionally contains no plaintext passwords.

## 1. Environment

```bash
set -euo pipefail
export ASTRA_IMAGE=registry.astrabase.asia/astravector:sha-26288b4
export ASTRA_EXPECTED_DIGEST=sha256:77174cf14b1856b57f95ff96e96ee8c4c04df83034bd9af5127aaba287a6393a
export ASTRA_PLATFORM=

uname -m
docker version --format '{{.Server.Arch}}'
docker buildx imagetools inspect "$ASTRA_IMAGE"
```

If Docker Desktop is `arm64` and the image manifest is not `linux/arm64`, set:

```bash
export ASTRA_PLATFORM="--platform linux/amd64"
```

## 2. Registry Login And Pull

```bash
read -s -p "Nexus reader password: " ASTRA_READER_PASSWORD
echo
export ASTRA_READER_PASSWORD
printf '%s' "$ASTRA_READER_PASSWORD" | docker login registry.astrabase.asia --username astra-reader --password-stdin

docker pull ${ASTRA_PLATFORM:-} "$ASTRA_IMAGE"
docker image inspect "$ASTRA_IMAGE" --format '{{json .RepoDigests}}'
```

The observed digest must include `$ASTRA_EXPECTED_DIGEST`. If `astra-reader` cannot pull the image, stop and report `NEXUS_DOCKER_READER_RBAC_FAIL`.

## 3. Disposable Dependencies

```bash
docker network create astravector-proof 2>/dev/null || true

docker rm -f astravector-proof-postgres astravector-proof-qdrant astravector-proof-runtime 2>/dev/null || true

docker run -d \
  --name astravector-proof-postgres \
  --network astravector-proof \
  -e POSTGRES_USER=astravector \
  -e POSTGRES_PASSWORD=astravector \
  -e POSTGRES_DB=astravector \
  pgvector/pgvector:pg16

docker run -d \
  --name astravector-proof-qdrant \
  --network astravector-proof \
  qdrant/qdrant:v1.14.1

for i in $(seq 1 60); do
  docker exec astravector-proof-postgres pg_isready -U astravector -d astravector && break
  sleep 2
done

for i in $(seq 1 60); do
  docker run --rm --network astravector-proof curlimages/curl:8.10.1 -fsS http://astravector-proof-qdrant:6333/ >/dev/null && break
  sleep 2
done
```

## 4. Fresh Model Download And Runtime Startup

```bash
docker volume rm astravector-proof-models 2>/dev/null || true
docker volume create astravector-proof-models

docker run -d ${ASTRA_PLATFORM:-} \
  --name astravector-proof-runtime \
  --network astravector-proof \
  -p 50051:50051 \
  -p 9090:9090 \
  -v astravector-proof-models:/models/bge-m3 \
  -e ASTRAVECTOR_NEXUS_USERNAME=astra-reader \
  -e ASTRAVECTOR_NEXUS_PASSWORD="$ASTRA_READER_PASSWORD" \
  -e ASTRAVECTOR_DB_URL=postgres://astravector:astravector@astravector-proof-postgres:5432/astravector \
  -e ASTRAVECTOR_QDRANT_URL=http://astravector-proof-qdrant:6333 \
  -e ASTRAVECTOR_QDRANT_COLLECTION=astravector_v004 \
  -e RUST_LOG=info \
  "$ASTRA_IMAGE"

docker logs -f astravector-proof-runtime
```

Expected bootstrap evidence:

```text
downloading model.onnx
downloading model.onnx_data
downloading tokenizer.json
PostgreSQL reachable
Qdrant reachable
model and dependency bootstrap complete
```

Bootstrap completion is not readiness. Continue to gRPC health.

## 5. Model Checksums In Volume

```bash
docker run --rm -v astravector-proof-models:/models/bge-m3 debian:trixie-slim \
  sh -c 'cd /models/bge-m3 && sha256sum model.onnx model.onnx_data tokenizer.json manifest.sha256'
```

Expected:

```text
f84251230831afb359ab26d9fd37d5936d4d9bb5d1d5410e66442f630f24435b  model.onnx
1eebfb28493f67bba03ce0ef64bfdc7fc5a3bd9d7493f818bb1d78cd798416b4  model.onnx_data
21106b6d7dab2952c1d496fb21d5dc9db75c28ed361a05f5020bbba27810dd08  tokenizer.json
```

## 6. Readiness

Use the real registered gRPC health service:

```bash
grpcurl -plaintext localhost:50051 grpc.health.v1.Health/Check
grpcurl -plaintext -d '{"service":"astravector.embedding.v1.AstraVectorRuntime"}' localhost:50051 grpc.health.v1.Health/Check
curl -fsS http://localhost:9090/metrics >/dev/null
```

Expected health status is `SERVING`.

## 7. Cached Restart

```bash
docker rm -f astravector-proof-runtime

docker run -d ${ASTRA_PLATFORM:-} \
  --name astravector-proof-runtime \
  --network astravector-proof \
  -p 50051:50051 \
  -p 9090:9090 \
  -v astravector-proof-models:/models/bge-m3 \
  -e ASTRAVECTOR_NEXUS_USERNAME=astra-reader \
  -e ASTRAVECTOR_NEXUS_PASSWORD="$ASTRA_READER_PASSWORD" \
  -e ASTRAVECTOR_DB_URL=postgres://astravector:astravector@astravector-proof-postgres:5432/astravector \
  -e ASTRAVECTOR_QDRANT_URL=http://astravector-proof-qdrant:6333 \
  -e ASTRAVECTOR_QDRANT_COLLECTION=astravector_v004 \
  "$ASTRA_IMAGE"

docker logs astravector-proof-runtime
```

Expected:

```text
cache valid: model.onnx
cache valid: model.onnx_data
cache valid: tokenizer.json
```

## 8. Corruption Recovery

```bash
docker rm -f astravector-proof-runtime
docker run --rm -v astravector-proof-models:/models/bge-m3 debian:trixie-slim \
  sh -c 'printf corrupt >/models/bge-m3/tokenizer.json'
```

Restart using the same command from section 7. Expected: checksum mismatch prevents the corrupt tokenizer from being used, bootstrap re-downloads or fails closed, and final `sha256sum tokenizer.json` matches the expected value.

## 9. Negative Dependency Gates

Invalid Nexus credentials:

```bash
docker volume rm astravector-proof-bad-creds 2>/dev/null || true
docker volume create astravector-proof-bad-creds
docker run --rm ${ASTRA_PLATFORM:-} \
  --network astravector-proof \
  -v astravector-proof-bad-creds:/models/bge-m3 \
  -e ASTRAVECTOR_NEXUS_USERNAME=astra-reader \
  -e ASTRAVECTOR_NEXUS_PASSWORD=invalid-for-one-bounded-proof \
  -e ASTRAVECTOR_DB_URL=postgres://astravector:astravector@astravector-proof-postgres:5432/astravector \
  -e ASTRAVECTOR_QDRANT_URL=http://astravector-proof-qdrant:6333 \
  -e ASTRAVECTOR_QDRANT_COLLECTION=astravector_v004 \
  "$ASTRA_IMAGE"
```

Nexus unavailable:

```bash
docker volume rm astravector-proof-nexus-down 2>/dev/null || true
docker volume create astravector-proof-nexus-down
docker run --rm ${ASTRA_PLATFORM:-} \
  --network astravector-proof \
  -v astravector-proof-nexus-down:/models/bge-m3 \
  -e ASTRAVECTOR_MODEL_REPOSITORY_URL=https://127.0.0.1:9/repository/astra-models/astravector/bge-m3/baseline-v1 \
  -e ASTRAVECTOR_NEXUS_USERNAME=astra-reader \
  -e ASTRAVECTOR_NEXUS_PASSWORD="$ASTRA_READER_PASSWORD" \
  -e ASTRAVECTOR_DB_URL=postgres://astravector:astravector@astravector-proof-postgres:5432/astravector \
  -e ASTRAVECTOR_QDRANT_URL=http://astravector-proof-qdrant:6333 \
  -e ASTRAVECTOR_QDRANT_COLLECTION=astravector_v004 \
  "$ASTRA_IMAGE"
```

PostgreSQL unavailable and Qdrant unavailable should be tested with the valid cached model volume and short bootstrap timeouts:

```bash
docker run --rm ${ASTRA_PLATFORM:-} \
  --network astravector-proof \
  -v astravector-proof-models:/models/bge-m3 \
  -e ASTRAVECTOR_BOOTSTRAP_POSTGRES_TIMEOUT_SECONDS=10 \
  -e ASTRAVECTOR_NEXUS_USERNAME=astra-reader \
  -e ASTRAVECTOR_NEXUS_PASSWORD="$ASTRA_READER_PASSWORD" \
  -e ASTRAVECTOR_DB_URL=postgres://astravector:astravector@127.0.0.1:9/astravector \
  -e ASTRAVECTOR_QDRANT_URL=http://astravector-proof-qdrant:6333 \
  -e ASTRAVECTOR_QDRANT_COLLECTION=astravector_v004 \
  "$ASTRA_IMAGE"

docker run --rm ${ASTRA_PLATFORM:-} \
  --network astravector-proof \
  -v astravector-proof-models:/models/bge-m3 \
  -e ASTRAVECTOR_BOOTSTRAP_QDRANT_TIMEOUT_SECONDS=10 \
  -e ASTRAVECTOR_NEXUS_USERNAME=astra-reader \
  -e ASTRAVECTOR_NEXUS_PASSWORD="$ASTRA_READER_PASSWORD" \
  -e ASTRAVECTOR_DB_URL=postgres://astravector:astravector@astravector-proof-postgres:5432/astravector \
  -e ASTRAVECTOR_QDRANT_URL=http://127.0.0.1:9 \
  -e ASTRAVECTOR_QDRANT_COLLECTION=astravector_v004 \
  "$ASTRA_IMAGE"
```

## 10. SIGTERM

```bash
start=$(date +%s)
docker stop --time 45 astravector-proof-runtime
end=$(date +%s)
echo "shutdown_seconds=$((end-start))"
docker inspect astravector-proof-runtime --format '{{.State.ExitCode}} {{.State.FinishedAt}}'
```

Expected: graceful exit within 45 seconds without forced kill.

## 11. Kubernetes Audit Notes

The private registry pull secret is named `astravector-registry-pull` in manifests. Create it separately from application secrets:

```bash
kubectl create secret docker-registry astravector-registry-pull \
  --docker-server=registry.astrabase.asia \
  --docker-username=astra-reader \
  --docker-password="$ASTRA_READER_PASSWORD"
```

`k8s/model-pvc.yaml` uses `ReadWriteMany`. This is required for `replicas: 2` with a shared model cache on different nodes. If the cluster only supports `ReadWriteOnce`, use one replica, same-node scheduling constraints, or a per-pod model cache.

## 12. Cleanup

```bash
docker rm -f astravector-proof-runtime astravector-proof-postgres astravector-proof-qdrant 2>/dev/null || true
docker network rm astravector-proof 2>/dev/null || true
```
