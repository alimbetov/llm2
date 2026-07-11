# AstraVector Runtime Quality Bench

- verdict: `FAIL`
- runtime_execution: `MODEL_BACKED_E2E_FAILED`
- skipped_reason: ``
- model_files_found: `true`
- tokenizer_found: `true`
- grpc_endpoint_reachable: `true`
- postgres_reachable: `true`
- qdrant_reachable: `true`
- auto_create_on_ingestion: `true`
- auto_create_on_search: `false`
- dense_available: `true`
- sparse_available: `true`
- hybrid_available: `true`
- graph_rag_available: `true`
- mmr_available: `true`
- require_dense: `true`
- require_sparse: `true`
- require_hybrid: `true`
- require_graph: `true`
- require_mmr: `true`
- sparse_embeddings_count: `97950`
- qdrant_sparse_config_present: `true`
- qdrant_sparse_points_sampled: `256`
- qdrant_sparse_points_with_vectors: `252`
- relations_loaded_count: `12`
- relations_ingested_count: `108`
- graph_edges_available_count: `588622`
- graph_expanded_contexts_count: `21`
- fixtures_ingested_count: `8`
- documents_registered_count: `8`
- documents_indexed_count: `6129`
- access_zones_auto_created_count: `3`
- outbox_created_count: `99570`
- outbox_completed_count: `99570`
- outbox_dead_letter_count: `0`
- qdrant_collection_count: `1`
- qdrant_points_count: `99498`
- qdrant_payload_verified: `true`
- retrieve_context_queries_total: `12`
- retrieve_context_queries_passed: `10`
- retrieve_context_queries_failed: `2`
- retrieve_context_queries_blocked: `0`

## By Reason

- GRAPH_EXPANSION_TIMEOUT: 2
- MISSING_REQUIRED_SOURCE: 2
- PRE_MMR_WEAK_CANDIDATE_FILTERED: 3

## Failures

- rag-bank-seed-001: missing retrieval source `GRAPH_EXPANDED`
- rag-bank-seed-002: missing retrieval source `GRAPH_EXPANDED`
