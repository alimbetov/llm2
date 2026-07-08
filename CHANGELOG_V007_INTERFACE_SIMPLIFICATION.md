# CHANGELOG v007-interface-simplification

## Added

- Added `AstraVectorIngestionFacade` for `llm_indexator`.
- Added `IndexLogicalDocument`, `GetDocumentVectorStatus`, `DeleteDocumentVectorsFacade`.
- Added `AstraVectorRetrievalFacade` for `ai_bro`.
- Added `RetrieveContext`, `ExplainRetrieve`.
- Added `AstraVectorAdminFacade` for simplified operator APIs.
- Added facade proto structures: `RequestContext`, `DocumentRef`, `OperationStatus`, `DocumentIdentity`, `LogicalBlock`, `SourceLocation`, `SourceLink`, `TokenAwareChunkingOptions`, `VectorIndexingOptions`, `TtlPolicy`, `RetrievedContext`, `Citation`, `Scores`.
- Registered facade services in `src/main.rs` with compression, auth interceptor, health reporter, and reflection descriptor support.
- Implemented initial Rust facade adapters in `src/grpc/mod.rs`:
  - `IndexLogicalDocument` delegates to existing reliable v004 indexing pipeline.
  - `RetrieveContext` delegates to existing Search and maps results to ai_bro-friendly contexts.
  - `ExplainRetrieve` delegates to ExplainSearch.
  - Admin facade delegates to DebugDocumentState and RetryVectorOutbox.

## Preserved

- Existing `AstraVectorRuntime` service remains available.
- Existing `AstraVectorV004Control` service remains available.
- Outbox, sync status, activation gate, Qdrant reconciliation, scroll pagination, TTL, diagnostics and adaptive runtime remain intact.

## Known limitations

- Compile/test must be confirmed locally because this environment does not provide `cargo`/`rustc`.
- Full persistent `LogicalBlock → Chunk` mapping is foundation-level metadata in this candidate.
- Auto activation for `AUTO_WHEN_READY` is not yet a complete asynchronous lifecycle worker behavior.
- Absolute TTL is contractual foundation; relative TTL is mapped to `ttl_days`.
