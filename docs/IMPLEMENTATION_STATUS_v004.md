# AstraVector v004 implementation status

## Implemented in source
- v004 protobuf contract with `access_zone_id`, chunk hierarchy, document version, group lifecycle, relevance, feedback and quarantine APIs.
- PostgreSQL 15 partitioned v004 schema: 32 HASH partitions for document versions, content chunks and vector bindings.
- Explicit enum/database mapping.
- Multi-granularity chunking engine and deterministic UUIDv5 identities.
- Atomic REQUIRED persistence repository method for vector, binding and outbox.
- Wait-or-takeover repository method.
- Outbox schema recovery indexes and idempotency key.
- Qdrant point existence and collection dimension validation.
- Reconciliation building block.
- Real dense cosine and sparse dot-product primitives.
- Rule-based enrichment validator.
- Shutdown coordinator.

## Partially integrated
- Existing v003 gRPC implementation still requires full adaptation to every new protobuf method and request field.
- v004 partitioned tables are created alongside legacy v003 tables; production cutover requires the migration runbook and access-zone mapping.
- Reconciliation worker initializes but scheduled full/access-zone scans are not yet wired.
- Enrichment worker uses the disabled provider by default.
- Cross-encoder and NLI remain interfaces/planned P2 work.

## Verification limits
Compilation and live integration must be executed in an environment with Rust toolchain, PostgreSQL 15 + pgvector, Qdrant and the real ONNX/tokenizer artifacts.
