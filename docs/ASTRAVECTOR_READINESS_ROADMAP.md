# AstraVector Readiness Roadmap

Current final status: `GRAPH_RAG_READY`

The model-backed runtime confidence gate is now green for dense, sparse and hybrid quick profiles. This is a runtime readiness milestone, not a production-candidate declaration.

## Closed For Runtime Ready

- `make quality-runtime-confidence-remote` returns PASS.
- `runtime_execution = CONFIDENCE_GATE_CONFIRMED`.
- `verdict = PASS`.
- Dense profile: `MODEL_BACKED_E2E_CONFIRMED`, `PASS`.
- Sparse profile: `MODEL_BACKED_E2E_CONFIRMED`, `PASS`.
- Hybrid profile: `MODEL_BACKED_E2E_CONFIRMED`, `PASS`.
- gRPC endpoint on `127.0.0.1:50051` is reachable.
- PostgreSQL, Qdrant, collection schema, model file and tokenizer file pass confidence preflight.
- Fixture checksum is computed.
- Cross-zone leakage and access-level violation counters are zero in the confidence run.
- `cargo fmt --check`, `cargo check --all-targets --all-features`, and `cargo clippy --all-targets --all-features -- -D warnings` pass.

## Runtime Confidence Boundary

`production_pass=true` in the confidence report means the confidence gate passed. It does not mean `PRODUCTION_CANDIDATE` or production-ready.

Required final readiness fields:

- `runtime_ready = true`
- `runtime_confidence_pass = true`
- `production_candidate = false`
- `production_ready = false`

## GraphRAG Policy

- `graph_rag_available = true`
- `graph_rag_required_for_ready = false`
- `graph_rag_required_for_production_candidate = true`

Graph diagnostic or skipped cases must not be counted as PASS. The focused GraphRAG quick profile passes; broader GraphRAG production-candidate proof remains required for `PRODUCTION_CANDIDATE`.

## Distractor And Hard-Negative Policy

Distractor fixture labels are evaluation/reporting labels only. Production retrieval rejects weak common-overlap candidates through evidence-based gates: lexical coverage, discriminating token matches, leading-term evidence, negative-evidence phrase detection, no-answer thresholds and MMR/final context filtering.

Hard-negative target for runtime ready:

- `forbidden_total_after = 0`
- `false_positive_rate = 0.0`

## Remaining Gates To Production Candidate

`PRODUCTION_CANDIDATE` requires:

- Larger GraphRAG production-candidate proof beyond the focused quick profile.
- Full all-target test suite proof beyond the focused runtime readiness gates.
- Clean migration validation on an empty database.
- Docker/Kubernetes deployment validation against the packaged artifact.
- Lifecycle E2E proof for activation, synchronization, retrieval, TTL/delete and recovery paths.
- Load, soak, backpressure, recovery, backup/restore, rollback, capacity and alerting proof.
- Reviewed/staged ownership for the large quality fixture and readiness evidence file set.

Until those gates pass, the correct status is `GRAPH_RAG_READY`, not production-ready.
