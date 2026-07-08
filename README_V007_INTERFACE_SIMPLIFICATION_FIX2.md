# AstraVector v007 interface simplification fix2

## Goal

This version completes the first runtime path for explainable RAG traceability:

```text
LogicalBlock -> AnnotatedTextSegment -> ChunkWithTrace -> PostgreSQL -> VectorBinding -> QdrantPoint -> RetrievedContext
```

## Implemented changes

- Added `AnnotatedTextSegment` and `ChunkSourceTrace` domain structures in `src/chunking/mod.rs`.
- Added `ChunkingEngine::chunk_segments(...)` for LogicalBlock-based tokenizer-aware chunking.
- `IndexLogicalDocument` now routes logical block metadata to annotated segment chunking rather than relying only on a flattened `source_text` path.
- Each generated chunk can carry:
  - `source_block_id`
  - `source_block_ids`
  - `source_location`
  - `source_links`
  - `trace_relation_type`
  - `trace_quality`
- PostgreSQL write path stores source trace into `content_chunks_v004` and `logical_block_chunk_mapping`.
- Qdrant publisher payload now includes:
  - `chunk_id`
  - `parent_chunk_id`
  - `source_block_id`
  - `trace_quality`
  - `trace_relation_type`
- `RetrieveContext` resolves matched chunk trace and returns source-aware `citation` and `source_links`.
- `DebugDocumentState` now exposes source trace fields in `DebugChunkInfoV005` and reports trace summary warnings.
- Added migration `0021_v007_fix2_logical_block_chunk_trace.sql`.

## Important limitations

This environment did not include Rust toolchain, `protoc`, PostgreSQL, Qdrant, or ONNX Runtime. Therefore compile/test/E2E smoke must be run locally before release.

Required local gate:

```bash
cargo fmt --check
cargo check --all-targets --all-features
cargo test --all-targets --all-features
```

E2E smoke:

```text
PostgreSQL + pgvector
Qdrant
ONNX BGE-M3
AstraVector runtime
Qdrant publisher
IndexLogicalDocument -> GetDocumentVectorStatus -> RetrieveContext -> DebugDocumentState
```
