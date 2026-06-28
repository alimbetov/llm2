# AstraVector_v003 — Implementation Status

## Реализовано в архиве

- protobuf v003: document metadata, per-item access level, TTL, representation type;
- lifecycle gRPC methods;
- migrations `0008` и `0009`;
- `vector_bindings` и `vector_outbox`;
- deterministic Qdrant point ID;
- REST Qdrant client for upsert/payload/delete;
- outbox publisher with `SKIP LOCKED`, retry и dead-letter;
- TTL expiration, soft-delete и delayed PostgreSQL purge;
- separate publisher and lifecycle binaries;
- L1 durable flag for REQUIRED protection;
- enrichment provider interface and validation/deduplication helper;
- relevance score API and baseline explainable scoring;
- Docker Compose Qdrant baseline;
- Kubernetes publisher/lifecycle manifests.

## Частично реализовано

- strict REQUIRED: L1 bypass исправлен; полная атомарность request/item/binding/outbox требует compile/integration validation;
- Search Enrichment: доменный интерфейс и storage schema реализованы, внешний/local LLM provider не подключён;
- Relevance Engine: baseline scoring реализован, cross-encoder/NLI provider не подключён;
- reconciliation: schema и statuses готовы, полноценный Qdrant scroll/rebuild worker остаётся следующим этапом;
- production Qdrant collection bootstrap и payload indexes требуют integration validation.

## Не подтверждено в текущей среде

В среде отсутствуют Rust toolchain, Docker, PostgreSQL, Qdrant и реальный ONNX artifact. Поэтому не выполнены `cargo check/test/clippy/build`, migrations against live PostgreSQL, Qdrant integration и ONNX parity.
