# Codex Task — AstraVector Production Image Contract

## Scope

Repository: `alimbetov/llm2`

Implementation branch: `agent/astravector-image-contract`

Base branch: `agent/fix491-persistence-recovery`

The purpose of this task is to turn the current AstraVector runtime into a production-oriented OCI container that is suitable for Docker now and Kubernetes later, while preserving the existing FIX491 persistence/recovery invariants.

## Role

Act as a senior Rust platform engineer and DevOps/SRE engineer.

Do not redesign AstraVector retrieval, persistence, chunking, tokenizer, embedding, Qdrant projection, GraphRAG, MMR, lifecycle, or recovery architecture.

## Primary goal

Produce a production-ready AstraVector OCI image that:

1. builds AstraVector from this repository;
2. does not bake the BGE-M3 model bundle into the image;
3. downloads the required BGE-M3 runtime bundle from Nexus only when the mounted model cache is absent or invalid;
4. verifies all model artifacts by SHA-256 before AstraVector starts;
5. receives Nexus credentials only at runtime;
6. receives PostgreSQL and Qdrant endpoints only at runtime;
7. fails closed if mandatory startup dependencies cannot become ready within bounded time;
8. runs correctly as a long-lived Docker/Kubernetes workload;
9. supports Kubernetes startup/readiness/liveness behavior without changing retrieval semantics;
10. can be built, tagged and pushed to `registry.astrabase.asia`;
11. can later be pulled and run on a Mac with external PostgreSQL/Qdrant URLs.

## Existing project facts that must be preserved

The current Dockerfile is already multi-stage and builds these binaries:

- `astravector-runtime`
- `astravector-qdrant-publisher`
- `astravector-lifecycle`
- `astravector-reconciliation`

The runtime image already contains `config/application.yaml` and `migrations`.

The current configuration already externalizes:

- `ASTRAVECTOR_DB_URL`
- `ASTRAVECTOR_QDRANT_URL`
- `ASTRAVECTOR_QDRANT_COLLECTION`
- `ASTRAVECTOR_MODEL_PATH`
- `ASTRAVECTOR_MODEL_SHA256`
- `ASTRAVECTOR_TOKENIZER_PATH`
- `ASTRAVECTOR_TOKENIZER_SHA256`

PostgreSQL is canonical state/source of truth. Qdrant is a rebuildable projection. Preserve:

```text
PostgreSQL -> vector_outbox -> Qdrant
```

FIX491 recovery, fencing, canonical fingerprinting, collection compatibility checks, Qdrant rebuild behavior and retrieval parity must not be weakened.

## Current artifact infrastructure

Nexus UI / Raw repository endpoint:

```text
https://nexus.astrabase.asia
```

Docker registry:

```text
https://registry.astrabase.asia
```

Raw repository:

```text
astra-models
```

BGE-M3 bundle directory:

```text
https://nexus.astrabase.asia/repository/astra-models/astravector/bge-m3/baseline-v1/
```

Verified artifacts:

```text
model.onnx
SHA256=f84251230831afb359ab26d9fd37d5936d4d9bb5d1d5410e66442f630f24435b

model.onnx_data
SHA256=1eebfb28493f67bba03ce0ef64bfdc7fc5a3bd9d7493f818bb1d78cd798416b4

tokenizer.json
SHA256=21106b6d7dab2952c1d496fb21d5dc9db75c28ed361a05f5020bbba27810dd08
```

The repository also contains:

```text
manifest.sha256
manifest.txt
```

## Secret handling

Use these variable names:

```text
ASTRAVECTOR_NEXUS_USERNAME
ASTRAVECTOR_NEXUS_PASSWORD
```

For manual/local verification, credentials will be supplied externally by the operator.

Never commit any actual password or token. Never hardcode credentials in:

- Dockerfile
- shell scripts
- Rust source
- `application.yaml`
- `.env.example`
- Kubernetes manifests
- image layers
- image labels
- documentation examples

Do not put credentials into request URLs. Do not print Authorization headers. Do not use shell tracing around secrets.

Publisher credentials are separate from reader credentials and are only for CI/CD `docker push`.

## Target image naming

Canonical registry:

```text
registry.astrabase.asia/astravector
```

Use immutable tags for release/proof, preferably both semantic and Git SHA tags, for example:

```text
registry.astrabase.asia/astravector:0.4.1-image-contract
registry.astrabase.asia/astravector:sha-<shortsha>
```

Do not use `latest` as the only production identity.

## Critical gate 1 — model artifact compatibility

Before changing model paths, inspect the actual runtime/model-loading code.

The current configuration declares an INT8 precision and historically defaults to a path similar to:

```text
/models/bge-m3/model.int8.onnx
```

The verified Nexus bundle currently contains:

```text
model.onnx
model.onnx_data
```

Determine from code and existing proof data which statement is true:

