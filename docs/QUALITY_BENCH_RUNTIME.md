# AstraVector Runtime Quality Bench

`quality_bench_runtime_quick` is the executable runtime quality bench. It is separate from static fixture validation and from retrieval-only remote checks.

## Modes

`make quality-fixtures` validates the `benchmarks/quality/**` data bank only: JSON/JSONL shape, schemas, profile references, document/query scale and relation integrity. It does not prove model loading, ingestion, Qdrant, outbox, or retrieval.

`make quality-quick` runs the quick quality evaluator in static mode unless `ASTRAVECTOR_QUALITY_ENDPOINT` is set. Static mode is useful for PR fixture checks, but it is not model-backed E2E evidence.

`make quality-quick-remote` calls `RetrieveContext` against an already prepared runtime. It does not ingest fixtures and does not create access zones. On a clean database it is expected to fail with `ACCESS_ZONE_NOT_FOUND`.

`make quality-runtime-quick` is the model-backed runtime path. It loads quick-profile fixtures, sends documents through the production gRPC ingestion facade, relies on ingestion-time access-zone auto-create, waits for outbox/Qdrant readiness, runs `RetrieveContext`, and writes runtime reports.

`make quality-runtime-dense-quick-remote` runs the same model-backed path with the `dense-only-quick` profile. Use it when the loaded ONNX artifact produces dense embeddings but no sparse output. It does not turn a production-candidate sparse gap into PASS; it isolates dense ingest/retrieve evidence from sparse capability failures.

`make quality-runtime-sparse-quick-remote`, `make quality-runtime-hybrid-quick-remote`, `make quality-runtime-graph-quick-remote`, and `make quality-runtime-full-capability-quick-remote` run strict capability profiles. These targets set `ASTRAVECTOR_QUALITY_REQUIRE_*` flags and fail with machine-readable reasons such as `SPARSE_UNAVAILABLE`, `QDRANT_SPARSE_CONFIG_MISSING`, `GRAPH_RELATIONS_NOT_INGESTED`, or `GRAPH_RAG_UNAVAILABLE` when the runtime evidence is incomplete.

`make quality-runtime-confidence-remote` is the retrieval confidence gate for sparse, hybrid and no-answer changes. It runs dense, sparse and hybrid runtime profiles, preserves per-profile snapshots, compares hard-negative results to `benchmarks/quality/baseline/hard-negative-baseline.json`, and fails if any required profile is skipped or blocked. `SKIPPED_ENDPOINT_NOT_SET`, `SKIPPED_RUNTIME_REQUIRED`, missing endpoint, missing Qdrant data, unavailable sparse/hybrid evidence, disabled no-answer policy, forbidden leakage and access/security violations are never production PASS.

## fix474j Runtime Ready Closure Notes

The confidence gate now performs explicit preflight diagnostics for the gRPC endpoint, PostgreSQL, Qdrant, Qdrant collection/schema, model file, and tokenizer file. `make quality-runtime-confidence-remote` also sets local DB/Qdrant/model defaults, enables ingestion-time access-zone auto-create, disables search-time auto-create, and assigns a unique `ASTRAVECTOR_QUALITY_RUN_ID` when one is not supplied.

The current local fix474j evidence is:

- dense runtime profile: `MODEL_BACKED_E2E_CONFIRMED`, `PASS`
- sparse runtime profile: `MODEL_BACKED_E2E_CONFIRMED`, `PASS`
- hybrid runtime profile: `MODEL_BACKED_E2E_CONFIRMED`, `PASS`
- confidence gate: `CONFIDENCE_GATE_CONFIRMED`, `PASS`
- runtime ready: `true`
- production candidate: `false`
- production ready: `false`

Do not treat the dense-only profile as full runtime readiness. `RUNTIME_READY` requires the confidence gate to pass with all mandatory profiles. `production_pass=true` in the confidence report means the confidence gate passed; it does not mean `PRODUCTION_CANDIDATE`.

The preflight records model and tokenizer file presence. Model-backed inference proof comes from the mandatory runtime profiles until a separate pre-runtime ONNX probe is added.

GraphRAG policy:

- `graph_rag_available = false`
- `graph_rag_required_for_ready = false`
- `graph_rag_required_for_production_candidate = true`

Graph diagnostic or skipped cases are not counted as PASS. GraphRAG remains a blocker for `PRODUCTION_CANDIDATE`, not for `RUNTIME_READY`.

