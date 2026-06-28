# AstraVector_v002 implementation status

## Implemented
- Real ONNX Runtime session adapter with dense output / CLS pooling and optional lexical sparse output.
- Provider candidate fallback with mandatory startup self-test.
- SHA-256 validation for model and tokenizer.
- Claim-before-compute, lease, atomic takeover, fencing token.
- PostgreSQL polling for PROCESSING entries with deadline.
- Request/item audit and idempotency replay/conflict handling.
- REQUIRED persistence only after commit; L1 update after successful commit.
- Concurrent submission of all RPC batch items.
- Deadline-aware bounded scheduler, query priority, real token-count hints and length buckets.
- L2 token metadata, recovery, cache retention, API-key interceptor, dynamic readiness and graceful shutdown.
- Prometheus instrumentation baseline.
- Kubernetes manifests and migration job.

## Verification limitation
The build environment used to generate this archive does not contain `cargo`/`rustc`. Therefore `cargo fmt`, `cargo check`, `cargo test`, `cargo clippy` and execution against a real BGE-M3 dense+sparse ONNX artifact were not run here. These remain mandatory quality gates before deployment. The ONNX adapter intentionally fails startup if configured output names/shapes are absent.

## Model artifact requirement
A production sparse capability is exposed only when the loaded ONNX graph includes the configured `lexical_weights` output. Dense-only exports remain usable for dense requests but reject learned sparse requests.
