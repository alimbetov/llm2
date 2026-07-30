# FIX487B/C Capacity Campaign Specification

## Parent

```text
parent_branch = agent/fix487b-mixed-load-baseline
parent_sha = 99fddeb6dba6784823647d574bf5b66f56edd6ad
campaign_branch = agent/fix487bc-capacity-soak
```

## Pilot Waiver

```text
CONCURRENCY_5_PILOT_WAIVED_BY_PRODUCT_OWNER
waiver_reason = PROCEED_DIRECTLY_TO_CAPACITY_CAMPAIGN
```

The waiver preserves the historical pilot status:

```text
FIX487B_CONCURRENCY_5_PILOT_BLOCKED
reason = EXPLICIT_PILOT_OPT_IN_REQUIRED
```

It does not mean the concurrency-5 pilot passed.

## Sequence

```text
preflight
→ concurrency 25
→ validate evidence
→ concurrency 50
→ validate evidence
→ concurrency 100
→ validate evidence
→ concurrency 200
→ validate evidence
→ capacity curve
→ soak concurrency selection
```

Each level uses a fresh phase-owned environment unless a deterministic reset is proven.

## Fixed Workload

Dataset and workload remain:

```text
dataset = fix487b-dataset-v1
workload = fix487b-mixed-profile-v1
```

The mixed workload ratio stays `25/35/10/15/5/5/5`.

## Levels

| Concurrency | Seed | Minimum measurement operations |
| --- | ---: | ---: |
| 25 | 487225 | 500 |
| 50 | 487250 | 1000 |
| 100 | 487300 | 1500 |
| 200 | 487400 | 2000 |

Per level:

```text
runtime warmup = 30 seconds
load warmup = 5 minutes
measurement = 10 minutes
cooldown max = 10 minutes
```
