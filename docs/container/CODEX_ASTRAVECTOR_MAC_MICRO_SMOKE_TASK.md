# Codex Task — AstraVector Mac Micro Smoke Proof

## Scope

Repository: `alimbetov/llm2`

Branch: `agent/astravector-image-contract`

This task is intentionally narrow. Do not redesign AstraVector, FIX491 persistence, retrieval, Qdrant projection, Docker image architecture, Kubernetes architecture, or Nexus infrastructure.

The goal is to close the remaining runtime gap with the smallest credible end-to-end proof on a Mac with limited disk space.

## Objective

Prepare and, where the execution environment permits, run a minimal local smoke test that proves this exact path:

```text
private Nexus Docker registry
  -> pull exact AstraVector image
  -> disposable pgvector PostgreSQL
  -> disposable Qdrant
  -> persistent Docker volume for BGE-M3 model cache
  -> authenticated BGE-M3 download from Nexus Raw
  -> SHA256 verification
  -> AstraVector startup
  -> PostgreSQL migration/startup
  -> Qdrant collection readiness
  -> ingest one tiny Russian text
  -> retrieve/query one Russian question
  -> confirm the returned evidence contains the expected fact
  -> restart AstraVector with the same model volume
  -> prove no 2.2 GB model re-download occurs
```

This is a micro smoke proof, not a load test, not a full recovery proof, not a full Kubernetes proof, and not an LLM generation test.

## Existing Image Under Test

Primary immutable test image:

```text
registry.astrabase.asia/astravector:sha-26288b4
```

Expected RepoDigest from prior build/push evidence:

```text
sha256:77174cf14b1856b57f95ff96e96ee8c4c04df83034bd9af5127aaba287a6393a
```

Do not silently substitute another image.

If a code fix is required, create a NEW immutable image tag and document the new Git SHA and digest. Do not overwrite `sha-26288b4`.

## Nexus Endpoints

Docker registry:

```text
https://registry.astrabase.asia
```

Raw model repository base:

```text
https://nexus.astrabase.asia/repository/astra-models/astravector/bge-m3/baseline-v1
```

Runtime reader username:

```text
astra-reader
```

IMPORTANT: do not commit or print the reader password. The operator will enter it interactively. The local test may use:

```bash
read -s -p "Astra reader password: " ASTRAVECTOR_NEXUS_PASSWORD
echo
export ASTRAVECTOR_NEXUS_PASSWORD
```

The password must not appear in:

- repository files;
- Dockerfile;
- docker-compose files;
- shell history where avoidable;
- command-line URLs;
- logs;
- test evidence;
- screenshots committed to Git.

## Disk-Space Constraint

The Mac has limited free disk space. The runbook must be conservative.

Before pulling anything, inspect:

```bash
df -h /
docker system df
```

Then perform SAFE cleanup only.

Allowed cleanup:

```bash
docker container prune -f
docker image prune -f
docker builder prune -f
```

If more space is required, the runbook may offer, but must clearly label as more aggressive:

```bash
docker image prune -a -f
docker builder prune -a -f
```

DO NOT automatically run:

```bash
docker volume prune
```

Volumes may contain useful local state.

Before downloading the BGE-M3 bundle, estimate required free space and fail early if the Mac cannot safely hold:

- AstraVector image;
- `pgvector/pgvector:pg16`;
- `qdrant/qdrant:v1.14.1`;
- BGE-M3 model bundle (~2.2 GB);
- temporary download files during atomic bootstrap;
- PostgreSQL/Qdrant runtime data;
- reasonable Docker overhead.

Use a conservative free-space threshold and document it. Do not fill the disk to near 100%.

## Platform Gate

The Mac is expected to be Apple Silicon.

Before runtime test:

```bash
uname -m
docker info --format '{{.Architecture}}'
```

Then pull the exact image and inspect:

```bash
docker pull registry.astrabase.asia/astravector:sha-26288b4

docker image inspect \
  registry.astrabase.asia/astravector:sha-26288b4 \
  --format 'ID={{.Id}} ARCH={{.Architecture}} OS={{.Os}} DIGESTS={{json .RepoDigests}}'
```

Required:

```text
ARCH=arm64
```

and the expected RepoDigest must be present.

If the image is not arm64-compatible, stop and report BLOCKED. Do not use emulation silently.

