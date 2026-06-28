# AstraVector_v002

## Актуализированное техническое задание на production-доработку

**Базовый проект:** `AstraVector_v001.zip`
**SHA-256 базового архива:**

```text
1a0fe1cc628973ebdcbec5edefc6a5438e0075863061658f455517e837e3e7af
```

**Целевая версия:** `AstraVector_v002`
**Архитектурный принцип:**

```text
AstraVector
+ PostgreSQL
+ ONNX Runtime
+ Moka L1 cache
```

Дополнительные брокеры, Redis и внешние lock-сервисы не используются.

---

# 1. Цель доработки

Довести существующий проект `AstraVector_v001` до минимально достаточного production-ready состояния, сохранив текущую структуру и стек.

Ключевые гарантии целевой версии:

1. Реальный BGE-M3 ONNX inference.
2. Отсутствие параллельного вычисления одного cache key несколькими pod.
3. Защита результата от stale owner.
4. Безопасные повторы запросов через `idempotency_key`.
5. Строгая семантика `PersistenceMode.REQUIRED`.
6. Управляемые очереди и deadlines.
7. Корректный request/item audit.
8. Readiness только после проверки модели.
9. Наблюдаемость и graceful shutdown.
10. Отсутствие фиктивных dense/sparse представлений.

---

# 2. Что остаётся без изменений

Сохраняются:

* Rust;
* Tokio;
* Tonic;
* ONNX Runtime;
* SQLx;
* PostgreSQL;
* pgvector;
* Moka;
* unary gRPC `Encode` и `EncodeBatch`;
* query/document queues;
* dense `float[1024]`;
* BGE learned sparse `indices[] + values[]`;
* автоматические SQLx migrations для development;
* отсутствие записи в Qdrant;
* отсутствие document parsing;
* отсутствие BM25 и reranker внутри AstraVector.

---

# 3. Обновлённый основной pipeline

```text
gRPC request
    ↓
service authentication
    ↓
request validation
    ↓
canonical request hash
    ↓
idempotency lookup
    ↓
request/item audit creation
    ↓
L1 Moka lookup
    ↓
L2 PostgreSQL lookup
    ↓
distributed claim before inference
    ├── COMPLETED
    │      → load result
    ├── PROCESSING by another owner
    │      → poll with deadline
    ├── FAILED retryable
    │      → acquire new lease
    └── NOT_FOUND
           → create claim
                 ↓
          query/document scheduler
                 ↓
          length bucket
                 ↓
          dynamic micro-batch
                 ↓
          real ONNX inference
                 ↓
          dense/sparse validation
                 ↓
          REQUIRED/BEST_EFFORT persistence
                 ↓
          L1 update
                 ↓
          request/item status update
                 ↓
          gRPC response
```

---

# 4. P0 — обязательные блокирующие доработки

## 4.1. Реальный ONNX Runtime

Необходимо реализовать production `OnnxBgeM3Engine`.

Обязанности:

* загрузить ONNX-файл;
* создать `ort::Session`;
* определить реальные input/output names;
* выбрать Execution Provider;
* построить tensors;
* выполнить inference;
* проверить output shapes;
* извлечь dense output;
* извлечь sparse lexical weights;
* выполнить pooling и normalization;
* преобразовать результат в DTO.

Минимальные inputs:

```text
input_ids
attention_mask
```

Опционально:

```text
token_type_ids
```

Поддерживаемые dense outputs:

```text
sentence_embedding [batch, 1024]
```

или:

```text
last_hidden_state [batch, sequence, 1024]
```

Для token-level output:

```text
dense_raw = last_hidden_state[:, 0, :]
dense = L2_NORMALIZE(dense_raw)
```

Запрещено использовать deterministic/test backend в production profile.

---

## 4.2. Проверка dense+sparse ONNX artifact

До объявления sparse capability модель должна фактически содержать sparse output:

```text
lexical_weights [batch, sequence]
```

или эквивалентный выход, явно описанный в model adapter.

Если sparse-head отсутствует:

```text
capabilities.learned_sparse = false
```

Запрос с `BGE_LEARNED_SPARSE`:

