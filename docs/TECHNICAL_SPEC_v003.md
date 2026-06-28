# AstraVector_v003 — Technical Specification

Базовая версия: `AstraVector_v002`, SHA-256 `b842485d6e190bab90adb37ef8d95fca6c7b0eec15a8d90b6607c145dcbe3ddc`.

## Архитектура

- Vector Runtime: dense + learned sparse, PostgreSQL persistence.
- Vector Binding: `access_level`, `ttl_days`, `expires_at`, lifecycle и Qdrant metadata.
- Transactional Outbox: `UPSERT_POINT`, `UPDATE_PAYLOAD`, `DELETE_POINT`.
- Qdrant Publisher: идемпотентная асинхронная проекция.
- Lifecycle: TTL expiration, soft-delete, delayed purge.
- Search Enrichment: интерфейс provider и validated representations.
- Relevance: explainable lexical/dense/consistency score contract.

## Source of truth

PostgreSQL является источником истины. Qdrant является восстанавливаемым поисковым индексом.

## Ключевые инварианты

1. `access_level` и `ttl_days` не входят в embedding cache key.
2. Один embedding может использоваться несколькими vector bindings.
3. Для REQUIRED L1 hit допустим только при `persisted_in_postgres=true`.
4. Qdrant не участвует в PostgreSQL ACID transaction.
5. Любое изменение Qdrant фиксируется outbox event в PostgreSQL.
6. Общий embedding удаляется только после исчезновения всех активных bindings.
