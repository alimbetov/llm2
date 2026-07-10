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
- sparse_embeddings_count: `63855`
- qdrant_sparse_config_present: `true`
- qdrant_sparse_points_sampled: `256`
- qdrant_sparse_points_with_vectors: `249`
- relations_loaded_count: `27`
- relations_ingested_count: `243`
- graph_edges_available_count: `387629`
- graph_expanded_contexts_count: `23`
- fixtures_ingested_count: `44`
- documents_registered_count: `44`
- documents_indexed_count: `4099`
- access_zones_auto_created_count: `3`
- outbox_created_count: `65463`
- outbox_completed_count: `65463`
- outbox_dead_letter_count: `0`
- qdrant_collection_count: `1`
- qdrant_points_count: `65403`
- qdrant_payload_verified: `true`
- retrieve_context_queries_total: `97`
- retrieve_context_queries_passed: `90`
- retrieve_context_queries_failed: `7`
- retrieve_context_queries_blocked: `0`

## By Reason

- FINAL_CONTEXT_SET_TOO_WEAK: 18
- GRAPH_EXPANSION_TIMEOUT: 1
- GRAPH_EXPECTED_RELATED_BLOCK_MISSING: 5
- MISSING_EXPECTED_BLOCK: 4
- MISSING_EXPECTED_DOCUMENT: 3
- MISSING_REQUIRED_SOURCE: 5
- POST_MMR_NO_ANSWER_TRIGGERED: 18
- PRE_MMR_WEAK_CANDIDATE_FILTERED: 81

## Failures

- technical-graph-001: graph related block missing `tech-recon-run-001`
- technical-graph-001: missing retrieval source `GRAPH_EXPANDED`
- technical-graph-003: missing block `tech-ttl-clean-002`
- technical-graph-003: missing retrieval source `GRAPH_EXPANDED`
- graph-002: graph related block missing `qdrant-003`
- graph-extra-006: missing document `tech-ttl-cleanup-001`
- graph-extra-006: missing block `tech-ttl-clean-002`
- graph-extra-007: graph related block missing `tech-recon-run-005`
- graph-extra-007: missing retrieval source `GRAPH_EXPANDED`
- graph-extra-009: missing document `tech-quality-bench-runbook-001`
- graph-extra-009: missing block `tech-qbench-003`
- graph-extra-009: graph related block missing `tech-qbench-004`
- graph-extra-009: missing retrieval source `GRAPH_EXPANDED`
- graph-extra-010: missing document `legal-retention-hold-001`
- graph-extra-010: missing block `legal-retention-hold-001`
- graph-extra-010: graph related block missing `legal-retention-conflict-001`
- graph-extra-010: missing retrieval source `VECTOR_DIRECT`
- graph-extra-010: missing retrieval source `GRAPH_EXPANDED`
