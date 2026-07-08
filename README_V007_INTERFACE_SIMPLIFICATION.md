# AstraVector v007-interface-simplification

## Цель

`v007-interface-simplification` добавляет публичные facade-контракты поверх текущего надёжного ядра AstraVector. Внутренние механизмы не удаляются: outbox, sync, activation gate, Qdrant reconciliation, TTL, diagnostics и safe adaptive runtime остаются внутри.

Основная граница ответственности:

```text
llm_indexator:
  file → text → LogicalBlock[] + source links + metadata

AstraVector:
  LogicalBlock[] → tokenizer-aware chunks → dense/sparse vectors → PostgreSQL/Qdrant → retrieval context

ai_bro:
  question → RetrieveContext → prompt → LLM answer
```

## Новые gRPC facade service

### AstraVectorIngestionFacade

Для `llm_indexator`:

```proto
service AstraVectorIngestionFacade {
  rpc IndexLogicalDocument(IndexLogicalDocumentRequest) returns (IndexLogicalDocumentResponse);
  rpc GetDocumentVectorStatus(GetDocumentVectorStatusRequest) returns (GetDocumentVectorStatusResponse);
  rpc DeleteDocumentVectorsFacade(DeleteDocumentVectorsFacadeRequest) returns (DeleteDocumentVectorsFacadeResponse);
}
```

Основной метод — `IndexLogicalDocument`. Он принимает `LogicalBlock[]`, а не готовые embedding chunks. AstraVector сам выполняет tokenizer-aware chunking на базе своего tokenizer/model contract.

### AstraVectorRetrievalFacade

Для `ai_bro`:

```proto
service AstraVectorRetrievalFacade {
  rpc RetrieveContext(RetrieveContextRequest) returns (RetrieveContextResponse);
  rpc ExplainRetrieve(ExplainRetrieveRequest) returns (ExplainRetrieveResponse);
}
```

`RetrieveContext` скрывает technical knobs и возвращает:

```text
matched_text
parent_text
citation
scores
source_links
metadata
```

### AstraVectorAdminFacade

Для операционной диагностики:

```proto
service AstraVectorAdminFacade {
  rpc DebugDocument(DebugDocumentRequest) returns (DebugDocumentResponse);
  rpc RetryOutbox(RetryOutboxFacadeRequest) returns (RetryOutboxFacadeResponse);
  rpc GetRuntimeHealth(GetRuntimeHealthRequest) returns (GetRuntimeHealthResponse);
}
```

## Важные ограничения текущего v007 candidate

1. `IndexLogicalDocument` реализован как facade над текущим v004 pipeline: LogicalBlock[] валидируются, сортируются и превращаются в tokenizer-aware source text для текущего `CreateMultiGranularityChunks`.
2. Полная трассировка `LogicalBlock → Chunk` пока хранится как JSON metadata foundation. Для production-grade block-to-chunk trace нужна отдельная таблица или расширение `content_chunks_v004`.
3. `ActivationPolicy.AUTO_WHEN_READY` в текущем candidate возвращает состояние `PUBLISHING`; фактическая автоактивация после завершения publisher должна быть реализована lifecycle worker-ом отдельным шагом.
4. `TtlPolicy.RELATIVE` мапится в `ttl_days`; `TtlPolicy.ABSOLUTE` принят в контракте, но требует отдельной persistence-доработки для точного `expires_at`.
5. Signed URL лучше генерировать во внешнем document-link-resolver или ai_bro. AstraVector возвращает stored source links/descriptors.

## Проверка после распаковки

```bash
cargo fmt --check
cargo check --all-targets --all-features
cargo test --all-targets --all-features
```

В этой среде Rust toolchain недоступен, поэтому компиляцию нужно подтвердить локально.

---

## v007 fix1 consistency hardening

See `docs/V007_FIX1_INTERFACE_CONSISTENCY.md` and `CHANGELOG_V007_INTERFACE_SIMPLIFICATION_FIX1.md`.

Main release-gate fixes:

- no default `INTERNAL` for `RetrieveContext`;
- strict `LogicalBlock[]` validation;
- unsupported `TTL_MODE_ABSOLUTE` and `AUTO_WHEN_READY` are explicitly rejected;
- delete facade returns `DELETE_SCHEDULED`;
- runtime health performs live dependency checks;
- migration foundation for `LogicalBlock -> Chunk` trace.

## v007 fix3 GraphRAG Lite

`fix3_graph_lite` extends the explainable retrieval path with a bounded PostgreSQL structural graph:

```text
Qdrant seed chunks
  → 1-hop graph expansion
  → TTL/access/freshness filtering
  → score merge / dedup
  → RetrievedContext[]
```

The graph is intentionally limited: no Neo4j, no LLM extraction, no all-to-all chunk web, and no multi-hop traversal. PostgreSQL remains the source of truth for chunks, trace, graph rows, and freshness; Qdrant remains the vector index.
