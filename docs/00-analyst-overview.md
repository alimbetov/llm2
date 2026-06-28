# Analyst Overview

## Purpose

Объяснить `AstraVector_v004` бизнес-техническим языком без необходимости знать Rust.

## Audience

Аналитики, бизнес-технические аналитики, менеджеры продукта, QA lead.

## Short Summary

`AstraVector_v004` — внутренний RAG Core. Он принимает документы, разбивает их на фрагменты, создает embeddings, хранит canonical state в PostgreSQL, синхронизирует searchable projection в Qdrant и выполняет secure retrieval с фильтрами доступа.

## What The Project Does

Основной поток:

```text
Document
-> chunking
-> embeddings
-> PostgreSQL canonical state
-> vector_outbox
-> Qdrant projection
-> Search
-> parent context for LLM
```

Сервис нужен, чтобы backend мог получать проверяемый original parent context для RAG-ответов, не отдавая LLM случайные synthetic summaries как доказательство.

## Who Calls AstraVector

Flutter/Desktop клиенты не должны вызывать AstraVector напрямую. Внешний фасад должен быть `ai_bro` или REST Gateway. Внутренние workers/backend services могут обращаться к AstraVector по gRPC.

```text
Flutter / Desktop / Airflow / Admin UI
        |
        v
ai_bro or REST Gateway
        |
        v
AstraVector_v004 gRPC
        |
        v
PostgreSQL + Qdrant + ONNX
```

## What Is Already Proven

Текущий проверенный scope:

- Build: PASS
- Migrations: PASS
- Civil Code corpus ingestion: PASS
- Dense retrieval/RAG: PASS
- Access isolation/access-level: PASS
- Consistency: PASS
- Atomicity failpoints: PASS
- Outbox fencing: PASS
- Dead-letter Qdrant failure: PASS
- Data integrity audit: PASS

## Current Official Verdict

`SECURE_RAG_CORE_CANDIDATE + CONSISTENCY_PASS`

Это значит:

- core RAG/indexing/retrieval функциональность работает;
- security isolation по `access_zone_id` и `access_level` проверен;
- controlled failure behavior проверен для текущего smoke scope.

Это не значит:

- production-ready;
- `RELIABILITY_CANDIDATE`;
- доказанное lifecycle поведение;
- доказанное recovery/reconciliation;
- доказанная load/backpressure устойчивость;
- доказанная observability;
- доказанная BM25/Sparse/Hybrid legal exact-match retrieval quality.

## How To Read PASS / FAIL / BLOCKED / SKIPPED

- `PASS`: тест выполнен и доказал assertion на текущем контуре.
- `FAIL`: тест выполнен и assertion не прошел.
- `BLOCKED`: smoke существует, но production capability отсутствует.
- `SKIPPED`: тест сознательно не выполнялся и не должен считаться PASS.

Пример: `BM25_RETRIEVAL_BLOCKED` означает, что smoke-контур создан, но production BM25/sparse/hybrid search еще не реализован. Это корректный результат, его нельзя превращать в PASS.

## Business And Technical Risks Still Open

- Lifecycle risk: expired/deleted/quarantined data еще требует полного доказательства исключения из Search.
- Recovery risk: нужен proof repair/rebuild после потери Qdrant point или payload mismatch.
- Load risk: sustained indexing/search load не доказан.
- Observability risk: metrics/readiness/tracing не закрыты.
- Retrieval quality risk: BM25/Sparse/Hybrid заблокирован, legal exact-match retrieval не доказан.

## Analyst Checklist Before Reporting Status

Перед статусным отчетом проверить:

- latest smoke profile name;
- PASS/FAIL/BLOCKED/SKIPPED counts;
- BM25 is PASS or BLOCKED;
- Lifecycle is PASS or not;
- Recovery/Reconciliation is PASS or not;
- отчет не называет проект production-ready без оснований.

## Expected Results

Корректная формулировка текущего состояния: `AstraVector_v004 has reached SECURE_RAG_CORE_CANDIDATE + CONSISTENCY_PASS`.

## Common Mistakes

- Писать production-ready после reliability-closing PASS.
- Называть BM25 качество доказанным при `BM25_RETRIEVAL_BLOCKED`.
- Считать Qdrant canonical state. Canonical state находится в PostgreSQL.
