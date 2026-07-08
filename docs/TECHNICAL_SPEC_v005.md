# Техническое задание

# AstraVector v005 Hybrid Retrieval Engine

## 1. Общая информация

### 1.1. Назначение системы

`AstraVector v005` — это production-oriented hybrid retrieval engine для RAG-систем.

Система предназначена для:

```text
1. Получения dense/sparse vectors от вопросов и текстов.
2. Индексации document chunk text.
3. Сохранения dense/sparse vectors в PostgreSQL.
4. Публикации dense/sparse vectors в Qdrant.
5. Выполнения dense / sparse / hybrid search.
6. Возврата matchedText и parentText для внешнего LLM orchestration layer.
7. Объяснения ranking через Explain API.
8. Диагностики состояния документа через Debug API.
```

`AstraVector v005` **не должен генерировать финальный LLM-ответ пользователю**. Он должен возвращать только retrieval context, scores, metadata и diagnostic information.

Финальный ответ формирует внешний слой, например:

```text
ai_bro / LLM Orchestrator
```

---

## 2. Целевые требования v005

Система считается готовой, если реализованы и проверены следующие требования:

```text
1. Вопрос превращается в dense+sparse vectors.
2. Document chunk text превращается в dense+sparse vectors.
3. Dense/sparse vectors сохраняются в PostgreSQL.
4. Publisher публикует dense/sparse vectors в Qdrant.
5. Документ активируется только после полной синхронизации.
6. Search HYBRID ищет по dense+sparse.
7. SearchResponse возвращает matchedText + parentText.
8. Explain показывает dense/sparse/fusion ranking.
9. Debug показывает состояние документа без ручного SQL.
10. Финальный LLM-ответ формирует внешний слой, не AstraVector.
```

---

## 3. Архитектурные принципы

### 3.1. Основной принцип

```text
PostgreSQL = source of truth
Qdrant     = vector search index
ONNX       = реальный embedding runtime
Outbox     = надёжная доставка PostgreSQL → Qdrant
Search     = retrieval, но не answer generation
```

### 3.2. Запрещённые подходы

В v005 не допускается:

```text
1. Fake sparse.
2. Mock Qdrant.
3. Stub Search.
4. Скрытый fallback в dense-only при DENSE_SPARSE_REQUIRED.
5. Успешный статус при неполной синхронизации.
6. Активация документа без полной публикации vectors.
7. Генерация финального LLM-ответа внутри AstraVector.
```

### 3.3. Граница ответственности

| Компонент     | Ответственность                                                      |
| ------------- | -------------------------------------------------------------------- |
| `AstraVector` | Encode, Index, Publish, Search, Explain, Debug                       |
| `PostgreSQL`  | Source of truth для документов, chunks, embeddings, bindings, outbox |
| `Qdrant`      | Быстрый vector index                                                 |
| `ai_bro`      | Prompt building, LLM call, final answer                              |
| LLM           | Генерация текста ответа                                              |
| UI / Flutter  | Отображение результата пользователю                                  |

Правильный поток:

```text
User question
   ↓
ai_bro
   ↓
AstraVector.Search
   ↓
matchedText + parentText + scores + metadata
   ↓
ai_bro builds prompt
   ↓
LLM generates answer
   ↓
user-facing response
```

---

# 4. Функциональные блоки

## 4.1. Encode API

### Назначение

Encode API должен получать dense/sparse vectors от текста или вопроса.

Примеры входа:

```text
что такое гражданско-правовой договор
```

```text
Гражданско-правовой договор — это соглашение между сторонами...
```

### Основной pipeline

```text
text
   ↓
BGE-M3 tokenizer
   ↓
ONNX model
   ↓
dense vector [1024]
   ↓
sparse raw weights
   ↓
build_sparse(...)
   ↓
indices[] + values[]
```

### Существующие методы

Сохраняются:

```proto
rpc Encode(EncodeRequest) returns (EncodeResponse);
rpc EncodeBatch(EncodeBatchRequest) returns (EncodeBatchResponse);
```

### Новый метод PreviewEmbedding

Добавить:

```proto
rpc PreviewEmbedding(PreviewEmbeddingRequest) returns (PreviewEmbeddingResponse);
```

Назначение: удобная диагностика embedding без необходимости формировать полный `EncodeBatchRequest`.

### PreviewEmbeddingRequest

```proto
message PreviewEmbeddingRequest {
  string correlation_id = 1;
  string text = 2;
  EncodingPurpose purpose = 3;
  EmbeddingModeV005 embedding_mode = 4;

  bool include_dense = 10;
  bool include_sparse = 11;
  bool include_full_dense = 12;
  uint32 top_sparse = 13;

  uint64 timeout_ms = 20;
}
```

