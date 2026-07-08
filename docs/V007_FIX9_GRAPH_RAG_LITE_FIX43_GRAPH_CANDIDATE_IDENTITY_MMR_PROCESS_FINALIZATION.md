# AstraVector v007 fix9 / GraphRAG Lite fix4.3

## Graph Candidate Identity & MMR Process Finalization Patch

This patch applies the fix4.3 hardening plan on top of fix4.2.

### Implemented changes

1. **Graph-expanded candidate identity**
   - `GraphChunkContextRecord` now carries optional binding identity fields:
     `qdrant_point_id`, `representation_type`, `dense_version`, `model_version`,
     `payload_version`, and `source_chunk_granularity`.
   - `fetch_contexts_for_graph_related_chunks(...)` joins active/legal-hold
     `vector_bindings_v004` and `embedding_cache_entries` to select a preferred
     graph candidate representation, prioritizing `ORIGINAL`.
   - Graph-expanded `SearchResultV004` metadata can now include stable identity
     fields and participate in point-aware embedding enrichment.

2. **Identity mode finalization**
   - `embedding_fetch_identity_mode` is restricted to `QDRANT_POINT_ID` in fix4.3.
   - `BINDING_ID` and `CHUNK_REPRESENTATION_VERSION` are explicitly reserved.

3. **Full candidate pool vs enrichment pool**
   - The embedding fetch prelimit no longer mutates `direct_results` and
     `graph_results` before strategy-aware selection.
   - The enrichment pool is derived separately and limited for fetch/cache work.

4. **Dense representation/version filtering**
   - `fetch_dense_embeddings_for_points(...)` filters `embedding_dense` by
     configured dense representation name and the cache entry dense version.

5. **Cache safety**
   - Invalid, empty, non-finite, or zero-norm embeddings are not inserted into the
     MMR embedding cache.

6. **MMR fallback correctness**
   - Dimension mismatch between two dense embeddings now falls back to token
     similarity instead of being counted as dense similarity with score `0.0`.

7. **Observability**
   - Added graph candidate identity metrics.
   - Added full/enrichment candidate pool metrics.
   - Added invalid/zero-norm/dimension-mismatch metrics.
   - Split candidate-missing and fetch-missing metrics while retaining backward
     compatible counters.

8. **API consistency**
   - `include_vectors=true` now produces `INCLUDE_VECTORS_IGNORED` warnings in
     debug paths as well as search paths.

9. **Benchmarks and tests**
   - `benches/mmr_bench.rs` now exercises production `apply_mmr_rerank(...)`
     rather than a toy local MMR implementation.
   - Added regression tests for mixed similarity, dimension mismatch fallback,
     graph append empty-direct behavior, and graph hit identity preservation.

### Required local validation

Run locally with Rust toolchain:

```bash
cargo fmt
cargo check --all-targets --all-features
cargo test --all-targets --all-features
cargo bench
```

Special attention:

- Prost regeneration after proto/config changes.
- SQLx mapping for pgvector values.
- `fetch_contexts_for_graph_related_chunks(...)` SQL compatibility with current migrations.
- `embedding_dense.representation_name` / `representation_version` filtering.
- Production MMR benchmark compilation.