## Registry Login

The operator must authenticate with the reader account before pull:

```bash
docker logout registry.astrabase.asia 2>/dev/null || true

docker login registry.astrabase.asia -u astra-reader
```

Password is entered interactively.

Do not write the Docker registry password into the runbook.

## Disposable Local Dependencies

Use disposable local containers only.

### PostgreSQL

Use:

```text
pgvector/pgvector:pg16
```

Suggested local values are acceptable because this is a disposable local smoke environment only:

```text
POSTGRES_USER=astravector
POSTGRES_PASSWORD=astravector
POSTGRES_DB=astravector
```

Do not reuse the production/server PostgreSQL.

### Qdrant

Use the repository-approved/pinned image:

```text
qdrant/qdrant:v1.14.1
```

Do not use `latest`.

### Docker Network

Create one dedicated network, e.g.:

```text
astravector-smoke
```

AstraVector should reach dependencies by container name:

```text
postgres
qdrant
```

Expected runtime URLs:

```text
ASTRAVECTOR_DB_URL=postgres://astravector:astravector@postgres:5432/astravector
ASTRAVECTOR_QDRANT_URL=http://qdrant:6333
```

## Model Cache

Create one named Docker volume, e.g.:

```text
astravector-bge-m3-cache
```

Mount it at:

```text
/models/bge-m3
```

This volume is intentionally preserved between first and second AstraVector starts.

The first start must use an empty model volume.

The second start must use the same populated volume.

Do not use a host bind mount unless there is a concrete reason.

## Expected Model Files and SHA256

Required files:

```text
model.onnx
model.onnx_data
tokenizer.json
manifest.sha256
```

Expected SHA256:

```text
model.onnx
f84251230831afb359ab26d9fd37d5936d4d9bb5d1d5410e66442f630f24435b

model.onnx_data
1eebfb28493f67bba03ce0ef64bfdc7fc5a3bd9d7493f818bb1d78cd798416b4

tokenizer.json
21106b6d7dab2952c1d496fb21d5dc9db75c28ed361a05f5020bbba27810dd08
```

After bootstrap, verify the files inside the named Docker volume using a temporary helper container, not by trusting logs alone.

Example strategy:

```bash
docker run --rm \
  -v astravector-bge-m3-cache:/models:ro \
  alpine:3.22 \
  sh -c 'sha256sum /models/model.onnx /models/model.onnx_data /models/tokenizer.json'
```

If the mount path differs because the volume is mounted directly at `/models/bge-m3`, adjust correctly.

Do not mark checksum PASS from manifest text only. Compute the hashes from downloaded bytes.

## AstraVector Runtime Environment

At minimum supply:

```text
ASTRAVECTOR_DB_URL
ASTRAVECTOR_QDRANT_URL
ASTRAVECTOR_QDRANT_COLLECTION=astravector_v004
ASTRAVECTOR_NEXUS_USERNAME=astra-reader
ASTRAVECTOR_NEXUS_PASSWORD=<interactive secret>
ASTRAVECTOR_MODEL_REPOSITORY_URL=https://nexus.astrabase.asia/repository/astra-models/astravector/bge-m3/baseline-v1
ASTRAVECTOR_MODEL_DIR=/models/bge-m3
ASTRAVECTOR_MODEL_PATH=/models/bge-m3/model.onnx
ASTRAVECTOR_TOKENIZER_PATH=/models/bge-m3/tokenizer.json
ASTRAVECTOR_MODEL_SHA256=f84251230831afb359ab26d9fd37d5936d4d9bb5d1d5410e66442f630f24435b
ASTRAVECTOR_MODEL_DATA_SHA256=1eebfb28493f67bba03ce0ef64bfdc7fc5a3bd9d7493f818bb1d78cd798416b4
ASTRAVECTOR_TOKENIZER_SHA256=21106b6d7dab2952c1d496fb21d5dc9db75c28ed361a05f5020bbba27810dd08
RUST_LOG=info
```

Use only additional mandatory env discovered from the actual branch.

Do not invent configuration values without checking `config/application.yaml` and startup code.

## Ports

Expose only ports actually required for local proof.

Expected:

```text
50051 -> AstraVector gRPC
9090  -> metrics, only if useful for readiness observation
```

Do not expose PostgreSQL or Qdrant publicly beyond localhost unless Docker networking already makes host exposure unnecessary.

