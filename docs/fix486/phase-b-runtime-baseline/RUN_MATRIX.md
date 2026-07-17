# FIX486B run matrix

| Stage | R1 clean cold | R2 clean repeat | R3 restart/recovery | Mandatory |
|---|---:|---:|---:|---:|
| Source/worktree identity | yes | yes | yes | yes |
| Static locked gates | yes | compare | no | yes |
| SQLx prepare check | yes | compare | no | yes |
| Clean PostgreSQL/Qdrant | yes | yes | no | yes |
| Container identities | yes | yes | yes | yes |
| Clean migrations | yes | yes | existing state | yes |
| Migration idempotency | yes | yes | no-op verification | yes |
| Model/tokenizer checks | yes | yes | same hashes | yes |
| Release build | yes | same binary preferred | same binary | yes |
| Reflection/health/metrics | yes | yes | yes | yes |
| Control ingestion | yes | yes | no reingest | yes |
| Idempotent repeated ingestion | yes | yes | not applicable | yes |
| Search probe | yes | yes | yes | yes |
| RetrieveContext probe | yes | yes | yes | yes |
| Normalized identity comparison | baseline | R1 vs R2 | R2 vs R3 | yes |
| Qdrant removal/recovery | no | no | yes | yes |
| PostgreSQL removal/recovery | no | no | yes | yes |
| Clean shutdown/port audit | yes | yes | yes | yes |

## Stage statuses

Each stage returns exactly one:

```text
PASS
FAIL
BLOCKED
SKIPPED
```

Rules:

- mandatory `BLOCKED` or `SKIPPED` is not PASS;
- shell exit code without machine-readable assertions is insufficient;
- a process merely remaining alive is not readiness proof;
- a retrieval response without identity assertions is not a control-probe PASS.

## Normalized comparison fields

### Must match

```text
source SHA
Cargo.lock/model/tokenizer/config/binary hashes
container image IDs when the environment is unchanged
migration head
service names
fixture content hash
zone/document/version identity
hierarchy shape and deterministic physical IDs
Search/RetrieveContext logical selected identity
stage verdict set
```

### May differ

```text
run ID
timestamps
PIDs
container IDs
ports when dynamic ports are intentionally used
latency and resource samples
log ordering that does not change assertions
```

## Failure codes

```text
DIRTY_WORKTREE
BASE_LINEAGE_MISMATCH
PREEXISTING_PORT_OWNER
POSTGRES_NOT_READY
QDRANT_NOT_READY
MIGRATION_FAILED
MIGRATION_NOT_IDEMPOTENT
MIGRATION_HEAD_MISMATCH
SCHEMA_INTEGRITY_VIOLATION
MODEL_NOT_FOUND
TOKENIZER_NOT_FOUND
MODEL_HASH_MISMATCH
TOKENIZER_HASH_MISMATCH
MODEL_WARMUP_FAILED
DENSE_DIMENSION_MISMATCH
RELEASE_BUILD_FAILED
REFLECTION_FAILED
HEALTH_NOT_SERVING
METRICS_UNAVAILABLE
CONTROL_INGEST_FAILED
CONTROL_INGEST_NOT_IDEMPOTENT
CONTROL_SEARCH_FAILED
CONTROL_RETRIEVE_FAILED
SEARCH_RETRIEVE_IDENTITY_MISMATCH
RESTART_STATE_LOST
FALSE_HEALTH_WITH_DEPENDENCY_DOWN
READINESS_DID_NOT_RECOVER
NONDETERMINISTIC_IDENTITY
NONDETERMINISTIC_RUNTIME_RESULT
LEAKED_PROCESS
LEAKED_PORT
EVIDENCE_IDENTITY_MISMATCH
```

## Phase handoff

R1–R3 PASS establishes the environment and execution contract for Phase C. It does not freeze or execute the full hierarchical bank.