### PreviewEmbeddingResponse

```proto
message PreviewEmbeddingResponse {
  string model_version = 1;
  string tokenizer_version = 2;
  string dense_version = 3;
  string sparse_version = 4;

  DensePreviewV005 dense = 10;
  SparsePreviewV005 sparse = 11;
  TokenizationPreviewV005 tokenization = 12;

  repeated DiagnosticWarningV005 warnings = 20;
}
```

```proto
message DensePreviewV005 {
  uint32 dimension = 1;
  float norm = 2;
  repeated float preview_values = 3;
  repeated float full_values = 4;
}

message SparsePreviewV005 {
  uint32 non_zero_count = 1;
  repeated uint32 indices = 2;
  repeated float values = 3;
}

message TokenizationPreviewV005 {
  uint32 token_count = 1;
  uint32 max_tokens = 2;
  bool truncated = 3;
}

message DiagnosticWarningV005 {
  string code = 1;
  string message = 2;
}
```

### EmbeddingMode

Добавить enum:

```proto
enum EmbeddingModeV005 {
  EMBEDDING_MODE_V005_UNSPECIFIED = 0;
  EMBEDDING_MODE_V005_DENSE_ONLY = 1;
  EMBEDDING_MODE_V005_DENSE_SPARSE_IF_AVAILABLE = 2;
  EMBEDDING_MODE_V005_DENSE_SPARSE_REQUIRED = 3;
}
```

### Поведение режимов

| Режим                       | Поведение                                                                  |
| --------------------------- | -------------------------------------------------------------------------- |
| `DENSE_ONLY`                | Строится только dense vector                                               |
| `DENSE_SPARSE_IF_AVAILABLE` | Строится dense, sparse строится при доступности; если sparse нет — warning |
| `DENSE_SPARSE_REQUIRED`     | Dense и sparse обязательны; если sparse нет — ошибка                       |

### Ошибка SPARSE_UNAVAILABLE

Если `DENSE_SPARSE_REQUIRED`, но ONNX не отдаёт sparse output:

```json
{
  "error": {
    "code": "SPARSE_UNAVAILABLE",
    "message": "Sparse embedding requested but model output is not available. Check ONNX model outputs or use DENSE_ONLY.",
    "details": {
      "modelVersion": "bge_m3_onnx_int8_v1",
      "availableOutputs": ["sentence_embedding"]
    }
  }
}
```

### Acceptance criteria

Encode API считается готовым, если:

```text
1. EncodeBatch возвращает dense vector размерности 1024.
2. EncodeBatch возвращает sparse indices/values при наличии sparse output.
3. PreviewEmbedding возвращает dense summary.
4. PreviewEmbedding возвращает sparse summary.
5. PreviewEmbedding возвращает token_count, max_tokens, truncated.
6. DENSE_SPARSE_REQUIRED падает с SPARSE_UNAVAILABLE, если sparse недоступен.
7. DENSE_SPARSE_IF_AVAILABLE возвращает warning, если sparse недоступен.
```

---

# 5. Index API

## 5.1. Назначение

Index API должен принимать document text, создавать chunks и сохранять dense/sparse vectors в PostgreSQL.

Основной метод:

```proto
rpc CreateMultiGranularityChunks(CreateMultiGranularityChunksRequest)
    returns (CreateMultiGranularityChunksResponse);
```

## 5.2. Pipeline

```text
sourceText
   ↓
SOURCE / PARENT / SUB_180 / SUB_260
   ↓
for each searchable chunk:
   BGE-M3 tokenizer
   ONNX inference
   dense vector
   sparse vector
   ↓
PostgreSQL:
   embedding_cache_entries
   embedding_dense
   embedding_sparse
   vector_bindings_v004
   vector_outbox
```

## 5.3. Chunking rule

Для короткого документа ожидается:

```text
SOURCE   = 1
PARENT   = 1
SUB_180  = 1
SUB_260  = 1
```

Векторизуются:

```text
PARENT
SUB_180
SUB_260
```

`SOURCE` используется как root/source chunk и не обязан иметь vector.

## 5.4. Требуемые counts

Для короткого документа:

```text
chunks         = 4
dense_vectors  = 3
sparse_vectors = 3
bindings       = 3
outbox         = 3
```

## 5.5. Расширение CreateMultiGranularityChunksRequest

```proto
message CreateMultiGranularityChunksRequest {
  string access_zone_id = 1;
  string document_id = 2;
  uint64 document_version = 3;
  string source_text = 4;
  AccessLevel access_level = 5;

  ChunkingProfileV004 profile = 6;
  optional uint32 ttl_days = 7;
  string idempotency_key = 8;
  map<string, string> metadata = 9;

  EmbeddingModeV005 embedding_mode = 20;
  PublishModeV005 publish_mode = 21;
}
```