A. `model.onnx` plus external `model.onnx_data` is the exact runtime artifact used by this branch;
B. configuration naming is stale or misleading;
C. a different INT8 artifact is actually required.

Do not silently rename, convert or substitute model files.

If a different artifact is required and cannot be proven from the repository/evidence, stop and report `ASTRAVECTOR_IMAGE_CONTRACT_BLOCKED` with the exact blocker.

## Critical gate 2 — ONNX Runtime packaging

`Cargo.toml` uses the `ort` crate with downloaded runtime binaries.

Verify that the final runtime image contains all dynamic libraries required by `astravector-runtime`. Do not assume the existing Dockerfile is sufficient merely because compilation succeeds.

Proof must include at least one of:

- successful `docker run` through real model initialization; or
- explicit runtime linker inspection plus successful container startup through model initialization.

## Model storage contract

Default model cache directory:

```text
/models/bge-m3
```

Expected files:

```text
/models/bge-m3/model.onnx
/models/bge-m3/model.onnx_data
/models/bge-m3/tokenizer.json
/models/bge-m3/manifest.sha256
```

The image must not contain the large model bundle.

The directory must work with:

- Docker named volume
- bind mount for Mac/local testing
- Kubernetes PVC
- Kubernetes `emptyDir` when persistence is not needed

A valid cached model must not be downloaded again on restart.

## Environment contract

Implement/document these variables where appropriate:

```text
ASTRAVECTOR_MODEL_REPOSITORY_URL=https://nexus.astrabase.asia/repository/astra-models/astravector/bge-m3/baseline-v1
ASTRAVECTOR_MODEL_DIR=/models/bge-m3
ASTRAVECTOR_MODEL_PATH=/models/bge-m3/model.onnx
ASTRAVECTOR_TOKENIZER_PATH=/models/bge-m3/tokenizer.json
ASTRAVECTOR_MODEL_SHA256=f84251230831afb359ab26d9fd37d5936d4d9bb5d1d5410e66442f630f24435b
ASTRAVECTOR_MODEL_DATA_SHA256=1eebfb28493f67bba03ce0ef64bfdc7fc5a3bd9d7493f818bb1d78cd798416b4
ASTRAVECTOR_TOKENIZER_SHA256=21106b6d7dab2952c1d496fb21d5dc9db75c28ed361a05f5020bbba27810dd08
ASTRAVECTOR_NEXUS_USERNAME
ASTRAVECTOR_NEXUS_PASSWORD
ASTRAVECTOR_DB_URL
ASTRAVECTOR_QDRANT_URL
ASTRAVECTOR_QDRANT_COLLECTION
RUST_LOG
```

A secret variable must never have a real default value in the repository.

If the actual model loader requires additional model-related environment variables, add them only after code inspection and document why.

## Container startup contract

Target lifecycle:

```text
container starts
  ->
validate required environment/configuration
  ->
ensure writable model directory exists
  ->
acquire model-cache lock if shared-cache concurrency is possible
  ->
verify existing artifacts
  ->
if all required artifacts are valid:
      reuse cache
else:
      download missing/invalid artifacts into temporary files
      verify SHA256
      atomically promote verified artifacts
  ->
release cache lock
  ->
perform bounded PostgreSQL dependency check
  ->
perform bounded Qdrant dependency check
  ->
exec astravector-runtime
```

The final shell wrapper must use `exec` for the main process so SIGTERM/SIGINT reach Rust directly.

Do not hide application readiness problems with infinite shell loops.

## Download requirements

Use `curl` or an equivalent reliable client in the runtime/bootstrap layer.

Required behavior:

- TLS certificate verification enabled;
- `--fail` or equivalent HTTP failure handling;
- retry transient network failures;
- bounded connection/request timeouts;
- bounded total bootstrap duration;
- temporary `.part` or unique temporary files;
- clean failed partial downloads;
- SHA-256 verification before atomic promotion;
- no checksum bypass;
- no credentials in URL;
- no secret values in logs;
- distinguish authentication failure from network failure where practical.

For a shared PVC, prevent two replicas from corrupting the same cache. Use a small auditable lock mechanism; do not introduce a distributed lock service.

If a cached artifact has a wrong checksum, never start using it. Safe re-download is preferred; fail closed if safe repair cannot be guaranteed.

## PostgreSQL contract

PostgreSQL is external.

Runtime input:

```text
ASTRAVECTOR_DB_URL
```

Do not embed PostgreSQL in this image.

Do not hardcode `localhost` in production deployment examples.

Inspect the existing startup code first. The application already has `required_on_startup` and migration behavior. Avoid duplicating database semantics in shell.

The bootstrap dependency check should only establish that the configured PostgreSQL endpoint is reachable/accepting connections within a bounded startup window. Application-level migration/schema/canonical checks remain Rust responsibilities.

