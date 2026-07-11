# AstraVector Runtime Quality Bench

- verdict: `PASS`
- runtime_execution: `MODEL_BACKED_E2E_CONFIRMED`
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
- sparse_embeddings_count: `104571`
- qdrant_sparse_config_present: `true`
- qdrant_sparse_points_sampled: `256`
- qdrant_sparse_points_with_vectors: `252`
- relations_loaded_count: `27`
- relations_ingested_count: `243`
- graph_edges_available_count: `627773`
- graph_expanded_contexts_count: `60`
- fixtures_ingested_count: `44`
- documents_registered_count: `44`
- documents_indexed_count: `6526`
- access_zones_auto_created_count: `3`
- outbox_created_count: `106263`
- outbox_completed_count: `106263`
- outbox_dead_letter_count: `0`
- qdrant_collection_count: `2`
- qdrant_points_count: `105282`
- qdrant_payload_verified: `true`
- retrieve_context_queries_total: `97`
- retrieve_context_queries_passed: `97`
- retrieve_context_queries_failed: `0`
- retrieve_context_queries_blocked: `0`

## By Reason

- FINAL_CONTEXT_SET_TOO_WEAK: 14
- POST_MMR_NO_ANSWER_TRIGGERED: 14
- PRE_MMR_WEAK_CANDIDATE_FILTERED: 81

## Failures

- none
