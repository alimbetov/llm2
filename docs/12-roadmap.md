# Roadmap

## Purpose

Зафиксировать следующие stages после текущего состояния.

## Audience

Аналитики, архитекторы, разработчики, roadmap owners.

## Short Summary

Текущий stage — `SECURE_RAG_CORE_CANDIDATE + CONSISTENCY_PASS`. До `RELIABILITY_CANDIDATE` нужно закрыть lifecycle, recovery/reconciliation, load/backpressure, observability и BM25/Sparse/Hybrid.

## Current Stage

```text
SECURE_RAG_CORE_CANDIDATE + CONSISTENCY_PASS
```

## Wave 4 Lifecycle

- TTL expiry.
- Legal hold blocks expiry/delete.
- Soft delete.
- Hard delete/purge.
- Quarantine.
- Retrieval exclusion.
- Qdrant delete/update sync.

Current proto methods for delete, TTL, legal hold, relevance, and quarantine return `UNIMPLEMENTED`; Wave 4 must replace that with real behavior and smoke evidence.

## Wave 5 Recovery/Reconciliation

- Missing Qdrant point repair.
- Orphan Qdrant point delete/quarantine.
- Payload mismatch repair.
- Collection rebuild.
- Idempotent reconciliation.

## Wave 6 Load/Backpressure

- Sustained indexing load.
- Sustained search load.
- Queue saturation.
- `RESOURCE_EXHAUSTED` policy.
- Latency percentiles.
- Memory stability.

## Wave 7 Observability

- Metrics endpoint.
- Outbox lag metrics.
- Queue depth.
- Error counters.
- Readiness degradation.
- `correlation_id` propagation.

## BM25 / Sparse / Hybrid

- `SearchMode` in Search API.
- Sparse score fields.
- BM25/sparse index.
- Hybrid fusion.
- Access/lifecycle filtering for sparse retrieval.

## Expected Results

После закрытия этих Wave можно обсуждать `RELIABILITY_CANDIDATE`; production-ready требует отдельного operational acceptance.

## Common Mistakes

- Смешивать dense retrieval PASS с BM25/hybrid PASS.
- Реализовать BM25 без Zone B leakage tests.
- Реализовать lifecycle без Qdrant payload/delete sync proof.
