# grpcurl Examples

## Purpose

Дать рабочие примеры для фактического proto.

## Audience

Разработчики, QA, support.

## Short Summary

Если reflection включен, можно использовать `grpcurl -plaintext 127.0.0.1:55051 list`. Если нет, используйте `-import-path proto -proto astravector_embedding.proto`.

## Reflection

```bash
grpcurl -plaintext 127.0.0.1:55051 list
grpcurl -plaintext 127.0.0.1:55051 describe astravector.embedding.v1.AstraVectorV004Control
```

## Without Reflection

```bash
grpcurl -plaintext \
  -import-path proto \
  -proto astravector_embedding.proto \
  127.0.0.1:55051 list
```

## Search

```bash
grpcurl -plaintext \
  -d '{
    "correlationId": "docs-search-example",
    "accessZoneId": "11111111-1111-4111-8111-111111111111",
    "callerAccessLevel": "PUBLIC",
    "query": "статья 223",
    "topK": 3,
    "candidateLimit": 50,
    "parentLimit": 3,
    "timeoutMs": 15000
  }' \
  127.0.0.1:55051 \
  astravector.embedding.v1.AstraVectorV004Control/Search
```

## RegisterDocumentVersion

```bash
grpcurl -plaintext \
  -d '{
    "accessZoneId": "11111111-1111-4111-8111-111111111111",
    "documentId": "72fd8953-9f11-5eef-a03c-ef47c3d40daa",
    "documentVersion": 1,
    "contentHash": "0000000000000000000000000000000000000000000000000000000000000000",
    "activationPolicy": "ACTIVE_LATEST_ONLY",
    "idempotencyKey": "docs-register-example"
  }' \
  127.0.0.1:55051 \
  astravector.embedding.v1.AstraVectorV004Control/RegisterDocumentVersion
```

`contentHash` должен быть фактическим SHA-256 документа. Значение выше является синтаксически валидным примером формата, но для реальной ingestion-команды нужно подставить настоящий hash.

## CreateMultiGranularityChunks

```bash
grpcurl -plaintext \
  -d '{
    "accessZoneId": "11111111-1111-4111-8111-111111111111",
    "documentId": "11111111-2222-4333-8444-555555555555",
    "documentVersion": 1,
    "sourceText": "Статья 1. Пример исходного текста для smoke-вызова.",
    "accessLevel": "PUBLIC",
    "profile": {
      "profileVersion": "docs-example-v1"
    },
    "metadata": {
      "source": "docs"
    },
    "idempotencyKey": "docs-chunk-example",
    "correlationId": "docs"
  }' \
  127.0.0.1:55051 \
  astravector.embedding.v1.AstraVectorV004Control/CreateMultiGranularityChunks
```

## ActivateDocumentVersion

```bash
grpcurl -plaintext \
  -d '{
    "accessZoneId": "11111111-1111-4111-8111-111111111111",
    "documentId": "11111111-2222-4333-8444-555555555555",
    "documentVersion": 1
  }' \
  127.0.0.1:55051 \
  astravector.embedding.v1.AstraVectorV004Control/ActivateDocumentVersion
```

## ResolveParentContext

```bash
grpcurl -plaintext \
  -d '{
    "accessZoneId": "11111111-1111-4111-8111-111111111111",
    "chunkIds": ["00000000-0000-0000-0000-000000000000"],
    "maxContextTokens": 1200,
    "callerAccessLevel": "PUBLIC"
  }' \
  127.0.0.1:55051 \
  astravector.embedding.v1.AstraVectorV004Control/ResolveParentContext
```

Замените `chunkIds` на реальные IDs из Search response.

## Expected Results

- `Search` возвращает `results` и `diagnostics`.
- `CreateMultiGranularityChunks` возвращает `rootChunkId`, `parentChunks`, `subChunks180`, `subChunks260`.
- Ошибочные UUID/access level/empty query должны возвращать gRPC error.

## Common Mistakes

- Использовать snake_case JSON вместо grpcurl JSON names вроде `accessZoneId`.
- Отправлять несуществующее поле `mode` для hybrid/BM25.
- Ожидать BM25-only Search до реализации production path.
