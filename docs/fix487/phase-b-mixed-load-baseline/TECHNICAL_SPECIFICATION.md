# FIX487B Mixed-Load Baseline Technical Specification

## Purpose

FIX487B adds a phase-owned mixed-load harness for AstraVector operations readiness. It must preserve the FIX487A retrieval freeze while exercising concurrent ingestion, Search, RetrieveContext, Graph-enabled RetrieveContext, lifecycle/status and projection pressure.

## Parent Checkpoint

```text
parent_branch = agent/fix487a-retrieval-freeze
parent_sha = ef0454704b1534115b21fa4aae8b1b7cd3d90ad3
working_branch = agent/fix487b-mixed-load-baseline
```

## Phase-Owned Runtime Identity

```text
Docker Compose project = astravector_fix487b
PostgreSQL port        = 60432
Qdrant HTTP port       = 6833
Qdrant gRPC port       = 6834
AstraVector gRPC port  = 50589
Metrics port           = 9059
Qdrant collection      = astravector_fix487b
```

## Non-Goals

FIX487B does not tune retrieval, ranking, GraphRAG, MMR, no-answer, chunking or frozen banks. Capacity levels `25/50/100/200`, soak, Kubernetes, backup/restore and alerting are later phases.

## Harness Components

- `scripts/fix487b_dataset.py` creates the deterministic synthetic dataset.
- `scripts/fix487b_mixed_load.py` creates the deterministic 100-operation schedule and bounded worker runner.
- `scripts/fix487b_audit.py` contains read-only audit classifiers and SQL snippets.
- `scripts/fix487b_evidence.py` creates and verifies mandatory evidence manifests.
- `scripts/fix487b-mixed-load-pilot.sh` is the official pilot entrypoint and is fail-closed behind `ASTRAVECTOR_FIX487B_EXECUTE_PILOT=true`.

## Acceptance

The harness may pass independently as:

```text
FIX487B_MIXED_LOAD_HARNESS_PASS
```

The model-backed pilot may pass only when a live run proves:

```text
FIX487B_CONCURRENCY_5_PILOT_PASS
```

No dry-run or blocked execution may be reported as pilot PASS.
