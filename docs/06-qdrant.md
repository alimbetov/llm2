# Qdrant

For the canonical local end-to-end profile, use [local/ASTRAVECTOR_LOCAL_END_TO_END_BOOK.md](local/ASTRAVECTOR_LOCAL_END_TO_END_BOOK.md). It uses Qdrant HTTP `http://127.0.0.1:6333`, Qdrant gRPC `127.0.0.1:6334`, and collection `astravector_local_demo`.

The older examples below can refer to smoke-test isolated ports and collections.

## Purpose

Показать, как проверять Qdrant projection и payload.

## Audience

Операторы, разработчики, QA.

## Short Summary

Qdrant содержит searchable projection. Для Civil Code ожидается 1517 active searchable points: PARENT, SUB_180, SUB_260. SOURCE не индексируется как searchable point.

## Health

```bash
curl -s http://127.0.0.1:56333/health
```

## Collection

```bash
curl -s http://127.0.0.1:56333/collections/astravector_smoke_v004 | jq
```

## Civil Code Point Count

```bash
curl -s -X POST \
  http://127.0.0.1:56333/collections/astravector_smoke_v004/points/count \
  -H 'Content-Type: application/json' \
  -d '{
    "exact": true,
    "filter": {
      "must": [
        {"key": "document_id", "match": {"value": "72fd8953-9f11-5eef-a03c-ef47c3d40daa"}},
        {"key": "lifecycle_status", "match": {"value": "ACTIVE"}}
      ]
    }
  }' | jq -r '.result.count'
```

Expected:

```text
1517
```

## Granularity Count

```bash
for g in PARENT SUB_180 SUB_260 SOURCE; do
  echo "$g:"
  curl -s -X POST \
    http://127.0.0.1:56333/collections/astravector_smoke_v004/points/count \
    -H 'Content-Type: application/json' \
    -d '{
      "exact": true,
      "filter": {
        "must": [
          {"key": "document_id", "match": {"value": "72fd8953-9f11-5eef-a03c-ef47c3d40daa"}},
          {"key": "chunk_granularity", "match": {"value": "'"$g"'"}}
        ]
      }
    }' | jq -r '.result.count'
done
```

Expected:

```text
PARENT: 326
SUB_180: 629
SUB_260: 562
SOURCE: 0
```

## Required Payload Fields

Qdrant payload must contain:

- `access_zone_id`
- `binding_id`
- `document_id`
- `document_version`
- `root_chunk_id`
- `source_chunk_id`
- `parent_chunk_id`
- `chunk_id`
- `chunk_granularity`
- `representation_type`
- `access_level`
- `lifecycle_status`
- `expires_at`
- `legal_hold`
- `payload_version`
- `model_version`
- `tokenizer_version`
- `chunking_profile_version`

## PostgreSQL vs Qdrant Naming

- PostgreSQL column: `granularity`
- Qdrant payload field: `chunk_granularity`

## Common Mistakes

- Проверять `SOURCE` points и ожидать 10. В текущем searchable projection expected `SOURCE: 0`.
- Писать в Qdrant напрямую, минуя `vector_outbox`.
