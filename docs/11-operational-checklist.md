# Operational Checklist

## Purpose

Дать короткие чеклисты для запуска, smoke и production discussion.

## Audience

Операторы, DevOps, QA, release manager.

## Short Summary

Перед любым статусным выводом проверяйте smoke evidence, а не только устное описание.

## Before Local Run

- PostgreSQL reachable.
- Qdrant reachable.
- `.env.smoke` loaded.
- Migrations ready.
- Smoke failpoints disabled unless needed.

## Before Smoke

- PostgreSQL clean or in expected state.
- Qdrant collection exists.
- AstraVector gRPC running.
- `DATABASE_URL` / `ASTRAVECTOR_DB_URL` correct.
- `QDRANT_COLLECTION` correct.
- `SMOKE_ACCESS_ZONE_A` and `SMOKE_ACCESS_ZONE_B` set.

## After Smoke

- PASS/FAIL/BLOCKED/SKIPPED counts checked.
- Reports generated.
- Evidence JSONL checked.
- Data integrity audit checked.
- No PASS without assertion.

## Before Production Discussion

- reliability-closing PASS.
- Lifecycle PASS.
- Recovery/Reconciliation PASS.
- Load/Backpressure PASS.
- Observability PASS.
- Security hardening PASS.
- BM25/Hybrid decision documented.
- Backup/restore tested.
- Runbooks available.

## Expected Results

До закрытия всех production discussion пунктов корректный статус остается ниже production-ready.

## Common Mistakes

- Обсуждать production без BM25/lifecycle/recovery/load evidence.
- Не проверять `BLOCKED` count.
- Использовать smoke failpoints вне test contour.
