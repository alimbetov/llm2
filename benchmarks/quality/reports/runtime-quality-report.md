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
- require_sparse: `false`
- require_hybrid: `false`
- require_graph: `true`
- require_mmr: `false`
- sparse_embeddings_count: `34725`
- qdrant_sparse_config_present: `true`
- qdrant_sparse_points_sampled: `256`
- qdrant_sparse_points_with_vectors: `240`
- relations_loaded_count: `27`
- relations_ingested_count: `243`
- graph_edges_available_count: `216031`
- graph_expanded_contexts_count: `0`
- fixtures_ingested_count: `25`
- documents_registered_count: `25`
- documents_indexed_count: `2450`
- access_zones_auto_created_count: `3`
- outbox_created_count: `36321`
- outbox_completed_count: `36321`
- outbox_dead_letter_count: `0`
- qdrant_collection_count: `1`
- qdrant_points_count: `36273`
- qdrant_payload_verified: `true`
- retrieve_context_queries_total: `3`
- retrieve_context_queries_passed: `0`
- retrieve_context_queries_failed: `3`
- retrieve_context_queries_blocked: `0`

## By Reason

- FINAL_CONTEXT_SET_TOO_WEAK: 1
- GRAPH_EXPECTED_RELATED_BLOCK_MISSING: 2
- MISSING_EXPECTED_BLOCK: 2
- MISSING_EXPECTED_DOCUMENT: 1
- MISSING_REQUIRED_PHRASE: 1
- MISSING_REQUIRED_SOURCE: 3
- POST_MMR_NO_ANSWER_TRIGGERED: 1

## Failures

- graph-001: missing phrase `Qdrant drift is repaired by reconciliation`
- graph-001: missing document `tech-reconciliation-001`
- graph-001: missing block `recon-001`
- graph-001: graph related block missing `recon-002`
- graph-001: missing retrieval source `VECTOR_DIRECT`
- graph-001: missing retrieval source `GRAPH_EXPANDED`
- graph-003: missing retrieval source `GRAPH_EXPANDED`
- graph-extra-007: missing block `tech-outbox-life-003`
- graph-extra-007: graph related block missing `tech-recon-run-005`
- graph-extra-007: missing retrieval source `GRAPH_EXPANDED`
