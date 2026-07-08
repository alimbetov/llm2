# AstraVector Quality Bench

## Purpose

AstraVector Quality Bench is the built-in evidence layer for RAG/retrieval quality. It validates curated fixtures and, when a live gRPC endpoint is provided, executes golden queries through `RetrieveContext`.

It measures whether AstraVector:

- retrieves expected documents and blocks;
- prevents `access_zone` and access-level leakage;
- represents GraphRAG contribution through `GRAPH_EXPANDED` sources and expected related blocks;
- checks MMR diversity through expected aspect coverage;
- handles hard negative queries with lexical overlap;
- retrieves the target section inside long documents;
- produces measurable JSON and Markdown quality reports.

## Test layer separation

- `smoke-tests/v004/` remains for operational smoke checks such as health, readiness, and basic outbox flows.
- `tests/` remains for unit, integration, E2E, and contract tests.
- `benchmarks/quality/` is the quality evaluation layer for RAG, GraphRAG, MMR, access isolation, long-document retrieval, negative behavior and reports.

## Enriched data bank in fix468

`fix468` extends the original Quality Bench foundation with an enriched curated data bank:

```text
technical-mini        production/runbook retrieval
legal-mini            synthetic legal-like rules; not real law
distractor-mini       lexical-overlap false-positive checks
long-doc-mini         target-block retrieval in long documents
ttl-legal-hold-mini   TTL and legal_hold semantics
```

The enriched bank targets at least:

```text
35+ documents
140+ blocks
100+ golden queries
20+ relations
15+ hard negative queries
15+ access isolation queries
10+ GraphRAG queries
10+ MMR diversity queries
10+ long-document queries
```

## Directory layout

```text
benchmarks/quality/
  corpora/*/documents.jsonl
  corpora/*/relations.jsonl
  queries/*.jsonl
  profiles/quick.json
  profiles/production-candidate.json
  reports/
  schemas/*.schema.json
```

## Fixture formats

### Documents

Each line in `documents.jsonl` is a document with:

```text
schema_version, document_id, title, corpus, access_zone_code, access_level, blocks
```

Each block must contain:

```text
block_id, type, text
```

`legal-mini` documents must include metadata:

```json
{
  "synthetic": true,
  "domain": "legal-like",
  "not_real_law": true
}
```

### Relations

Each line in `relations.jsonl` connects two blocks and is used to validate GraphRAG expectations.

### Golden queries

Each line in `queries/*.jsonl` contains:

```text
schema_version, id, category, question, context, expected
```

The enriched `expected` section may include:

```text
must_contain_phrases
must_contain_document_ids
must_contain_block_ids
forbidden_phrases
forbidden_document_ids
forbidden_access_zones
forbidden_block_ids
expected_related_block_ids
expected_aspects
min_expected_aspect_coverage
hard_negative
max_false_positive_contexts
allowed_document_ids
required_ranked_before
runtime_required
```

## Profiles

### quick

`benchmarks/quality/profiles/quick.json` is intended for local and PR validation. It includes the foundation corpus plus `technical-mini`, `distractor-mini`, and `legal-mini`.

### production-candidate

`benchmarks/quality/profiles/production-candidate.json` is the strict release gate profile. It includes all enriched corpora and stricter thresholds.

## Metrics

The report contains retrieval metrics:

```text
recall_at_1, recall_at_3, recall_at_5, recall_at_10, mrr, ndcg_at_10,
expected_document_hit_rate, expected_block_hit_rate, exact_phrase_hit_rate, empty_context_rate
```

Access/security metrics:

```text
cross_zone_leakage_count, access_level_violation_count,
forbidden_phrase_leakage_count, forbidden_document_leakage_count,
forbidden_block_leakage_count
```

GraphRAG metrics:

```text
graph_expansion_rate, graph_expected_related_hit_rate,
graph_helped_count, graph_hurt_count, graph_helped_to_hurt_ratio, graph_noise_rate
```

MMR and diversity metrics:

