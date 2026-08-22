# Codex Task: AstraVector Portable Local Deployment Bundle

Repository: `alimbetov/llm2`

Branch: `agent/astravector-image-contract`

## Role

Act as a senior DevOps/SRE engineer with strong Docker Compose, Rust service runtime, PostgreSQL/pgvector, Qdrant, private registry, and reproducible deployment experience.

Do not redesign AstraVector internals. This task is about packaging the already-proven runtime into a portable local deployment bundle that can be recreated on another developer workstation without relying on memory or ad-hoc commands.

## Primary Goal

Create a minimal, reproducible local deployment bundle for AstraVector that allows a developer on another machine to recreate the runtime environment using Docker Compose.

The bundle must define and document:

1. AstraVector image identity;
2. PostgreSQL/pgvector runtime;
3. Qdrant runtime;
4. model cache volume;
5. Nexus model download contract;
6. private registry pull contract;
7. environment variables;
8. secrets handling;
9. persistent volumes;
10. health/readiness checks;
11. startup and shutdown commands;
12. one minimal ingestion/retrieval smoke test;
13. cleanup and restore semantics.

The bundle must be usable on macOS Docker Desktop first, but remain ordinary Docker Compose without Mac-only assumptions where avoidable.

## Read Before Editing

Before changing files, inspect at least:

- `Dockerfile`
- `docker/entrypoint.sh`
- `docker/model-bootstrap.sh`
- `config/application.yaml`
- `.env.example`
- `docs/container/ASTRAVECTOR_IMAGE_CONTRACT.md`
- `docs/container/ASTRAVECTOR_IMAGE_CONTRACT_RESULT.md`
- `docs/container/ASTRAVECTOR_RUNTIME_PROOF_RESULT.md`
- `docs/container/ASTRAVECTOR_MAC_MICRO_SMOKE_RESULT.md`
- relevant gRPC/proto interfaces used for ingestion, activation, health, and retrieval
- FIX491 recovery documentation only as needed to preserve persistence semantics

Write a short implementation plan before editing.

## Current Verified Image

Use the latest image actually tested by the Mac micro-smoke unless a later immutable image is produced by a scoped fix:

```text
registry.astrabase.asia/astravector:sha-1cb6065
```

Recorded digest:

```text
sha256:b0567810b5ea3df752ff8ba559fcf16bc46b245878e798b8888dcf93426ee6ad
```

Do not silently replace this image with `latest`.

If the branch already contains a newer immutable image that fixes fresh model download and/or SIGTERM, inspect and document why it supersedes `sha-1cb6065`.

## Supporting Images

Use explicit image references:

```text
pgvector/pgvector:pg16
qdrant/qdrant:v1.14.1
```

Prefer immutable digests if they are already available and verified in the current environment. Do not invent digests.

## Nexus Endpoints

Private Docker registry:

```text
https://registry.astrabase.asia
```

Raw model repository:

```text
https://nexus.astrabase.asia/repository/astra-models/astravector/bge-m3/baseline-v1
```

Runtime reader username may be documented as:

```text
astra-reader
```

Do not commit any real password.

## Model Contract

Required model files:

```text
model.onnx
model.onnx_data
tokenizer.json
manifest.sha256
```

Expected SHA-256 values:

```text
model.onnx
f84251230831afb359ab26d9fd37d5936d4d9bb5d1d5410e66442f630f24435b

model.onnx_data
1eebfb28493f67bba03ce0ef64bfdc7fc5a3bd9d7493f818bb1d78cd798416b4

tokenizer.json
21106b6d7dab2952c1d496fb21d5dc9db75c28ed361a05f5020bbba27810dd08
```

Default cache path inside AstraVector:

```text
/models/bge-m3
```

The Compose bundle must use a named volume for this path.

The bundle must support two operating modes:

### Mode A: empty model cache

AstraVector downloads the model from Nexus using runtime reader credentials.

### Mode B: pre-populated model cache

AstraVector verifies existing model files and starts without re-downloading the 2.2 GB artifact.

Document clearly that the currently observed fresh large-file download defect may still block Mode A until resolved. Do not hide that limitation.

## Persistence Invariant

Preserve the existing architecture:

```text
PostgreSQL = canonical state / source of truth
Qdrant = rebuildable projection
PostgreSQL -> vector_outbox -> Qdrant
```

The Compose bundle must not weaken or invert this relationship.

## Required Deliverables

Create exactly these primary files unless an existing project convention strongly justifies another location:

```text
deploy/local/docker-compose.astravector.yml
deploy/local/.env.example
deploy/local/README.md
```

Optional small helper scripts are allowed only if they reduce repeated manual commands and remain auditable, for example:

```text
deploy/local/scripts/health.sh
deploy/local/scripts/smoke.sh
deploy/local/scripts/cleanup.sh
```

