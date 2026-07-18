# FIX486C Frozen Executable Bank Result

## Identity

| Field | Value |
|---|---|
| Source branch | `<branch>` |
| Final source SHA | `<sha>` |
| Approved base SHA | `8fd29b2acd166992f953a5020be81b076e581403` |
| Bank ID | `fix486-hierarchical-bank` |
| Bank version | `1.0.0` |
| Bank status | `FROZEN` |
| Aggregate SHA-256 | `<sha256>` |
| Evidence run ID | `<run-id>` |
| Evidence manifest SHA-256 | `<sha256>` |

## Frozen payload

| File | SHA-256 |
|---|---|
| corpus | `<sha256>` |
| queries | `<sha256>` |
| qrels | `<sha256>` |
| graph | `<sha256>` |
| lifecycle | `<sha256>` |

## Structural result

| Assertion | Result |
|---|---|
| Cases | `10` |
| Queries | `11` |
| Qrels | `11` |
| Orphan qrels | `0` |
| Unresolved parents | `0` |
| Unresolved children | `0` |
| Unknown Graph endpoints | `0` |
| Hash mismatches | `0` |
| Extra bank files | `0` |

## Executability result

| Stage | Result |
|---|---|
| Verify-only | `<PASS/BLOCKED>` |
| Query dry-run | `<PASS/BLOCKED>` |
| Queries scheduled | `<n>/11` |
| Production-path ingestion | `<PASS/BLOCKED/NOT_RUN>` |
| Runtime identity-map export | `<PASS/BLOCKED/NOT_RUN>` |
| Evidence completeness | `<PASS/BLOCKED>` |

## Mandatory gates

| Gate | Result | Exit code |
|---|---:|---:|
| fmt | `<status>` | `<code>` |
| locked check | `<status>` | `<code>` |
| locked all-target tests | `<status>` | `<code>` |
| locked clippy | `<status>` | `<code>` |
| SQLx prepare | `<status>` | `<code>` |
| existing bank contracts | `<status>` | `<code>` |
| frozen-bank contracts | `<status>` | `<code>` |
| Python verifier | `<status>` | `<code>` |
| aggregate Makefile gate | `<status>` | `<code>` |

## Defects

| Severity | Open | Fixed and rerun |
|---|---:|---:|
| P0 | `<n>` | `<n>` |
| P1 | `<n>` | `<n>` |

## Scope exclusions

This result does not certify child/parent correctness, isolation/lifecycle, failure/degradation, Graph parent quality, MMR/token-budget quality, Mac load, production candidate or production readiness.

## Evidence

External evidence: `<absolute-path>`

Manifest pointer: `<repository-path>`

## Verdict

`<FIX486_FROZEN_EXECUTABLE_BANK_PASS | FIX486_FROZEN_EXECUTABLE_BANK_BLOCKED>`