Prefer the lightest robust implementation. Do not install a full PostgreSQL server. A small PostgreSQL client is acceptable only if justified.

## Qdrant contract

Qdrant is external.

Runtime input:

```text
ASTRAVECTOR_QDRANT_URL
```

Do not embed Qdrant in the image.

Do not change:

- dense dimension 1024
- cosine distance
- sparse vector semantics
- payload index semantics
- collection compatibility logic
- canonical projection builder
- outbox/reconciliation semantics
- FIX491 recovery behavior

Dependency checking must be bounded. Do not confuse simple endpoint reachability with collection compatibility; the latter remains application/recovery logic.

## Docker image requirements

Review and minimally improve the existing multi-stage Dockerfile.

Requirements:

- keep `cargo build --locked --release --bins` or an equivalent locked release build;
- preserve Rust 1.88 unless the repository itself proves another requirement;
- preserve `Cargo.lock`;
- do not copy model artifacts into any final image layer;
- install only required runtime packages;
- retain `ca-certificates` and `libgomp1` if actually required;
- add `curl` only if used by bootstrap;
- create a non-root runtime user if compatible with ONNX/runtime/model volume permissions;
- prefer a writable `/models` mount and otherwise read-only application filesystem;
- do not require privileged mode;
- do not use host networking;
- do not disable TLS verification.

Ensure `/app/config/application.yaml`, migrations and all required binaries/libraries still exist at runtime.

## Docker vs Kubernetes bootstrap design

Keep model distribution outside Rust business logic.

Preferred design:

- one reusable bootstrap script in the image;
- Docker entrypoint calls the bootstrap, then `exec astravector-runtime`;
- Kubernetes may use the same image/script in an initContainer and share `/models` with the main container, or use the normal entrypoint if that results in a simpler correct manifest.

Do not duplicate two unrelated implementations of checksum/download logic.

If initContainer is chosen, ensure the main container cannot race ahead before model verification completes.

## Kubernetes contract

Add a minimal production-oriented example, not a platform framework.

Expected resources/documentation:

- Deployment
- Service
- model PVC example OR explicit `emptyDir` alternative
- Secret references for Nexus reader credentials
- Secret reference for PostgreSQL URL if it contains credentials
- optional Secret reference for Qdrant API key if used
- startupProbe
- readinessProbe
- livenessProbe

Requirements:

- non-root if practical;
- no plaintext secrets in committed YAML;
- writable model mount separated from application filesystem;
- `readOnlyRootFilesystem: true` if the actual runtime permits it;
- drop unnecessary Linux capabilities;
- `allowPrivilegeEscalation: false` if compatible;
- expose only actual AstraVector ports;
- graceful SIGTERM behavior preserved;
- sensible termination grace period based on existing application shutdown behavior;
- no embedded PostgreSQL/Qdrant deployment in the production example;
- no Helm, operator, service mesh, Vault or autoscaling in this task.

## Health/readiness contract

Inspect current runtime health implementation before adding probes.

Do not invent a new endpoint if the branch already exposes a suitable health mechanism.

Use the smallest correct probe strategy:

- startup probe proves the process has completed heavy initialization;
- readiness proves the service can accept requests according to existing readiness semantics;
- liveness proves the process is alive without creating restart loops during slow model/DB initialization.

If only gRPC health exists, use a Kubernetes-compatible approach based on actual support in the target Kubernetes version or a small included probe utility. Document the choice.

Do not make metrics-port availability equivalent to application readiness unless code proves that semantics.

## Local Mac validation target

After Codex completes implementation and the image is pushed, the operator will pull and run the image on a Mac.

Therefore provide exact documentation for a flow similar to:

```text
docker login registry.astrabase.asia

docker pull registry.astrabase.asia/astravector:<immutable-tag>

docker run ...
```

The local Docker run must support:

- a persistent named/bind volume for `/models/bge-m3`;
- externally supplied Nexus reader credentials;
- externally supplied PostgreSQL URL;
- externally supplied Qdrant URL;
- port publication for the actual AstraVector service/metrics ports;
- first run downloads/verifies model;
- second run reuses model cache.

Do not assume `host.docker.internal` unless documenting it explicitly as a Mac-only example. The general contract must accept arbitrary URLs.

## Build/test gates

Before claiming success run the repository's applicable static and test gates, including at minimum:

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

Also run the canonical FIX491 proof where the required local dependencies are available:

```bash
make verify-fix491-persistence-recovery
```

Do not convert an environment-related inability to run FIX491 into a fake PASS. Record it as BLOCKED with reason if required infrastructure is absent.

## Required container verification matrix

Prove as many of these live as the environment permits:

