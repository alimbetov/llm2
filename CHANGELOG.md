# CHANGELOG

## v007/fix475 — Focused GraphRAG Runtime Proof

- Added focused GraphRAG runtime proof: production relation fixtures are ingested through metadata, persisted/queryable in PostgreSQL graph tables, expanded through runtime 1-hop retrieval, and validated by `make quality-runtime-graph-quick-remote`.
- Updated readiness status to `GRAPH_RAG_READY` while keeping `production_candidate=false`; larger GraphRAG production-candidate proof remains open.

## v007/fix474j — Runtime Ready Final Evidence Closure

- Closed the final sparse/hybrid retrieval quality failures without weakening expected fixtures or treating SKIP/static output as PASS.
- Verified dense, sparse and hybrid runtime profiles as `MODEL_BACKED_E2E_CONFIRMED` / `PASS` against the live local runtime.
- Verified `make quality-runtime-confidence-remote` as `CONFIDENCE_GATE_CONFIRMED` / `PASS`, establishing `RUNTIME_READY`.
- Added automatic generation of `benchmarks/quality/reports/final-readiness-report.json` from every confidence run so stale readiness artifacts are overwritten by fresh confidence evidence.
- Documented that `production_pass=true` means confidence gate passed, not `PRODUCTION_CANDIDATE`.
- Documented GraphRAG policy: unavailable and not required for `RUNTIME_READY`, but required for `PRODUCTION_CANDIDATE`.
- Documented that distractor labels are evaluation/reporting labels only; production retrieval uses evidence-based weak-candidate rejection, not hardcoded fixture IDs.

## v007/fix474f — Runtime Ready Closure Verification

- Fixed runtime startup configuration parsing for empty-string env defaults and access-zone regex defaults in `config/application.yaml`.
- Added stronger confidence-gate preflight diagnostics for gRPC, PostgreSQL, Qdrant, model/tokenizer files, Qdrant collection presence, and vector schema.
- Updated `quality-runtime-confidence-remote` to provide local DB/Qdrant/model defaults, ingestion-time access-zone auto-create, and a unique quality run id.
- Fixed Dockerfile duplicate `--bins`, added `.dockerignore`, and verified the local Docker image build on `rust:1.96-trixie`.
- Updated testcontainers to use a pgvector-capable PostgreSQL image; remaining E2E failures are Qdrant/lifecycle synchronization issues, not missing `vector.control`.
- Historical checkpoint: dense runtime profile passed while sparse/hybrid profiles still failed at fix474f time; this gap is closed by `v007/fix474j`.

## v007/fix474e — Runtime Confidence Gate for Sparse/Hybrid/No-Answer

- Added `benchmarks/quality/baseline/hard-negative-baseline.json` as the machine-readable fix474c hard-negative baseline for confidence comparison.
- Added `scripts/quality-runtime-confidence.sh` and Makefile targets `quality-runtime-confidence`, `quality-runtime-confidence-remote`, and `quality-runtime-confidence-report`.
- Confidence runs now require a `quality_run_id`, preserve dense/sparse/hybrid runtime report snapshots, reject SKIP/BLOCKED mandatory profiles, compare hard-negative before/after results, and enforce zero forbidden leakage for production PASS.
- Added diagnostic-only mode via `ASTRAVECTOR_QUALITY_CONFIDENCE_DIAGNOSTIC_ONLY=true`; it exits 0 for evidence gathering but reports `production_pass=false`.
- Added `quality_confidence_report_contracts` static tests for PASS, endpoint failure, skipped profile, sparse/hybrid blocked, hard-negative target failure, forbidden leakage, and diagnostic-only verdict rules.

## v007/fix474d — No-Answer Threshold Calibration and Hard-Negative Guardrails

- Added explicit `search.no_answer` defaults for dense, sparse and hybrid retrieval thresholds, sparse-only matched-term checks, exact technical-token boosting and hard-negative strictness.
- Added production retrieval no-answer filtering before graph/MMR and a final no-answer gate after MMR, returning empty contexts when final evidence is too weak.
- Added exact technical-token sparse boost using the shared `SparseTechnicalEncoder` classes, without query-id or phrase-specific bypasses.
- Added runtime quality report diagnostics for no-answer defaults, execution order, pre-MMR filtered counts, post-MMR trigger counts and debug behavior.
- Added focused static hard-negative tests proving empty no-answer contexts pass, forbidden leaks fail, and default `max_false_positive_contexts` is strict zero.

## v007/fix474c — Technical Token Lexical Sparse Encoder

