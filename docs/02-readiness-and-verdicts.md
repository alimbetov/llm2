# Readiness And Verdicts

## Purpose

Зафиксировать официальный статус, что он означает и чего не доказывает.

## Audience

Аналитики, руководители, QA, разработчики, операторы.

## Short Summary

Текущий статус: `SECURE_RAG_CORE_CANDIDATE + CONSISTENCY_PASS`.

## Current Status

`AstraVector_v004 = SECURE_RAG_CORE_CANDIDATE + CONSISTENCY_PASS`

## What This Status Means

Secure RAG core работает, а consistency under controlled failures доказана в текущем smoke scope.

Подтверждено:

- indexing;
- dense retrieval;
- Civil Code corpus;
- access-zone/access-level isolation;
- atomicity failpoints;
- outbox fencing;
- dead-letter Qdrant failure;
- data integrity audit.

## What This Status Does Not Mean

- Не production-ready.
- Не `RELIABILITY_CANDIDATE`.
- Не `LEGAL_RAG_QUALITY_CANDIDATE`.
- Не доказана BM25/Sparse/Hybrid retrieval quality.
- Не закрыты lifecycle gRPC methods: delete, TTL, legal hold, relevance/quarantine registered in proto but not implemented in the current runtime.

## Status Ladder

| Level | Meaning | Current |
|---|---|---|
| Prototype | Basic idea only | Passed |
| RAG Core Candidate | Indexing and retrieval work | Passed |
| Secure RAG Core Candidate | Access isolation proven | Passed |
| Consistency Pass | Atomicity/outbox/dead-letter proven | Passed |
| Lifecycle Pass | TTL/delete/legal hold proven | Not yet |
| Recovery/Reconciliation Pass | Qdrant repair/rebuild proven | Not yet |
| Reliability Candidate | Lifecycle + recovery + load + observability | Not yet |
| Production Candidate | Full operational readiness | Not yet |

## Current Smoke Results

Latest documented results:

```text
reliability-closing: PASS 9 / FAIL 0 / BLOCKED 0 / SKIPPED 0
bm25-hybrid-retrieval: BM25_RETRIEVAL_BLOCKED
```

## Open Blockers

- Lifecycle
- Recovery/Reconciliation
- Load/Backpressure
- Observability
- Security hardening beyond access zones
- BM25/Sparse/Hybrid retrieval
- Proto methods that currently return `UNIMPLEMENTED`: delete, TTL, legal hold, relevance/quarantine.

## Expected Results

Статусные отчеты должны использовать точную формулировку: `SECURE_RAG_CORE_CANDIDATE + CONSISTENCY_PASS`.

## Common Mistakes

- Поднимать статус до `RELIABILITY_CANDIDATE` после одного reliability-closing PASS.
- Скрывать `BM25_RETRIEVAL_BLOCKED`.
- Считать `SKIPPED` или `BLOCKED` успешным тестом.
