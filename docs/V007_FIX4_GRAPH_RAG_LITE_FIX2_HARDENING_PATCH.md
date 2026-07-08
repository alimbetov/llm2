# AstraVector v007 fix4: GraphRAG Lite fix2 Hardening Patch

This patch hardens GraphRAG Lite fix2 instead of introducing a new feature line.

Implemented scope:

1. `CHUNK_SEMANTIC_SIMILAR` relation support.
2. Same-document in-memory semantic edge builder.
3. L2 embedding normalization and dot-product semantic scoring.
4. `semantic_power` configurable nonlinear semantic edge scoring.
5. Direct + graph result merge before final context truncation.
6. Configurable graph relation weights and hop penalties.
7. Semantic graph build, graph expansion, merge and batch persistence metrics.
8. `sqlx::QueryBuilder` batch insert/upsert for graph nodes and edges.
9. Unique graph-edge identity constraint migration.
10. Debug graph metadata extensions for semantic edge statistics and scoring config.
11. Criterion benchmark skeletons for semantic graph build and graph merge.

Important runtime behavior:

- Direct vector/hybrid results and graph-expanded candidates are collected independently.
- Candidates are deduplicated by `matched_chunk_id` and sorted by `final_score` before final limiting.
- `CHUNK_SEMANTIC_SIMILAR` can be enabled or disabled through `graph_rag.retrieval.allowed_relations`.
- `graph_rag.scoring.semantic_power = 1.0` preserves linear behavior.
- Values below `1.0` reduce semantic-edge penalty; values above `1.0` make semantic relation filtering stricter.

Not included in this hardening patch:

- FAISS backend.
- SIMD / packed_simd implementation.
- LRU embedding cache.
- Qdrant semantic edge backend.
- Cross-document semantic graph.
- MMR / softmax / sigmoid reranking.