- Added `LEXICAL_BASELINE_TECHNICAL` sparse mode with a shared production `SparseTechnicalEncoder` for document ingestion and query retrieval.
- Added deterministic SHA-256 based sparse index mapping with token-class namespaces; no process-local dynamic dictionary is used for raw technical tokens.
- Added technical token extraction for numeric IDs with leading zeros, alpha-numeric IDs, error codes, underscore identifiers, paths/endpoints, filenames, IP/port values, UUIDs, version/fix tokens and gRPC service/method names.
- Added class-aware log-TF weighting, default IDF, L2 normalization and contract tests proving document/query index consistency.
- Added sparse technical benchmark corpus and golden queries, plus runtime report diagnostics for encoder version, sparse index strategy and query token analysis.
- Documented that hard-negative no-answer calibration remains a separate retrieval-quality patch.

## v007/fix472 — Quality Bench Access-Level Ingestion Mapping Fix

- Fixed runtime quality fixture ingestion to map fixture `access_level` through the production `AccessLevel` enum instead of indexing every fixture as `RESTRICTED`.
- Added `access_level_audit` to `runtime-quality-report.json` with fixture, PostgreSQL and Qdrant access-level distributions.
- Added `ACCESS_LEVEL_FIXTURE_MAPPING_MISMATCH` diagnostics for the case where Qdrant points exist but PUBLIC retrieval returns zero contexts because indexed payloads are all `access_level=4`.
- Added diagnostic-only `ASTRAVECTOR_QUALITY_FORCE_CALLER_ACCESS_LEVEL`; reports using it include `forced_caller_access_level` and are not counted as production quality PASS.
- Documented that the fix must not weaken the production `RetrieveContext` filter `access_level <= caller_access_level`.

## v007/fix470 — Capability-aware Runtime Quality Bench Diagnostics

- Added runtime capability detection for dense, sparse, hybrid, GraphRAG and MMR availability in `runtime-quality-report.json`.
- Runtime query diagnostics now distinguish `PASSED`, `FAILED`, `BLOCKED` and `SKIPPED_RUNTIME_REQUIRED`.
- Added reason-code breakdowns such as `SPARSE_UNAVAILABLE`, `MISSING_EXPECTED_DOCUMENT`, `MISSING_EXPECTED_BLOCK` and `MISSING_REQUIRED_PHRASE`.
- Added `by_category`, `by_reason`, structured `runtime-failures.jsonl` rows and preserved `runtime-candidates.jsonl`.
- Added `dense-only-quick` profile, `dense-only-golden` queries, and Makefile targets `quality-runtime-dense-quick` / `quality-runtime-dense-quick-remote`.
- Documented that `MODEL_BACKED_E2E_FAILED` can mean retrieval quality or capability gaps even when ingestion, outbox and Qdrant succeeded.

## v007/fix469 — Executable Model-backed Quality Bench patch candidate

- Added `tests/quality_bench_runtime_quick.rs` to execute `benchmarks/quality/**` through the production gRPC ingestion facade and `RetrieveContext`.
- Added runtime preflight diagnostics for model/tokenizer files, PostgreSQL, Qdrant, gRPC endpoint reachability, and access-zone auto-create flags.
- Runtime bench now writes `runtime-quality-report.json`, `runtime-quality-report.md`, `runtime-failures.jsonl`, and `runtime-candidates.jsonl`.
- Added Makefile targets `quality-runtime-quick`, `quality-runtime-quick-remote`, and `quality-runtime-full`.
- Added `docs/QUALITY_BENCH_RUNTIME.md` describing runtime mode, environment, reports, and common failures.

## v007/fix468 — Quality Bench Enriched Data Bank patch candidate

- Expanded `benchmarks/quality/` with enriched curated corpora: `technical-mini`, `legal-mini`, `distractor-mini`, `long-doc-mini`, and `ttl-legal-hold-mini`.
- Increased the Quality Bench bank to 35+ documents, 140+ blocks, 100+ golden queries, and 20+ GraphRAG/relation edges.
- Added hard negatives with lexical overlap, long-document target-block checks, access-zone/access-level conflict matrices, GraphRAG expected related blocks, and MMR expected aspect coverage.
- Extended `query.schema.json` and report schema with enriched expected fields and metrics such as `forbidden_block_ids`, `expected_related_block_ids`, `expected_aspects`, `hard_negative_false_positive_rate`, and `long_document_target_block_hit_rate`.
- Strengthened `tests/quality_fixtures_contracts.rs` to validate enriched corpus scale and all expected/forbidden references.
- Extended `tests/quality_bench_quick.rs` remote-mode evaluation and reports for enriched expected fields while keeping static validation mode safe for CI.
- Updated `quick` and `production-candidate` Quality Bench profiles and documentation.

