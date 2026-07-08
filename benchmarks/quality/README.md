# AstraVector Quality Bench Data Bank

This directory contains curated JSONL fixtures for AstraVector RAG/retrieval quality checks.

## Corpora

- `synthetic-mini` — base technical facts from the foundation patch.
- `access-zone-mini` — tenant and access-level conflict checks.
- `graph-rag-mini` — foundation GraphRAG links.
- `mmr-diversity-mini` — duplicate/aspect MMR checks.
- `technical-mini` — AstraVector production/runbook retrieval.
- `legal-mini` — synthetic legal-like rules, explicitly not real law.
- `distractor-mini` — lexical-overlap false-positive checks.
- `long-doc-mini` — target block retrieval in long documents.
- `ttl-legal-hold-mini` — TTL and legal_hold semantics.

## Query files

Golden queries live under `queries/*.jsonl` and use `schema_version = "1.0"`.

## Reports

Generated reports are written to `reports/` by `cargo test --test quality_bench_quick` or the Makefile targets.