## First-Start Proof

The first AstraVector start must demonstrate all of the following:

1. model volume was empty before start;
2. Nexus authentication succeeds;
3. `model.onnx` downloads;
4. `model.onnx_data` downloads;
5. `tokenizer.json` downloads;
6. all three SHA256 values match expected values;
7. PostgreSQL becomes reachable;
8. Qdrant becomes reachable;
9. AstraVector performs its normal application-owned migrations/startup;
10. ONNX model initialization succeeds;
11. gRPC service becomes healthy/ready.

Do not treat the line `model and dependency bootstrap complete` as runtime readiness.

Readiness must be proven against the running AstraVector service.

## Health Check

Inspect the branch and use the real registered gRPC health service.

Expected service from prior audit:

```text
astravector.embedding.v1.AstraVectorRuntime
```

Verify this against current code before using it.

Use `grpcurl` if available, or a purpose-built temporary grpcurl container/tool.

Expected check conceptually:

```text
grpc.health.v1.Health/Check
service = astravector.embedding.v1.AstraVectorRuntime
```

Do not invent an HTTP `/ready` endpoint if none exists.

## Micro Ingestion + Retrieval Smoke

This is the most important business-level gate.

Use exactly one tiny Russian text and one Russian question.

Canonical smoke text:

```text
AstraVector хранит каноническое состояние документов в PostgreSQL. Qdrant используется как перестраиваемая поисковая проекция. Модель BGE-M3 загружается из Nexus и используется для построения эмбеддингов.
```

Canonical question:

```text
Где AstraVector хранит каноническое состояние документов?
```

Expected semantic evidence:

```text
В PostgreSQL.
```

Important: AstraVector is a retrieval system, not necessarily a generative-answer system.

Therefore PASS does NOT require a natural-language generated answer.

PASS requires that the retrieval response returns a chunk/evidence payload whose content clearly contains the fact that the canonical state is stored in PostgreSQL.

The implementation must inspect existing public/internal ingestion/retrieval APIs on this branch and use the smallest real API path already supported.

Do not invent a new endpoint solely for this smoke test.

Prefer the existing REST retrieval boundary if it can ingest/retrieve the required object through an existing supported path; otherwise use the existing gRPC ingestion/retrieval contract.

Before writing commands, inspect:

- proto definitions;
- REST boundary;
- ingestion facade/CLI/test helpers;
- existing E2E tests;
- FIX491 retrieval parity tooling.

The smoke input must create the minimum valid document/block/chunk structure required by the actual ingestion contract.

Document every ID/value used so the test is deterministic and rerunnable.

## Second-Start Cache Proof

Stop/remove only the AstraVector container.

Preserve:

```text
astravector-bge-m3-cache
postgres
qdrant
```

Start AstraVector again using the exact same image and model volume.

Required proof:

- bootstrap logs show cache-valid behavior;
- no new 2.2 GB model download occurs;
- runtime becomes ready again;
- the previously ingested text remains retrievable;
- the same Russian question returns the same relevant evidence.

This is the core restart/cache proof.

## Minimal Negative Gate

Because disk space is constrained, do not run the full negative matrix unless necessary.

Run only ONE cheap fail-closed negative test after the successful smoke:

Preferred:

- start a temporary AstraVector container with a fresh empty model volume and intentionally invalid Nexus password;
- confirm startup fails with a clear model download/authentication failure;
- remove the temporary container and temporary empty volume immediately.

Do not corrupt the main verified model cache for this micro smoke task.

Do not redownload 2.2 GB solely to test corruption recovery on the Mac.

## SIGTERM Gate

With healthy AstraVector running:

```bash
docker stop --time 45 <container>
```

Verify the process stops normally within the configured grace period and is not killed because signal propagation failed.

Then restart it once more with the same model volume if needed to preserve final usable state.

## Cleanup Policy

At the end, provide two cleanup modes.

### KEEP MODEL CACHE

Recommended because re-downloading BGE-M3 costs ~2.2 GB and time.

Remove:

- disposable AstraVector container;
- disposable PostgreSQL container/data if not needed;
- disposable Qdrant container/data if not needed;
- temporary smoke helper containers.

Keep:

```text
astravector-bge-m3-cache
```

### FULL CLEANUP

