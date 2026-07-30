# FIX487A Retrieval Freeze Manifest

## Baseline

The retrieval freeze is anchored to:

```text
4843ce624724eceb865f64c6282d2841a69fcb88
```

## Protected Source Modules

The guard treats the following retrieval modules as protected:

- `src/grpc/mod.rs`
- `src/graph/mod.rs`
- `src/retrieval/**`
- `src/chunking/**`
- `src/qdrant/mod.rs`

Operational-only edits in these modules are permitted only when their changed hunks are limited to metrics, tracing, cancellation, concurrency, bounded timeouts or cleanup. Semantic edits are rejected.

## Protected Retrieval Symbols

The Phase A audit identified these high-risk retrieval symbols in `src/grpc/mod.rs`:

- `select_results_with_strategy_aware_mmr`
- `apply_mmr_rerank`
- `graph_seed_candidate_passes`
- `query_has_graph_recovery_intent`
- `select_graph_seed_candidates`
- `compare_graph_seed_candidates`
- `stable_result_rank`
- `strong_technical_query_tokens`
- `violates_query_exclusion_terms`
- `apply_no_answer_exact_technical_boost`
- `no_answer_candidate_passes`
- `is_negative_mention_evidence`
- `apply_pre_mmr_no_answer_filter`
- `apply_segmented_pre_mmr_no_answer_filter`
- `final_no_answer_should_trigger`
- `restore_graph_supported_direct_survivors`
- `graph_survivor_fallback_intent`
- `graph_expanded_relation_evidence_passes`
- `apply_post_mmr_technical_no_answer_filter`

Changes to these symbols require an explicit retrieval-quality phase, not FIX487 operations readiness.

## Protected Config Surface

The protected config surface includes:

- `config/application.yaml`
- `config/application-fix486*.yaml`
- `src/config/mod.rs`

Protected keys include:

- `search.rrf_k`
- `search.hybrid_fusion_method`
- `search.hybrid_dense_weight`
- `search.hybrid_sparse_weight`
- `search.query_processing.*_candidate_limit`
- `search.query_processing.segment_rrf_k`
- `search.no_answer.*`
- `graph_rag.retrieval.*`
- `graph_rag.scoring.*`
- `graph_rag.rerank.mmr_*`
- `chunking.*`
- tokenizer and token-budget safety parameters
- dense, sparse and hybrid model/vector parameters

## Protected Frozen Banks

The guard protects frozen benchmark evidence inputs:

- `benchmarks/hierarchical/fix486/**`
- `benchmarks/hierarchical/fix486g-supplemental/**`
- `benchmarks/quality/queries/**`
- `benchmarks/quality/qrels/**`
- `benchmarks/quality/profiles/**`
- `benchmarks/quality/corpora/**`
- `benchmarks/quality/judgments/**`
- `docs/fix486/**`

These files must not be modified in FIX487 operations readiness.

## Allowed Phase A Files

Phase A is allowed to add or update:

- `docs/fix487/**`
- `scripts/fix487_*.py`
- `scripts/fix487-*.sh`
- `tests/test_fix487_*.py`
- `Makefile` only for the `verify-fix487a-retrieval-freeze` target.

## Completion Invariant

The guard must report:

```text
retrieval_freeze_manifest_complete = true
protected_config_changed = 0
protected_fixture_changed = 0
protected_qrel_changed = 0
unapproved_retrieval_symbol_changed = 0
```