Distractor fixture labels are used for evaluation/reporting only. Production retrieval rejects weak common-overlap candidates based on evidence, not by hardcoded distractor IDs or fixture labels.

## Sparse Mode

`v007/fix474c` enables `SPARSE_MODE=LEXICAL_BASELINE_TECHNICAL` when the loaded ONNX artifact is dense-only. The current local BGE-M3 ONNX file exposes `token_embeddings` and `sentence_embedding`; it does not expose a learned sparse output. The technical lexical baseline is deterministic: document ingestion and query retrieval both call the same production `SparseTechnicalEncoder` core, apply class-aware log-TF weights, L2-normalize weights, and write the result to the existing `embedding_sparse` table and Qdrant sparse vector field.

This is not a fake sparse vector path. It is a production-safe lexical sparse baseline until a neural sparse-capable artifact is provided. Dense-only PASS still does not imply sparse/hybrid PASS; sparse/hybrid profiles require PostgreSQL sparse rows, sampled Qdrant sparse vectors, and exact-token fixture hits.

The encoder extracts ordinary words plus technical token classes: numeric identifiers with leading zeros, alpha-numeric identifiers, error codes, underscore identifiers, paths/endpoints, filenames, IPv4/port endpoints, UUIDs, version/fix tokens, and gRPC service/method names. Leading zeros are preserved as strings; `00000445543` is never normalized to `445543`.

Raw technical tokens map to Qdrant sparse indices through a stable SHA-256 based hash with token-class namespace salts. AstraVector does not use a process-local dynamic dictionary such as `HashMap<String, usize>` for sparse indices, because ingestion and retrieval can run in different processes or after restarts. IDF is currently `1.0`; the baseline uses class-aware log-TF weighting until corpus-level IDF is implemented.

Runtime reports expose `sparse.encoder_version`, `sparse.technical_sparse_index_strategy`, `sparse.document_query_encoder_consistency_checked`, and query-level `candidate_debug.sparse_query_non_zero_terms` plus technical token lists for benchmark diagnostics.

## Required Runtime

Start AstraVector with real model paths and local infrastructure before running the remote runtime target. The runtime bench expects:

```bash
ASTRAVECTOR_QUALITY_ENDPOINT=http://localhost:50051
ASTRAVECTOR_QUALITY_RUNTIME_MODE=ingest-and-retrieve
ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true
ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_SEARCH=false
ASTRAVECTOR_DB_URL=postgres://astravector:astravector@127.0.0.1:55432/astravector
ASTRAVECTOR_QDRANT_URL=http://127.0.0.1:6333
ASTRAVECTOR_QDRANT_COLLECTION=astravector_v004
```

Model preflight accepts either concrete paths:

```bash
ASTRAVECTOR_MODEL_PATH=/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/onnx/model.onnx
ASTRAVECTOR_TOKENIZER_PATH=/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/tokenizer.json
```

or a model root:

```bash
ASTRAVECTOR_MODEL_DIR=/Users/ruslanalimbetov/Documents/llm2/models
```

## Commands

Static fixture gate:

```bash
make quality-fixtures
```

Runtime quick against a running local endpoint:

```bash
make quality-runtime-quick-remote
```

Dense-only runtime quick against a running local endpoint:

```bash
make quality-runtime-dense-quick-remote
```

Sparse, hybrid, graph, and full-capability quick profiles:

```bash
make quality-runtime-sparse-quick-remote
make quality-runtime-hybrid-quick-remote
make quality-runtime-graph-quick-remote
make quality-runtime-full-capability-quick-remote
```

Confidence gate:

```bash
make quality-runtime-confidence-remote
make quality-runtime-confidence-report
```

The gate writes profile snapshots and aggregate reports:

```text
benchmarks/quality/reports/runtime-quality-report.dense.json
benchmarks/quality/reports/runtime-quality-report.sparse.json
benchmarks/quality/reports/runtime-quality-report.hybrid.json
benchmarks/quality/reports/runtime-confidence-report.json
benchmarks/quality/reports/runtime-confidence-report.md
```

Each confidence run uses `ASTRAVECTOR_QUALITY_RUN_ID` if supplied, otherwise a generated `fix474e-YYYYMMDD-HHMMSS` value. The runtime bench stores the run id in fixture metadata and report rows, and uses it to namespace deterministic runtime document UUIDs. Generated reports are runtime artifacts and should not be committed as source.

