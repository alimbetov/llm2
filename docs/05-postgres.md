# PostgreSQL

## Purpose

Показать, как подключаться к PostgreSQL smoke DB и проверять canonical state.

## Audience

Операторы, разработчики, QA.

## Short Summary

PostgreSQL является source of truth. Проверки нужно делать по parent tables, не по отдельным partition tables.

## Connect

```bash
PGPASSWORD='astravector_smoke_password' \
psql -h 127.0.0.1 -p 55432 -U astravector -d astravector_smoke
```

## Set Schema

```sql
SET search_path TO astravector, public;
```

## List Tables

```sql
\dt astravector.*
```

## Partitioning

`content_chunks_v004` и `document_versions` являются partitioned tables. Для обычной диагностики query parent tables directly. Partition tables использовать только при низкоуровневом debugging.

## Civil Code Chunks

```sql
SELECT granularity, count(*) AS cnt
FROM astravector.content_chunks_v004
WHERE document_id = '72fd8953-9f11-5eef-a03c-ef47c3d40daa'
GROUP BY granularity
ORDER BY granularity;
```

Expected:

```text
PARENT  326
SOURCE  10
SUB_180 629
SUB_260 562
```

## Synced Bindings

```sql
SELECT c.granularity, count(*) AS synced_bindings
FROM astravector.vector_bindings_v004 vb
JOIN astravector.content_chunks_v004 c
  ON c.access_zone_id = vb.access_zone_id
 AND c.id = vb.chunk_id
WHERE vb.document_id = '72fd8953-9f11-5eef-a03c-ef47c3d40daa'
  AND vb.lifecycle_status = 'ACTIVE'
  AND vb.qdrant_sync_status = 'SYNCED'
GROUP BY c.granularity
ORDER BY c.granularity;
```

Expected:

```text
PARENT  326
SUB_180 629
SUB_260 562
```

## Outbox

```sql
SELECT o.operation, o.status, count(*) AS cnt
FROM astravector.vector_outbox o
JOIN astravector.vector_bindings_v004 vb
  ON vb.id = o.binding_id
WHERE vb.document_id = '72fd8953-9f11-5eef-a03c-ef47c3d40daa'
GROUP BY o.operation, o.status
ORDER BY o.operation, o.status;
```

Expected:

```text
UPSERT_POINT | COMPLETED | 1517
```

## Integrity Audit Checks

Primary report:

```bash
cat smoke-tests/v004/reports/data-integrity-audit-report.md
```

Expected current summary:

```text
bindings_without_chunk: 0
active_searchable_chunks_without_document_version: 0
synced_bindings_without_completed_outbox: 0
orphan_parent_chunk_id: 0
orphan_source_chunk_id: 0
duplicate_searchable_binding_logical_keys: 0
```

## Common Mistakes

- В PostgreSQL поле называется `granularity`.
- В Qdrant payload поле называется `chunk_granularity`.
- Не считать Qdrant canonical state.
