# FIX486F Contract-First Implementation Checkpoint

## Status

```text
APPROVED_FOR_PHASE_OWNED_RUNNER_IMPLEMENTATION
```

This is a pre-runner implementation checkpoint. It is not an official Phase F
runtime verdict and does not claim runtime proof PASS.

## Identity

```text
branch: codex/fix486f-stale-orphan-hydration-proof
tested source SHA: dc817ca2201e2871f724ecfd9c7756b407cc861f
implementation commit: dc817ca2201e2871f724ecfd9c7756b407cc861f
```

## Frozen bank

```text
version: 1.0.0
status: FROZEN
aggregate SHA-256: cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff
payload mutations: 0
```

## Implementation result

| Capability | Result |
|---|---:|
| `FIX486F-P0-001` canonical binding validation | `FIXED` |
| Binding-backed hydration | `PASS` |
| One SQL batch / `WITH ORDINALITY` / no N+1 | `PASS` |
| Exhaustive terminal outcomes | `PASS` |
| Partial degradation semantics | `PASS` |
| Total timeout transport failure | `PASS` |
| Candidate rejection reserve | `PASS` |
| Deduplication after successful hydration | `PASS` |
| Request-scoped bounded failpoint plan | `PASS` |
| Empty/whitespace parent guard | `PASS` |
| Bounded metric labels and protected traces | `PASS` |
| Search/RetrieveContext semantic parity | `PASS` |

## Validation evidence

| Command | Result |
|---|---:|
| `cargo fmt --all --check` | `PASS` |
| `cargo check --locked --all-targets --all-features` | `PASS` |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | `PASS` |
| hydration and failpoint unit tests | `7/7 PASS` |
| `fix486f_failure_semantics_contracts` | `11/11 PASS` |
| PostgreSQL Testcontainers hydration regression | `1/1 PASS` |
| Phase A contracts | `3/3 PASS` |
| Phase C contracts | `4/4 PASS` |
| Phase D contracts | `5/5 PASS` |
| Phase E contracts | `9/9 PASS` |

The PostgreSQL regression proves that a valid binding hydrates its canonical
parent and that substituting another parent while retaining the matched chunk and
binding is classified as `BINDING_INVALID`.

## Runtime boundary

```text
official Phase F runner: NOT IMPLEMENTED IN THIS CHECKPOINT
official Phase F runtime proof: NOT STARTED
official evidence run ID: NONE
official verdict: NONE
```

The next permitted activity is implementation and review of the phase-owned
runner, Compose/config, audit SQL and fail-closed evidence tooling. The official
runtime proof may start only after that runner checkpoint is approved.