## v007/fix467 — Quality Bench Foundation patch candidate

- Added `benchmarks/quality/` with curated JSONL corpora, relations, golden queries, profiles, schemas and report directory.
- Added `tests/quality_fixtures_contracts.rs` for fixture structure and quality dataset validation.
- Added `tests/quality_bench_quick.rs` with static validation mode and optional remote gRPC `RetrieveContext` mode via `ASTRAVECTOR_QUALITY_ENDPOINT`.
- Added ignored `tests/quality_bench_production_candidate.rs` placeholder for the future strict testcontainers ingestion gate.
- Added `docs/QUALITY_BENCH.md`, Makefile targets and CI jobs `quality-fixtures`, `quality-quick`, and release-only `quality-production-candidate`.
- Quality Bench reports are generated as `benchmarks/quality/reports/quality-report.json` and `quality-report.md`.


## v007/fix466 — Spring-like configuration profiles

- Added profile overlays: `config/application-dev.yaml`, `config/application-test.yaml`, `config/application-prod.yaml`.
- Updated `AppConfig::load` to load `config/application.yaml` first and merge `application-{profile}.yaml` based on `ASTRAVECTOR_PROFILE` / `ASTRAVECTOR_ENV`.
- Added `docs/CONFIG_PROFILES.md` and profile contract tests.
- Kept explicit custom config compatibility: non-`application.yaml` configs do not automatically load profile overlays unless `ASTRAVECTOR_PROFILE_CONFIG` is provided.


## v007/fix464 — P1 consistency and gateway hardening

- Added atomic PostgreSQL binding claim before Qdrant `DELETE_POINT`.
- Changed `mark_synced rows=0` from silent metric-only success to fenced `OwnershipLost` error.
- Fixed graceful shutdown ordering: readiness false, cancel token, then drain.
- Made recovery/retention workers `CancellationToken` aware.
- Added recovery error logging/metrics for embedding-cache and document-indexing recovery.
- Added trusted-gateway proof for forwarded identity headers and restricted gRPC NetworkPolicy ingress to `app=astravector-gateway`.
- Added real tonic ingestion facade E2E coverage for `IndexLogicalDocument` + `ActivateDocumentVersion`.
- Added `tests/fix464_p1_hardening_contracts.rs`.

# v007/fix462-enhanced — Production Candidate validation gates

- Added network-level tonic RetrieveContext E2E gate.
- Added SQLx schema validation requirement to CI.
- Added optional smoke-load RetrieveContext test.
- Added qdrant_reconciliation_enabled rollback flag.
- Enhanced legal-hold reconciliation classification with orphan/skipped metrics.
- Added operator docs for config, migration, rollback, observability and smoke load.


## v007/fix460 — RAG/GraphRAG/TTL production-candidate hardening

- Added production-path testcontainers E2E coverage for registration, persistence, embedding cache, vector bindings, outbox publication, Qdrant search, TTL cleanup, and post-cleanup visibility rejection.
- Added document-version visibility filtering to GraphRAG context fetch and direct chunk text/trace fetch.
- Added final PostgreSQL visibility recheck before returning `RetrieveContext` contexts.
- Fixed MMR embedding enrichment for multi-zone searches and added TTL/document visibility filters to embedding fetches.
- Hardened TTL cleanup with PostgreSQL binding-based Qdrant deletion, Qdrant extra-point reconciliation, and `rows_affected` fencing before child cleanup.
- Added real TTL backlog/dead-letter gauges and corrected Qdrant active/available concurrency metrics.
- Added configurable hybrid fusion method and dense/sparse weights.
- Added migration `0035_v007_fix460_rag_ttl_consistency.sql` and operations note `docs/FIX460_RAG_TTL_CONSISTENCY.md`.


## V007 fix4.5.6 — Index TTL Cleanup Runtime Wiring & Registry Cache Consistency

