# AstraVector Local End-to-End Book

Эта книга показывает один проверяемый локальный путь: поднять PostgreSQL/Qdrant, запустить `astravector-runtime`, загрузить русский текст через production gRPC API, дождаться публикации vectors, активировать document version и выполнить semantic search.

## Глава 1. Что Такое AstraVector

PostgreSQL хранит canonical state: document versions, chunks, embeddings, bindings, lifecycle status и transactional outbox. Qdrant хранит заменяемую vector projection для поиска. ONNX model создаёт embeddings. Rust/Tonic runtime принимает gRPC calls. Outbox доставляет изменения из PostgreSQL в Qdrant.

```text
grpcurl / Rust client
        |
        v
AstraVector Rust runtime
        |
        +--> PostgreSQL canonical state
        |
        +--> transactional outbox
                  |
                  v
                Qdrant
```

## Глава 2. Что Будет Сделано

```text
sample-ru.txt
→ IndexLogicalDocument
→ chunks
→ embeddings
→ bindings
→ Qdrant points
→ ActivateDocumentVersion
→ Search
→ найденный parent text
```

## Глава 3. Требования К Машине

Проверенные инструменты:

```bash
rustc --version
cargo --version
docker --version
docker compose version
psql --version
curl --version
jq --version
grpcurl --version
python3 --version
```

Минимальная версия Rust из `Cargo.toml`: `1.88`.

На macOS установите Docker Desktop, Rust через `rustup`, `grpcurl` и `jq` через Homebrew. На Linux установите Docker Engine/Compose plugin, Rust, PostgreSQL client, `curl`, `jq`, `grpcurl` и `python3`.

## Глава 4. Клонирование И Проверка Проекта

```bash
git clone https://github.com/alimbetov/llm2.git
cd llm2/astravector
git status --short
cargo metadata --no-deps --format-version=1 | jq '.packages[].targets[] | select(.kind[] == "bin") | .name'
```

Фактический runtime binary: `astravector-runtime`.

## Глава 5. Подготовка BGE-M3 ONNX

Модель не скачивается автоматически. Создайте локальный env-файл:

```bash
cp .env.local-demo.example .env.local-demo
```

Заполните:

```bash
ASTRAVECTOR_MODEL_PATH=/absolute/path/to/model.onnx
ASTRAVECTOR_TOKENIZER_PATH=/absolute/path/to/tokenizer.json
ASTRAVECTOR_MODEL_SHA256=
ASTRAVECTOR_TOKENIZER_SHA256=
```

Проверка:

```bash
scripts/local-demo/check-model.sh
```

Скрипт проверяет существование файлов, UTF-8/JSON tokenizer и SHA-256 через Python, поэтому одинаково работает на macOS и Linux.

## Глава 6. Запуск PostgreSQL И Qdrant

Canonical local-demo profile:

| Компонент | Адрес |
|---|---|
| PostgreSQL | `127.0.0.1:55432` |
| Qdrant HTTP | `http://127.0.0.1:6333` |
| Qdrant gRPC | `127.0.0.1:6334` |
| AstraVector gRPC | `127.0.0.1:50051` |
| AstraVector metrics | `http://127.0.0.1:9090` |
| Qdrant collection | `astravector_local_demo` |

```bash
make local-demo-infra-up
make local-demo-infra-wait
docker compose ps
docker compose logs postgres
docker compose logs qdrant
```

PostgreSQL:

```bash
PGPASSWORD=astravector psql -h 127.0.0.1 -p 55432 -U astravector -d astravector -c 'SELECT current_database(), current_user, now();'
```

Qdrant:

```bash
curl -sS http://127.0.0.1:6333/collections | jq .
```

Smoke-test isolated ports and FIX487 phase-owned ports are separate from this local-demo profile.

## Глава 7. PostgreSQL Migrations

Runtime has `postgres.auto_migrate=true` in the local profile. Explicit development command:

```bash
export DATABASE_URL='postgres://astravector:astravector@127.0.0.1:55432/astravector'
cargo sqlx migrate run
```