```text
duplicate_rate_before_mmr, duplicate_rate_after_mmr,
expected_aspect_coverage, mmr_expected_aspect_coverage,
mmr_dense_mode_used_count, mmr_token_fallback_count
```

Enriched corpus metrics:

```text
hard_negative_false_positive_rate
long_document_target_block_hit_rate
legal_similar_rule_confusion_count
distractor_false_positive_count
access_zone_conflict_accuracy
access_level_conflict_accuracy
```

Consistency and latency metrics are included as report fields; full runtime ingestion validation is planned for the next runtime bench patch.

## Local commands

Validate fixtures:

```bash
make quality-fixtures
```

Generate a static quality report without a live endpoint:

```bash
make quality-quick
```

Run against a live endpoint:

```bash
ASTRAVECTOR_QUALITY_ENDPOINT=http://localhost:50051 ASTRAVECTOR_QUALITY_API_KEY=dev-secret make quality-quick-remote
```

## CI

`quality-fixtures` validates JSONL fixtures, reference integrity and enriched data-bank minimums.

`quality-quick` generates `quality-report.json` and `quality-report.md` in static validation mode, or remote gRPC mode if `ASTRAVECTOR_QUALITY_ENDPOINT` is set.

For executable model-backed E2E evidence use the fix469 runtime bench documented in
`docs/QUALITY_BENCH_RUNTIME.md`. `make quality-runtime-quick-remote` ingests the
quality fixtures through the production gRPC ingestion facade, relies on
ingestion-time access-zone auto-create, waits for outbox/Qdrant readiness, and
then evaluates `RetrieveContext`. It writes `runtime-quality-report.json`,
`runtime-quality-report.md`, `runtime-failures.jsonl`, and
`runtime-candidates.jsonl`.

Runtime reports are capability-aware. `MODEL_BACKED_E2E_FAILED` can still include
successful ingestion, completed outbox events and Qdrant points; inspect
`capabilities`, `sparse`, `graph`, `by_category`, `by_mode` and `by_reason`
before treating it as an infrastructure failure. `BLOCKED` query rows identify missing runtime
capabilities such as `SPARSE_UNAVAILABLE`, while `FAILED` rows mean retrieval ran
but missed fixture assertions. For dense-only ONNX artifacts, run:

```bash
make quality-runtime-dense-quick-remote
```

For strict capability profiles, run:

```bash
make quality-runtime-sparse-quick-remote
make quality-runtime-hybrid-quick-remote
make quality-runtime-graph-quick-remote
make quality-runtime-full-capability-quick-remote
```

These targets set `ASTRAVECTOR_QUALITY_REQUIRE_DENSE`, `ASTRAVECTOR_QUALITY_REQUIRE_SPARSE`, `ASTRAVECTOR_QUALITY_REQUIRE_HYBRID`, `ASTRAVECTOR_QUALITY_REQUIRE_GRAPH`, and `ASTRAVECTOR_QUALITY_REQUIRE_MMR` as appropriate. Missing sparse vectors, missing Qdrant sparse config, non-ingested relation fixtures, and unavailable GraphRAG are reported as explicit reason codes and must not be interpreted as PASS.

`v007/fix474c` uses `SPARSE_MODE=LEXICAL_BASELINE_TECHNICAL` for the current dense-only ONNX artifact. The model still provides dense vectors through ONNX; sparse vectors are deterministic technical lexical vectors persisted to PostgreSQL and projected to Qdrant. Document ingestion and query retrieval share the same `SparseTechnicalEncoder`, so token extraction, normalization, stable hash index mapping, class weights and L2 normalization are identical for documents and questions.

The technical sparse baseline supports exact matching for numeric identifiers, alpha-numeric identifiers, error codes, underscore table/field names, endpoint paths, filenames, IP/port values, UUIDs, version/fix tokens and gRPC method names. Leading zeros are preserved. Sparse indices are stable SHA-256 based hashes with class namespaces rather than process-local dictionary ids. IDF is currently the default `1.0`, so hard-negative no-answer calibration remains a separate retrieval-quality task.