- Added runtime wiring for `IndexTtlCleanupWorker` in `main.rs` when `index_ttl.enabled && index_ttl.cleanup_enabled`.
- Added `lifecycle::spawn_index_ttl_cleanup(...)` background worker for stale `DELETING`, TTL cleanup batches, and optional tombstone purge.
- Added Access Zone Registry direct PostgreSQL fallback on cache miss for both `access_zone_code` and UUID `access_zone_id`.
- Split access-zone diagnostics into `ACCESS_ZONE_NOT_FOUND`, `ACCESS_ZONE_DISABLED`, and `ACCESS_ZONE_DELETED`.
- Hardened TTL cleanup: graph cleanup failures now surface through `index_ttl_graph_cleanup_failed_total` and mark the document version as `DELETE_FAILED`; tombstone purge is transactional.
- Added fix4.5.6 acceptance test skeleton for TTL worker and registry cache consistency.


## v007 fix4.5.5 — Access Zone Auto-Provisioning, Bind Fix & Registry Lifecycle Hardening

- Fixed `StartLogicalDocumentIngestion` bind order for `ingestion_sessions_v004.access_zone_code`: `access_zone_code` now has its own bind between `access_zone_id` and `document_id`.
- Added controlled ingestion-time access zone auto-provisioning: unknown valid `access_zone_code` may create a UUID-backed `access_zones` row when `access_zone_registry.auto_create_on_ingestion=true`.
- Search/RetrieveContext remain read-only with respect to access-zone registry and never auto-create zones.
- Added `access_zone_registry` auto-create config flags and validation that `auto_create_on_search` remains false.
- Added migration `0031_v007_fix455_access_zone_auto_provisioning.sql` with audit fields: `auto_created`, `created_by`, `created_reason`, `first_seen_at`, `last_seen_at`.
- Added metrics for auto-create attempts/success/failures/denials and Code Matrix TTL assignments.
- Added documentation `docs/V007_FIX16_FIX455_ACCESS_ZONE_AUTO_PROVISIONING.md` and ignored integration-test skeletons for fix4.5.5 acceptance scenarios.

# v007 fix4.5.4 — Access Zone Registry, Code Matrix TTL Policy & UUID-backed Zone Resolution

- Added `astravector.access_zones` registry with UUID-backed `access_zone_id` and immutable external `access_zone_code` (`0000..9999`).
- Added code-matrix default TTL policy persisted as `access_zones.default_ttl_days`; matrix is not recomputed dynamically for existing zones.
- Added proto fields for `access_zone_code` / `access_zone_codes` on ingestion, Search and RetrieveContext contracts.
- Added `AccessZoneResolver` to resolve external codes to UUIDs and enforce ACTIVE-zone status.
- Search/RetrieveContext now can resolve code-based multi-zone requests into UUID-backed filters.
- Qdrant payload now includes `access_zone_code` as a diagnostic alias while continuing to filter by UUID `access_zone_id`.
- Added registry/cache/mismatch/invalid-code metrics and documentation.


## v007 fix13 / GraphRAG Lite fix4.5.2 — Finalize Stale-State Safety, Streaming Contract & Integration Proof

- Protected active `FINALIZING` ingestion sessions from normal TTL cleanup; stale finalizing now uses heartbeat/stale-timeout semantics.
- Added `finalizing_started_at`, `finalizing_heartbeat_at`, `last_error_code`, `last_error_message`, `last_error_at` schema support.
- Added bounded finalize mode config and non-terminal memory guard behavior.
- Finalize completion now checks `rows_affected` and returns `INGESTION_FINALIZE_LOST_OWNERSHIP` on lost ownership.
- Append now validates `batch_content_hash` server-side before storing batch metadata.
- Finalize now validates staged batch hash in `validate_staged_batch_consistency`.
- Qdrant collection lifecycle calls are wrapped with config-driven retry.
- `TRUNCATE_LAST_CHUNK` now accounts for already-used prefix tokens.
- Added CI-ready Python gRPC load smoke stub generation assets and integration-test feature marker.



## v007 fix12 / GraphRAG Lite fix4.5.1 - Finalize Concurrency, Streaming Memory Safety & Testcontainers Proof Patch

- Hardened `FinalizeLogicalDocumentIngestion` with atomic `ACTIVE -> FINALIZING` ownership acquisition.
- Added explicit completed/finalizing/failed/aborted/expired handling for chunked finalize replay.
- Added failure marking for indexing errors so sessions no longer remain stuck in `FINALIZING`.
- Reworked staged block loading to use bounded paged reads controlled by `finalize_read_batch_size` and memory guards.
- Reordered `StartLogicalDocumentIngestion` idempotency lookup before active-session limit enforcement.
- Reordered `AppendLogicalDocumentBlocks` replay detection before `max_blocks_per_document` enforcement.
- Split cleanup retention for completed staging rows versus completed session `result_response_json` replay.
- Added fix4.5.1 migration for `completed_blocks_cleaned_at` and `result_expires_at`.
- Improved Qdrant non-retryable HTTP status mapping to domain errors.
- Propagated `DROPPED_CHUNK_IDS_TRUNCATED` into search diagnostics.
- Added runnable/skip-safe `tests/load/test_fix451_grpc_load_smoke.py` scaffold for real gRPC smoke when generated stubs are available.

