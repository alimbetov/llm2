# FIX491 Qdrant Recovery Result

Verdict: `QDRANT_PROJECTION_RECOVERY_PASS`

Implemented:

- Shared canonical projection builder used by outbox and reconciliation.
- Qdrant recovery fence helpers.
- Outbox and reconciliation projection writes use shared advisory fencing.
- Qdrant complete payload scroll API for audit.
- Qdrant rebuild from PostgreSQL active/synced/searchable bindings using persisted vectors only.
- Explicit destructive rebuild opt-in via `--replace-existing`.
- Qdrant expected-vs-actual audit for missing, orphan and payload mismatch counters.

Focused static evidence:

```text
cargo check --locked --all-targets --all-features: PASS
cargo clippy --locked --all-targets --all-features -- -D warnings: PASS
cargo test --locked --test fix491_projection_contracts -- --nocapture: PASS
```

Runtime local-demo Qdrant proof:

```text
pre-rebuild qdrant-audit:
expected_eligible_bindings = 19776
actual_points = 25722
missing_points = 7050
orphan_points = 12996
payload_mismatches = 0
scan_completed = true
verdict = QDRANT_PROJECTION_DRIFT

qdrant-rebuild --replace-existing:
expected_eligible_bindings = 19776
batches_scanned = 40
points_upserted = 19776
failed_points = 0
used_inference_fallback = false
verdict = QDRANT_REBUILD_COMPLETED

post-rebuild qdrant-audit:
expected_eligible_bindings = 19776
actual_points = 19776
missing_points = 0
orphan_points = 0
payload_mismatches = 0
pages_scanned = 20
points_scanned = 19776
scan_completed = true
verdict = QDRANT_PROJECTION_CONSISTENT
```

Discovered and fixed during proof:

```text
QDRANT_RECOVERY_FENCE_IDLE_TRANSACTION_TIMEOUT
```

The initial rebuild held a transaction-level advisory lock across the full rebuild and PostgreSQL terminated the connection due to `idle-in-transaction timeout`. The fix uses a session-level advisory lock on a dedicated connection with explicit release.