```proto
enum PublishModeV005 {
  PUBLISH_MODE_V005_UNSPECIFIED = 0;
  PUBLISH_MODE_V005_OUTBOX = 1;
  PUBLISH_MODE_V005_NONE = 2;
}
```

Default:

```text
embedding_mode = DENSE_SPARSE_REQUIRED
publish_mode = OUTBOX
```

## 5.6. Расширение CreateMultiGranularityChunksResponse

```proto
message IndexSummaryV005 {
  uint32 chunks_total = 1;
  uint32 source_chunks = 2;
  uint32 parent_chunks = 3;
  uint32 sub180_chunks = 4;
  uint32 sub260_chunks = 5;

  uint32 dense_vectors = 10;
  uint32 sparse_vectors = 11;
  uint32 bindings = 12;
  uint32 outbox_created = 13;

  string status = 20;
}
```

Добавить в response:

```proto
IndexSummaryV005 summary = 20;
```

## 5.7. Transaction rule

Сохранение должно быть атомарным:

```text
content_chunks_v004
embedding_cache_entries
embedding_dense
embedding_sparse
vector_bindings_v004
vector_outbox
```

Если `DENSE_SPARSE_REQUIRED`, но sparse не построен для любого searchable chunk — вся операция должна завершиться ошибкой и rollback.

## 5.8. Acceptance criteria

Index API считается готовым, если:

```text
1. RegisterDocumentVersion создаёт document_versions.
2. CreateMultiGranularityChunks создаёт SOURCE/PARENT/SUB chunks.
3. Dense vectors сохраняются в embedding_dense.
4. Sparse vectors сохраняются в embedding_sparse.
5. Для каждого searchable chunk создаётся binding.
6. Для каждого binding создаётся UPSERT_POINT в vector_outbox.
7. При DENSE_SPARSE_REQUIRED отсутствие sparse приводит к rollback.
8. Повторный вызов с тем же idempotency_key идемпотентен.
```

---

# 6. Publish API

## 6.1. Назначение

Publish API обеспечивает доставку vectors из PostgreSQL в Qdrant.

Основной компонент:

```text
astravector-qdrant-publisher
```

## 6.2. Publisher flow

```text
start
   ↓
load config
   ↓
connect PostgreSQL
   ↓
connect Qdrant
   ↓
ensure_collection_dense_sparse
   ↓
validate_collection_schema
   ↓
claim vector_outbox rows
   ↓
load binding
   ↓
load dense vector
   ↓
load sparse vector
   ↓
upsert Qdrant point
   ↓
mark outbox COMPLETED
   ↓
mark binding SYNCED
```

## 6.3. Qdrant collection schema

Publisher должен создавать collection автоматически, если она отсутствует:

```json
{
  "vectors": {
    "dense": {
      "size": 1024,
      "distance": "Cosine"
    }
  },
  "sparse_vectors": {
    "sparse": {
      "index": {
        "on_disk": false
      }
    }
  }
}
```

## 6.4. Schema compatibility check

Если collection уже существует, publisher должен проверить:

```text
1. Есть named vector dense.
2. dense.size = 1024.
3. dense.distance = Cosine.
4. Есть sparse vector sparse.
5. Collection name соответствует config.
6. Vector names соответствуют config.
```

Если схема несовместима:

```text
QDRANT_COLLECTION_SCHEMA_MISMATCH
```

Upsert не выполняется.

## 6.5. Qdrant point schema

Каждый point должен содержать:

```json
{
  "id": "qdrant_point_id",
  "vector": {
    "dense": [0.01, 0.02],
    "sparse": {
      "indices": [100, 205, 901],
      "values": [0.8, 0.6, 0.4]
    }
  },
  "payload": {
    "access_zone_id": "...",
    "document_id": "...",
    "document_version": 1,
    "chunk_id": "...",
    "parent_chunk_id": "...",
    "root_chunk_id": "...",
    "source_chunk_id": "...",
    "chunk_granularity": "SUB_180",
    "access_level": "INTERNAL",
    "lifecycle_status": "ACTIVE",
    "representation_type": "ORIGINAL",
    "model_version": "bge_m3_onnx_int8_v1",
    "tokenizer_version": "bge_m3_tokenizer_v1",
    "dense_version": "...",
    "sparse_version": "...",
    "chunking_version": "..."
  }
}
```

## 6.6. GetVectorSyncStatus

Метод:

```proto
rpc GetVectorSyncStatus(GetVectorSyncStatusRequest)
    returns (GetVectorSyncStatusResponse);
```

Response:

