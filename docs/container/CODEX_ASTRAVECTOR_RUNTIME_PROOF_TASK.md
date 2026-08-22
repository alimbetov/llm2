# Codex Task: AstraVector Runtime Proof and Mac Pull/Run Closure

## Repository / branch

- Repository: `alimbetov/llm2`
- Branch: `agent/astravector-image-contract`
- Base recovery branch: `agent/fix491-persistence-recovery`

## Purpose

The image-contract implementation already exists and an image has been pushed to the private Nexus Docker registry. The current result is still `ASTRAVECTOR_IMAGE_CONTRACT_BLOCKED` because the final image has not yet been proven end-to-end with:

1. authenticated model download from Nexus RAW;
2. SHA-256 verification of the exact BGE-M3 bundle;
3. successful ONNX runtime/model initialization;
4. external PostgreSQL;
5. external Qdrant;
6. AstraVector becoming ready;
7. restart with the same model cache without a second 2.2 GB download;
8. pull and execution of the published image on a Mac Docker host.

This task is a proof/closure task first. Do not redesign AstraVector.

## Existing published image

Expected image references:

```text
registry.astrabase.asia/astravector:0.4.1-image-contract
registry.astrabase.asia/astravector:sha-26288b4
```

Previously recorded digest:

```text
sha256:77174cf14b1856b57f95ff96e96ee8c4c04df83034bd9af5127aaba287a6393a
```

Do not assume the recorded digest is still authoritative. Resolve and record the digest actually pulled during this proof.

## Nexus endpoints

Docker Registry:

```text
https://registry.astrabase.asia
```

RAW model repository:

```text
https://nexus.astrabase.asia/repository/astra-models/astravector/bge-m3/baseline-v1
```

Reader username for runtime model access:

```text
astra-reader
```

The reader password is an operator-supplied runtime secret. Do NOT commit it anywhere, do NOT place it in Dockerfile/ENV, do NOT place it in documentation, and do NOT echo it in logs or evidence. For local interactive execution use `read -s`, environment injection, a temporary env file outside Git, or an equivalent secret-safe mechanism.

The Docker registry pull can use the same reader account if that account has Docker read privileges. If Docker pull authorization fails, report the RBAC mismatch explicitly; do not fall back to admin.

Publisher credentials are not needed for runtime proof unless a corrected image must be pushed after a verified defect fix. If a repush is needed, require runtime-supplied publisher credentials and never commit them.

## Model contract

Required files:

```text
model.onnx
model.onnx_data
tokenizer.json
manifest.sha256
```

Expected SHA-256:

```text
model.onnx
f84251230831afb359ab26d9fd37d5936d4d9bb5d1d5410e66442f630f24435b

model.onnx_data
1eebfb28493f67bba03ce0ef64bfdc7fc5a3bd9d7493f818bb1d78cd798416b4

tokenizer.json
21106b6d7dab2952c1d496fb21d5dc9db75c28ed361a05f5020bbba27810dd08
```

The proof MUST show that the final runtime uses `model.onnx` together with adjacent external data `model.onnx_data` and does not silently substitute another model artifact.

## Existing implementation to preserve unless proven defective

Current contract includes:

- multi-stage Rust image build;
- non-root runtime user `10001`;
- model bootstrap outside Rust business logic;
- `ASTRAVECTOR_MODEL_REPOSITORY_URL`;
- `ASTRAVECTOR_MODEL_DIR`;
- `ASTRAVECTOR_MODEL_PATH`;
- `ASTRAVECTOR_MODEL_DATA_SHA256`;
- `ASTRAVECTOR_TOKENIZER_PATH`;
- `ASTRAVECTOR_NEXUS_USERNAME`;
- `ASTRAVECTOR_NEXUS_PASSWORD`;
- `ASTRAVECTOR_DB_URL`;
- `ASTRAVECTOR_QDRANT_URL`;
- `ASTRAVECTOR_QDRANT_COLLECTION`;
- checksum verification;
- temporary download files and atomic promotion;
- shared-cache bootstrap locking;
- PostgreSQL/Qdrant bounded TCP reachability checks;
- entrypoint `exec` into `astravector-runtime`;
- Kubernetes PVC model cache.

Do not replace these without a demonstrated defect and evidence.

# Phase 1 — Audit current branch before changing anything

Inspect at minimum:

```text
Dockerfile
docker/entrypoint.sh
docker/model-bootstrap.sh
config/application.yaml
.env.example
k8s/deployment.yaml
k8s/configmap.yaml
k8s/secret.example.yaml
k8s/model-pvc.yaml
docs/container/ASTRAVECTOR_IMAGE_CONTRACT.md
docs/container/ASTRAVECTOR_IMAGE_CONTRACT_RESULT.md
src/inference/**
src/bin/** or the actual runtime startup path
current gRPC health registration
```

