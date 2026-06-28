# gRPC API

## Purpose

Документировать реальные proto методы и поля, без придуманных request fields.

## Audience

Backend developers, gateway developers, QA.

## Short Summary

Файл proto: `proto/astravector_embedding.proto`. Основной v004 service: `astravector.embedding.v1.AstraVectorV004Control`.

## AstraVectorV004Control Methods

Реальные методы:

- `Search`
- `CreateMultiGranularityChunks`
- `GetChunkGroup`
- `ResolveParentContext`
- `RegisterDocumentVersion`
- `ActivateDocumentVersion`
- `DeleteChunkGroup`
- `UpdateChunkGroupTtl`
- `SetChunkGroupLegalHold`
- `GetRelevanceEvaluationV004`
- `SubmitRelevanceFeedbackV004`
- `ListQuarantinedPoints`
- `ResolveQuarantinedPoint`

Current implementation note:

- `Search`, `CreateMultiGranularityChunks`, `GetChunkGroup`, `ResolveParentContext`, `RegisterDocumentVersion`, and `ActivateDocumentVersion` are implemented in the current runtime.
- `DeleteChunkGroup`, `UpdateChunkGroupTtl`, `SetChunkGroupLegalHold`, `GetRelevanceEvaluationV004`, `SubmitRelevanceFeedbackV004`, `ListQuarantinedPoints`, and `ResolveQuarantinedPoint` are registered in proto but currently return `UNIMPLEMENTED`.

## RegisterDocumentVersion

Purpose: зарегистрировать document version.

Caller: backend ingestion/corpus worker.

Request fields:

- `access_zone_id`
- `document_id`
- `document_version`
- `content_hash`
- `activation_policy`
- `idempotency_key`

Response fields:

- `document_id`
- `document_version`
- `status`

Idempotency: повтор с тем же content hash должен быть безопасным; другой hash для того же ключа/version должен считаться конфликтом.

Access-zone behavior: version принадлежит `access_zone_id`.

Error behavior: invalid UUID/content hash или конфликт должен возвращать gRPC error.

## CreateMultiGranularityChunks

Purpose: создать SOURCE/PARENT/SUB_180/SUB_260 chunks, embeddings, bindings и outbox events.

Caller: ingestion/corpus worker.

Request fields:

- `access_zone_id`
- `document_id`
- `document_version`
- `source_text`
- `access_level`
- `ttl_days`
- `profile`
- `metadata`
- `idempotency_key`
- `correlation_id`

Response fields:

- `root_chunk_id`
- `parent_chunks`
- `sub_chunks_180`
- `sub_chunks_260`
- `total_chunks`
- `status`

Idempotency: повторный вызов с тем же fingerprint не должен создавать duplicate canonical chunks/bindings/outbox events.

Access-zone behavior: все chunks и bindings scoped by `access_zone_id`.

Error behavior: empty source text, invalid UUID/version/access level должны возвращать gRPC error.

## ActivateDocumentVersion

Purpose: перевести document version в `ACTIVE` после chunks/embeddings/bindings/outbox completion.

Request fields:

- `access_zone_id`
- `document_id`
- `document_version`

Response fields:

- `document_id`
- `document_version`
- `status`

Behavior: activation запрещена без chunks, embeddings, bindings и completed outbox. Для `ACTIVE_LATEST_ONLY` предыдущая active version становится superseded.

## Search

Purpose: dense retrieval по indexed Qdrant points с batch parent context fetch из PostgreSQL.

Request fields:

- `correlation_id`
- `access_zone_id`
- `caller_access_level`
- `query`
- `top_k`
- `candidate_limit`
- `parent_limit`
- `filters`
- `timeout_ms`

Response fields:

- `results`
- `diagnostics`

Result fields include:

- `document_id`
- `document_version`
- `root_chunk_id`
- `source_chunk_id`
- `parent_chunk_id`
- `matched_chunk_id`
- `matched_granularity`
- `parent_text`
- `scores`
- `citation`
- `access_zone_id`
- `access_level`

Current limitation: no `SearchMode`; no BM25-only or hybrid execution path; `SearchScoresV004` contains only `dense_score` and `final_score`.

`SearchRequestV004.filters` exists in proto for future extension. The current proven retrieval contract does not enforce arbitrary caller-provided filters. The enforced filters are `access_zone_id`, caller `access_level`, `lifecycle_status = ACTIVE`, active document version via PostgreSQL parent fetch, non-expired chunks, and searchable granularities.

## ResolveParentContext

Purpose: resolve original parent contexts for matched chunk IDs.

Request fields:

- `access_zone_id`
- `chunk_ids`
- `max_context_tokens`
- `caller_access_level`

Response fields:

- `contexts`

Access-zone behavior: contexts are scoped to `access_zone_id` and caller access level.

## GetChunkGroup

Purpose: load chunk group by `root_chunk_id`.

Request fields:

- `access_zone_id`
- `root_chunk_id`
- `caller_access_level`

Response fields:

- `chunks`

## DeleteChunkGroup

Current runtime status: `UNIMPLEMENTED`.

Request fields:

- `access_zone_id`
- `root_chunk_id`
- `correlation_id`

Response fields:

- `affected_bindings`
- `status`

Lifecycle/delete behavior is not implemented or proven as production-ready behavior.

## UpdateChunkGroupTtl

Current runtime status: `UNIMPLEMENTED`.

Request fields:

- `access_zone_id`
- `root_chunk_id`
- `ttl_days`
- `update_mode`

Response fields:

- `affected_bindings`
- `status`

## SetChunkGroupLegalHold

Current runtime status: `UNIMPLEMENTED`.

Request fields:

- `access_zone_id`
- `root_chunk_id`
- `enabled`
- `reason`
- `legal_hold_until`

Response fields:

- `affected_bindings`
- `status`

## Relevance And Quarantine Methods

Methods exist in proto:

- `GetRelevanceEvaluationV004`
- `SubmitRelevanceFeedbackV004`
- `ListQuarantinedPoints`
- `ResolveQuarantinedPoint`

Use actual proto fields before building callers.

Current runtime status: all relevance and quarantine methods listed above return `UNIMPLEMENTED`.


## Common Mistakes

- Отправлять `mode: "hybrid"` в `SearchRequestV004`. Такого поля нет.
- Ожидать `sparse_score` в `SearchScoresV004`. Такого поля нет.
- Использовать `tenant_id` в v004 control calls. Эти методы используют `access_zone_id`.
- Считать `SearchRequestV004.filters` доказанным произвольным user-filter механизмом. Сейчас он не является validated production filter surface.
