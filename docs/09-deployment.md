# Deployment

## Purpose

Описать deployment boundary и базовые эксплуатационные правила.

## Audience

DevOps, операторы, backend leads.

## Short Summary

`AstraVector_v004` — internal service. Не выставлять напрямую в Flutter/Desktop clients. Использовать `ai_bro` или REST Gateway как внешний фасад.

## Deployment Boundary

```text
Flutter / Desktop / Airflow
        |
        v
HTTPS REST / JWT / Basic Auth
        |
        v
ai_bro or REST Gateway
        |
        v
gRPC
        |
        v
AstraVector_v004
        |
        v
PostgreSQL + Qdrant + ONNX
```

## Dependencies

- PostgreSQL
- Qdrant
- ONNX model/tokenizer files
- network access между сервисом, PostgreSQL и Qdrant
- logs/metrics backend

## Do Not Do

- Не писать напрямую в Qdrant.
- Не писать напрямую в AstraVector tables вне migrations/controlled tools.
- Не включать smoke failpoints в production.
- Не expose Qdrant to clients.
- Не expose smoke/admin endpoints publicly.

## Deployment Checklist

- `DATABASE_URL` / `ASTRAVECTOR_DB_URL` configured.
- Qdrant endpoint configured.
- `QDRANT_COLLECTION` / `ASTRAVECTOR_QDRANT_COLLECTION` configured.
- Migrations applied.
- Qdrant collection exists.
- AstraVector gRPC reachable.
- Smoke passed on staging.
- Logs and metrics enabled.
- Backup/restore policy defined.

## Expected Results

После deployment backend/gateway должен обращаться к AstraVector по gRPC, а внешние клиенты должны работать через approved REST facade.

## Common Mistakes

- Открывать Qdrant наружу.
- Запускать production с test-only failpoints.
- Объявлять production-ready без lifecycle/recovery/load/observability proof.
