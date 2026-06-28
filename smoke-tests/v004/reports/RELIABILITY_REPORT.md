# AstraVector v004 Reliability Report

- Project: `/Users/ruslanalimbetov/Documents/llm2/AstraVector_v004`
- Source task: `/Users/ruslanalimbetov/Documents/llm2/codex_tasks/AstraVector_v004 Documentation.pdf`
- Evidence baseline: `smoke-tests/v004/reports/SMOKE_REPORT.md`
- Verdict: prototype / partial Core E2E, not a production candidate.

## Component Versions

- Rust: `rustc 1.96.0 (ac68faa20 2026-05-25) (Homebrew)`
- Cargo: `cargo 1.96.0 (30a34c682 2026-05-25) (Homebrew)`
- PostgreSQL: `15.18`
- Qdrant: smoke container `qdrant/qdrant:latest`
- Runtime provider: `CPU`
- Contract: `astravector_embedding_contract_v4_0`

## Model And Corpus

- Model path: `/Users/ruslanalimbetov/Documents/llm/llm-common/models/bge-m3-onnx-int8/model_quantized.onnx`
- Model SHA-256: `a2b85cf92f6e162189b2363f9e757b08196c75f0fb16f2e00377ca20c6ba5555`
- Tokenizer path: `/Users/ruslanalimbetov/Documents/llm/llm-common/models/bge-m3-onnx-int8/tokenizer.json`
- Tokenizer SHA-256: `3e657ddc9bb3a7425f881e701aedfee5911936be4d6f4efde8a2bc557eb34844`
- Corpus path: `/Users/ruslanalimbetov/Documents/llm2/data/Гражданский кодекс РК`
- Corpus SHA-256: `99520a0a66337707d8d5f1e2b647086d15aeea8e79e228b871b35748eb681d13`
- Corpus shape: one UTF-8 plain-text file without extension.

## Current Smoke Summary

- PASS: 10
- FAIL: 0
- BLOCKED: 13
- SKIPPED: 0

Confirmed PASS gates:

- Build gate: `cargo fmt --check`, `cargo check --all-targets --all-features`, `cargo test --all-targets --all-features`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo build --release`, `cargo build --release --locked`.
- Infrastructure: PostgreSQL 15 and Qdrant smoke containers are reachable.
- Migrations: schema applies and v004 partition checks pass.
- Runtime services: gRPC reflection lists `AstraVectorRuntime`, `AstraVectorV004Control`, `grpc.health.v1.Health`.
- Health: standard gRPC health and runtime health return `SERVING`.
- Encode: real ONNX encode returns `ITEM_COMPLETED`, dense dimension `1024`, runtime provider `CPU`.
- Persistence hygiene: `bindings_without_chunks = 0`, `bindings_without_cache = 0`.
- Relevance: score response is bounded and has an evaluation ID.
- Shutdown: smoke process shutdown succeeds.

## Reliability Gate Status

| Gate | Status | Evidence | Blocking file or method |
|---|---:|---|---|
| Build | PASS | `smoke-tests/v004/reports/build-report.md` | none |
| Migrations | PASS | `smoke-tests/v004/results/migrations.json` | none |
| Runtime health | PASS | `smoke-tests/v004/logs/runtime-health.json` | none |
| Encode | PASS | `smoke-tests/v004/logs/encode-response.json` | none |
| Document version lifecycle | BLOCKED | `smoke-tests/v004/logs/document-version.err` | `src/grpc/mod.rs::AstraVectorV004ControlService::register_document_version` returns `UNIMPLEMENTED` |
| Multi-granularity chunking | BLOCKED | `smoke-tests/v004/logs/chunking.err` | `src/grpc/mod.rs::AstraVectorV004ControlService::create_multi_granularity_chunks` returns `UNIMPLEMENTED` |
| REQUIRED atomic persistence | BLOCKED | no REQUIRED document ingestion path yet | v004 document/chunking service layer missing |
| Idempotency concurrency | BLOCKED | no concurrency smoke implemented | `smoke-tests/v004/scripts/24-idempotency-concurrency-smoke.sh` missing |
| Outbox E2E | BLOCKED | `smoke-tests/v004/results/outbox.json` | no v004 bindings/outbox events created without chunking |
| Qdrant sync/rebuild | BLOCKED | no ACTIVE bindings in smoke | reconciliation run-loop and rebuild smoke missing |
| Retrieval | BLOCKED | `smoke-tests/v004/results/retrieval.json` | production search API not implemented |
| Parent context | BLOCKED | v004 control skeleton only | `resolve_parent_context` returns `UNIMPLEMENTED` |
| Access isolation | BLOCKED | `smoke-tests/v004/results/access-isolation.json` | retrieval/control API not implemented for zone matrix |
| TTL | BLOCKED | `smoke-tests/v004/results/ttl.json` | group TTL requires persisted v004 group/binding |
| Legal hold | BLOCKED | `smoke-tests/v004/results/legal-hold.json` | `set_chunk_group_legal_hold` returns `UNIMPLEMENTED` |
| Delete/purge | BLOCKED | `smoke-tests/v004/results/delete.json` | `delete_chunk_group` returns `UNIMPLEMENTED` |
| Reconciliation | BLOCKED | `src/bin/astravector-reconciliation.rs` | binary initializes `Reconciler` then waits for signal; no scan/repair loop |
| Recovery/fault injection | BLOCKED | `smoke-tests/v004/results/recovery.json` | no test-only failpoints or outage harness |
| Observability | BLOCKED | `smoke-tests/v004/results/observability.json` | metrics endpoint not reachable in current smoke run |
| Corpus E2E | BLOCKED | `smoke-tests/v004/results/corpus.json` | corpus discovery passes, ingestion/indexing needs v004 chunking API |

## Data Counts From Current Smoke

These are zero because full document ingestion is blocked before chunk creation.

| Entity | Count |
|---|---:|
| documents | 0 |
| source units | 0 |
| SOURCE chunks | 0 |
| PARENT chunks | 0 |
| SUB_180 chunks | 0 |
| SUB_260 chunks | 0 |
| canonical embeddings | 0 |
| vector bindings | 0 |
| outbox events | 0 |
| Qdrant points | 0 |

## Mandatory Next Waves

1. Wave A: replace the `AstraVectorV004ControlService` skeleton with real document-version, chunking, chunk group and parent-context services.
2. Wave B: implement REQUIRED persistence transaction with failpoints under `smoke-failpoints`.
3. Wave C: complete outbox fencing, lease renewal, Qdrant point builder and publisher assertions.
4. Wave D: implement retrieval API with access-zone and access-level filters.
5. Wave F/G: implement reconciliation run-loop, checkpointing, outage recovery and no-silent-errors smoke.
6. Wave H: run full Civil Code corpus ingestion and retrieval threshold tests.

## Final Reliability Verdict

AstraVector v004 is not production-ready and not yet an E2E candidate. It is currently a prototype with verified build, runtime health, real encode, baseline persistence hygiene and relevance smoke. The Core path from document file to Qdrant-backed retrieval remains blocked by missing v004 control implementations, retrieval API, reconciliation loop, recovery harness and corpus ingestion.
