# AstraVector v004 Smoke Discovery

Generated from the current repository shape, not from desired-state assumptions.

## Cargo

- Package: `astravector-runtime`
- Edition: Rust 2021
- Features: `cpu` default, `cuda`, `tensorrt`
- Binary targets found:
  - `astravector-runtime` from `src/main.rs`
  - `astravector-enrichment`
  - `astravector-qdrant-publisher`
  - `astravector-reconciliation`
  - `astravector-lifecycle`

## Runtime Commands

- Runtime: `cargo run --bin astravector-runtime`
- Migrations: `cargo run --bin astravector-runtime -- migrate`
- Qdrant publisher: `cargo run --bin astravector-qdrant-publisher`
- Lifecycle one-shot: `cargo run --bin astravector-lifecycle`
- Reconciliation worker: `cargo run --bin astravector-reconciliation`
- Enrichment worker: `cargo run --bin astravector-enrichment`

## Configuration

- Default config: `config/application.yaml`
- Smoke config: `smoke-tests/v004/config/application-smoke.yaml`
- Required model/tokenizer variables:
  - `ASTRAVECTOR_MODEL_PATH`
  - `ASTRAVECTOR_TOKENIZER_PATH`
  - checksums are required only when `service.environment=production`
- PostgreSQL URL variable: `ASTRAVECTOR_DB_URL`
- Qdrant variables:
  - `ASTRAVECTOR_QDRANT_ENABLED`
  - `ASTRAVECTOR_QDRANT_URL`
  - `ASTRAVECTOR_QDRANT_COLLECTION`

## gRPC

Proto file: `proto/astravector_embedding.proto`

Implemented and registered service in `src/main.rs`:

- `astravector.embedding.v1.AstraVectorRuntime`

Methods implemented in `src/grpc/mod.rs`:

- `Encode`
- `EncodeBatch`
- `GetContract`
- `GetCapabilities`
- `Health`
- `DeleteDocumentVectors`
- `UpdateVectorMetadata`
- `ExtendVectorTtl`
- `GetVectorSyncStatus`
- `EvaluateRelevance`

Declared but not registered in `src/main.rs`:

- `astravector.embedding.v1.AstraVectorV004Control`

This blocks direct smoke coverage for `CreateMultiGranularityChunks`, document-version activation, parent resolution, quarantine API, legal hold, and group TTL through gRPC.

## PostgreSQL Schema

Migrations create schema `astravector` and `pgvector` extension.

Core v004 tables:

- `astravector.document_versions`
- `astravector.content_chunks_v004`
- `astravector.vector_bindings_v004`
- `astravector.qdrant_quarantine`
- `astravector.reconciliation_runs`
- `astravector.reconciliation_findings`
- `astravector.enrichment_jobs`
- `astravector.relevance_feedback_v004`

Legacy/core embedding tables used by runtime:

- `astravector.embedding_requests`
- `astravector.embedding_items`
- `astravector.embedding_cache_entries`
- `astravector.embedding_dense`
- `astravector.embedding_sparse`
- `astravector.vector_outbox`

Partition expectations from migration `0010_v004_access_zone_partitioning.sql`:

- `document_versions`: 32 hash partitions
- `content_chunks_v004`: 32 hash partitions
- `vector_bindings_v004`: 32 hash partitions
- `qdrant_quarantine`: 32 hash partitions from migration `0012`

## Qdrant

- Runtime uses REST client from `src/qdrant/mod.rs`.
- Smoke collection: `astravector_smoke_v004`
- Smoke ports: HTTP `56333`, gRPC `56334`

## Existing Tests

- `tests/checksum.rs`
- `tests/dense.rs`
- `tests/sparse.rs`
- `tests/sparse_invalid.rs`
- `tests/v004_domain.rs`

## Known Blockers From Code Inspection

- `cargo` and `rustc` were not on PATH in the current execution environment during discovery.
- `grpcurl` was not on PATH during discovery.
- The supplied corpus path is a single file without extension, not a directory.
- `AstraVectorV004Control` is declared in proto but not registered by the runtime server.
- No Rust implementation of the generated `AstraVectorV004Control` server trait was found under `src/`.
- Reconciliation binary initializes a `Reconciler` and waits for Ctrl-C; no periodic run-loop is wired.
- Enrichment binary uses `DisabledEnrichmentProvider` only.
- Lifecycle binary is one-shot; runtime also spawns lifecycle loop.
- `main.rs` logs `AstraVector v0.3.0 starting` even in v004 package.