1. image builds from the target branch;
2. image does not contain the large Nexus model artifacts;
3. missing model cache -> authenticated Nexus download -> SHA-256 PASS -> runtime proceeds;
4. valid model cache -> no re-download;
5. corrupted `model.onnx` -> mismatch detected -> safe replacement or fail closed;
6. corrupted `model.onnx_data` -> mismatch detected -> safe replacement or fail closed;
7. corrupted `tokenizer.json` -> mismatch detected -> safe replacement or fail closed;
8. invalid Nexus credentials -> clear bounded startup failure;
9. Nexus unavailable -> retries, then bounded failure;
10. PostgreSQL unavailable -> bounded startup failure/readiness behavior as designed;
11. Qdrant unavailable -> bounded startup failure/readiness behavior as designed;
12. PostgreSQL + Qdrant available -> runtime initializes and becomes ready;
13. restart with same model volume -> no ~2.2 GB re-download;
14. SIGTERM reaches the Rust process and shutdown is graceful;
15. ONNX Runtime loads successfully inside the final runtime image;
16. Kubernetes manifests pass basic client-side/schema validation available in the environment;
17. image can be tagged for Nexus registry;
18. image can be pushed to Nexus only when publisher credentials are available.

## Registry push contract

Do not embed registry publisher credentials in any file.

When credentials are available externally:

```bash
docker login registry.astrabase.asia -u <publisher-user>
docker tag <local-image> registry.astrabase.asia/astravector:<immutable-tag>
docker push registry.astrabase.asia/astravector:<immutable-tag>
```

Record the final pushed image name, digest and Git SHA in the proof report.

## Deliverables

Implement only what inspection proves is needed. Expected files may include:

```text
Dockerfile
docker/entrypoint.sh
docker/model-bootstrap.sh
.env.example
deployment/k8s/astravector-deployment.yaml
deployment/k8s/README.md
docs/container/ASTRAVECTOR_IMAGE_CONTRACT.md
docs/container/ASTRAVECTOR_IMAGE_CONTRACT_RESULT.md
```

File names may differ if the repository already has a better established structure.

Prefer small auditable scripts over large frameworks.

## Required documentation

Create/update `docs/container/ASTRAVECTOR_IMAGE_CONTRACT.md` with:

- image responsibilities;
- responsibilities explicitly outside the image;
- model artifact contract;
- checksums;
- model cache behavior;
- Nexus endpoint contract;
- secret handling;
- PostgreSQL contract;
- Qdrant contract;
- startup sequence;
- Docker local run examples;
- Kubernetes lifecycle;
- probes;
- failure semantics;
- image versioning/tagging policy;
- rollback procedure.

Create `docs/container/ASTRAVECTOR_IMAGE_CONTRACT_RESULT.md` with evidence for each executed gate.

Never place real secrets in evidence files.

## Non-goals

Do not:

- redesign retrieval;
- move tokenizer/chunking outside AstraVector;
- move BGE-M3 inference outside AstraVector;
- change GraphRAG/MMR/search behavior;
- alter FIX491 persistence/recovery semantics;
- make Qdrant canonical;
- add MinIO/SeaweedFS to AstraVector;
- embed PostgreSQL or Qdrant;
- introduce Helm;
- introduce Kubernetes operators;
- introduce service mesh;
- introduce Vault;
- introduce HPA/autoscaling;
- refactor unrelated Rust code;
- change public API contracts unless required by an already-existing health interface defect and explicitly justified.

## Implementation discipline

Before modifying files:

1. inspect `Dockerfile`;
2. inspect `Cargo.toml` and ONNX Runtime linkage behavior;
3. inspect `config/application.yaml`;
4. inspect actual model loading code and exact expected ONNX artifact path/layout;
5. inspect main startup path and signal handling;
6. inspect current gRPC/HTTP/metrics health/readiness implementation;
7. inspect PostgreSQL startup/migration behavior;
8. inspect Qdrant startup/collection compatibility behavior;
9. inspect FIX491 recovery code and evidence;
10. inspect existing Docker/Kubernetes scripts/docs;
11. write a short implementation plan into the result document;
12. implement the smallest coherent change set;
13. run static gates;
14. run container verification;
15. push only after local proof passes and publisher credentials are externally available.

## Final verdict

End `docs/container/ASTRAVECTOR_IMAGE_CONTRACT_RESULT.md` with exactly one:

```text
ASTRAVECTOR_IMAGE_CONTRACT_PASS
ASTRAVECTOR_IMAGE_CONTRACT_FAIL
ASTRAVECTOR_IMAGE_CONTRACT_BLOCKED
```

`PASS` is allowed only when all mandatory gates that are technically executable in the environment have passed and no unresolved model compatibility/runtime packaging defect remains.

If Nexus push cannot be executed solely because publisher credentials are unavailable in Codex, the implementation may be otherwise complete, but the result must explicitly mark the push gate BLOCKED rather than pretending it passed. The branch must still contain exact operator commands so the push can be performed later.