Diagnostic-only evidence collection:

```bash
ASTRAVECTOR_QUALITY_CONFIDENCE_DIAGNOSTIC_ONLY=true make quality-runtime-confidence-remote
```

Diagnostic-only exits 0 and writes `verdict=DIAGNOSTIC_ONLY`, `production_pass=false`, and `not_production_pass=true`. It is not a production PASS.

Production-candidate profile against a running endpoint:

```bash
make quality-runtime-full
```

If `ASTRAVECTOR_QUALITY_ENDPOINT` is not set, `quality_bench_runtime_quick` writes a `SKIPPED_ENDPOINT_NOT_SET` report and exits without claiming runtime quality.

## Reports

The runtime bench writes:

```text
benchmarks/quality/reports/runtime-quality-report.json
benchmarks/quality/reports/runtime-quality-report.md
benchmarks/quality/reports/runtime-failures.jsonl
benchmarks/quality/reports/runtime-candidates.jsonl
```

The JSON report is structured by stage:

```text
preflight  - model/tokenizer, gRPC, PostgreSQL, Qdrant, auto-create flags
ingestion  - fixtures ingested, documents registered/indexed, auto-created zones
outbox     - created/completed/dead-letter counts
qdrant     - collection count, point count, payload verification
access_level_audit - fixture, PostgreSQL and Qdrant access-level distributions
retrieval  - query pass/fail counts and quality metrics
no_answer  - threshold defaults, exact technical boost strategy and filter counters
capabilities - dense/sparse/hybrid/GraphRAG/MMR availability detected from runtime evidence
capability_requirements - strict `ASTRAVECTOR_QUALITY_REQUIRE_*` gate flags
sparse    - PostgreSQL sparse embedding count plus Qdrant sparse config/vector sample evidence
hybrid    - availability, fusion strategy and exposed branch/fused hit counters
graph     - loaded relation fixtures, ingested relation count, graph edge count and expanded-context evidence
by_category - PASSED/FAILED/BLOCKED/SKIPPED counts and recall per fixture category
by_mode     - PASSED/FAILED/BLOCKED/SKIPPED counts for dense, sparse, hybrid, graph and MMR query modes
by_reason   - machine-readable reason-code counts
failures   - exact machine-readable failure reasons
```

`MODEL_BACKED_E2E_CONFIRMED` is emitted only when ingestion, outbox, Qdrant, and retrieval all pass. Static mode and retrieval-only mode must not use that status.

`MODEL_BACKED_E2E_FAILED` does not necessarily mean the runtime did not work. Check the stage sections first. A report can show successful ingestion, ACTIVE document versions, completed outbox events, and Qdrant points while retrieval quality gates still fail.

Query statuses:

- `PASSED`: the returned contexts satisfied the fixture assertions.
- `FAILED`: retrieval executed but missed required documents, blocks, phrases, access constraints, ranking, latency or other quality expectations.
- `BLOCKED`: the fixture required a capability that was unavailable, for example sparse/hybrid search when the loaded ONNX artifact has no sparse output.
- `SKIPPED_RUNTIME_REQUIRED`: reserved for fixtures that cannot be evaluated in the current runtime mode.

`SPARSE_UNAVAILABLE` means sparse or hybrid retrieval was requested, but runtime evidence shows `sparse_available=false`. The production-candidate profile may still fail overall, but the reason is reported separately from ordinary retrieval misses.

`SPARSE_QUERY_VECTOR_EMPTY`, `SPARSE_ENCODER_UNAVAILABLE`, `SPARSE_ENCODER_OUTPUT_EMPTY`, `SPARSE_EMBEDDINGS_MISSING`, and `QDRANT_SPARSE_POINTS_MISSING` identify the exact stage where sparse evidence is absent. `HYBRID_SPARSE_BRANCH_EMPTY` and `HYBRID_FUSION_EMPTY` identify hybrid execution that did not actually use sparse evidence.

## No-Answer Policy

`v007/fix474d` adds an explicit first-pass no-answer policy. These are calibration defaults, not final relevance truths:

```yaml
search:
  no_answer:
    enabled: true
    min_dense_score: 0.25
    min_sparse_score: 0.10
    min_hybrid_score: 0.30
    sparse_only_min_matched_terms: 2
    sparse_only_require_technical_token: true
    exact_technical_boost: 0.50
    hard_negative_strict: true
```

