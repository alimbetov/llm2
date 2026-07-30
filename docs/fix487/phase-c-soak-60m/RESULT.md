# FIX487C 60-Minute Soak Result

## Status

No official soak has been run.

```text
FIX487C_SOAK_60M_BLOCKED
reason = EXPLICIT_SOAK_OPT_IN_REQUIRED
```

## Static Gates

```text
make verify-fix487c-soak-contracts                           PASS
python3 -m unittest -v tests/test_fix487c_soak.py             PASS, 6/6
bash -n scripts/fix487c-soak-60m.sh                          PASS
cargo fmt --all --check                                      PASS
cargo check --locked --all-targets --all-features            PASS
cargo clippy --locked --all-targets --all-features -- -D warnings PASS
cargo test --locked --all-targets --all-features             PASS
```

Without explicit opt-in:

```text
make verify-fix487c-soak-60m
FIX487C_BLOCKED=EXPLICIT_SOAK_OPT_IN_REQUIRED
```

Soak requires a valid capacity result and:

```bash
ASTRAVECTOR_FIX487C_EXECUTE_SOAK=true
FIX487BC_CAPACITY_EVIDENCE_DIR=<capacity evidence directory>
```

## Verdict

```text
FIX487C_SOAK_60M_BLOCKED
```
