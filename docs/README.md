# Documentation Map

## Purpose

Помочь разным читателям быстро найти нужный уровень детализации по `AstraVector_v004`.

## Audience

Аналитики, разработчики, операторы, тестировщики, архитекторы.

## Short Summary

Документация разделена на обзор, архитектуру, readiness, локальную разработку, эксплуатацию, smoke tests и gRPC API.

| Reader | Start here | Purpose |
|---|---|---|
| Analyst | `00-analyst-overview.md` | Понять назначение проекта, зрелость, риски и roadmap |
| Developer | `03-local-development.md` | Собрать и запустить локально |
| DevOps / Operator | `09-deployment.md` | Развернуть и сопровождать |
| Support / Tester | `08-smoke-tests.md` | Запускать и интерпретировать smoke tests |
| Architect | `01-architecture.md` | Понять design и data flow |

## Detailed Sections

- `00-analyst-overview.md`: бизнес-технический обзор без необходимости знать Rust.
- `01-architecture.md`: компоненты, PostgreSQL, Qdrant, outbox, access model.
- `02-readiness-and-verdicts.md`: официальный статус и границы статуса.
- `03-local-development.md`: инструменты, порты, workflow.
- `04-configuration.md`: env-переменные и failpoints.
- `05-postgres.md`: SQL checks и expected counts.
- `06-qdrant.md`: health, counts, payload fields.
- `07-build-and-run.md`: команды сборки и реальные binary names.
- `08-smoke-tests.md`: профили, отдельные тесты, PASS/FAIL/BLOCKED.
- `09-deployment.md`: deployment boundary и checklist.
- `10-troubleshooting.md`: практические ошибки.
- `11-operational-checklist.md`: чеклисты перед запуском, smoke и обсуждением production.
- `12-roadmap.md`: следующие Wave.
- `api/grpc-api.md`: реальные proto методы и поля.
- `api/grpcurl-examples.md`: grpcurl примеры под текущий proto.

## Expected Results

Читатель должен понимать, что текущий статус — `SECURE_RAG_CORE_CANDIDATE + CONSISTENCY_PASS`, но не production-ready.

## Common Mistakes

- Считать `BLOCKED` за `PASS`.
- Называть проект `RELIABILITY_CANDIDATE` до lifecycle/recovery/load/observability.
- Использовать `tenant_id` в v004 RAG Core контексте вместо `access_zone_id`.