The execution order is dense search, sparse search, hybrid/fusion, pre-MMR weak candidate filtering, graph expansion, MMR, final no-answer policy, empty context return for weak evidence, then formatting/truncation. Required reason codes are `PRE_MMR_WEAK_CANDIDATE_FILTERED`, `POST_MMR_NO_ANSWER_TRIGGERED`, `FINAL_CONTEXT_SCORE_BELOW_THRESHOLD`, and `FINAL_CONTEXT_SET_TOO_WEAK`.

Exact technical token matches use the shared `SparseTechnicalEncoder`; no query-id or phrase-specific exceptions are allowed. Strong technical token classes are `numeric_exact`, `alphanumeric`, `error_code`, `underscore_identifier`, `path`, `filename`, `ip_or_port`, `uuid`, `grpc_method`, and `version_token`. The selected strategy is `boosted_sparse_score = sparse_score * (1.0 + exact_technical_boost)`, and sparse-only exact technical candidates are allowed when `sparse_score >= min_sparse_score * 2.0`.

Full candidate diagnostics are enabled by `RetrieveContext`/`Search` debug detail or by `ASTRAVECTOR_RETRIEVAL_DEBUG_CANDIDATES=true` / `ASTRAVECTOR_QUALITY_DEBUG_CANDIDATES=true`. With debug disabled, the policy performs only scoring and exact-token checks and does not emit full document text.

Manual sparse checks:

```bash
psql "$ASTRAVECTOR_DB_URL" -c 'select count(*) from astravector.embedding_sparse;'
curl -s http://127.0.0.1:6333/collections/astravector_v004 | jq '.result.config.params.sparse_vectors'
curl -s -X POST http://127.0.0.1:6333/collections/astravector_v004/points/scroll \
  -H 'Content-Type: application/json' \
  -d '{"limit":10,"with_payload":true,"with_vector":["sparse"]}' | jq
```

`GRAPH_RELATIONS_NOT_INGESTED` means relation fixtures were loaded from `relations.jsonl`, but the runtime ingestion path did not persist those fixture relations into the graph index. Loaded relation files alone do not count as GraphRAG capability evidence.

Read `by_category` and `by_mode` to see whether failures are concentrated in lexical sparse, hybrid, graph, MMR, access isolation, or dense semantic queries. Read `by_reason` to separate missing expected evidence from capability gaps such as `SPARSE_UNAVAILABLE`.

## Common Failures

`ACCESS_ZONE_NOT_FOUND` in `quality-quick-remote` usually means the fixtures were not ingested first. Search and `RetrieveContext` intentionally do not create access zones. Access zones must be created only through ingestion with `ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true`.

`QDRANT_NOT_POPULATED` means ingestion did not produce completed outbox events and searchable Qdrant points. Check the outbox section first; a dead-letter or retry backlog is usually the useful symptom.

`MODEL_FILES_NOT_FOUND` means the runner could not find the ONNX model or tokenizer from `ASTRAVECTOR_MODEL_PATH`/`ASTRAVECTOR_TOKENIZER_PATH` or the `ASTRAVECTOR_MODEL_DIR` fallback.

`AUTO_CREATE_ON_SEARCH_MUST_BE_FALSE` means the runtime environment is unsafe for this bench. Search-time zone creation is intentionally rejected.

`QDRANT_FILTER_ZERO_HITS` with non-zero Qdrant points usually means a mandatory retrieval filter removed every candidate. Check `access_level_audit` first. If fixtures declare `access_level=PUBLIC` but PostgreSQL and Qdrant show every searchable binding as `4`, the problem is an ingestion mapping bug, not a retrieval security bug. Do not fix this by weakening `RetrieveContext` access filtering; `access_level <= caller_access_level` must remain strict.

`ACCESS_LEVEL_FIXTURE_MAPPING_MISMATCH` means the runner detected fixture access levels that do not match indexed PostgreSQL/Qdrant payload levels. For diagnostics only, `ASTRAVECTOR_QUALITY_FORCE_CALLER_ACCESS_LEVEL=RESTRICTED` can confirm that contexts reappear when the caller is allowed to see restricted data. Reports generated with this override include `forced_caller_access_level` and must not be treated as production quality PASS.

## Remaining Carryover

The runner still keeps implementation in one test file for low integration risk. Future improvements are a shared `tests/support/quality_runtime` module, richer MRR/rank metrics, latency percentiles from all queries, and an optional harness that starts a disposable runtime automatically.
