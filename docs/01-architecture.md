# Architecture

## Purpose

Описать архитектуру `AstraVector_v004`, основные компоненты и поток данных.

## Audience

Архитекторы, разработчики, операторы, аналитики.

## Short Summary

`AstraVector_v004` — internal RAG Core service. Он не является публичным API для mobile/frontend клиентов. PostgreSQL — source of truth, Qdrant — rebuildable vector projection.

## High-Level Architecture

```text
Backend / Worker / Gateway
        |
        v
Rust/Tonic gRPC service
        |
        +--> ONNX embedding runtime
        +--> PostgreSQL canonical state
        +--> vector_outbox
        +--> Qdrant vector projection
```

## Components

- Rust/Tonic gRPC service: принимает Encode/Search/v004 control calls.
- PostgreSQL canonical state: хранит document versions, chunks, bindings, outbox, cache.
- Qdrant vector projection: хранит searchable points.
- ONNX embedding runtime: строит dense embeddings; sparse capability зависит от ONNX artifact.
- `vector_outbox`: надежная синхронизация Qdrant.
- `smoke-tests/v004`: проверяемый acceptance и reliability contour.

## PostgreSQL Canonical State

PostgreSQL — источник истины. Qdrant можно перестроить из PostgreSQL/vector cache.

Core tables:

- `document_versions`
- `content_chunks_v004`
- `vector_bindings_v004`
- `vector_outbox`
- `_sqlx_migrations`

## Qdrant Projection

Qdrant содержит searchable vector points. Его нельзя считать canonical state и нельзя обновлять в обход outbox.

## Multi-Granularity Model

- `SOURCE`: source container, не searchable.
- `PARENT`: original text context для LLM.
- `SUB_180`: searchable sub chunk.
- `SUB_260`: searchable sub chunk.

Expected Civil Code projection:

PostgreSQL:

```text
SOURCE: 10
PARENT: 326
SUB_180: 629
SUB_260: 562
```

Qdrant:

```text
PARENT: 326
SUB_180: 629
SUB_260: 562
SOURCE: 0
TOTAL: 1517
```

## Outbox Model

`vector_outbox` отвечает за reliable Qdrant synchronization.

Operations:

- `UPSERT_POINT`
- `DELETE_POINT`
- future lifecycle operations через update/delete payload

Statuses:

- `PENDING`
- `RETRY_PENDING`
- `PROCESSING`
- `COMPLETED`
- `DEAD_LETTER`

`lock_generation` защищает от stale worker completion. Если worker A взял событие, затем lock expired и worker B reclaimed это событие, worker A больше не может завершить его старым generation.

## Access Model

`access_zone_id` изолирует зоны/рабочие области. `access_level` ограничивает видимость document/chunk. В v004 RAG Core документации используйте `access_zone_id`, а не `tenant_id`, если речь о chunking/search/document-version path.

## Expected Results

Search должен возвращать только original PostgreSQL PARENT/SOURCE content как evidence и фильтровать по `access_zone_id`, `access_level`, `lifecycle_status`, active document version.

## Common Mistakes

- Путать PostgreSQL column `granularity` и Qdrant payload field `chunk_granularity`.
- Индексировать `SOURCE` как обычный searchable point без явной необходимости.
- Писать напрямую в Qdrant, минуя `vector_outbox`.
