# Troubleshooting

## Purpose

Дать практические проверки для типичных ошибок.

## Audience

Операторы, разработчики, support.

## Short Summary

Начинайте с connectivity, затем schema/search_path, затем сверяйте PostgreSQL canonical state, outbox и Qdrant projection.

## PostgreSQL Connection Refused

```bash
nc -vz 127.0.0.1 55432
docker ps | grep postgres
```

Проверить env:

```bash
echo "$ASTRAVECTOR_DB_URL"
echo "$DATABASE_URL"
```

## Qdrant Unavailable

```bash
nc -vz 127.0.0.1 56333
curl -v http://127.0.0.1:56333/health
```

## Wrong Schema

```sql
SHOW search_path;
SET search_path TO astravector, public;
```

## Column Does Not Exist

Частая путаница:

- PostgreSQL `content_chunks_v004` field: `granularity`
- Qdrant payload field: `chunk_granularity`

Если SQL пишет `column "chunk_granularity" does not exist`, значит вы используете Qdrant payload field name в PostgreSQL query.

## Qdrant Count Mismatch

Сравнить три слоя:

1. PostgreSQL synced searchable bindings.
2. `vector_outbox` `COMPLETED` events.
3. Qdrant `ACTIVE` points.

Запуск integrity audit:

```bash
./smoke-tests/v004/scripts/run-full-smoke.sh --only data-integrity-audit --keep-running
cat smoke-tests/v004/reports/data-integrity-audit-report.md
```

## BM25 Blocked

`BM25_RETRIEVAL_BLOCKED` ожидаем до реализации production sparse/BM25/hybrid search.

Проверка:

```bash
./smoke-tests/v004/scripts/run-full-smoke.sh --only bm25-hybrid-retrieval --keep-running
cat smoke-tests/v004/reports/BM25_HYBRID_RETRIEVAL_REPORT.md
```

## Expected Results

- Connectivity checks отвечают success.
- `search_path` включает `astravector`.
- Integrity audit показывает 0 violations.

## Common Mistakes

- Лечить Qdrant руками вместо восстановления из PostgreSQL/outbox/reconciliation.
- Считать BM25 blocked regression-ом dense retrieval. Это отдельный незакрытый capability.