Verify specifically:

1. exact runtime model file names;
2. whether ONNX external data is loaded automatically from the adjacent `model.onnx_data`;
3. Rust-side checksum behavior versus bootstrap checksum behavior;
4. gRPC health service registration names;
5. whether Kubernetes `startupProbe` service name exactly matches an actually registered health service;
6. PostgreSQL startup semantics;
7. Qdrant startup/compatibility semantics;
8. migrations are still owned by the application, not shell bootstrap;
9. FIX491 invariant remains intact:
   `PostgreSQL canonical state -> vector_outbox -> Qdrant rebuildable projection`.

If a probe service name is wrong, fix only the probe or health registration needed for correctness. Do not redesign the API.

# Phase 2 — Static and build gates

Run and record exact exit codes:

```bash
bash -n docker/model-bootstrap.sh
bash -n docker/entrypoint.sh
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

If the full test suite shows the previously observed flaky E2E provenance failure:

- rerun the exact failed test at least 3 times;
- do not call the suite PASS if one or more reruns still fail;
- classify it as deterministic defect or flaky test with evidence;
- do not modify retrieval semantics merely to make the image task green.

Run the canonical FIX491 proof only if its environment dependencies are available. If stages fail with command-not-found/127, identify the missing executable/command and fix the proof harness only if the defect is on this branch and within scope. Do not falsify PASS.

# Phase 3 — Verify image contents and architecture

Build or inspect the published image and prove:

- no `model.onnx`, `model.onnx_data`, `tokenizer.json`, PyTorch model, or equivalent model payload is baked into the image;
- final image contains required CA certificates and runtime libraries;
- `ldd /usr/local/bin/astravector-runtime` has no unresolved dependency;
- no secret is present in `docker history`, image ENV, labels, or copied files;
- process runs as non-root;
- entrypoint ends in `exec` semantics;
- writable paths are limited to required mounts/temp locations.

On Apple Silicon Mac, do not assume architecture. Determine host architecture and image manifest architecture before pull/run:

```bash
uname -m
docker version --format '{{.Server.Arch}}'
docker buildx imagetools inspect registry.astrabase.asia/astravector:sha-26288b4
```

If the published image is only `linux/amd64` and the Mac Docker host is `arm64`, the proof may use Docker Desktop emulation with explicit `--platform linux/amd64`, but record this fact. Do not claim native arm64 support unless an arm64 image exists and is tested.

# Phase 4 — Mac Docker runtime proof runbook

Create an operator-executable runbook:

```text
docs/container/ASTRAVECTOR_MAC_RUNTIME_PROOF.md
```

The runbook must be copy/paste-safe and must not contain plaintext passwords.

## 4.1 Docker registry login

Use reader credentials interactively:

```bash
read -s -p "Nexus reader password: " ASTRA_READER_PASSWORD
echo
export ASTRA_READER_PASSWORD
printf '%s' "$ASTRA_READER_PASSWORD" | \
  docker login registry.astrabase.asia \
  --username astra-reader \
  --password-stdin
```

Expected:

```text
Login Succeeded
```

If reader cannot pull the image, report `NEXUS_DOCKER_READER_RBAC_FAIL` and stop. Do not use admin.

## 4.2 Pull exact image and record digest

Prefer immutable SHA tag:

```bash
docker pull registry.astrabase.asia/astravector:sha-26288b4

docker image inspect \
  registry.astrabase.asia/astravector:sha-26288b4 \
  --format '{{json .RepoDigests}}'
```

Record the observed digest in evidence.

If Mac is ARM and image is AMD64-only:

```bash
docker pull --platform linux/amd64 \
  registry.astrabase.asia/astravector:sha-26288b4
```

Use the same explicit platform for `docker run`.

# Phase 5 — Disposable local PostgreSQL and Qdrant on Mac

Create one isolated network:

```bash
docker network create astravector-proof 2>/dev/null || true
```

Use disposable containers, not host-installed databases.

PostgreSQL MUST have pgvector available because AstraVector migrations/schema may depend on it. Use the project-tested compatible image/version, preferring the same pgvector/PostgreSQL family used by FIX491 testcontainers.

Example shape only; inspect the project before finalizing exact image/tag:

```bash
docker run -d \
  --name astravector-proof-postgres \
  --network astravector-proof \
  -e POSTGRES_USER=astravector \
  -e POSTGRES_PASSWORD=astravector \
  -e POSTGRES_DB=astravector \
  pgvector/pgvector:pg16
```

Use a Qdrant version compatible with the project and evidence, not an arbitrary latest if the repository pins or documents one.

Example shape:

```bash
docker run -d \
  --name astravector-proof-qdrant \
  --network astravector-proof \
  qdrant/qdrant:<verified-compatible-tag>