Verification note: Rust toolchain was unavailable in the patching environment; local `cargo fmt/check/test/bench` is required before approval.

## V007 Fix11 / GraphRAG Lite fix4.5 — Chunked Finalize & Resilience Completion

- Completed chunked ingestion finalize flow: staged blocks are rehydrated, hash-checked, indexed through `IndexLogicalDocument`, and stored for idempotent replay.
- Hardened Start/Append idempotency with request fingerprints, batch metadata, batch hash validation, transactional counters, and batch byte limits.
- Added `ingestion_session_batches_v004` and finalize replay fields via migration `0026_v007_fix45_chunked_finalize_resilience_completion.sql`.
- Added staging cleanup worker and session limit enforcement.
- Added inference retry/backoff and made Qdrant retry config-driven.
- Implemented SOURCE chunk `FULL_TEXT` / `METADATA_ONLY` / `DISABLED` runtime behavior.
- Applied config-driven search/graph limits and improved token-budget truncation behavior.
- Added skip-safe pytest load-smoke scaffold.

Verification note: `cargo` is unavailable in this execution environment; Rust fmt/check/test/bench must be run locally before production approval.

# fix4.4-rev2 — Load Hardening, Large Document Ingestion & Runtime Resilience

- Added configurable ingestion limits and large-document rejection guidance.
- Added chunked ingestion proto contract and PostgreSQL staging migrations.
- Added basic Start/Append/GetStatus/Abort staging handlers; Finalize remains guarded until staging rehydration tests are added.
- Added bounded concurrent document embedding submit via `Scheduler::submit_many`.
- Added Qdrant retry/backoff wrapper for upsert and search operations.
- Added post-MMR token-budget truncation diagnostics and conservative drop-lowest-score behavior.
- Added fix4.4-rev2 documentation and load-test skeleton.


## v007 fix9 / GraphRAG Lite fix4.3 — Graph Candidate Identity & MMR Process Finalization

- Added graph-expanded candidate embedding identity fields (`qdrant_point_id`, representation metadata, dense/model versions).
- Restricted `embedding_fetch_identity_mode` to `QDRANT_POINT_ID` for fix4.3; unsupported modes are reserved.
- Separated embedding enrichment pool from full candidate pool to avoid recall loss before strategy-aware selection.
- Added dense representation/version filtering to point-aware embedding fetch.
- Prevented invalid/empty/zero-norm vectors from being cached.
- Added dimension-mismatch fallback from dense MMR to token fallback.
- Added graph candidate identity, enrichment pool, invalid embedding, and dimension mismatch metrics.
- Added debug-path `INCLUDE_VECTORS_IGNORED` warning.
- Replaced toy MMR benchmark with production `apply_mmr_rerank(...)` benchmark cases.
- Added regression tests for graph identity and MMR fallback semantics.


## V007 Fix8 / GraphRAG Lite fix4.2 — Embedding Identity & Production Cache Hardening

- Added identity-aware dense embedding fetch for MMR using `qdrant_point_id`.
- Added `qdrant_point_id`, `representation_type`, `dense_version`, `model_version`, `payload_version`, and `chunk_granularity` to search result metadata.
- Added controlled chunk fallback config for embedding fetch.
- Replaced MMR embedding cache implementation with `moka::future::Cache`.
- Added pair-level dense/token fallback in MMR; mixed sessions are reported as `MIXED`.
- Added pre-limiting of embedding fetch candidate pools.
- Made `GRAPH_AS_CONTEXT_APPEND` handle empty direct result pools without suppressing graph appendix budget.
- Made `mmr_allow_direct_candidates` and `mmr_allow_graph_candidates` active behavior flags.
- Applied configurable graph relation debug limit in strategy-aware metadata merge.
- Added warning behavior for `include_vectors=true`, because search response embeddings remain internal-only.
- Extended diagnostics and metrics for cache/fetch/pair-level MMR behavior.


## V007 fix7 / GraphRAG Lite fix4.1 — End-to-End MMR & Strategy Semantics Patch

