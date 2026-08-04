# FIX488 Technical Specification

FIX488 adds a reproducible local end-to-end book and tooling for real text ingestion and dense search.

Scope:

- documentation;
- local-demo config overlay;
- shell/Python helpers;
- examples;
- Make targets;
- local-demo tests;
- execution/result reports after live validation.

Out of scope:

- production retrieval semantics;
- Graph/MMR/RRF tuning;
- quality fixtures, qrels and frozen evidence.

Canonical profile:

```text
PostgreSQL: 127.0.0.1:55432
Qdrant HTTP: http://127.0.0.1:6333
Qdrant gRPC: 127.0.0.1:6334
AstraVector gRPC: 127.0.0.1:50051
metrics: http://127.0.0.1:9090
collection: astravector_local_demo
```

