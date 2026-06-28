# AstraVector_v004

`AstraVector_v004` — внутренний RAG Core сервис на Rust/Tonic. Он использует PostgreSQL как canonical state, Qdrant как vector projection и ONNX Runtime для embeddings.

## Текущий статус

`SECURE_RAG_CORE_CANDIDATE + CONSISTENCY_PASS`

Это означает:

- indexing/RAG core работает;
- корпус Гражданского кодекса РК индексируется;
- dense retrieval работает;
- access-zone/access-level isolation проверены;
- atomicity failpoints прошли;
- outbox fencing прошел;
- dead-letter/Qdrant failure прошел;
- PostgreSQL <-> Qdrant consistency подтверждена текущим smoke-контуром.

Это не означает production-ready. Проект не является `RELIABILITY_CANDIDATE`.

## Quick Start

```bash
cd /Users/ruslanalimbetov/Documents/llm2/AstraVector_v004
cargo fmt --check
cargo check --all-targets --all-features
./smoke-tests/v004/scripts/run-full-smoke.sh --profile reliability-closing --keep-running
```

Проверка BM25/Sparse/Hybrid сейчас должна возвращать BLOCKED, а не PASS:

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

- BM25/Sparse/Hybrid retrieval: `BM25_RETRIEVAL_BLOCKED`.
- Lifecycle: не закрыт.
- Recovery/Reconciliation: не закрыт.
- Load/Backpressure: не закрыт.
- Observability: не закрыт.
- Security hardening beyond access zones: не закрыт.
