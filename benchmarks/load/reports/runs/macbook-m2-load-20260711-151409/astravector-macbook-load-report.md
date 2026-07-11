# AstraVector MacBook M2 Load Report

## Executive verdict

`FAIL`: RECOVERY_SLO_FAILED.

## Capacity

- Stable: `6 RPS`
- Saturation: `None`
- Failure: `8 RPS`

## Soak

The 60-minute machine interval completed with `100.000%` success, p95 `309.6 ms`, and RSS slope `-34.49 MiB/hour`.

## Recovery

Recovery success was `97.664%` with p95 `1716.4 ms`; verdict `FAIL`.

## Limitations

All components and the load generator ran on the same MacBook. The result is a single-host local capacity benchmark and is not equivalent to Kubernetes or production-server capacity.
