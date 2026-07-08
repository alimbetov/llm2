# AstraVector v006-safe-adaptive-runtime

## Added

- Safe adaptive runtime module: `src/adaptive/mod.rs`.
- Modes: `OFF`, `DRY_RUN`, `AUTO_SAFE`.
- Guardrails: min/max/step/cooldown/TTL.
- Runtime in-memory override store with structured audit logs.
- Adaptive metrics for decisions, dry-run, applied overrides, rejected decisions, expired overrides.
- Dynamic `qdrant.scroll_page_size` usage in Qdrant scroll pagination.
- Dynamic `publisher.batch_size` and `outbox.poll_interval_ms` usage in Qdrant publisher/outbox worker.
- Adaptive config section in `config/application.yaml`.
- Migration `0019_v006_adaptive_runtime_overrides.sql` for persistent audit/override storage foundation.
- Unit tests for DRY_RUN, AUTO_SAFE and guardrail behavior.

## Safety

The adaptive runtime is intentionally scoped to performance parameters only. It does not tune model versions, tokenizer versions, chunking profile, RRF, dense/sparse weights, access-control rules or activation semantics.

## Verification required

This artifact was statically audited in the sandbox. The environment does not provide `cargo`, `rustc`, `protoc`, PostgreSQL, Qdrant or ONNX Runtime. Run locally:

```bash
cargo fmt
cargo check --all-features
cargo test --all-features
```
