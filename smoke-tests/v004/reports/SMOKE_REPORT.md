# AstraVector v004 Smoke Report

- Generated: 2026-06-27T15:27:51Z
- Project: /Users/ruslanalimbetov/Documents/llm2/AstraVector_v004
- Git commit: unknown
- Rust: rustc 1.96.0 (ac68faa20 2026-05-25) (Homebrew)
- Cargo: cargo 1.96.0 (30a34c682 2026-05-25) (Homebrew)
- PostgreSQL: PostgreSQL 15.18 (Debian 15.18-1.pgdg12+1) on aarch64-unknown-linux-gnu, compiled by gcc (Debian 12.2.0-14+deb12u1) 12.2.0, 64-bit
- Qdrant: qdrant - vector search engine

| ID | Test | Status | Duration ms | Evidence |
|---|---|---:|---:|---|
| 1 | bm25-hybrid-retrieval | BLOCKED | 311 | smoke-tests/v004/results/bm25-hybrid-retrieval.json |

## Counts
- PASS: 0
- FAIL: 0
- BLOCKED: 1
- SKIPPED: 0

## Production Blockers

- Wave 1 validates indexing, retrieval, corpus ingestion, RAG quality, and integrity only.
- Wave 2+ remains required for access-security, TTL/legal-hold/delete semantics, reconciliation/rebuild, failpoints, overload, and observability.
- See FULL_POWER_SMOKE_REPORT.md for the current candidate verdict.
