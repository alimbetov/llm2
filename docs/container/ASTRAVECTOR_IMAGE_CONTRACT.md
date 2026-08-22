# AstraVector Image Contract

## Responsibilities

The OCI image builds AstraVector release binaries from this repository and starts `astravector-runtime` with a verified BGE-M3 model cache. It contains application configuration, migrations, bootstrap scripts, ONNX Runtime dynamic libraries copied through the Rust build output, and only small runtime packages required for TLS, downloads, TCP reachability checks and CPU inference.

The image does not contain PostgreSQL, Qdrant, registry publisher credentials, Nexus reader credentials, or the large BGE-M3 bundle.

## Model Artifacts

Runtime model cache directory:

```text
/models/bge-m3
```

Required files:

```text
model.onnx      f84251230831afb359ab26d9fd37d5936d4d9bb5d1d5410e66442f630f24435b
model.onnx_data 1eebfb28493f67bba03ce0ef64bfdc7fc5a3bd9d7493f818bb1d78cd798416b4
tokenizer.json  21106b6d7dab2952c1d496fb21d5dc9db75c28ed361a05f5020bbba27810dd08
```

`model.int8.onnx` is not used by this branch. Code inspection shows `OnnxBgeM3Engine::load` calls ONNX Runtime with `cfg.model.path`, while ONNX external tensor data is resolved by ONNX Runtime from `model.onnx_data` next to `model.onnx`. The former `model.int8.onnx` default was stale configuration naming.

## Startup Sequence

`astravector-entrypoint` runs `astravector-model-bootstrap` and then uses `exec` for the command, so SIGTERM/SIGINT reach the Rust process directly.

Bootstrap:

1. validates mandatory environment variables;
2. creates and checks writable `/models/bge-m3`;
3. acquires a local `mkdir` lock inside the model cache;
4. verifies cached artifacts by SHA-256;
5. downloads missing or invalid artifacts from Nexus into `.part` files;
6. verifies downloaded files before atomic promotion;
7. writes `manifest.sha256`;
8. performs bounded PostgreSQL TCP reachability;
9. performs bounded Qdrant TCP reachability;
10. execs `astravector-runtime`.

The shell layer only checks reachability. PostgreSQL migrations, canonical audit, Qdrant collection compatibility, rebuild and retrieval parity remain Rust/FIX491 responsibilities.

## Environment

Required at runtime:

```text
ASTRAVECTOR_MODEL_REPOSITORY_URL
ASTRAVECTOR_MODEL_DIR
ASTRAVECTOR_MODEL_PATH
ASTRAVECTOR_TOKENIZER_PATH
ASTRAVECTOR_MODEL_SHA256
ASTRAVECTOR_MODEL_DATA_SHA256
ASTRAVECTOR_TOKENIZER_SHA256
ASTRAVECTOR_NEXUS_USERNAME
ASTRAVECTOR_NEXUS_PASSWORD
ASTRAVECTOR_DB_URL
ASTRAVECTOR_QDRANT_URL
ASTRAVECTOR_QDRANT_COLLECTION
```

No real secrets are committed. Nexus credentials are passed to curl through a temporary netrc file under the model mount, never embedded in URLs.

## Docker Run

```bash
docker login registry.astrabase.asia
docker pull registry.astrabase.asia/astravector:0.4.1-image-contract
docker volume create astravector-bge-m3
docker run --rm \
  --name astravector-runtime \
  -p 50051:50051 \
  -p 9090:9090 \
  -v astravector-bge-m3:/models/bge-m3 \
  -e ASTRAVECTOR_NEXUS_USERNAME="$ASTRAVECTOR_NEXUS_USERNAME" \
  -e ASTRAVECTOR_NEXUS_PASSWORD="$ASTRAVECTOR_NEXUS_PASSWORD" \
  -e ASTRAVECTOR_DB_URL="$ASTRAVECTOR_DB_URL" \
  -e ASTRAVECTOR_QDRANT_URL="$ASTRAVECTOR_QDRANT_URL" \
  -e ASTRAVECTOR_QDRANT_COLLECTION=astravector_v004 \
  registry.astrabase.asia/astravector:0.4.1-image-contract
```

On Mac, `host.docker.internal` may be used only when PostgreSQL or Qdrant run on the Mac host. Arbitrary external URLs are supported.

First run downloads and verifies the model. A second run with the same volume reuses the cache and does not re-download.

## Kubernetes

The committed `k8s/` manifests are a minimal production example:

- `k8s/deployment.yaml` uses the same image and bootstrap entrypoint;
- `k8s/configmap.yaml` contains non-secret defaults and checksums;
- `k8s/secret.example.yaml` names required secret keys without values;
- `k8s/model-pvc.yaml` provides a writable model cache;
- probes use Kubernetes native gRPC probes against port `50051`;
- the container runs non-root, drops capabilities, disables privilege escalation and uses a read-only root filesystem.

For non-persistent environments, replace the PVC volume with `emptyDir`. Multiple pods sharing one PVC rely on the bootstrap `mkdir` lock and checksum verification before promotion.

## Image Tags

Canonical image:

```text
registry.astrabase.asia/astravector
```

Use immutable tags, for example:

```text
registry.astrabase.asia/astravector:0.4.1-image-contract
registry.astrabase.asia/astravector:sha-<shortsha>
```

Do not use `latest` as the only production identity.

## Push

Only an operator or CI job with publisher credentials may push:

```bash
docker login registry.astrabase.asia -u <publisher-user>
docker tag astravector:0.4.1-image-contract registry.astrabase.asia/astravector:0.4.1-image-contract
docker push registry.astrabase.asia/astravector:0.4.1-image-contract
```

Record the pushed digest with:

```bash
docker inspect --format='{{index .RepoDigests 0}}' registry.astrabase.asia/astravector:0.4.1-image-contract
```

## Rollback

Rollback is image-tag based:

```bash
kubectl set image deployment/astravector-runtime astravector=registry.astrabase.asia/astravector:<previous-immutable-tag>
kubectl rollout status deployment/astravector-runtime
```

PostgreSQL remains canonical state and Qdrant remains rebuildable projection. Do not roll back by deleting PostgreSQL data or Qdrant collections as part of image rollback.
