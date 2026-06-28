# Configuration

## Purpose

Описать важные переменные окружения и smoke configuration.

## Audience

Разработчики, DevOps, операторы.

## Short Summary

Основные зависимости: PostgreSQL, Qdrant, gRPC port, smoke access zones, test-only failpoints.

## PostgreSQL

```bash
POSTGRES_HOST=127.0.0.1
POSTGRES_PORT=55432
POSTGRES_DB=astravector_smoke
POSTGRES_USER=astravector
POSTGRES_PASSWORD=astravector_smoke_password
DATABASE_URL=postgres://astravector:astravector_smoke_password@127.0.0.1:55432/astravector_smoke
```

Runtime также использует:

```bash
ASTRAVECTOR_DB_URL=postgres://astravector:astravector_smoke_password@127.0.0.1:55432/astravector_smoke
```

## Qdrant

```bash
QDRANT_HTTP_URL=http://127.0.0.1:56333
QDRANT_GRPC_HOST=127.0.0.1
QDRANT_GRPC_PORT=56334
QDRANT_COLLECTION=astravector_smoke_v004
ASTRAVECTOR_QDRANT_URL=http://127.0.0.1:56333
ASTRAVECTOR_QDRANT_COLLECTION=astravector_smoke_v004
```

## Smoke

```bash
SMOKE_GRPC_ADDR=127.0.0.1:55051
SMOKE_ACCESS_ZONE_A=11111111-1111-4111-8111-111111111111
SMOKE_ACCESS_ZONE_B=22222222-2222-4222-8222-222222222222
ASTRAVECTOR_CONFIG=smoke-tests/v004/config/application-smoke.yaml
```

## Smoke Failpoints

Failpoints предназначены только для тестового контура.

```bash
ASTRAVECTOR_SMOKE_FAILPOINTS_ENABLED=true
ASTRAVECTOR_SMOKE_QDRANT_FAIL_MODE=always_fail
ASTRAVECTOR_SMOKE_QDRANT_FAIL_MODE=fail_n_times
ASTRAVECTOR_SMOKE_QDRANT_FAIL_MODE=none
ASTRAVECTOR_SMOKE_QDRANT_FAIL_COUNT=3
```

Не включать smoke failpoints в production.

## Expected Results

После загрузки `.env.smoke` smoke scripts должны видеть PostgreSQL, Qdrant и gRPC адреса без ручного дублирования.

## Common Mistakes

- Использовать `DATABASE_URL`, когда runtime ожидает `ASTRAVECTOR_DB_URL`.
- Менять `QDRANT_COLLECTION` без пересоздания/миграции коллекции.
- Включать failpoints в обычном runtime.
