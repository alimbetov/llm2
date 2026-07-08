# V007 Fix8 / GraphRAG Lite fix4.2
# Embedding Identity & Production Cache Hardening Patch

## Purpose

This patch hardens the fix4.1 end-to-end MMR implementation. It targets the defects found during the repeated static audit:

1. Dense embeddings must be fetched by retrieval identity, preferably `qdrant_point_id`, not plain `chunk_id`.
2. MMR must use pair-level dense/token fallback instead of switching the whole session to token fallback when one embedding is missing.
3. Embedding enrichment should limit the candidate pools before DB/cache lookup.
4. MMR embedding cache should use a production cache API rather than `Mutex<HashMap>`.
5. Graph relation debug limits must use config, not a hardcoded value.
6. `GRAPH_AS_CONTEXT_APPEND` must handle empty direct results correctly.
7. `mmr_allow_direct_candidates` and `mmr_allow_graph_candidates` are now active behavior flags.
8. `include_vectors=true` is explicitly reported as ignored for search responses.

## Key changes

### Identity-aware embedding fetch

The search result metadata now records:

- `qdrant_point_id`
- `representation_type`
- `dense_version`
- `model_version`
- `payload_version`
- `chunk_granularity`

The primary embedding fetch path now uses:

```rust
fetch_dense_embeddings_for_points(access_zone_id, qdrant_point_ids)
```

Plain chunk-based fetch remains available only as a controlled fallback when enabled by config:

```yaml
graph_rag:
  rerank:
    embedding_fetch_identity_mode: QDRANT_POINT_ID
    embedding_fetch_allow_chunk_fallback: false
```

### Pair-level MMR fallback

The MMR similarity function now works per pair:

```text
if both candidates have dense embeddings:
  dense dot product
else:
  token Jaccard fallback for this pair only
```

The effective session similarity source is reported as:

- `DENSE_EMBEDDING`
- `TOKEN_JACCARD`
- `MIXED`

### Cache hardening

The MMR embedding cache now uses `moka::future::Cache` with TTL and max capacity. Cache keys are identity-aware:

```text
access_zone_id:point:qdrant_point_id
```

When chunk fallback is explicitly enabled, keys include representation and model version:

```text
access_zone_id:chunk:chunk_id:representation_type:dense_version
```

### Strategy and config consistency

`GRAPH_AS_CONTEXT_APPEND` now computes graph budget from actual selected direct candidates:

```text
graph_budget = min(graph_context_append_limit, final_context_limit - direct_selected.len())
```

`max_graph_relations_debug_per_candidate` is now applied by the metadata merge path used in strategy-aware selection.

`mmr_allow_direct_candidates` and `mmr_allow_graph_candidates` now affect MMR behavior. When disabled for a group, that group falls back to score order.

### Diagnostics and metrics

The proto diagnostics were extended with:

- cache hits/misses
- fetch errors/timeouts
- skipped fetch flags
- dense/token pair comparison counters
- effective MMR similarity source
- warning codes

## Required local validation

```bash
cargo fmt
cargo check --all-targets --all-features
cargo test --all-targets --all-features
cargo bench
```

The patch was statically applied in an environment without Rust toolchain. Local compile/test validation is mandatory.