```text
FAILED_PRECONDITION
```

Запрещено:

* вычислять sparse через частоты слов;
* подменять learned sparse BM25;
* возвращать пустой sparse как успешный;
* генерировать тестовые sparse weights в production.

---

# 5. Строгая state machine cache entry

Статусы cache entry:

```text
PROCESSING
COMPLETED
FAILED
```

Рекомендуемая модель результата claim:

```rust
enum ClaimResult {
    Acquired {
        cache_entry_id: Uuid,
        lease_token: i64,
    },
    Completed {
        cache_entry_id: Uuid,
        result: CachedEmbedding,
    },
    ProcessingByOther {
        cache_entry_id: Uuid,
        lease_expires_at: DateTime<Utc>,
    },
    RetryAcquired {
        cache_entry_id: Uuid,
        lease_token: i64,
    },
}
```

Inference разрешён только для:

```text
Acquired
RetryAcquired
```

---

# 6. Distributed claim до inference

## 6.1. Новый cache key

Перед формированием key необходимо:

1. Проверить enum values.
2. Удалить duplicate representations.
3. Отсортировать representations.
4. Использовать стабильное строковое представление.

```text
cache_key = SHA-256(
    tenant_id
    + workspace_id
    + text_hash
    + purpose
    + chunk_type
    + input_profile_version
    + tokenizer_version
    + model_version
    + dense_version
    + sparse_version
    + pooling_version
    + normalization_version
    + sorted_requested_representations
)
```

Порядок representations не должен менять cache key.

---

## 6.2. Первичный claim

Claim создаётся до ONNX inference.

```sql
INSERT INTO astravector.embedding_cache_entries (
    id,
    tenant_id,
    workspace_id,
    cache_key,
    text_hash,
    purpose,
    chunk_type,
    tokenizer_version,
    model_version,
    dense_version,
    sparse_version,
    status,
    owner_instance_id,
    lease_token,
    processing_started_at,
    lease_expires_at
)
VALUES (
    :id,
    :tenant_id,
    :workspace_id,
    :cache_key,
    :text_hash,
    :purpose,
    :chunk_type,
    :tokenizer_version,
    :model_version,
    :dense_version,
    :sparse_version,
    'PROCESSING',
    :instance_id,
    1,
    now(),
    now() + :lease_duration
)
ON CONFLICT (cache_key) DO NOTHING
RETURNING id, lease_token;
```

Если строка вставлена, текущий pod становится owner.

Если строка не вставлена, необходимо прочитать существующее состояние.

---

# 7. Lease и fencing token

## 7.1. Новые поля

Создать миграцию:

```text
V007__production_reliability.sql
```

```sql
ALTER TABLE astravector.embedding_cache_entries
    ADD COLUMN IF NOT EXISTS lease_token bigint NOT NULL DEFAULT 0;

ALTER TABLE astravector.embedding_cache_entries
    ADD COLUMN IF NOT EXISTS lease_expires_at timestamptz;

ALTER TABLE astravector.embedding_cache_entries
    ADD COLUMN IF NOT EXISTS model_input_token_count integer;

ALTER TABLE astravector.embedding_cache_entries
    ADD COLUMN IF NOT EXISTS truncated boolean NOT NULL DEFAULT false;

ALTER TABLE astravector.embedding_cache_entries
    ADD COLUMN IF NOT EXISTS retry_count integer NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_cache_processing_lease
    ON astravector.embedding_cache_entries (
        lease_expires_at
    )
    WHERE status = 'PROCESSING';
```

---

## 7.2. Takeover

Takeover выполняется одним атомарным `UPDATE`.

```sql
UPDATE astravector.embedding_cache_entries
SET owner_instance_id = :new_owner,
    lease_token = lease_token + 1,
    processing_started_at = now(),
    lease_expires_at = now() + :lease_duration,
    retry_count = retry_count + 1,
    error_code = NULL,
    error_message = NULL
WHERE id = :cache_entry_id
  AND status IN ('PROCESSING', 'FAILED')
  AND (
      status = 'FAILED'
      OR lease_expires_at < now()
  )
RETURNING id, lease_token;
```