Only on explicit operator request:

- remove smoke containers;
- remove smoke network;
- remove PostgreSQL/Qdrant volumes created specifically by this smoke;
- remove `astravector-bge-m3-cache`;
- optionally remove smoke-only images.

Never run broad `docker volume prune` as part of cleanup.

## Evidence Requirements

Create/update:

```text
docs/container/ASTRAVECTOR_MAC_MICRO_SMOKE_RUNBOOK.md
docs/container/ASTRAVECTOR_MAC_MICRO_SMOKE_RESULT.md
```

The result document must record:

- date/time;
- Mac architecture;
- Docker architecture;
- free disk before/after cleanup;
- exact image tag;
- exact RepoDigest;
- dependency image tags;
- whether model download was fresh or cached;
- computed SHA256 x3;
- PostgreSQL readiness;
- Qdrant readiness;
- ONNX initialization evidence;
- gRPC health result;
- ingestion command/contract used;
- retrieval command/contract used;
- returned evidence excerpt, limited to what is necessary;
- second-start cache evidence;
- negative auth test result;
- SIGTERM result;
- disk usage after smoke;
- cleanup state.

Do not include passwords, Authorization headers, `.netrc` contents, Docker auth config contents, DB secrets beyond disposable local defaults, or any secret values.

## Implementation Rules

Before changing any code:

1. read this task completely;
2. read `docs/container/CODEX_ASTRAVECTOR_IMAGE_CONTRACT_TASK.md`;
3. read `docs/container/CODEX_ASTRAVECTOR_RUNTIME_PROOF_TASK.md`;
4. read `docs/container/ASTRAVECTOR_IMAGE_CONTRACT_RESULT.md`;
5. read `docs/container/ASTRAVECTOR_RUNTIME_PROOF_RESULT.md`;
6. inspect current `Dockerfile`;
7. inspect `docker/model-bootstrap.sh`;
8. inspect `docker/entrypoint.sh`;
9. inspect `config/application.yaml`;
10. inspect gRPC health registration;
11. inspect ingestion/retrieval APIs and existing E2E tests;
12. inspect existing k8s manifests only for consistency; do not expand K8s scope;
13. write a short execution plan in the result document;
14. only then execute or make minimal fixes.

Do not refactor unrelated code.

Do not change retrieval semantics.

Do not change FIX491 persistence/recovery semantics.

Do not alter canonical PostgreSQL -> outbox -> Qdrant projection behavior.

Do not add a new LLM/generator.

Do not add MinIO, Redis, Kafka, Helm, Vault, service mesh, operators, autoscaling or monitoring stacks.

Do not rebuild the image unless a concrete defect is found.

Do not push a replacement image unless a concrete defect is fixed.

If a replacement image is required:

- use a new Git commit;
- use a new immutable `sha-<shortsha>` tag;
- record the new registry digest;
- do not overwrite old immutable tags.

## Codex Environment Limitation

If Codex cannot access the operator's Mac Docker daemon or cannot safely inject the Nexus password:

- do NOT claim PASS;
- do NOT invent runtime outputs;
- produce the exact operator runbook;
- mark live Mac-only gates BLOCKED;
- stop after making only necessary static/runbook corrections.

The operator will execute the runbook manually on the Mac and provide outputs for final verification.

## Final Verdict

Exactly one final verdict:

```text
ASTRAVECTOR_MAC_MICRO_SMOKE_PASS
ASTRAVECTOR_MAC_MICRO_SMOKE_FAIL
ASTRAVECTOR_MAC_MICRO_SMOKE_BLOCKED
```

PASS requires ALL of:

- exact private image pulled successfully;
- expected digest confirmed;
- arm64 compatibility confirmed;
- fresh Nexus model download succeeded with runtime reader credentials;
- computed SHA256 x3 matched;
- ONNX model initialized;
- PostgreSQL startup/migrations succeeded;
- Qdrant startup/compatibility succeeded;
- AstraVector gRPC health succeeded;
- one Russian text was ingested through a real supported API;
- one Russian question retrieved evidence containing the expected PostgreSQL fact;
- restart with the same model volume did not re-download the model;
- retrieval still worked after restart;
- invalid Nexus credential negative test failed closed;
- SIGTERM behavior passed.

If any required live step was not actually executed, verdict must be BLOCKED rather than PASS.
