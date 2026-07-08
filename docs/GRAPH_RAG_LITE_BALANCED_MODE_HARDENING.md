# AstraVector / GraphRAG Lite Hardening Patch — Balanced Mode

## Summary

This patch closes the remaining GraphRAG Lite fix2 hardening gaps with the Balanced Mode implementation:

- P0: graph rebuild timeout now covers cleanup, structural build, semantic build, validation and persistence.
- P1: graph merge strategies are implemented, `final_context_limit` is strict by default, response-level retrieval debug is extended, and merge benchmarks are real.
- P2: graph-specific typed errors are introduced, structured tracing is added, relation-distribution metrics are emitted, and optional Rayon semantic build is wired through `semantic_parallel_enabled`.

## Key behavior

### Full graph rebuild timeout

`SAVEPOINT graph_build` wraps cleanup and the full rebuild flow. If timeout happens, runtime rolls back to the savepoint and preserves the old graph.

### Merge strategies

Supported values:

- `SCORE_THEN_TRUNCATE`
- `DIRECT_FIRST`
- `GRAPH_AS_CONTEXT_APPEND`

### Strict final context limit

Default mode is:

```yaml
graph_rag:
  retrieval:
    final_context_limit_mode: STRICT
```

`AT_LEAST_TOP_K` remains available for backward-compatible behavior.

### Optional Rayon

```yaml
graph_rag:
  build:
    semantic_parallel_enabled: false
    semantic_parallelism: 0
```

When enabled, semantic edge candidate generation uses Rayon `par_iter`.

## Validation checklist

Run locally with Rust toolchain:

```bash
cargo fmt --check
cargo check --all-targets --all-features
cargo test --all-targets --all-features
cargo bench
```