Avoid a large framework around Compose.

## Docker Compose Requirements

The Compose file must define at least these services:

```text
postgres
qdrant
astravector
```

### PostgreSQL

Use `pgvector/pgvector:pg16`.

Use a named volume for `/var/lib/postgresql/data`.

Environment must come from `.env` values such as:

```text
POSTGRES_DB
POSTGRES_USER
POSTGRES_PASSWORD
```

Add a healthcheck using `pg_isready`.

Do not expose PostgreSQL publicly unless local debugging requires an explicit optional host port. Prefer internal Compose networking by default.

### Qdrant

Use `qdrant/qdrant:v1.14.1`.

Use a named volume for Qdrant storage.

Add a meaningful healthcheck using an endpoint available in that image/version.

Do not enable anonymous external exposure beyond localhost unless necessary for the smoke workflow.

### AstraVector

Use the immutable AstraVector image tag/digest selected above.

Mount a named model volume at:

```text
/models/bge-m3
```

Pass at least:

```text
ASTRAVECTOR_DB_URL
ASTRAVECTOR_QDRANT_URL
ASTRAVECTOR_QDRANT_COLLECTION
ASTRAVECTOR_MODEL_REPOSITORY_URL
ASTRAVECTOR_NEXUS_USERNAME
ASTRAVECTOR_NEXUS_PASSWORD
ASTRAVECTOR_SPARSE_REQUIRED
RUST_LOG
```

Use service DNS names, not localhost:

```text
postgres
qdrant
```

Add `depends_on` using health conditions where supported by Compose semantics.

Expose only the service interfaces required for local testing, typically:

```text
50051 gRPC
9090 metrics
```

Bind host ports to `127.0.0.1` where practical.

Do not embed model files, database files, or secrets into the image.

## `.env.example` Requirements

The committed `.env.example` must contain placeholders only.

It should include at least:

```text
POSTGRES_DB=astravector
POSTGRES_USER=astravector_app
POSTGRES_PASSWORD=CHANGE_ME

ASTRAVECTOR_DB_URL=postgres://astravector_app:CHANGE_ME@postgres:5432/astravector

ASTRAVECTOR_QDRANT_URL=http://qdrant:6333
ASTRAVECTOR_QDRANT_COLLECTION=astravector_v004

ASTRAVECTOR_NEXUS_USERNAME=astra-reader
ASTRAVECTOR_NEXUS_PASSWORD=CHANGE_ME
ASTRAVECTOR_MODEL_REPOSITORY_URL=https://nexus.astrabase.asia/repository/astra-models/astravector/bge-m3/baseline-v1

ASTRAVECTOR_SPARSE_REQUIRED=false
RUST_LOG=info
```

If additional mandatory environment variables exist, inspect the bootstrap/application and include them.

Never commit actual passwords.

Ensure a local `.env` is ignored by Git if not already covered.

## Private Registry Pull Contract

Document that the operator must authenticate before `docker compose pull`:

```bash
docker login registry.astrabase.asia -u astra-reader
```

Password must be entered interactively or obtained from a secure local secret source.

Do not store registry password in the Compose YAML.

Explain that Docker Desktop credential storage is separate from container runtime Nexus credentials used for the model download.

These are two distinct authentication paths:

```text
Docker daemon -> registry.astrabase.asia -> pulls AstraVector image
AstraVector container -> nexus.astrabase.asia -> downloads model bundle
```

## Portable Restore Scenarios

The README must explicitly document three restore modes.

### Scenario 1: Empty Service

Goal: recreate a clean AstraVector service with no prior documents.

Required:

```text
Docker
Compose bundle
.env secrets
registry access
Nexus model access OR preloaded model cache
```

PostgreSQL and Qdrant volumes may be newly created.

### Scenario 2: Service With Documents

Goal: restore previously indexed documents.

Required source of truth:

```text
PostgreSQL backup/volume
```

Qdrant may be restored from its volume for speed, but must be documented as rebuildable from canonical PostgreSQL state using existing reconciliation/recovery semantics.

Do not claim that PostgreSQL can be omitted.

### Scenario 3: Nexus-Independent Restore

Goal: start without downloading the model from Nexus.

Required:

```text
pre-populated model cache volume or exported model directory
```

Document how to seed the named volume safely and verify SHA256 before startup.

## Backup/Transfer Guidance

The README must distinguish configuration artifacts from state artifacts.

Configuration artifacts:

```text
docker-compose.astravector.yml
.env.example
README
immutable image identity
model SHA256 values
```

Secrets:

```text
local .env or secret manager
```

State:

```text
PostgreSQL data/backup
Qdrant volume (optional for fast restore)
model cache (optional if Nexus remains reachable)
```

Do not recommend copying a live PostgreSQL volume filesystem while PostgreSQL is running.

