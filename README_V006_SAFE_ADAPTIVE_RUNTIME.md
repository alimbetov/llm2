# AstraVector v006-safe-adaptive-runtime

This build adds a narrow safe adaptive runtime layer. It does not modify model versions, tokenizer versions, chunking profiles, access-control semantics, activation rules, dense/sparse weights, or RRF behavior.

Implemented safe adaptive scope:

- modes: `OFF`, `DRY_RUN`, `AUTO_SAFE`;
- adaptive in-memory runtime overrides with TTL;
- guardrails: min/max/step/cooldown/ttl;
- audit logs through structured tracing;
- metrics for adaptive decisions/rejections/dry-run/applied/expired overrides;
- dynamic usage for `qdrant.scroll_page_size` during Qdrant scroll pagination;
- dynamic usage for `publisher.batch_size` and `outbox.poll_interval_ms` in the outbox worker.

Default mode is `DRY_RUN`. Production should keep `DRY_RUN` until decisions are reviewed for at least one observation window.

Forbidden by design:

- `model_version`;
- `tokenizer_version`;
- `chunking_profile`;
- `dense_weight` / `sparse_weight`;
- `rrf_k`;
- access/security semantics;
- activation gate rules;
- Qdrant vector schema.

Required local verification:

```bash
cargo fmt
cargo check --all-features
cargo test --all-features
```