```proto
message GetVectorSyncStatusResponse {
  string document_status = 1;

  uint32 expected_bindings = 10;
  uint32 synced_bindings = 11;
  uint32 pending_bindings = 12;
  uint32 failed_bindings = 13;

  uint32 outbox_pending = 20;
  uint32 outbox_retry_pending = 21;
  uint32 outbox_completed = 22;
  uint32 outbox_failed = 23;

  string qdrant_collection = 30;
  bool qdrant_collection_exists = 31;
  uint32 qdrant_points_expected = 32;
  uint32 qdrant_points_found = 33;

  bool ready_to_activate = 40;

  string last_sync_attempt_at = 50;
  string last_sync_error_code = 51;
  string last_sync_error_message = 52;

  repeated DiagnosticWarningV005 warnings = 60;
}
```

## 6.7. RetryVectorOutbox

Метод:

```proto
rpc RetryVectorOutbox(RetryVectorOutboxRequest)
    returns (RetryVectorOutboxResponse);
```

Request:

```proto
message RetryVectorOutboxRequest {
  string access_zone_id = 1;
  string document_id = 2;
  uint64 document_version = 3;

  optional string operation = 4;
  optional string status = 5;
}
```

Response:

```proto
message RetryVectorOutboxResponse {
  uint32 matched = 1;
  uint32 reset_to_pending = 2;
  repeated string affected_outbox_ids = 3;
}
```

Default retry statuses:

```text
FAILED
RETRY_PENDING
```

## 6.8. Acceptance criteria

Publish API готов, если:

```text
1. Publisher создаёт dense+sparse Qdrant collection.
2. Publisher проверяет совместимость существующей collection.
3. Publisher публикует dense+sparse named vectors.
4. Qdrant points count = vector_bindings count.
5. Outbox status становится COMPLETED.
6. Binding qdrant_sync_status становится SYNCED.
7. Ошибки Qdrant пишутся в error_code/error_message.
8. RetryVectorOutbox возвращает события в PENDING.
9. GetVectorSyncStatus показывает состояние без SQL.
```

---

# 7. Search API

## 7.1. Назначение

Search API принимает вопрос пользователя и возвращает ranked retrieval context.

Search API не возвращает финальный LLM-ответ.

## 7.2. Search modes

Добавить enum:

```proto
enum SearchModeV005 {
  SEARCH_MODE_V005_UNSPECIFIED = 0;
  SEARCH_MODE_V005_DENSE = 1;
  SEARCH_MODE_V005_SPARSE = 2;
  SEARCH_MODE_V005_HYBRID = 3;
}
```

Default:

```text
HYBRID
```

## 7.3. SearchRequest

```proto
message SearchRequestV004 {
  string correlation_id = 1;
  string access_zone_id = 2;
  AccessLevel caller_access_level = 3;
  string query = 4;
  uint32 top_k = 5;
  uint32 candidate_limit = 6;
  uint32 parent_limit = 7;
  repeated SearchFilterV004 filters = 8;
  uint32 timeout_ms = 9;

  SearchModeV005 search_mode = 20;
  bool include_debug = 21;
  bool include_vectors = 22;
  EmbeddingModeV005 embedding_mode = 23;

  optional string model_version = 30;
  optional string tokenizer_version = 31;
  optional string dense_version = 32;
  optional string sparse_version = 33;
  optional string chunking_version = 34;
}
```

## 7.4. Search flow

```text
query
   ↓
query embedding:
   dense vector
   sparse vector
   ↓
if searchMode = DENSE:
   Qdrant dense search
if searchMode = SPARSE:
   Qdrant sparse search
if searchMode = HYBRID:
   Qdrant dense search
   Qdrant sparse search
   RRF fusion
   ↓
deduplicate by parent_chunk_id
   ↓
load matchedText
   ↓
load parentText from PostgreSQL
   ↓
return SearchResponse
```

## 7.5. SearchResult

Добавить `matched_text`.

```proto
message SearchResultV004 {
  string document_id = 1;
  uint64 document_version = 2;
  string root_chunk_id = 3;
  string source_chunk_id = 4;
  string parent_chunk_id = 5;
  string matched_chunk_id = 6;
  ChunkGranularityV004 matched_granularity = 7;
  string parent_text = 8;
  SearchScoresV004 scores = 9;
  SearchCitationV004 citation = 10;
  string access_zone_id = 11;
  AccessLevel access_level = 12;

  string matched_text = 20;
}
```

## 7.6. Семантика matchedText

```text
1. Если matchedGranularity = SUB_180:
   matchedText = текст SUB_180 chunk.

2. Если matchedGranularity = SUB_260:
   matchedText = текст SUB_260 chunk.

3. Если matchedGranularity = PARENT:
   matchedText = текст PARENT chunk.
   В этом случае matchedText может совпадать с parentText.

4. SOURCE chunk не должен попадать в SearchResponse.
```

