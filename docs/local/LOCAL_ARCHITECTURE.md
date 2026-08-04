# Local Architecture

```text
grpcurl / local-demo helper
        |
        v
AstraVector Rust runtime
        |
        +--> PostgreSQL canonical state
        |
        +--> transactional outbox
                  |
                  v
                Qdrant
```

PostgreSQL is the canonical source of truth for document versions, chunks, embeddings, bindings, lifecycle state and outbox events. Qdrant is a replaceable vector projection used by retrieval. The ONNX BGE-M3 model produces query and document embeddings. Rust/Tonic exposes the gRPC runtime and facade APIs.

FIX488 uses only local-demo configuration and tooling. It does not change retrieval ranking, Graph admission, MMR, quality fixtures, qrels or frozen evidence.