Если возвращена строка — takeover успешен.

Если возвращено 0 строк — другой pod уже получил lease.

---

## 7.3. Fencing при сохранении

Dense, sparse и `COMPLETED` должны сохраняться только владельцем актуального lease.

Транзакция должна:

1. Проверить ownership.
2. Записать dense.
3. Записать sparse.
4. Обновить cache entry.
5. Обновить item.
6. Обновить request.
7. Выполнить commit.

Финальное обновление:

```sql
UPDATE astravector.embedding_cache_entries
SET status = 'COMPLETED',
    model_input_token_count = :token_count,
    truncated = :truncated,
    completed_at = now(),
    last_accessed_at = now(),
    lease_expires_at = NULL
WHERE id = :cache_entry_id
  AND status = 'PROCESSING'
  AND owner_instance_id = :owner_instance_id
  AND lease_token = :lease_token;
```

Если затронуто `0` строк:

```text
OWNERSHIP_LOST
```

Транзакция должна быть отменена.

Старый pod не должен перезаписывать результат нового owner.

---

# 8. Lease duration

Добавить конфигурацию:

```yaml
cache:
  l2:
    lease_duration_seconds: 30
    processing_poll_interval_ms: 100
    processing_poll_max_interval_ms: 500
```

Lease duration должна быть больше максимального ожидаемого:

```text
queue wait
+ inference
+ persistence
+ safety margin
```

Для v002 отдельный heartbeat не обязателен, если выполняется условие:

```text
lease_duration > maximum supported request deadline + safety margin
```

Если фактический inference может длиться дольше lease, необходимо либо:

* увеличить lease;
* либо реализовать периодическое lease renewal.

---

# 9. Ожидание чужого PROCESSING

При обнаружении чужого активного `PROCESSING` запрещено:

* запускать второй inference;
* немедленно считать запрос ошибочным;
* ждать бесконечно.

Используется PostgreSQL polling.

Алгоритм:

```text
while deadline not expired:
    read cache status

    if COMPLETED:
        load result
        return

    if FAILED and retry allowed:
        attempt takeover

    if PROCESSING and lease expired:
        attempt takeover

    sleep poll_interval with jitter
```

Начальный интервал:

```text
100 ms
```

Допускается ограниченный backoff:

```text
100 → 150 → 250 → 400 → 500 ms
```

Интервал не должен превышать оставшийся deadline.

Если deadline истёк:

```text
DEADLINE_EXCEEDED
```

Для ожидания между pod не вводятся Redis, LISTEN/NOTIFY или брокеры сообщений.

---

# 10. Request audit

Каждый RPC должен создавать или находить запись:

```text
astravector.embedding_requests
```

Статусы:

```text
RECEIVED
PROCESSING
COMPLETED
PARTIALLY_COMPLETED
FAILED
```

Сохраняемые данные:

* `emb_task_id`;
* `correlation_id`;
* `idempotency_key`;
* `tenant_id`;
* `workspace_id`;
* `caller_service`;
* `purpose`;
* `access_level`;
* `persistence_mode`;
* sorted requested representations;
* `request_hash`;
* item count;
* contract/model/tokenizer versions;
* timestamps;
* error code;
* error message.

`correlation_id` идентифицирует конкретный RPC.

`idempotency_key` идентифицирует логическую операцию.

---

# 11. Item audit

Для каждого item создаётся запись в:

```text
astravector.embedding_items
```

Статусы:

```text
RECEIVED
CACHE_HIT
WAITING_FOR_CACHE
PROCESSING
COMPLETED
FAILED
```

Сохранять:

* request ID;
* chunk ID;
* chunk type;
* parent chunk ID;
* cache entry ID;
* text hash;
* text length;
* token count;
* truncated;
* timestamps;
* error code;
* error message.

Request status агрегируется по item statuses.

---

# 12. Idempotency

## 12.1. Canonical request hash

До расчёта hash:

* representations сортируются;
* duplicate representations удаляются;
* items сохраняют входной порядок;
* UUID приводятся к lowercase canonical form;
* hash текста вычисляется AstraVector.