## 7.7. SearchScores

```proto
message SearchScoresV004 {
  float dense_score = 1;
  float sparse_score = 2;
  float fusion_score = 3;
  float final_score = 4;
}
```

## 7.8. RRF fusion

```text
fusion_score =
  1 / (rrf_k + dense_rank)
+ 1 / (rrf_k + sparse_rank)
```

Default:

```text
rrf_k = 60
```

Если кандидат найден только dense:

```text
fusion_score = 1 / (rrf_k + dense_rank)
```

Если только sparse:

```text
fusion_score = 1 / (rrf_k + sparse_rank)
```

## 7.9. Access filtering

Qdrant search должен фильтровать:

```text
access_zone_id = request.access_zone_id
access_level <= caller_access_level
lifecycle_status = ACTIVE
```

PostgreSQL parent lookup должен повторно проверять:

```text
access_zone_id
access_level
document status = ACTIVE
lifecycle_status = ACTIVE
```

## 7.10. Version filtering

Если в `SearchRequest` переданы:

```text
model_version
tokenizer_version
dense_version
sparse_version
chunking_version
```

они должны применяться:

```text
1. В Qdrant filter.
2. В PostgreSQL lookup.
```

## 7.11. Acceptance criteria

Search API готов, если:

```text
1. SearchMode DENSE работает.
2. SearchMode SPARSE работает.
3. SearchMode HYBRID работает.
4. HYBRID использует dense + sparse + RRF.
5. SearchResponse возвращает matchedText.
6. SearchResponse возвращает parentText.
7. Search не возвращает неактивные документы.
8. Search не возвращает документы из другого accessZoneId.
9. Search не возвращает документы с accessLevel выше callerAccessLevel.
10. Search может фильтровать по modelVersion/tokenizerVersion/denseVersion/sparseVersion/chunkingVersion.
11. Search не вызывает LLM и не формирует финальный answer.
```

---

# 8. Explain API

## 8.1. Назначение

Explain API должен технически объяснить retrieval-решение:

```text
почему найден chunk
какие dense candidates
какие sparse candidates
как сработал fusion
какой parent выбран
какие фильтры применены
```

## 8.2. Метод

```proto
rpc ExplainSearch(ExplainSearchRequest)
    returns (ExplainSearchResponse);
```

## 8.3. ExplainSearchRequest

```proto
message ExplainSearchRequest {
  string correlation_id = 1;
  string access_zone_id = 2;
  AccessLevel caller_access_level = 3;
  string query = 4;

  SearchModeV005 search_mode = 10;
  EmbeddingModeV005 embedding_mode = 11;

  uint32 top_k = 20;
  uint32 candidate_limit = 21;
  uint64 timeout_ms = 22;
}
```

## 8.4. ExplainSearchResponse

```proto
message ExplainSearchResponse {
  string query = 1;
  QueryEmbeddingSummaryV005 query_embedding = 2;

  repeated ExplainCandidateV005 dense_candidates = 10;
  repeated ExplainCandidateV005 sparse_candidates = 11;
  repeated ExplainFusionCandidateV005 fusion = 12;
  repeated ExplainSelectedParentV005 selected_parents = 13;

  repeated AppliedFilterV005 applied_filters = 20;
  SearchDiagnosticsV004 diagnostics = 30;
}
```

```proto
message QueryEmbeddingSummaryV005 {
  uint32 dense_dimension = 1;
  uint32 sparse_non_zero_count = 2;
  repeated SparseTokenPreviewV005 top_sparse_tokens = 3;
}

message SparseTokenPreviewV005 {
  uint32 token_id = 1;
  float weight = 2;
}

message ExplainCandidateV005 {
  uint32 rank = 1;
  float score = 2;
  string qdrant_point_id = 3;
  string chunk_id = 4;
  string parent_chunk_id = 5;
  ChunkGranularityV004 granularity = 6;
}

message ExplainFusionCandidateV005 {
  uint32 rank = 1;
  string chunk_id = 2;
  optional uint32 dense_rank = 3;
  optional uint32 sparse_rank = 4;
  float dense_score = 5;
  float sparse_score = 6;
  float fusion_score = 7;
  string reason = 8;
}

message ExplainSelectedParentV005 {
  string parent_chunk_id = 1;
  string selected_because = 2;
}

message AppliedFilterV005 {
  string key = 1;
  string op = 2;
  string value = 3;
}
```

## 8.5. Acceptance criteria

Explain API готов, если показывает:

