# AstraVector v005-hardening-statistical-fix2
## Qdrant scroll pagination for exact reconciliation

## Purpose

This fix closes the P0 pagination risk found in `AstraVector_v005_hardening_statistical_fix1`: Qdrant point-id reconciliation must read every page from Qdrant `scroll`, not only the first page.

The affected production decisions are:

- `GetVectorSyncStatus`
- `DebugDocumentState`
- `ActivateDocumentVersion`, indirectly through document sync status

## Implemented changes

### Qdrant client

`QdrantClient::point_ids_by_document(...)` now delegates to:

```rust
point_ids_by_document_paginated(access_zone_id, document_id, document_version)
```

The paginated method:

- follows `next_page_offset` until it is absent/null;
- requests ids only with `with_payload=false` and `with_vector=false`;
- uses configurable `scroll_page_size`;
- enforces `scroll_max_pages`;
- enforces `scroll_max_points`;
- enforces `scroll_timeout_secs`;
- detects repeated offsets and returns `QDRANT_SCROLL_LOOP`;
- uses semaphore-based `scroll_max_concurrency`;
- never returns partial results as success.

### Configuration

Added to `qdrant` config:

```yaml
scroll_page_size: ${ASTRAVECTOR_QDRANT_SCROLL_PAGE_SIZE:-1000}
scroll_max_pages: ${ASTRAVECTOR_QDRANT_SCROLL_MAX_PAGES:-1000}
scroll_max_points: ${ASTRAVECTOR_QDRANT_SCROLL_MAX_POINTS:-1000000}
scroll_timeout_secs: ${ASTRAVECTOR_QDRANT_SCROLL_TIMEOUT_SECS:-30}
scroll_max_concurrency: ${ASTRAVECTOR_QDRANT_SCROLL_MAX_CONCURRENCY:-4}
```

### Debug API

`DebugQdrantInfoV005` now includes:

```proto
string scroll_status = 20;
uint32 scroll_pages_read = 21;
uint32 scroll_points_read = 22;
```

`DebugDocumentState` calls `point_ids_by_document_paginated(...)` directly to expose scroll diagnostics.

### Metrics

Added/used metrics:

- `astravector_qdrant_scroll_requests_total`
- `astravector_qdrant_scroll_pages_total`
- `astravector_qdrant_scroll_points_total`
- `astravector_qdrant_scroll_errors_total{reason}`
- `astravector_qdrant_scroll_latency_seconds`
- `astravector_qdrant_scroll_limit_exceeded_total`
- `astravector_qdrant_scroll_concurrent_inflight`

## Acceptance criteria

- A document with 15,000 Qdrant points is reconciled across all pages.
- A missing point after the first page is detected.
- A repeated `next_page_offset` returns `QDRANT_SCROLL_LOOP`.
- Exceeding `scroll_max_pages` or `scroll_max_points` returns `QDRANT_SCROLL_LIMIT_EXCEEDED`.
- Timeout returns `QDRANT_SCROLL_TIMEOUT`.
- Partial scroll result is never treated as ready/success.
- `count_points_by_document` is not used for activation/status/debug decisions.

## Required local validation

```bash
cargo fmt
cargo check --all-features
cargo test --all-features
```

Then run integration smoke with PostgreSQL + Qdrant + ONNX + publisher.