```text
request_hash = SHA-256(
    tenant_id
    + workspace_id
    + purpose
    + access_level
    + persistence_mode
    + sorted_representations
    + ordered(chunk_id, chunk_type, parent_chunk_id, text_hash)
    + contract_version
)
```

В `request_hash` не входят:

* `correlation_id`;
* timestamp;
* instance ID;
* queue metadata.

---

## 12.2. Повторный запрос

Поиск:

```sql
SELECT *
FROM astravector.embedding_requests
WHERE tenant_id = :tenant_id
  AND workspace_id = :workspace_id
  AND idempotency_key = :idempotency_key;
```

Поведение:

### Тот же hash, request COMPLETED

Восстановить response из:

```text
embedding_requests
→ embedding_items
→ cache entries
→ dense/sparse
```

Повторный ONNX inference не выполняется.

### Тот же hash, request PROCESSING

Ожидать завершения с учётом gRPC deadline.

### Тот же hash, request FAILED

Применить ограниченную retry policy.

### Другой hash

```text
FAILED_PRECONDITION
IDEMPOTENCY_KEY_REUSED_WITH_DIFFERENT_PAYLOAD
```

---

# 13. Семантика persistence

## 13.1. NONE

```text
L1 разрешён
L2 не используется
audit не обязателен
```

Для production рекомендуется сохранять минимальный request audit даже при `NONE`, если это разрешено политикой безопасности.

---

## 13.2. BEST_EFFORT

Рекомендуется для query.

Если inference успешен, а PostgreSQL persistence неуспешен:

```text
embedding status = COMPLETED
persistence status = FAILED
```

L1 может быть заполнен после успешного inference.

---

## 13.3. REQUIRED

Рекомендуется для document chunks.

Успех разрешён только после:

```text
dense inserted
sparse inserted
cache COMPLETED
item statuses persisted
request status persisted
transaction committed
```

До commit запрещено:

* возвращать `COMPLETED`;
* считать operation завершённой;
* сохранять результат в L1 как финально успешный.

После commit:

```text
update L1
→ return response
```

При DB error:

```text
UNAVAILABLE
```

или:

```text
INTERNAL
```

в зависимости от типа ошибки.

---

# 14. Короткие транзакции

## Transaction 1

```text
idempotency check
create/update request
create items
claim cache entries
commit
```

## Вне транзакции

```text
tokenization
queue wait
batching
ONNX inference
dense/sparse post-processing
```

## Transaction 2

```text
verify owner + lease token
insert dense
insert sparse
update cache
update items
aggregate request status
commit
```

Запрещено держать transaction или row lock во время ONNX inference.

---

# 15. Параллельная отправка EncodeBatch

Все items одного RPC должны быть отправлены в scheduler конкурентно.

```rust
let futures = request
    .items
    .into_iter()
    .enumerate()
    .map(|(index, item)| process_item(index, item));

let mut results = futures::future::join_all(futures).await;
results.sort_by_key(|result| result.input_index);
```

Не использовать последовательную схему:

```text
submit item
await
submit next item
await
```

Bounded queue остаётся главным ограничителем нагрузки.

---

# 16. Deadline и cancellation

Каждый scheduler job содержит:

```rust
struct InferenceJob {
    deadline: tokio::time::Instant,
    cancellation: CancellationToken,
    // ...
}
```

Проверять deadline:

1. До idempotency lookup.
2. До L2 polling.
3. До queue submit.
4. Перед batch inference.
5. Перед persistence.
6. Перед response.

Использовать:

```rust
tokio::time::timeout_at(deadline, operation)
```

Если клиент отменил RPC до inference:

```text
job отменяется
```

Если inference уже начался:

* сам ONNX batch может завершиться;
* результат сохраняется, если есть другие активные consumers или claim должен быть завершён;
* отменённому клиенту результат не отправляется после истечения deadline.

---

# 17. Checksum validation

При startup вычислять SHA-256:

* model file;
* tokenizer file.

Если заданный checksum не совпадает:

```text
startup failure
readiness = false
```

Пустой checksum разрешён только:

```text
development profile
```

В production пустой checksum запрещён.

---

# 18. Readiness и self-test

READY устанавливается только после:

* config validation;
* PostgreSQL migration;
* tokenizer load;
* tokenizer checksum;
* model checksum;
* provider selection;
* ONNX session creation;
* warmup;
* dense self-test;
* sparse self-test либо явное отключение sparse;
* scheduler startup;
* repository readiness.

Self-test проверяет:

```text
dense dimension == 1024
dense finite
dense non-zero
dense norm ≈ 1

sparse indices/values same length
sparse values finite
sparse indices unique
```

---

# 19. Dynamic readiness degradation

Readiness переключается в `false`, если:

* scheduler worker остановился;
* ONNX session перестала работать;
* выбранный provider потерян;
* внутренний worker завершился с panic/error;
* обязательная PostgreSQL недоступна;
* periodic runtime self-check не проходит.

Конфигурация:

```yaml
postgres:
  required_for_readiness: true
```

Для query-only BEST_EFFORT deployment может быть:

```yaml
postgres:
  required_for_readiness: false
```

---

# 20. Scheduler и length buckets

Используются реальные buckets:

```text
1–64
65–128
129–256
257–512
```

Каждый job должен иметь фактический token count до помещения в inference bucket.

Рекомендуемая структура:

```text
QUERY:
  bucket_64
  bucket_128
  bucket_256

DOCUMENT:
  bucket_64
  bucket_128
  bucket_256
  bucket_512
```

Query остаётся приоритетным.

После установленного количества query batches scheduler должен обработать document batch, если он ожидает.

---

# 21. Job, не поместившийся в текущий batch

Если job не помещается по token budget:

```text
не возвращать RESOURCE_EXHAUSTED
```

Job должен быть сохранён как pending и использован в следующем batch.

`RESOURCE_EXHAUSTED` допустим только при:

* полном bounded queue;
* hard input limit violation;
* невозможности принять job из-за configured resource limit.

---

# 22. Исправление race query/document

Результат выбора очереди должен включать тип:

```rust
enum QueueKind {
    Query,
    Document,
}

struct SelectedJob {
    queue_kind: QueueKind,
    job: InferenceJob,
}
```

Profile и batch limits выбираются по `queue_kind`, а не по состоянию, вычисленному до `tokio::select!`.

Закрытие одной queue не должно завершать scheduler, пока вторая активна.

---

# 23. Полная validation

До обработки проверить:

* корректный UUID;
* purpose не `UNSPECIFIED`;
* access level не `UNSPECIFIED`;
* persistence mode не `UNSPECIFIED`;
* chunk type только `PARENT` или `CHILD`;
* representations не пусты;
* representations известны;
* duplicate representations нормализованы;
* items не пусты;
* duplicate chunk ID отсутствует;
* child имеет parent ID;
* parent разрешён конфигурацией;
* text не пуст;
* text byte limit не превышен;
* content hash совпадает;
* expected versions совместимы.

Если parent disabled:

```text
FAILED_PRECONDITION
PARENT_EMBEDDING_DISABLED
```

---

# 24. L2 metadata

Cache entry хранит:

```text
model_input_token_count
truncated
```

L2 cache hit должен возвращать тот же metadata, что исходный inference.

Недопустимо возвращать:

```text
token_count = 0
```

если исходный token count известен.

---

# 25. Prometheus metrics

Обязательная фактическая instrumentation:

```text
astravector_requests_total
astravector_items_total
astravector_request_duration_seconds

astravector_queue_depth
astravector_queue_wait_seconds

astravector_batch_size
astravector_batch_padded_tokens
astravector_batch_duration_seconds

astravector_tokenization_duration_seconds
astravector_inference_duration_seconds

astravector_l1_cache_hits_total
astravector_l1_cache_misses_total
astravector_l2_cache_hits_total
astravector_l2_cache_misses_total

astravector_claim_total
astravector_claim_wait_seconds
astravector_lease_takeover_total
astravector_ownership_lost_total

astravector_persistence_duration_seconds
astravector_persistence_failures_total

astravector_errors_total
astravector_readiness
astravector_execution_provider_info
```