```text
1. Query embedding summary.
2. sparseNonZeroCount.
3. top_sparse_tokens.
4. Dense candidates.
5. Sparse candidates.
6. Fusion ranking.
7. Selected parent context.
8. Applied filters.
9. Scores and ranks.
10. Diagnostics timings.
```

---

# 9. Debug API

## 9.1. Назначение

Debug API должен показывать состояние документа без ручного SQL.

## 9.2. Метод

```proto
rpc DebugDocumentState(DebugDocumentStateRequest)
    returns (DebugDocumentStateResponse);
```

## 9.3. DebugDocumentStateRequest

```proto
message DebugDocumentStateRequest {
  string access_zone_id = 1;
  string document_id = 2;
  uint64 document_version = 3;

  bool include_chunks = 10;
  bool include_vectors = 11;
  bool include_outbox = 12;
  bool include_qdrant = 13;
}
```

## 9.4. DebugDocumentStateResponse

```proto
message DebugDocumentStateResponse {
  DebugDocumentInfoV005 document = 1;
  repeated DebugChunkInfoV005 chunks = 2;
  DebugVectorInfoV005 vectors = 3;
  DebugOutboxInfoV005 outbox = 4;
  DebugQdrantInfoV005 qdrant = 5;
  bool ready_to_activate = 6;
  repeated DiagnosticWarningV005 warnings = 7;
}
```

```proto
message DebugDocumentInfoV005 {
  string status = 1;
  string content_hash = 2;
  string model_version = 3;
  string tokenizer_version = 4;
  string chunking_version = 5;
}

message DebugChunkInfoV005 {
  string chunk_id = 1;
  string parent_chunk_id = 2;
  string root_chunk_id = 3;
  ChunkGranularityV004 granularity = 4;
  uint32 actual_token_count = 5;
  string lifecycle_status = 6;
}

message DebugVectorInfoV005 {
  uint32 dense_count = 1;
  uint32 sparse_count = 2;
  uint32 bindings_count = 3;
}

message DebugOutboxInfoV005 {
  uint32 pending = 1;
  uint32 retry_pending = 2;
  uint32 completed = 3;
  uint32 failed = 4;
}

message DebugQdrantInfoV005 {
  string collection = 1;
  bool collection_exists = 2;
  uint32 points_expected = 3;
  uint32 points_found = 4;

  repeated MissingQdrantPointV005 points_missing = 10;
}

message MissingQdrantPointV005 {
  string chunk_id = 1;
  string binding_id = 2;
  string qdrant_point_id = 3;
  string reason = 4;
}
```

## 9.5. Missing point reasons

```text
QDRANT_POINT_NOT_FOUND
BINDING_NOT_SYNCED
OUTBOX_NOT_COMPLETED
QDRANT_COLLECTION_MISSING
```

## 9.6. Acceptance criteria

Debug API готов, если без SQL видно:

```text
1. document status;
2. chunks;
3. dense vector count;
4. sparse vector count;
5. binding count;
6. outbox status;
7. Qdrant collection state;
8. Qdrant points count;
9. missing Qdrant points;
10. readyToActivate.
```

---

# 10. Activation

## 10.1. Правило обычной активации

Документ может перейти в `ACTIVE` только если:

```text
1. document_versions.status = INDEXING или REGISTERED.
2. Все searchable chunks созданы.
3. Все searchable chunks имеют dense vector.
4. Если sparse required — все searchable chunks имеют sparse vector.
5. Для всех searchable chunks есть vector_bindings_v004.
6. Все UPSERT_POINT outbox события завершены COMPLETED.
7. Все bindings имеют qdrant_sync_status = SYNCED.
8. Qdrant collection существует.
9. Qdrant points count по document/version/accessZone совпадает с expected count.
```

Если условие не выполнено:

```json
{
  "code": "DOCUMENT_NOT_READY_TO_ACTIVATE",
  "message": "Document cannot be activated because vector synchronization is incomplete.",
  "details": {
    "expectedBindings": 3,
    "syncedBindings": 2,
    "pendingOutbox": 1,
    "qdrantPointsExpected": 3,
    "qdrantPointsFound": 2
  }
}
```

## 10.2. Force activation

Расширить request:

```proto
message ActivateDocumentVersionRequest {
  string access_zone_id = 1;
  string document_id = 2;
  uint64 document_version = 3;

  bool force_activate = 10;
  string force_reason = 11;
}
```

Default:

```text
force_activate = false
```

Force activation разрешён только admin/internal principal.

Даже при `force_activate = true` нельзя активировать документ, если:

```text
1. document_id не существует.
2. access_zone_id не совпадает.
3. chunks отсутствуют полностью.
4. нет ни одного vector binding.
5. force_reason пустой.
```

Force activation должна логироваться:

