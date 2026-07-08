# AstraVector v005-hardening-statistical-fix2

This archive applies the P0 Qdrant scroll pagination hardening on top of fix1.

Main changes:

- `QdrantClient::point_ids_by_document` now delegates to a paginated implementation.
- `point_ids_by_document_paginated` follows `next_page_offset` until completion.
- Scroll has page-size, max-pages, max-points, timeout and concurrency limits.
- Repeated offset is rejected as `QDRANT_SCROLL_LOOP`.
- Timeout/limit/Qdrant errors are returned as errors; partial results are not accepted as success.
- `DebugDocumentState` exposes scroll status/pages/points via `DebugQdrantInfoV005`.
- Qdrant scroll metrics are emitted through the existing metrics crate.

Required local validation:

```bash
cargo fmt
cargo check --all-features
cargo test --all-features
```

Then run full smoke with PostgreSQL + Qdrant + ONNX + publisher.