Prefer logical/consistent backup mechanisms for PostgreSQL if documenting a backup workflow.

## Disk-Conscious Mac Workflow

The README must include a low-disk workflow because the target Mac may have limited storage.

Before pull:

```bash
docker system df
```

Safe cleanup examples may include:

```bash
docker container prune -f
docker image prune -f
docker builder prune -f
```

Aggressive cleanup must be explicitly marked and must not automatically prune volumes.

Never include `docker volume prune` in an automatic setup command.

Explain approximate storage pressure from:

- AstraVector image;
- pgvector image;
- Qdrant image;
- ~2.2 GB BGE-M3 model cache;
- PostgreSQL/Qdrant data growth.

## Health Contract

Use the existing gRPC health service if available.

Expected service name currently verified:

```text
astravector.embedding.v1.AstraVectorRuntime
```

Document a command that verifies `SERVING`.

Do not treat `docker ps` alone as readiness.

## Micro Smoke Contract

Provide a single-language Russian smoke path.

Canonical text:

```text
AstraVector хранит каноническое состояние документов в PostgreSQL.
Qdrant используется как перестраиваемая поисковая проекция.
Модель BGE-M3 загружается из Nexus и используется для построения эмбеддингов.
```

Question:

```text
Где AstraVector хранит каноническое состояние документов?
```

Expected retrieval evidence must contain the meaning:

```text
AstraVector хранит каноническое состояние документов в PostgreSQL.
```

Do not require a generative LLM answer if AstraVector returns retrieval evidence/chunks rather than generated natural-language answers.

The smoke workflow must verify at least:

```text
AstraVector health = SERVING
ingestion accepted
vectors published
version activated
retrieval returns expected evidence
```

Reuse existing proto/CLI/test patterns from the repository instead of inventing a new public API.

## Known Defects That Must Be Preserved Transparently

Current Mac micro-smoke reported:

### Fresh large model download

Fresh `model.onnx_data` download from Nexus may fail across the public path due to interrupted transfer and lack of effective HTTP range resume.

Do not claim a portable fresh bootstrap PASS until this is fixed and verified.

### SIGTERM

The latest micro-smoke observed:

```text
ExitCode=137
OOMKilled=false
```

after `docker stop --time 45`.

Do not claim graceful shutdown PASS until a scoped fix is implemented and verified.

The portable bundle may still be created, but README/status documentation must clearly state these limitations.

## Validation Requirements

At minimum run static checks:

```bash
docker compose -f deploy/local/docker-compose.astravector.yml config
```

If Docker is available, additionally validate:

```bash
docker compose -f deploy/local/docker-compose.astravector.yml --env-file deploy/local/.env pull
```

only if safe credentials are supplied.

Do not expose secret values in transcripts or committed artifacts.

If live runtime is available, execute the micro smoke once.

If not, mark live gates BLOCKED rather than fabricating PASS.

## Result Document

Create:

```text
docs/container/ASTRAVECTOR_PORTABLE_LOCAL_BUNDLE_RESULT.md
```

It must contain:

- branch and tested SHA;
- files created/changed;
- selected image identity;
- Compose config validation result;
- secret scan result;
- local runtime result if executed;
- restore scenario audit;
- known unresolved defects;
- final verdict.

Final verdict must be exactly one of:

```text
ASTRAVECTOR_PORTABLE_LOCAL_BUNDLE_PASS
ASTRAVECTOR_PORTABLE_LOCAL_BUNDLE_FAIL
ASTRAVECTOR_PORTABLE_LOCAL_BUNDLE_BLOCKED
```

PASS is allowed only if:

- Compose parses;
- no secrets are committed;
- image, PostgreSQL, Qdrant, model and volume contracts are internally consistent;
- README contains reproducible setup/restore instructions;
- smoke commands correspond to real project interfaces;
- there are no misleading claims about current fresh-download or SIGTERM defects.

A live end-to-end runtime is not mandatory for PASS of the bundle artifact itself, but if live gates are not executed they must be explicitly marked `NOT_RUN`/`BLOCKED` in the result document.

## Non-Goals

Do not:

- change retrieval semantics;
- change FIX491 persistence/recovery semantics;
- redesign BGE-M3 inference;
- add Kubernetes changes unless required only to keep documentation consistent;
- add Helm;
- add Vault;
- add MinIO or object storage;
- add Prometheus/Grafana;
- add service mesh;
- create a new application API;
- bundle PostgreSQL or Qdrant into the AstraVector image;
- commit secrets;
- use `latest` as the only image identity;
- remove existing model checksum verification.

## Working Style

Before editing:

1. inspect current state;
2. identify mandatory runtime env values;
3. identify existing ingestion/retrieval invocation paths;
4. identify current known defects;
5. write a concise implementation plan.

Then implement the smallest portable bundle possible.

Do not overengineer.
