# AstraVector_v004

`AstraVector_v004` — внутренний RAG Core сервис на Rust/Tonic. Он использует PostgreSQL как canonical state, Qdrant как vector projection и ONNX Runtime для embeddings.

## Текущий статус

`GRAPH_RAG_READY`

Локальный runtime поднимается, `grpcurl -plaintext 127.0.0.1:50051 list` отвечает, а model-backed confidence gate проходит в ingest-and-retrieve режиме:

- dense profile: `MODEL_BACKED_E2E_CONFIRMED`, `PASS`
- sparse profile: `MODEL_BACKED_E2E_CONFIRMED`, `PASS`
- hybrid profile: `MODEL_BACKED_E2E_CONFIRMED`, `PASS`
- confidence gate: `CONFIDENCE_GATE_CONFIRMED`, `PASS`
- runtime ready: `true`
- production candidate: `false`
- production ready: `false`

`production_pass=true` в runtime confidence report означает, что confidence gate пройден. Это не означает `PRODUCTION_CANDIDATE`. Актуальная сводка: [docs/ASTRAVECTOR_READINESS_REPORT.md](docs/ASTRAVECTOR_READINESS_REPORT.md).

## Quick Start

```bash
cd /Users/ruslanalimbetov/Documents/llm2/astravector
cargo fmt --check
cargo check --all-targets --all-features
make quality-runtime-confidence-remote
```

Проверка BM25/Sparse/Hybrid не должна считаться PASS без фактического sparse/hybrid runtime evidence:

```bash
./smoke-tests/v004/scripts/run-full-smoke.sh --only bm25-hybrid-retrieval --keep-running
```

## Основная документация

- [Карта документации](docs/README.md)
- [Analyst overview](docs/00-analyst-overview.md)
- [Архитектура](docs/01-architecture.md)
- [Readiness и verdicts](docs/02-readiness-and-verdicts.md)
- [Local development](docs/03-local-development.md)
- [Конфигурация](docs/04-configuration.md)
- [PostgreSQL](docs/05-postgres.md)
- [Qdrant](docs/06-qdrant.md)
- [Build and run](docs/07-build-and-run.md)
- [Smoke tests](docs/08-smoke-tests.md)
- [Deployment](docs/09-deployment.md)
- [Troubleshooting](docs/10-troubleshooting.md)
- [Operational checklist](docs/11-operational-checklist.md)
- [Roadmap](docs/12-roadmap.md)
- [gRPC API](docs/api/grpc-api.md)
- [grpcurl examples](docs/api/grpcurl-examples.md)

## Главные ограничения

- GraphRAG: focused quick profile PASS; larger production-candidate proof остаётся blocker для `PRODUCTION_CANDIDATE`.
- Full all-target suite: требует отдельного production-candidate proof.
- Deployment/Kubernetes validation: не закрыт.
- Load/Backpressure/soak: не закрыт.
- Recovery, backup/restore, rollback: не закрыт.
- Security hardening beyond access zones: не закрыт.

## fix462 production-candidate validation

The fix462 enhanced gate requires network gRPC RetrieveContext E2E, SQLx schema validation, observability validation, and a smoke load check before the artifact can be called `PRODUCTION CANDIDATE`.

