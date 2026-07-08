# V007 fix3 GraphRAG Lite implementation notes

This version implements the final consolidated GraphRAG Lite scope on top of `AstraVector_v007_interface_simplification_fix2`.

## Implemented

- PostgreSQL partitioned graph schema in `migrations/0022_v007_fix3_graph_lite.sql`.
- `src/graph` module with structural graph domain model and bounded graph builder.
- Priority/depth-based block selection for graph nodes.
- `max_children_per_block`, node/edge limits, adjacent sibling edges only.
- `CHUNK_SAME_TABLE` only for chunks with a stable `source_location.table_id`; adjacent row-style edges only and capped by `max_same_table_edges`.
- Graph freshness fields plus retrieval-time filtering through `content_chunks_v004` as source of truth.
- Transactional graph cleanup/build/insert in the existing V004 persistence transaction.
- Savepoint-based `WARN_AND_CONTINUE` graph build failure mode.
- Batch graph insert repository method.
- `RetrieveContext`/`SearchRequestV004` graph expansion fields.
- 1-hop graph expansion from seed chunks.
- Dense/sparse Qdrant parallel search with partial fallback.
- Graph-expanded result metadata: `retrieval_source`, `graph_seed_chunk_id`, `graph_relation_type`, `graph_relation_score`, `graph_relation_boost`, `graph_hop_distance`.
- DebugDocument graph summary.

## Runtime caveat

This environment does not provide `cargo`, `rustc`, `protoc`, PostgreSQL, Qdrant, or ONNX Runtime. Compile and E2E execution must be verified locally.

Required local gate:

```bash
cargo fmt --check
cargo check --all-targets --all-features
cargo test --all-targets --all-features
```

Required E2E gate:

```text
IndexLogicalDocument
→ graph build
→ Qdrant publisher
→ RetrieveContext(enable_graph_expansion=false)
→ RetrieveContext(enable_graph_expansion=true)
→ DebugDocumentState
```