```text
document_id
document_version
access_zone_id
force_reason
caller
timestamp
previous_status
new_status
```

## 10.3. Acceptance criteria

Activation готов, если:

```text
1. Документ не активируется при pending outbox.
2. Документ не активируется при unsynced bindings.
3. Документ не активируется при missing sparse, если required.
4. Документ не активируется при missing Qdrant points.
5. Документ активируется только после полной синхронизации.
6. force_activate доступен только admin/internal.
7. force_reason обязателен.
8. Force activation логируется.
```

---

# 11. Security

## 11.1. Разделение API

Production/public:

```text
Search
```

Internal/admin:

```text
PreviewEmbedding
ExplainSearch
DebugDocumentState
RetryVectorOutbox
ListVectorOutboxFailures
ForceActivate
```

## 11.2. Обязательные проверки

```text
1. accessZoneId обязателен.
2. callerAccessLevel обязателен для Search/Explain.
3. Qdrant search filter по accessZoneId/accessLevel.
4. PostgreSQL parent lookup повторно проверяет accessZoneId/accessLevel.
5. Debug/Explain/Retry доступны только internal/admin.
6. includeVectors=false по умолчанию.
7. full dense vector не возвращается без includeFullDense=true.
```

---

# 12. Configuration

Добавить или уточнить в `config/application.yaml`:

```yaml
embedding:
  dense:
    enabled: true
    dimension: 1024
    distance: COSINE

  sparse:
    enabled: ${ASTRAVECTOR_SPARSE_ENABLED:-true}
    required: ${ASTRAVECTOR_SPARSE_REQUIRED:-true}
    min_weight: ${ASTRAVECTOR_SPARSE_MIN_WEIGHT:-0.01}
    max_non_zero: ${ASTRAVECTOR_SPARSE_MAX_NON_ZERO:-256}

qdrant:
  collection: ${ASTRAVECTOR_QDRANT_COLLECTION:-astravector_v004}
  auto_create_collection: ${ASTRAVECTOR_QDRANT_AUTO_CREATE_COLLECTION:-true}
  dense_vector_name: ${ASTRAVECTOR_QDRANT_DENSE_VECTOR_NAME:-dense}
  sparse_vector_name: ${ASTRAVECTOR_QDRANT_SPARSE_VECTOR_NAME:-sparse}
  validate_collection_schema: ${ASTRAVECTOR_QDRANT_VALIDATE_SCHEMA:-true}

search:
  default_mode: ${ASTRAVECTOR_SEARCH_DEFAULT_MODE:-HYBRID}
  candidate_limit: ${ASTRAVECTOR_SEARCH_CANDIDATE_LIMIT:-50}
  parent_limit: ${ASTRAVECTOR_SEARCH_PARENT_LIMIT:-5}
  rrf_k: ${ASTRAVECTOR_SEARCH_RRF_K:-60}

explain:
  top_sparse_tokens: ${ASTRAVECTOR_EXPLAIN_TOP_SPARSE_TOKENS:-5}
```

---

# 13. Observability

## 13.1. Sparse metrics

```text
astravector_sparse_available
astravector_sparse_required_requests_total
astravector_sparse_unavailable_total
astravector_sparse_fallback_total
astravector_sparse_non_zero_count
```

## 13.2. Index metrics

```text
astravector_index_requests_total
astravector_index_chunks_total
astravector_index_dense_vectors_total
astravector_index_sparse_vectors_total
astravector_index_failures_total
```

## 13.3. Outbox metrics

```text
astravector_outbox_pending_total
astravector_outbox_retry_pending_total
astravector_outbox_completed_total
astravector_outbox_failed_total
```

## 13.4. Search metrics

```text
astravector_search_requests_total
astravector_search_latency_ms
astravector_search_dense_candidates_total
astravector_search_sparse_candidates_total
astravector_search_results_total
```

---

# 14. Порядок реализации

## Stage 1 — Proto + Config foundation

```text
1. Добавить EmbeddingModeV005.
2. Добавить SearchModeV005.
3. Добавить PreviewEmbedding messages.
4. Добавить ExplainSearch messages.
5. Добавить DebugDocumentState messages.
6. Расширить SearchRequest/SearchResult.
7. Расширить ActivateDocumentVersionRequest.
8. Расширить RetryVectorOutboxRequest.
9. Расширить GetVectorSyncStatusResponse.
10. Добавить config параметры.
```

## Stage 2 — PreviewEmbedding + sparse strict mode

```text
1. Реализовать PreviewEmbedding.
2. Добавить token_count/max_tokens/truncated.
3. Добавить top_sparse_tokens.
4. Реализовать DENSE_SPARSE_REQUIRED.
5. Реализовать SPARSE_UNAVAILABLE.
```