- Added batch PostgreSQL dense embedding fetch for MMR candidates.
- Added optional in-memory embedding cache with TTL/max entries.
- Added safe token fallback on embedding fetch errors/timeouts/missing vectors.
- Added strategy-aware MMR for SCORE_THEN_TRUNCATE, DIRECT_FIRST, and GRAPH_AS_CONTEXT_APPEND.
- Added group-specific MMR lambdas: mmr_lambda_direct and mmr_lambda_graph.
- Made FAIL_INDEXING override WARN_AND_CONTINUE for semantic large-document policy.
- Converted retrieval_sources and graph_relations metadata to JSON array format.
- Added embedding fetch diagnostics, score calibration diagnostics, and additional metrics.
- Added documentation: docs/V007_FIX7_GRAPH_RAG_LITE_FIX41_END_TO_END_MMR_STRATEGY_SEMANTICS.md.


## v007 fix6 — GraphRAG Lite fix4 Mathematical Retrieval Optimization

- Added embedding-aware MMR with token fallback and missing-embedding metrics.
- Added direct/graph score calibration config and metadata.
- Replaced semantic graph full sort with top-K heap selection.
- Added dedicated Rayon pool behavior for `semantic_parallelism > 0`.
- Preserved secondary retrieval sources and graph relation metadata during dedup.
- Made `FAIL_INDEXING` semantic large-document policy strict.
- Added explicit `GRAPH_AS_CONTEXT_APPEND` direct/graph budgets and validation.
- Documented `score_normalization=MIN_MAX` as reserved/future only.


## v007-interface-simplification

- Added public facade gRPC APIs for llm_indexator, ai_bro, and admin/operator use.
- Added `IndexLogicalDocument` for LogicalBlock[] ingestion with tokenizer-aware chunking delegated to AstraVector.
- Added `RetrieveContext` for ai_bro to receive matched_text, parent_text, citation, scores, source_links and metadata.
- Added `SourceLink`, `SourceLocation`, `DocumentIdentity`, `RequestContext`, `DocumentRef`, and standard operation status contracts.
- Preserved existing v004/v005 internal reliability mechanisms: outbox, activation gate, sync status, Qdrant reconciliation, TTL, diagnostics and adaptive runtime.

# 0.4.0
- Added access-zone partitioning, multi-granularity chunking, reliability primitives, reconciliation, relevance and quarantine contracts.

# v0.3.0

- Added vector bindings with access level and TTL.
- Added Qdrant outbox projection and publisher.
- Added lifecycle worker and purge rules.
- Added enrichment/relevance modules and protobuf APIs.
- Added REQUIRED-aware L1 cache metadata.

# Changelog

- fix485 follow-up: make the gRPC query deadline runtime-configurable through
  `ASTRAVECTOR_GRPC_QUERY_DEADLINE_MS` while preserving the 1000 ms default.
- Degrade a Hybrid request with an explicit diagnostic when an optional
  PostgreSQL FTS segment times out or is unavailable and another retrieval
  branch already produced evidence; cancellation and failures without fallback
  evidence remain request failures.
- Harden the Mac model-backed load evidence with detected hardware values,
  Cargo.lock identity, explicit load deadline budgets, and locked Cargo gates.

## 0.2.0
- Production reliability redesign according to AstraVector_v002 specification.
- Real ONNX adapter, provider self-test, checksums.
- Claim-before-inference, lease/fencing/takeover and bounded polling.
- Idempotency and request/item audit.
- Strict REQUIRED semantics.
- Concurrent batch submission and deadline-aware bucket scheduler.
- Authentication, metrics, retention, readiness and graceful shutdown.
- Kubernetes baseline manifests.

## v005-hardening-statistical-fix2

- Added paginated Qdrant scroll for document point-id reconciliation.
- Added scroll pagination config: page size, max pages, max points, timeout and max concurrency.
- Added loop detection for repeated Qdrant `next_page_offset`.
- Added failure-as-not-ready semantics by returning errors on timeout/limit/loop/Qdrant failures instead of partial success.
- Added Qdrant scroll metrics.
- Extended `DebugQdrantInfoV005` with scroll status/pages/points diagnostics.
- `DebugDocumentState` now uses the paginated Qdrant point-id helper directly.

## v007-interface-simplification-fix2

- Added runtime foundation for explainable RAG traceability.
- Added annotated segment chunking path for `IndexLogicalDocument`.
- Stored chunk source trace in PostgreSQL and mapping table.
- Added `source_block_id` and trace fields to Qdrant payload.
- Enhanced `RetrieveContext` to resolve matched chunk trace into citation/source links.
- Extended `DebugChunkInfoV005` with source trace fields.
- Added migration `0021_v007_fix2_logical_block_chunk_trace.sql`.