```

Wait for both dependencies with bounded loops and collect logs if readiness fails.

# Phase 6 — Fresh model-cache proof

Create a dedicated Docker volume:

```bash
docker volume rm astravector-proof-models 2>/dev/null || true
docker volume create astravector-proof-models
```

Run the image with:

```text
ASTRAVECTOR_NEXUS_USERNAME=astra-reader
ASTRAVECTOR_NEXUS_PASSWORD=<runtime secret>
ASTRAVECTOR_DB_URL=postgres://astravector:astravector@astravector-proof-postgres:5432/astravector
ASTRAVECTOR_QDRANT_URL=http://astravector-proof-qdrant:6333
ASTRAVECTOR_QDRANT_COLLECTION=astravector_v004
```

The password must be passed as an environment variable without printing it. Do not use `set -x`.

Use the dedicated model volume mounted at:

```text
/models/bge-m3
```

The first-start proof MUST record log evidence for:

1. `model.onnx` absent -> authenticated Nexus download;
2. `model.onnx_data` absent -> authenticated Nexus download;
3. `tokenizer.json` absent -> authenticated Nexus download;
4. each checksum verified before promotion;
5. PostgreSQL reachable;
6. Qdrant reachable;
7. Rust process starts;
8. ONNX model initialization succeeds;
9. service becomes ready.

Do not treat bootstrap completion alone as runtime readiness.

# Phase 7 — Verify downloaded model bytes inside the volume

Use a disposable helper container or equivalent to calculate SHA-256 from the shared volume.

Expected values MUST match exactly:

```text
f84251230831afb359ab26d9fd37d5936d4d9bb5d1d5410e66442f630f24435b  model.onnx
1eebfb28493f67bba03ce0ef64bfdc7fc5a3bd9d7493f818bb1d78cd798416b4  model.onnx_data
21106b6d7dab2952c1d496fb21d5dc9db75c28ed361a05f5020bbba27810dd08  tokenizer.json
```

The proof must fail on any mismatch.

# Phase 8 — Readiness/health proof

Inspect actual health implementation and use the correct protocol.

Required proof:

- container remains running;
- gRPC health is SERVING for the service actually registered by AstraVector;
- metrics endpoint on 9090 responds if the runtime exposes it;
- Kubernetes probe configuration uses a valid gRPC health service name or the default health service intentionally;
- readiness represents application readiness, not merely open TCP ports.

Do not invent an HTTP `/ready` endpoint solely for this task unless there is no viable existing health mechanism and a minimal addition is justified.

# Phase 9 — Cached restart proof

Stop/remove only the AstraVector runtime container while preserving:

- PostgreSQL;
- Qdrant;
- `astravector-proof-models` volume.

Start AstraVector again with the same model volume.

Required evidence:

```text
cache valid: model.onnx
cache valid: model.onnx_data
cache valid: tokenizer.json
```

There MUST NOT be a second full model download.

Record restart time separately from first-download startup time.

# Phase 10 — Corruption fail-closed proof

With AstraVector stopped, deliberately corrupt a disposable copy or one artifact in the proof volume.

Preferred target: small `model.onnx` or `tokenizer.json`, not the 2.2 GB file.

Restart.

Required behavior:

- checksum mismatch is detected;
- corrupt artifact is never used;
- implementation either atomically re-downloads and verifies it, or fails closed according to documented policy;
- after successful remediation, final checksum matches expected value.

Do not corrupt the canonical Nexus repository artifact.

# Phase 11 — Invalid Nexus credential proof

Use a fresh empty model volume and an intentionally invalid reader password.

Required behavior:

- authenticated model download fails;
- no corrupt/partial final artifact is promoted;
- process exits non-zero within bounded retry/timeout;
- logs clearly identify download/auth failure without printing secrets.

Do not perform enough repeated invalid requests to trigger long-lived Nexus authentication throttling. One bounded scenario is sufficient.

# Phase 12 — Nexus unavailable proof

Use a fresh empty model volume and override model repository URL to a controlled unreachable endpoint or blocked test endpoint.

Required behavior:

- bounded retries;
- bounded connect/download timeout;
- non-zero exit;
- no final corrupt artifact;
- clear failure log.

Do not disable TLS verification.

# Phase 13 — PostgreSQL unavailable proof

Use valid cached model volume but invalid/unreachable PostgreSQL endpoint.

Required behavior:

- no model re-download;
- bootstrap waits only for bounded configured timeout;
- process exits non-zero before runtime starts OR follows documented startup semantics;
- failure is clearly identified as PostgreSQL dependency failure.

# Phase 14 — Qdrant unavailable proof

Use valid cached model volume and valid PostgreSQL but invalid/unreachable Qdrant endpoint.

Required behavior:

- no model re-download;
- bounded dependency timeout;
- non-zero exit before runtime starts OR documented behavior;
- clear Qdrant dependency failure.

# Phase 15 — SIGTERM proof

With a healthy running container:

```bash
docker stop --time 45 <container>
```

Prove:

- signal reaches `astravector-runtime` through entrypoint `exec`;
- graceful shutdown path runs;
- container exits without forced kill if implementation permits;
- record shutdown duration and exit status.

# Phase 16 — PostgreSQL/Qdrant semantic sanity

After successful runtime startup:

- verify migrations applied successfully;
- verify no migration checksum mismatch;
- verify configured Qdrant collection exists or is created according to application config;
- verify dense schema is 1024/Cosine;
- verify sparse vector support remains enabled per branch config;
- verify no change to FIX491 canonical-state/projected-state invariant.

Do not use shell bootstrap to create schemas or Qdrant collection directly.

# Phase 17 — Image correction policy

If any live proof exposes an implementation defect:

1. reproduce minimally;
2. document failing evidence;
3. implement smallest scoped fix;
4. rerun static gates;
5. rebuild image;
6. use a NEW immutable tag based on new Git SHA;
7. push using runtime-supplied `astra-publisher` credentials;
8. rerun the failed live proof from a clean state;
9. never overwrite evidence claiming the old image passed.

Do not mutate `sha-26288b4` to point to a different build.

# Phase 18 — Kubernetes validation

Do not require a live cluster for basic manifest validation.

At minimum:

- parse/render YAML locally;
- ensure imagePullSecrets are documented if registry is private;
- ensure model PVC access mode is compatible with `replicas: 2` and the intended Kubernetes storage class;
- flag that `ReadWriteOnce` may not support two replicas on different nodes;
- verify non-root UID can write to the mounted model volume;
- verify `readOnlyRootFilesystem: true` does not block runtime temp/config needs;
- verify startup/readiness/liveness gRPC probe names against actual health registration;
- verify Nexus/DB/Qdrant credentials remain in Secret, not ConfigMap;
- verify registry pull secret is separate from application Nexus reader secret when appropriate.

If the manifest uses a PVC shared by multiple replicas, explicitly document the storage semantics required (`RWX`, same-node RWO constraints, or per-pod cache strategy). Do not leave this implicit.

# Deliverables

Create/update:

```text
docs/container/ASTRAVECTOR_MAC_RUNTIME_PROOF.md
docs/container/ASTRAVECTOR_RUNTIME_PROOF_RESULT.md
```

If code fixes are required, update only scoped files and explain each change.

`ASTRAVECTOR_RUNTIME_PROOF_RESULT.md` must include:

- tested Git SHA;
- tested image ref;
- observed image digest;
- Mac architecture;
- Docker architecture/platform used;
- PostgreSQL image/tag;
- Qdrant image/tag;
- Nexus authenticated model-download result;
- checksum result for all 3 model artifacts;
- ONNX initialization result;
- PostgreSQL migration/startup result;
- Qdrant startup/schema result;
- gRPC health/readiness result;
- first-start model download result;
- cached restart result;
- corruption test result;
- invalid credential result;
- Nexus unavailable result;
- PostgreSQL unavailable result;
- Qdrant unavailable result;
- SIGTERM result;
- Kubernetes manifest audit result;
- any known flaky test evidence;
- FIX491 status on the tested SHA;
- blockers, if any.

# Final verdict

Return exactly one top-level verdict:

```text
ASTRAVECTOR_RUNTIME_PROOF_PASS
ASTRAVECTOR_RUNTIME_PROOF_FAIL
ASTRAVECTOR_RUNTIME_PROOF_BLOCKED
```

PASS is allowed only if all mandatory live runtime gates have actually executed and passed on the recorded image digest.

Do not claim PASS from source inspection, image build, push success, or bootstrap-only tests.

## Mandatory PASS gates

- reader can pull image from private registry;
- exact image digest recorded;
- first start downloads model from Nexus using runtime reader credentials;
- all model SHA-256 values match;
- ONNX initialization succeeds;
- PostgreSQL is external and migrations/startup succeed;
- Qdrant is external and runtime reaches compatible collection state;
- AstraVector becomes genuinely ready;
- restart with same model volume does not re-download model;
- corruption is detected and handled fail-closed/atomically;
- invalid Nexus credentials fail safely;
- Nexus unavailable fails within bounded timeout;
- PostgreSQL unavailable fails according to bounded startup contract;
- Qdrant unavailable fails according to bounded startup contract;
- SIGTERM graceful path is proven;
- no secret is committed or exposed in evidence;
- Kubernetes probe/service-name and PVC semantics are valid/documented.

If the Codex execution environment cannot access the operator's Mac Docker daemon, it must still produce the exact Mac runbook and mark live Mac-only gates `BLOCKED`. The operator will execute the runbook manually and provide logs for final closure.