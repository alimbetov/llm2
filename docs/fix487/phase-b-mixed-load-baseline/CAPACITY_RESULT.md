# FIX487B/C Capacity Result

## Status

Implementation checkpoint only.

```text
CONCURRENCY_5_PILOT_WAIVED_BY_PRODUCT_OWNER
waiver_reason = PROCEED_DIRECTLY_TO_CAPACITY_CAMPAIGN
```

Historical status preserved:

```text
FIX487B_CONCURRENCY_5_PILOT_BLOCKED
reason = EXPLICIT_PILOT_OPT_IN_REQUIRED
```

## Capacity Table

| Concurrency | Verdict | Ops | Success % | p95 | p99 | Peak RSS | Cooldown |
| --- | --- | ---: | ---: | ---: | ---: | ---: | --- |
| 25 | NOT_RUN | 0 | 0 | n/a | n/a | n/a | n/a |
| 50 | NOT_RUN | 0 | 0 | n/a | n/a | n/a | n/a |
| 100 | NOT_RUN | 0 | 0 | n/a | n/a | n/a | n/a |
| 200 | NOT_RUN | 0 | 0 | n/a | n/a | n/a | n/a |

## Curve

```text
maximum_stable_concurrency = NOT_MEASURED
first_controlled_saturation_concurrency = NOT_MEASURED
recommended_operating_concurrency = NOT_MEASURED
```

## Static Gates

```text
make verify-fix487a-retrieval-freeze                         PASS
make verify-fix487b-contracts                                PASS
make verify-fix487bc-capacity-contracts                      PASS
make verify-fix487c-soak-contracts                           PASS
python3 unit capacity tests                                  PASS, 9/9
python3 unit soak tests                                      PASS, 6/6
bash -n scripts/fix487bc-capacity-campaign.sh                PASS
bash -n scripts/fix487c-soak-60m.sh                          PASS
cargo fmt --all --check                                      PASS
cargo check --locked --all-targets --all-features            PASS
cargo clippy --locked --all-targets --all-features -- -D warnings PASS
cargo test --locked --all-targets --all-features             PASS
cargo sqlx prepare --check -- --all-targets --all-features   BLOCKED, DATABASE_URL not set
```

## Heavy Runner Gate

Without explicit opt-in:

```text
make verify-fix487bc-capacity-campaign
FIX487BC_BLOCKED=EXPLICIT_CAPACITY_OPT_IN_REQUIRED
```

## Verdict

```text
FIX487BC_CAPACITY_CAMPAIGN_BLOCKED
reason = EXPLICIT_CAPACITY_OPT_IN_REQUIRED
```
