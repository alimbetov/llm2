# FIX487B Mixed-Load Baseline Result

## Identity

- branch: `agent/fix487b-mixed-load-baseline`
- parent SHA: `ef0454704b1534115b21fa4aae8b1b7cd3d90ad3`
- dataset version: `fix487b-dataset-v1`
- workload profile version: `fix487b-mixed-profile-v1`

## Status

Harness validation passed.

The official model-backed pilot has not been accepted as PASS. The runner is fail-closed and currently blocks unless the operator explicitly sets:

```bash
ASTRAVECTOR_FIX487B_EXECUTE_PILOT=true
```

Without that flag:

```text
FIX487B_BLOCKED=EXPLICIT_PILOT_OPT_IN_REQUIRED
```

SQLx was attempted without a phase-owned `DATABASE_URL` and correctly did not pass:

```text
error: `--database-url` or `DATABASE_URL` must be set
```

## Expected Harness Counters

```text
deterministic_dataset = true
deterministic_schedule = true
bounded_worker_count = 5
unbounded_queue = false
retrieval_freeze = PASS
```

## Static Gates

```text
make verify-fix487b-contracts                                                                  PASS
python3 -m py_compile scripts/fix487b_dataset.py scripts/fix487b_mixed_load.py ...             PASS
python3 -m unittest -v tests/test_fix487b_dataset.py ...                                        PASS, 21/21
bash -n scripts/fix487b-mixed-load-pilot.sh                                                    PASS
make verify-fix487a-retrieval-freeze                                                           PASS
cargo fmt --all --check                                                                        PASS
cargo check --locked --all-targets --all-features                                              PASS
cargo clippy --locked --all-targets --all-features -- -D warnings                              PASS
cargo test --locked --all-targets --all-features                                               PASS
cargo sqlx prepare --check -- --all-targets --all-features                                     BLOCKED, DATABASE_URL not set
make verify-fix487b-mixed-load-pilot                                                           BLOCKED, EXPLICIT_PILOT_OPT_IN_REQUIRED
```

## Freeze Guard

```text
retrieval_freeze_manifest_complete = true
protected_config_changed = 0
protected_fixture_changed = 0
protected_qrel_changed = 0
unapproved_retrieval_symbol_changed = 0
```

## Verdict

```text
FIX487B_MIXED_LOAD_HARNESS_PASS
FIX487B_CONCURRENCY_5_PILOT_BLOCKED
reason=EXPLICIT_PILOT_OPT_IN_REQUIRED
```

The next required step is the live model-backed pilot with a clean worktree, phase-owned PostgreSQL/Qdrant, model/tokenizer paths and `ASTRAVECTOR_FIX487B_EXECUTE_PILOT=true`.
