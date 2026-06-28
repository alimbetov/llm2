# AstraVector v004 Performance Baseline

- Project: `/Users/ruslanalimbetov/Documents/llm2/AstraVector_v004`
- Source task: `/Users/ruslanalimbetov/Documents/llm2/codex_tasks/AstraVector_v004 Documentation.pdf`
- Baseline status: partial smoke baseline only.
- Verdict: no production latency or throughput SLO can be claimed yet.

## What Was Measured

The current smoke confirms that a single real encode request can complete through the local CPU ONNX path:

- Item status: `ITEM_COMPLETED`
- Dense dimension: `1024`
- Dense vector length: `1024`
- Runtime provider: `CPU`
- Contract: `astravector_embedding_contract_v4_0`

The final smoke run also measured step durations in `smoke-tests/v004/reports/smoke-results.json`.

| Smoke step | Status | Duration ms |
|---|---:|---:|
| preflight | PASS | 71 |
| build | PASS | 2896 |
| infra | PASS | 534 |
| migrations | PASS | 2862 |
| services | PASS | 20388 |
| health | PASS | 122 |
| encode | PASS | 135 |
| persistence | PASS | 123 |
| relevance | PASS | 90 |
| shutdown | PASS | 1078 |

## What Was Not Measured

The following PDF-required baseline dimensions are not available yet because the Core E2E path is blocked before document ingestion and retrieval:

- source tokens/sec;
- chunks/sec;
- embeddings/sec for ingestion batches;
- transaction latency for REQUIRED document ingestion;
- outbox points/sec;
- end-to-end document latency;
- retrieval QPS;
- retrieval P50/P95/P99;
- query embedding latency under concurrency;
- Qdrant latency;
- parent fetch latency;
- overload accepted/rejected counts;
- memory/RSS long-run trend;
- connection pool return-to-idle observation;
- queue depth return-to-zero observation.

## Required Future Baseline Matrix

Create raw JSON/CSV outputs under `smoke-tests/v004/reports/performance/` for:

| Profile | Concurrency | Required outputs |
|---|---:|---|
| query | 1, 10, 50, 100, 200 | QPS, P50, P95, P99, errors, resource exhausted |
| ingestion | 1, 4, 16, 32 | source tokens/sec, chunks/sec, embeddings/sec, transaction latency |
| mixed query/ingestion | 10, 50, 100 | query priority, rejected ingestion, queue depth, recovery after load |
| Qdrant publisher | backlog sizes | points/sec, retry count, oldest pending age |
| reconciliation | batch sizes | scanned/sec, repaired/sec, checkpoint age |

## Baseline Blockers

- `AstraVectorV004Control` methods currently return `UNIMPLEMENTED`, so document ingestion cannot create source units, chunks, bindings or outbox events.
- Production retrieval API is not implemented, so query QPS and retrieval latency cannot be measured.
- Metrics endpoint is blocked in the current smoke result, so no Prometheus-derived latency histograms were captured.
- Overload/backpressure smoke scripts required by the PDF are not implemented yet.
- Long-run soak smoke is not implemented yet.

## Current Performance Verdict

Only a smoke-level single encode duration is available. Do not use it as a production baseline. The first valid performance baseline should be captured only after document-version, chunking, REQUIRED persistence, outbox/Qdrant sync and retrieval gates are PASS.
