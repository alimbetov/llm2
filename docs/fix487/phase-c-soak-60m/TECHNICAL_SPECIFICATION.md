# FIX487C 60-Minute Soak Technical Specification

## Purpose

FIX487C runs a 60-minute mixed-load soak at a safe concurrency derived from the FIX487B/C capacity curve.

## Soak Selection

```text
soak_concurrency = floor(maximum_stable_concurrency * 0.75)
```

Soak is blocked when no capacity level is `STABLE`.

## Sequence

```text
fresh phase-owned environment
runtime warmup = 5 minutes
load warmup = 10 minutes
measurement = 60 minutes
cooldown max = 15 minutes
```

Soak uses:

```text
dataset = fix487b-dataset-v1
workload = fix487b-mixed-profile-v1
seed = 487460
```
