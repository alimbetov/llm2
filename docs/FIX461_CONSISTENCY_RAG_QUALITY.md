# AstraVector v007/fix461 — Consistency and RAG Quality Patch

## Scope

fix461 is a focused consistency/RAG-quality patch on top of fix460. It targets the unified P0/P1 pool from the fix459/fix460 audits and keeps the release goal at **PRODUCTION CANDIDATE**, not PRODUCTION READY.

## Implemented hardening

### TTL lifecycle and PostgreSQL ↔ Qdrant consistency

- Added PostgreSQL delete fencing with `document_versions.delete_operation_id` before Qdrant point deletion.
- Final `DELETED` transition now requires the same `delete_operation_id` that fenced the operation.
- TTL cleanup now marks `vector_bindings_v004` as `DELETED` / `qdrant_sync_status='DELETED'` after successful document deletion.
- Qdrant delete point selection excludes `legal_hold` bindings.
- TTL claim excludes documents with legal-hold vector bindings.

### Multi-zone retrieval safety

- Final visibility recheck now returns compound `(access_zone_id, chunk_id)` keys.
- Multi-zone matched text and trace fetches now return compound `(access_zone_id, chunk_id)` maps.
- Direct/Graph merge and dedup now use `access_zone_id:matched_chunk_id` identity instead of only `matched_chunk_id`.

### GraphRAG visibility

- Graph expansion now joins `document_versions` and rejects expired/deleting/superseded document versions before candidate creation.
- Graph context fetch already had document visibility checks in fix460; fix461 keeps this as a required invariant.

### MMR consistency

- Chunk fallback embedding fetch now filters by `representation_name` and dense version.
- MMR point cache keys include document version, payload version, model version and dense version.
- Missing/invalid result access zone no longer silently falls back to the first search zone.

### Hybrid fusion and limits

- `NORMALIZED_WEIGHTED_SCORE` now performs min-max normalization before applying dense/sparse weights.
- Hybrid fusion config validation now checks method, weights, and `rrf_k`.
- Graph/direct retrieval candidate limits are clamped by configured hard caps.

## Remaining mandatory validation

This archive was produced without running Rust compile/test gates in this environment. Before merge, run:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo check --all-targets --all-features
cargo test --all-targets --all-features
cargo test --features integration-tests --test e2e_testcontainers -- --nocapture
```

## Release status

`fix461` should be treated as a **patch candidate** until CI passes. After CI and a real RetrieveContext/HYBRID/GraphRAG/MMR E2E pass, it can be promoted to **PRODUCTION CANDIDATE**.