Не использовать high-cardinality labels:

* tenant;
* workspace;
* chunk ID;
* request ID;
* correlation ID.

---

# 26. Cache retention

Retention worker должен отдельно очищать:

* query audit;
* document audit;
* failed requests;
* unused cache entries.

Cache entry удаляется, если:

```text
status != PROCESSING
last_accessed_at < threshold
lease_expires_at is null or expired
```

Использовать batch deletion:

```text
1000–5000 rows
```

При нескольких pod использовать:

```text
FOR UPDATE SKIP LOCKED
```

Dense и sparse удаляются через `ON DELETE CASCADE`.

---

# 27. Service authentication

Минимальный вариант без усложнения:

```text
x-api-key
```

Ключ передаётся через gRPC metadata и сравнивается с secret из environment.

Требования:

* constant-time comparison;
* ключ не логируется;
* health/readiness policy задаётся отдельно;
* production endpoint без authentication запрещён.

mTLS остаётся конфигурируемой следующей ступенью.

---

# 28. Graceful shutdown

При SIGTERM:

```text
readiness=false
→ stop accepting new requests
→ close queue senders
→ drain pending jobs
→ finish active inference
→ finish REQUIRED persistence
→ stop recovery/retention workers
→ close DB pool
→ exit
```

Конфигурация:

```yaml
shutdown:
  drain_timeout_seconds: 30
```

После timeout незавершённые операции завершаются как `UNAVAILABLE`.

Claims не должны искусственно отмечаться `COMPLETED`. Они либо корректно завершаются, либо остаются до lease expiry и последующего takeover.

---

# 29. Retry policy

Retries только для transient errors:

* временная ошибка подключения PostgreSQL;
* deadlock;
* serialization failure;
* кратковременная ошибка чтения cache status;
* provider initialization в AUTO mode.

Конфигурация:

```yaml
retry:
  max_attempts: 3
  initial_delay_ms: 50
  multiplier: 2.0
  max_delay_ms: 500
  jitter: true
```

Retry учитывает gRPC deadline.

Не повторять:

* validation errors;
* idempotency conflict;
* checksum mismatch;
* model shape mismatch;
* NaN/Infinity;
* unsupported sparse;
* parent disabled.

---

# 30. PostgreSQL startup policy

Добавить:

```yaml
postgres:
  enabled: true
  auto_migrate: true
  required_on_startup: true
  required_for_readiness: true
```

Для production document indexing:

```text
required_on_startup = true
```

Для query-only BEST_EFFORT deployment допускается:

```text
required_on_startup = false
required_for_readiness = false
```

В таком режиме repository должен периодически переподключаться.

---

# 31. Quality gates

Обязательные команды:

```bash
cargo fmt --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Также:

```bash
cargo audit
```

рекомендуется включить в CI.

Production build:

```bash
cargo build --release
```

---

# 32. P2 — production hardening

После выполнения P0/P1:

* Kubernetes Deployment;
* Service;
* ConfigMap;
* Secret;
* migration Job;
* readiness/liveness/startup probes;
* PodDisruptionBudget;
* HPA;
* NetworkPolicy;
* resource requests/limits;
* graceful termination;
* load tests;
* chaos tests;
* Rust/Python parity tests;
* GPU provider tests.

---

# 33. DB migration strategy

Development:

```yaml
postgres:
  auto_migrate: true
```

Production:

```yaml
postgres:
  auto_migrate: false