## v007 interface simplification fix3 graph lite

- Added PostgreSQL-partitioned GraphRAG Lite tables for structural document/block/chunk graph.
- Added bounded graph builder with priority/depth block selection and adjacent sibling edges.
- Added table-aware `CHUNK_SAME_TABLE` edge support with explicit edge cap.
- Added transactional graph build inside the indexing transaction using graph cleanup and batch insert.
- Added graph build `WARN_AND_CONTINUE` behavior through savepoint rollback.
- Added 1-hop graph expansion for retrieval with TTL/access/freshness filtering.
- Added dense/sparse parallel Qdrant search and partial fallback warnings.
- Added graph metadata in retrieval results and graph summary in debug output.

## v007 fix4 — GraphRAG Lite fix2 Hardening Patch

- Added semantic relation `CHUNK_SEMANTIC_SIMILAR`.
- Added in-memory semantic edge builder over dense embeddings.
- Added L2 normalization and configurable `semantic_power` scoring.
- Added direct + graph result merge before final truncation.
- Added configurable relation weights and hop penalties.
- Added semantic/debug graph summary fields.
- Added graph metrics hooks.
- Reworked graph edge persistence to use `sqlx::QueryBuilder` batch insert/upsert.
- Added migration `0023_v007_fix4_graph_rag_lite_fix2_hardening.sql`.
- Added Criterion benchmark skeletons.

## v007 fix4 GraphRAG Lite Balanced Mode hardening patch

- Extended GraphRAG rebuild timeout to cover semantic build, structural build, cleanup and batch persistence.
- Added strict `final_context_limit_mode` with default `STRICT`.
- Implemented merge strategies: `SCORE_THEN_TRUNCATE`, `DIRECT_FIRST`, `GRAPH_AS_CONTEXT_APPEND`.
- Added response-level retrieval debug fields to `SearchDiagnosticsV004`.
- Added optional Rayon path for semantic edge generation.
- Fixed duplicate semantic prepared embedding insertion.
- Added relation-distribution metrics for graph expansion, merge, filtering and persistence.
- Replaced graph merge placeholder benchmark with real merge benchmark cases.

## v007 fix5 — GraphRAG Lite fix3 Reranking & Diversity Safety

- Added MMR-style diversity reranking after direct+graph merge.
- Added MMR diagnostics to `SearchDiagnosticsV004`.
- Added MMR metrics.
- Reduced default `semantic_max_chunks_for_in_memory` from 1000 to 500.
- Added `semantic_large_document_policy=SKIP_SEMANTIC`.
- Added reserved learned reranker config with strict validation disabled by default.
- Added MMR unit tests and benchmark skeleton.

## v007 fix4.5.3 — Index TTL Lifecycle, Multi-Zone Access Contract & Batch Deletion

- Added `ttl_days` to chunked ingestion start contract.
- Added `access_zone_ids` to Search/RetrieveContext contracts for multi-zone retrieval.
- Added config blocks `index_ttl` and `access_zones`.
- Added migration `0029_v007_fix453_index_ttl_multi_zone_batch_deletion.sql` for document lifecycle fields, TTL backfill, indexes and tombstone support.
- Added Qdrant multi-zone search filter and numeric `expires_at_epoch` filter.
- Added far-future Qdrant epoch strategy for never-expire documents.
- Added PostgreSQL multi-zone context fetch helpers and GraphRAG multi-zone expansion helpers.
- Added lifecycle cleanup helpers for batch Qdrant deletion, DELETE_FAILED retry, stale DELETING recovery and tombstone purge.
- Documented payload drift mitigation and future reconciliation path.

## v007 fix4.5.7 — Production Readiness, Testcontainers Proof & Cleanup Recovery Hardening

- Added typed TTL cleanup stages and stable error-code mapping.
- Removed string-matching based TTL cleanup error classification.
- Added idempotency counter for already-absent Qdrant points during cleanup retry.
- Added multi-zone search large-set metric.
- Added production-readiness migration indexes for access-zone registry and TTL cleanup retry paths.
- Added executable fix4.5.7 contract tests and environment-gated PostgreSQL/Qdrant harness checks.
- Documented Code Matrix TTL correction: `1500–1999` maps to `365` days; `1000–1499` maps to `182` days.
- Added alerting guidance for TTL backlog, cleanup failures, registry fallback, auto-created zones, and large multi-zone searches.