`v007/fix474d` adds the first production no-answer calibration pass. Default thresholds are visible under `search.no_answer` and in `runtime-quality-report.json`. The runtime applies weak-candidate filtering before graph expansion/MMR and a final no-answer gate after MMR. Hard-negative fixtures default to `max_false_positive_contexts=0`; non-zero fixture overrides are reported as `NON_ZERO_MAX_FALSE_POSITIVE_CONTEXTS_USED`.

`v007/fix474e` adds `make quality-runtime-confidence-remote`, a confidence gate for retrieval behavior changes. It is stricter than `quality-runtime-*` individual profile runs: dense, sparse and hybrid profiles must execute, sparse/hybrid must not be blocked, no-answer must be enabled, hard-negative after results are compared with `benchmarks/quality/baseline/hard-negative-baseline.json`, and forbidden/access leakage must be zero. Changing hard-negative fixtures requires manually updating `hard-negative-baseline.json`; fixture checksum enforcement is currently reported as `NOT_IMPLEMENTED`.

Quality target meanings:

- `quality-fixtures`: JSONL/schema/reference integrity only.
- `quality-quick`: static or retrieval-only quick check.
- `quality-runtime-*`: model-backed individual profile runs.
- `quality-runtime-confidence`: production confidence gate for retrieval behavior changes.

If Qdrant points exist but PUBLIC queries return zero contexts, inspect
`access_level_audit` in `runtime-quality-report.json`. `access_level=4` for all
fixtures means the ingestion runner or production ingestion mapping converted
fixture access levels incorrectly. Do not repair that symptom by relaxing
`RetrieveContext`; the production filter `access_level <= caller_access_level`
must stay strict.

## Reports

Generated files:

```text
benchmarks/quality/reports/quality-report.json
benchmarks/quality/reports/quality-report.md
benchmarks/quality/reports/failures.jsonl
benchmarks/quality/reports/candidates.jsonl
```

## Known limitations in fix468

- Full self-starting testcontainers ingestion mode is not yet implemented; the
  fix469 runtime bench currently targets an already running model-backed
  AstraVector endpoint.
- `production-candidate` test is still present as a planned strict gate and marked `#[ignore]`.
- LLM-as-judge, answer correctness, faithfulness, large nightly corpus and generated corpora are out of scope.

## Roadmap

- `fix469`: executable runtime quality bench with gRPC ingestion, outbox sync, Qdrant sync and RetrieveContext evaluation.
- `fix470`: capability-aware runtime diagnostics with PASSED/FAILED/BLOCKED query status, reason-code breakdowns and a dense-only quick profile.
- `fix471`: large/nightly corpus, synthetic corpus generator, DENSE/SPARSE/HYBRID comparisons.
- `fix472`: local LLM-as-judge for faithfulness, answer correctness, and citation correctness.
- `fix473`: full-capability runtime profiles with strict dense/sparse/hybrid/GraphRAG/MMR requirements, `by_mode`, sparse Qdrant evidence and graph relation evidence.
- `fix474`: sparse/hybrid runtime enablement with deterministic lexical sparse baseline, PostgreSQL sparse persistence and Qdrant sparse vector evidence.
- `fix474c`: technical-token lexical sparse encoder with stable-hash indices and mandatory document/query encoder consistency.
- `fix474d`: default no-answer thresholds, exact technical-token sparse boost, hard-negative static tests and runtime report diagnostics.
- `fix474e`: runtime confidence gate with baseline comparison, quality run id, dense/sparse/hybrid snapshots and strict SKIP/block/leakage handling.
- `fix472`: Quality Bench access-level ingestion mapping fix with PostgreSQL/Qdrant access-level audit and diagnostic forced-caller mode.
- `fix473`: large/nightly corpus, synthetic corpus generator, DENSE/SPARSE/HYBRID comparisons.
- `fix474`: local LLM-as-judge for faithfulness, answer correctness, and citation correctness.
