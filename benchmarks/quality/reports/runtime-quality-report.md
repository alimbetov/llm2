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
- require_graph: `false`
- require_mmr: `true`
- sparse_embeddings_count: `61560`
- qdrant_sparse_config_present: `true`
- qdrant_sparse_points_sampled: `256`
- qdrant_sparse_points_with_vectors: `249`
- relations_loaded_count: `27`
- relations_ingested_count: `243`
- graph_edges_available_count: `374075`
- graph_expanded_contexts_count: `0`
- fixtures_ingested_count: `44`
- documents_registered_count: `44`
- documents_indexed_count: `3967`
- access_zones_auto_created_count: `3`
- outbox_created_count: `63168`
- outbox_completed_count: `63168`
- outbox_dead_letter_count: `0`
- qdrant_collection_count: `1`
- qdrant_points_count: `63108`
- qdrant_payload_verified: `true`
- retrieve_context_queries_total: `10`
- retrieve_context_queries_passed: `9`
- retrieve_context_queries_failed: `1`
- retrieve_context_queries_blocked: `0`

## By Reason

- MISSING_EXPECTED_BLOCK: 1
- PRE_MMR_WEAK_CANDIDATE_FILTERED: 10

## Failures

- mmr-extra-001: missing block `tech-outbox-life-001`