## v007 fix459 — production-candidate hardening

- Added real PostgreSQL + Qdrant testcontainers lifecycle test.
- Hardened GraphRAG joins by `(access_zone_id, node_id)`.
- Switched GraphRAG parent context to safe `LEFT JOIN` fallback.
- Added document lifecycle/TTL filters to parent context fetches.
- Added strict DB recheck option for ingestion access-zone resolution.
- Added typed auto-create conflict handling for disabled/deleted zones.
- Added `RetryDocumentDeletion` admin gRPC method.
- Made `next_delete_attempt_at` authoritative for TTL delete retry scheduling.
- Fixed Kubernetes gRPC probe service name.
- Added migration 0034 for `document_versions.metadata` and hardening indexes.

## v007/fix461 — Consistency/RAG Quality patch candidate

- Added document delete fencing before Qdrant TTL cleanup using `delete_operation_id`.
- TTL cleanup now marks `vector_bindings_v004` deleted after successful document deletion.
- Legal-hold bindings are excluded from document TTL point deletion and TTL claim.
- Final visibility, text fetch, trace fetch, merge and dedup use compound `(access_zone_id, chunk_id)` semantics.
- GraphRAG expansion now joins `document_versions` to reject expired/superseded documents earlier.
- MMR chunk fallback now filters dense embedding representation/version.
- MMR cache keys now include document version, payload version, model version and dense version for point-based candidates.
- Hybrid normalized weighted fusion now normalizes dense/sparse scores before applying weights.
- Added fix461 migration `0036_v007_fix461_consistency_fencing.sql`.
- Added `docs/FIX461_CONSISTENCY_RAG_QUALITY.md` and `docs/ALERTS.md`.

Note: this build must pass Rust compile/test gates before being considered a release artifact.

## v007/fix462 — production-candidate closure patch

- Added RetrieveContext RPC-path E2E coverage scaffold to the testcontainers lifecycle test.
- Made direct parent grouping compound-key based: `(access_zone_id, parent_id)`.
- Added zone identity fields to `RelatedChunk` and switched production GraphRAG expansion to seed keys `(access_zone_id, chunk_id)`.
- Added PostgreSQL-checked Qdrant extra-point reconciliation to avoid deleting legal-hold points.
- Fixed tombstone purge ordering by deleting `vector_bindings_v004` before `content_chunks_v004`.
- Added migration for `document_versions.last_delete_error_stage` used by `RetryDocumentDeletion`.
- Strengthened `delete_operation_id` guards for lifecycle transitions around TTL cleanup.

## v007/fix463 — production-candidate-stabilization

- Added outbox operation-version fencing for UPSERT/UPDATE/DELETE_POINT events.
- Hardened binding TTL generation for delete events and TTL extension races.
- Hardened reconciliation payload contract and legal-hold handling.
- Implemented active reconciliation CLI/worker modes.
- Added finalizing heartbeat during long chunked ingestion indexing.
- Hardened tombstone purge against live outbox rows.
- Fixed gRPC `grpc-timeout` parser and query timeout clamping helpers.
- Added RetrieveContext access-zone lineage to response metadata/proto.
- Fixed Kubernetes Qdrant runtime config, NetworkPolicy egress, migration command and image tags.
- Added SQLx online CI gate, Docker build gate, K8s dry-run gate and Makefile `verify-fix463`.

# 0.4.1-fix465-p2-production-hardening

- Added Qdrant payload index creation for retrieval filter fields.
- Counted all retrieval sources in `retrieved_contexts_by_source_total`.
- Sanitized checksum mismatch user-facing errors.
- Replaced dynamic PostgreSQL `SET` timeout statements with `set_config` bind parameters.
- Added blocking self-contained smoke-load Testcontainers gate.
- Aligned active image tags to `0.4.1-fix465-p2-production-hardening`.
- Removed no-op enrichment worker from production Docker image scope.
- Added Grafana dashboard JSONs and observability documentation.

## v007/fix474 — Sparse/Hybrid Runtime Enablement

- Added deterministic lexical sparse baseline for dense-only ONNX artifacts.
- Persisted lexical sparse vectors through the existing `embedding_sparse` path.
- Projected sparse vectors through the existing Qdrant sparse vector field.
- Extended runtime quality reporting with `sparse_mode`, sparse sample evidence, and hybrid availability fields.
- Documented that dense-only PASS does not imply sparse/hybrid PASS.