Required commands:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo check --all-targets --all-features
cargo test --all-targets --all-features
cargo sqlx prepare --check -- --all-targets --all-features
cargo test --features integration-tests --test e2e_testcontainers -- --nocapture
```

Optional smoke load:

```bash
ASTRA_VECTOR_SMOKE_RETRIEVE_ENDPOINT=http://127.0.0.1:50051 \
ASTRA_VECTOR_SMOKE_ACCESS_ZONE_ID=<zone-uuid> \
cargo test --features integration-tests --test smoke_load_retrieve_context -- --ignored --nocapture
```

Operator docs:

- `docs/CONFIG.md`
- `docs/MIGRATION.md`
- `docs/ALERTS.md`
- `docs/FIX462_ROLLBACK_PLAN.md`
- `docs/FIX462_OBSERVABILITY_VALIDATION.md`
- `docs/FIX462_SMOKE_LOAD_TEST.md`

## fix463 production-candidate stabilization

Run the full verification gate before declaring this build a production candidate:

```bash
make verify-fix463
cargo test --features integration-tests --test e2e_testcontainers -- --nocapture
docker build -t astravector-runtime:0.4.1-fix465-p2-production-hardening .
kubectl apply --dry-run=server -f k8s/
```

Operational notes are in:

- `docs/FIX463_PRODUCTION_CANDIDATE_STABILIZATION.md`
- `docs/OUTBOX_FENCING.md`
- `docs/RECONCILIATION_RUNBOOK.md`
- `docs/INGESTION_FINALIZE_RECOVERY.md`
- `docs/KUBERNETES_DEPLOYMENT.md`


## fix465 P2 production hardening

Active image tag: `astravector-runtime:0.4.1-fix465-p2-production-hardening`.

Run the production-hardening gate:

```bash
make verify-fix465
./scripts/check-version-alignment.sh
```

Grafana dashboards are stored in `observability/grafana/`.


## Configuration profiles

AstraVector supports Spring-like profile overlays: `config/application.yaml`, `config/application-dev.yaml`, `config/application-test.yaml`, `config/application-prod.yaml`. See `docs/CONFIG_PROFILES.md`.


## Quality Bench

AstraVector includes an enriched curated RAG quality bench under `benchmarks/quality/`. It validates retrieval quality, access-zone/access-level isolation, GraphRAG related blocks, MMR expected aspect coverage, hard negatives with lexical overlap, long-document target block retrieval, TTL/legal_hold expectations, consistency expectations and latency gates.

Commands:

```bash
make quality-fixtures
make quality-quick
```

Remote RetrieveContext mode:

```bash
ASTRAVECTOR_QUALITY_ENDPOINT=http://localhost:50051 make quality-quick-remote
```

Model-backed runtime mode:

```bash
ASTRAVECTOR_QUALITY_ENDPOINT=http://localhost:50051 \
ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true \
make quality-runtime-quick-remote
```

Runtime confidence gate for retrieval logic changes:

```bash
make quality-runtime-confidence-remote
make quality-runtime-confidence-report
```

`quality-runtime-confidence-remote` is stricter than a single runtime profile. It requires dense, sparse and hybrid profile snapshots, rejects skipped mandatory profiles, compares hard-negative results with `benchmarks/quality/baseline/hard-negative-baseline.json`, and writes `benchmarks/quality/reports/runtime-confidence-report.json` plus `.md`. Diagnostic collection can be run with `ASTRAVECTOR_QUALITY_CONFIDENCE_DIAGNOSTIC_ONLY=true`, but diagnostic-only output is not a production PASS.

See `docs/QUALITY_BENCH.md` and `docs/QUALITY_BENCH_RUNTIME.md`.
## Quality Bench

AstraVector includes a curated RAG quality bench under `benchmarks/quality/`. It validates retrieval quality, access-zone isolation, GraphRAG contribution, MMR diversity, negative query behavior, consistency expectations and latency gates.

Run locally:

```bash
make quality-fixtures
make quality-quick
```

Run against a live gRPC endpoint:

```bash
ASTRAVECTOR_QUALITY_ENDPOINT=http://localhost:50051 make quality-quick-remote
```

Run the executable model-backed runtime bench:

```bash
ASTRAVECTOR_QUALITY_ENDPOINT=http://localhost:50051 \
ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true \
make quality-runtime-quick-remote
```

If the loaded ONNX artifact exposes dense embeddings but no sparse output, use
the capability-aware dense profile:

```bash
ASTRAVECTOR_QUALITY_ENDPOINT=http://localhost:50051 \
ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true \
make quality-runtime-dense-quick-remote
```

`MODEL_BACKED_E2E_FAILED` is not automatically an ingest/runtime failure. The
runtime report separates `PASSED`, `FAILED`, `BLOCKED` and
`SKIPPED_RUNTIME_REQUIRED` query rows, and reports `SPARSE_UNAVAILABLE` when a
sparse or hybrid fixture cannot run against a dense-only model.

In `v007/fix474c`, sparse/hybrid runtime profiles use
`SPARSE_MODE=LEXICAL_BASELINE_TECHNICAL` when the ONNX artifact exposes only
dense outputs. The lexical baseline is deterministic technical-token sparse
encoding; it is persisted in PostgreSQL and projected to Qdrant sparse vectors.
Document ingestion and query retrieval share the same `SparseTechnicalEncoder`
core, preserving leading zeros and mapping raw tokens to stable SHA-256 based
sparse indices instead of process-local dictionary ids. Dense-only PASS is still
reported separately from sparse/hybrid PASS.

If Qdrant points exist but PUBLIC runtime queries return zero contexts, inspect
`access_level_audit` in `benchmarks/quality/reports/runtime-quality-report.json`.
All fixtures indexed as `access_level=4` indicates an access-level ingestion
mapping bug. Do not weaken the production `RetrieveContext` access filter to
hide that failure.

See [`docs/QUALITY_BENCH.md`](docs/QUALITY_BENCH.md) and
[`docs/QUALITY_BENCH_RUNTIME.md`](docs/QUALITY_BENCH_RUNTIME.md).
