# FIX492 Acceptance Criteria

## Scope

This checklist validates only external ingestion contract hardening. It does not validate AstraIndexator implementation.

## A. Hash contract

- [ ] `batch_content_hash` has a versioned public algorithm name.
- [ ] Canonical field inclusion/order is documented.
- [ ] Canonical map ordering is documented.
- [ ] UTF-8/escaping/whitespace/default-field behavior is documented.
- [ ] Source-link ordering behavior is documented.
- [ ] Exact lowercase SHA-256 wire form is documented.
- [ ] `final_content_hash` has a versioned public algorithm name.
- [ ] Final rendered byte sequence is documented completely.
- [ ] CRLF/LF and leading/trailing whitespace behavior are explicit.
- [ ] Hash mismatch behavior is tested.

## B. Golden vectors

- [ ] Minimal batch vector.
- [ ] Russian Unicode vector.
- [ ] Map-order equivalence vectors.
- [ ] Block-order significance vectors.
- [ ] Source-link vector.
- [ ] Default/empty-field vector.
- [ ] Final-document vectors.
- [ ] Manifest contains expected canonical bytes/hash values.
- [ ] Rust contract tests consume the same fixtures.
- [ ] Java reference algorithm reproduces all hashes.

## C. Typed session state

- [ ] Additive enum exists.
- [ ] Existing `string status` fields remain wire-compatible.
- [ ] Start response exposes typed state.
- [ ] Append response exposes typed state.
- [ ] Abort response exposes typed state.
- [ ] GetStatus response exposes typed state.
- [ ] One canonical Rust mapping owns state conversion.
- [ ] Unknown/legacy DB state maps safely to UNSPECIFIED rather than panic.

## D. Typed error contract

- [ ] Typed error enum/message added additively.
- [ ] Legacy `error_code` / `error_message` remain populated.
- [ ] retryable is explicit.
- [ ] `BATCH_HASH_MISMATCH` has stable typed mapping.
- [ ] `FINAL_CONTENT_HASH_MISMATCH` has stable typed mapping.
- [ ] session expired/not-found/state errors have stable mapping.
- [ ] staging corruption has stable mapping.
- [ ] transient dependency failure is distinguishable from validation failure.

## E. Retry/idempotency

- [ ] same session + same batch index + same hash replay is accepted.
- [ ] same session + same batch index + different hash is rejected.
- [ ] finalize mismatch remains client-correctable.
- [ ] ambiguous finalize can be reconciled through GetStatus.
- [ ] docs contain retry/no-retry matrix for Java/AstraIndexator.

## F. Compatibility

- [ ] no existing protobuf field removed.
- [ ] no existing protobuf field number reused.
- [ ] no RPC renamed.
- [ ] existing clients using legacy status/error fields still work.
- [ ] persisted session rows from before FIX492 remain readable.

## G. Regression gates

- [ ] cargo fmt --all --check
- [ ] cargo check --locked --all-targets --all-features
- [ ] cargo clippy --locked --all-targets --all-features -- -D warnings
- [ ] cargo test --locked --all-targets --all-features
- [ ] access-zone contract tests unchanged/pass
- [ ] TTL lifecycle tests unchanged/pass
- [ ] retrieval tests unchanged/pass

## Required result

Create after implementation:

```text
docs/fix492/RESULT.md
```

with one exact verdict:

```text
FIX492_EXTERNAL_INGESTION_CONTRACT_PASS
FIX492_EXTERNAL_INGESTION_CONTRACT_FAIL
FIX492_EXTERNAL_INGESTION_CONTRACT_BLOCKED
```