## Глава 8. Сборка Rust Runtime

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo build --release --locked
```

## Глава 9. Local Configuration

Local overlay: `config/application-local-demo.yaml`.

Он переопределяет только local operational values: PostgreSQL URL, Qdrant URL/collection, gRPC/metrics ports, security disabled, ingestion access-zone auto-create, dense default search и `sparse.required=false` для dense-only tutorial. Retrieval thresholds, Graph admission, MMR, frozen qrels и quality fixtures не меняются.

## Глава 10. Запуск AstraVector

```bash
make local-demo-runtime-start
```

Artifacts:

```text
.local-demo/runtime.pid
.local-demo/runtime.log
.local-demo/demo.env
```

## Глава 11. Проверка Runtime

```bash
grpcurl -plaintext 127.0.0.1:50051 list
grpcurl -plaintext 127.0.0.1:50051 describe astravector.embedding.v1.AstraVectorV004Control
grpcurl -plaintext 127.0.0.1:50051 grpc.health.v1.Health/Check
```

Expected services include `AstraVectorRuntime`, `AstraVectorV004Control`, `AstraVectorIngestionFacade`, `AstraVectorRetrievalFacade`, `AstraVectorAdminFacade`, `grpc.health.v1.Health` and reflection.

## Глава 12. Тестовый Текст

Файл: `examples/local-demo/sample-ru.txt`.

Он содержит уникальный anchor `ASTRAVECTOR_LOCAL_DEMO_2026` и объясняет PostgreSQL, Qdrant и outbox.

## Глава 13. Загрузка Текста

Canonical ingestion method for FIX488:

```text
AstraVectorIngestionFacade.IndexLogicalDocument
```

Он выбран потому, что это public facade, а server implementation внутри вызывает production `CreateMultiGranularityChunks` path.

```bash
scripts/local-demo/load-text.sh examples/local-demo/sample-ru.txt
```

Скрипт читает UTF-8, считает SHA-256, создаёт deterministic UUID, использует access-zone code `0488`, строит JSON через Python `json` и сохраняет response в `.local-demo/ingestion-response.json`.

## Глава 14. Ожидание Qdrant Publication

```bash
scripts/local-demo/wait-vector-sync.sh
```

Polling bounded: 120 seconds, 1 second interval. PASS только когда expected bindings, synced bindings, completed outbox and Qdrant points согласованы.

## Глава 15. Activation

```bash
scripts/local-demo/activate-document.sh
```

Activation RPC:

```text
AstraVectorV004Control.ActivateDocumentVersion
```

## Глава 16. Semantic Search

```bash
scripts/local-demo/search.sh 'Где AstraVector хранит каноническое состояние?'
scripts/local-demo/search.sh 'Для чего используется Qdrant?'
scripts/local-demo/search.sh 'ASTRAVECTOR_LOCAL_DEMO_2026'
```

FIX488 uses `SEARCH_MODE_V005_DENSE` and `EMBEDDING_MODE_V005_DENSE_ONLY` for the canonical tutorial. Sparse/hybrid are intentionally not required here.

## Глава 17. Expected Search Response

Сокращённая форма:

```json
{
  "results": [
    {
      "documentId": "...",
      "documentVersion": "1",
      "matchedChunkId": "...",
      "parentChunkId": "...",
      "parentText": "AstraVector хранит каноническое состояние...",
      "accessZoneId": "...",
      "accessLevel": "PUBLIC",
      "scores": {
        "denseScore": 0.0,
        "finalScore": 0.0
      }
    }
  ],
  "diagnostics": {
    "queryEmbeddingMs": 0,
    "qdrantSearchMs": 0,
    "parentFetchMs": 0,
    "totalMs": 0
  }
}
```

Actual numeric scores are written to evidence after the real run.

## Глава 18. PostgreSQL Audit

```bash
scripts/local-demo/inspect-postgres.sh
```

Проверяются фактические tables:

```text
astravector.document_versions
astravector.content_chunks_v004
astravector.vector_bindings_v004
astravector.vector_outbox
```

## Глава 19. Qdrant Audit

```bash
scripts/local-demo/inspect-qdrant.sh
```

Скрипт получает collection info, point count и scroll payload без вывода полного dense vector.

## Глава 20. Полный Сценарий Одной Командой

```bash
make local-demo-e2e
```

Workflow:

```text
check prerequisites
→ check model
→ start PostgreSQL/Qdrant
→ wait infrastructure
→ migrations
→ build Rust runtime
→ start runtime
→ wait gRPC readiness
→ load sample text
→ wait vector sync
→ activate
→ semantic search
→ exact-anchor search
→ PostgreSQL audit
→ Qdrant audit
→ final PASS/FAIL
```

Successful data is intentionally left running for inspection.

## Глава 21. Stop And Cleanup

```bash
make local-demo-down
```

Stops runtime and containers without deleting volumes.

```bash
make local-demo-reset
```

Stops runtime and removes local demo Docker volumes. Use only when local data can be deleted.

## Глава 22. Troubleshooting

See [TROUBLESHOOTING.md](TROUBLESHOOTING.md).