```

Перед Deployment запускается migration Job.

`CREATE TABLE IF NOT EXISTS` не используется как замена эволюционным migrations.

Каждое изменение схемы — отдельная versioned migration.

---

# 34. Новые тесты приёмки

## Distributed claim

Два pod получают один cache key.

Ожидается:

```text
один owner
один inference
второй pod polling
```

## Fencing

Старый owner пытается сохранить после takeover.

Ожидается:

```text
UPDATE affects 0 rows
OWNERSHIP_LOST
result not overwritten
```

## PROCESSING wait

Второй pod видит активный lease.

Ожидается:

```text
polling until COMPLETED
no duplicate inference
```

## PROCESSING deadline

Результат не готов до client deadline.

Ожидается:

```text
DEADLINE_EXCEEDED
```

## Idempotent replay

```text
same key + same hash
```

Ожидается:

```text
stored response
no inference
```

## Idempotency conflict

```text
same key + different hash
```

Ожидается:

```text
FAILED_PRECONDITION
```

## REQUIRED DB failure

DB падает до commit.

Ожидается:

```text
no COMPLETED response
no final L1 entry
```

## Batch concurrency

16 items одного RPC.

Ожидается:

```text
concurrent scheduler submission
one or several real micro-batches
input order preserved in response
```

## Scheduler pending job

Job не помещается в текущий batch.

Ожидается:

```text
processed in next batch
not RESOURCE_EXHAUSTED
```

## Checksum mismatch

Ожидается:

```text
NOT_READY
```

## Dynamic readiness

Scheduler worker аварийно завершён.

Ожидается:

```text
readiness=false
```

## Shutdown

SIGTERM во время REQUIRED persistence.

Ожидается:

```text
finish transaction within drain timeout
or return UNAVAILABLE
```

---

# 35. Обновлённые критерии готовности AstraVector_v002

Версия готова к тестовому production-контуру, если:

1. Реальный ONNX session работает.
2. Dense output подтверждён.
3. Sparse output подтверждён или capability отключена.
4. Claim выполняется до inference.
5. Активен lease token.
6. Takeover атомарен.
7. Fencing предотвращает stale write.
8. PROCESSING polling ограничен deadline.
9. Idempotency реализована.
10. Idempotent response восстанавливается из БД.
11. Request audit реализован.
12. Item audit реализован.
13. REQUIRED подтверждается только после commit.
14. L1 для REQUIRED заполняется после commit.
15. Items одного RPC отправляются конкурентно.
16. Deadline передаётся во все стадии.
17. Cancellation обрабатывается.
18. Checksums проверяются.
19. Readiness зависит от self-test.
20. Readiness может динамически отключаться.
21. Length buckets реально используются.
22. Pending job не теряется и не отклоняется ошибочно.
23. Query/document race исправлен.
24. Parent-disabled validation реализована.
25. Representations канонизируются перед hash.
26. L2 хранит token metadata.
27. Metrics фактически записываются.
28. Retention очищает audit и cache.
29. Service authentication включена.
30. Graceful shutdown реализован.
31. Retry ограничен и deadline-aware.
32. Проект проходит `fmt/check/test/clippy`.
33. Multi-pod dedup test пройден.
34. Lease takeover test пройден.
35. Rust/Python parity test пройден.
36. PostgreSQL failure scenarios пройдены.
37. Docker image собирается.
38. Production migration job работает.
39. Kubernetes probes работают.
40. README соответствует реализации.

---

# 36. Что не добавляется

В AstraVector_v002 не добавляются:

* Redis;
* RabbitMQ;
* Kafka;
* отдельный lock-service;
* отдельный task-service;
* event sourcing;
* streaming gRPC;
* REST embedding API;
* Qdrant client;
* BM25;
* reranker;
* workflow engine;
* service mesh как обязательная зависимость.

---

# 37. Ожидаемый результат

Целевой архив:

```text
AstraVector_v002.zip
```

Должен содержать:

* обновлённый исходный код;
* настоящий ONNX adapter;
* migrations V007+;
* обновлённый protobuf;
* unit tests;
* integration tests;
* concurrency tests;
* parity test tooling;
* Dockerfile;
* docker-compose;
* Kubernetes manifests;
* migration Job;
* README;
* implementation status;
* known limitations;
* результаты quality gates;
* инструкции CPU/CUDA/TensorRT.

---

# 38. Финальная формула надёжности

```text
claim before compute
+ lease
+ fencing
+ bounded PROCESSING polling
+ idempotency
+ REQUIRED after commit
+ deadline-aware scheduler
+ readiness after self-test
+ audit
+ metrics
```

Это обеспечивает надёжность AstraVector без введения дополнительных инфраструктурных компонентов.
