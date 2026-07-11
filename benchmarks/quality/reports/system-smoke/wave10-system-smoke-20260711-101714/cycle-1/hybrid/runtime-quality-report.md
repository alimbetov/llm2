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
- require_graph: `false`
- require_mmr: `false`
- sparse_embeddings_count: `93171`
- qdrant_sparse_config_present: `true`
- qdrant_sparse_points_sampled: `256`
- qdrant_sparse_points_with_vectors: `252`
- relations_loaded_count: `12`
- relations_ingested_count: `108`
- graph_edges_available_count: `560545`
- graph_expanded_contexts_count: `0`
- fixtures_ingested_count: `29`
- documents_registered_count: `29`
- documents_indexed_count: `5853`
- access_zones_auto_created_count: `3`
- outbox_created_count: `94779`
- outbox_completed_count: `94779`
- outbox_dead_letter_count: `0`
- qdrant_collection_count: `1`
- qdrant_points_count: `94719`
- qdrant_payload_verified: `true`
- retrieve_context_queries_total: `64`
- retrieve_context_queries_passed: `64`
- retrieve_context_queries_failed: `0`
- retrieve_context_queries_blocked: `0`

## By Reason

- FINAL_CONTEXT_SET_TOO_WEAK: 12
- POST_MMR_NO_ANSWER_TRIGGERED: 12
- PRE_MMR_WEAK_CANDIDATE_FILTERED: 64

## Failures

- none