## Stage 3 — Index dense+sparse hardening

```text
1. CreateMultiGranularityChunks принимает embeddingMode.
2. Sparse required приводит к rollback.
3. IndexSummary считает dense/sparse/bindings/outbox.
4. Проверить counts.
```

## Stage 4 — Publisher hardening

```text
1. ensure_collection_dense_sparse.
2. validate_collection_schema.
3. Qdrant payload version metadata.
4. GetVectorSyncStatus с last_sync_attempt_at.
5. RetryVectorOutbox с фильтром status.
6. Debug qdrant_points_missing.
```

## Stage 5 — Search hardening

```text
1. SearchMode DENSE/SPARSE/HYBRID.
2. matchedText semantics.
3. modelVersion/tokenizerVersion filters.
4. parentText lookup с PostgreSQL access check.
5. includeDebug/includeVectors rules.
```

## Stage 6 — Explain + Debug

```text
1. ExplainSearch показывает query_sparse_top_tokens.
2. ExplainSearch показывает dense/sparse candidates.
3. ExplainSearch показывает fusion ranking.
4. DebugDocumentState показывает missing Qdrant points.
5. DebugDocumentState показывает readyToActivate.
```

## Stage 7 — Activation hardening

```text
1. Строгий activation gate.
2. Qdrant point count validation.
3. force_activate для admin/internal.
4. force_reason required.
5. structured audit log.
```

---

# 15. Final E2E acceptance test

Полный E2E тест v005 должен проверять:

```text
1. Health = SERVING.
2. GetCapabilities содержит DENSE + BGE_LEARNED_SPARSE.
3. PreviewEmbedding(question):
   - dense dimension = 1024
   - sparse nonZeroCount > 0
   - token_count > 0
   - top_sparse_tokens not empty

4. RegisterDocumentVersion создаёт document version.

5. CreateMultiGranularityChunks:
   - chunks = 4
   - dense_vectors = 3
   - sparse_vectors = 3
   - bindings = 3
   - outbox = 3

6. Publisher:
   - создаёт Qdrant collection dense+sparse
   - validate_collection_schema = OK
   - публикует 3 points
   - outbox = COMPLETED
   - bindings = SYNCED

7. GetVectorSyncStatus:
   - pointsExpected = 3
   - pointsFound = 3
   - lastSyncAttemptAt not empty
   - readyToActivate = true

8. ActivateDocumentVersion:
   - без force проходит только после полной sync
   - при incomplete sync возвращает DOCUMENT_NOT_READY_TO_ACTIVATE

9. Search DENSE:
   - возвращает matchedText
   - возвращает parentText

10. Search SPARSE:
   - возвращает matchedText
   - возвращает parentText

11. Search HYBRID:
   - использует dense + sparse
   - возвращает fusionScore
   - возвращает matchedText + parentText

12. Search с modelVersion filter:
   - возвращает только matching version
   - не возвращает old model vectors при заданном фильтре

13. ExplainSearch:
   - показывает query sparse top tokens
   - показывает dense candidates
   - показывает sparse candidates
   - показывает fusion ranking

14. DebugDocumentState:
   - показывает chunks
   - показывает dense/sparse counts
   - показывает outbox statuses
   - показывает Qdrant points
   - points_missing пустой при полной sync

15. RetryVectorOutbox:
   - умеет reset FAILED
   - умеет reset RETRY_PENDING
   - умеет фильтровать по operation/status

16. AstraVector не вызывает LLM.
17. AstraVector не возвращает финальный human answer.
```

---

# 16. Out of scope для v005

В v005 не входят:

```text
1. Финальная LLM-генерация ответа.
2. Cross-encoder reranker.
3. Kafka/RabbitMQ.
4. UI dashboard.
5. Multi-model routing.
6. Сложные ACL policies beyond accessZone/accessLevel.
7. Query vector cache.
8. Auto activation by default.
9. LLM-generated explanation.
10. Token-id to readable token decoding.
```

---

# 17. Итоговое заключение

`AstraVector v005` должен быть реализован как честный, наблюдаемый и управляемый hybrid retrieval engine.

Система должна выполнять:

```text
text/question → dense+sparse
document chunk → dense+sparse
PostgreSQL persistence
Qdrant publishing
sync validation
activation gate
hybrid search
matchedText + parentText
explain
debug
```

Финальный LLM-ответ остаётся ответственностью внешнего orchestration layer, например `ai_bro`.

Главное требование v005:

```text
никаких заглушек;
никаких скрытых fallback;
никаких fake sparse;
никаких успешных статусов при неполной синхронизации;
никакой генерации LLM-ответа внутри AstraVector.
```
