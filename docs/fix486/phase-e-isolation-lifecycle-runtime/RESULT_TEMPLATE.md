# FIX486E Result Template

## Official status

```text
Phase: fix486e
Verdict: FIX486_ISOLATION_LIFECYCLE_RUNTIME_PROOF_PASS | FIX486_ISOLATION_LIFECYCLE_RUNTIME_PROOF_BLOCKED
```

## Repository identity

```text
Repository: alimbetov/llm2
Branch: codex/fix486e-isolation-lifecycle-runtime-proof
Base SHA: 377852cc6d7ff315b8d7eb27762672d794fd7a9c
Tested source SHA: <sha>
Pushed head SHA: <sha or NOT_PUSHED>
PR: <number or NOT_CREATED>
PR state: draft/open/merged/not-created
```

## Frozen bank identity

```text
Bank ID: fix486-hierarchical-bank
Version: 1.0.0
Status: FROZEN
Aggregate SHA-256: cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff
Frozen payload changed: false
```

## Evidence identity

```text
Run ID: <run-id>
External evidence path: <path>
Evidence file count: <count>
Checksums SHA-256: <sha>
Manifest SHA-256: <sha>
Start UTC: <timestamp>
End UTC: <timestamp>
```

## Runtime identity

```text
Binary SHA-256: <sha>
Resolved config SHA-256: <sha>
Model SHA-256: <sha>
Tokenizer SHA-256: <sha>
Cargo.lock SHA-256: <sha>
Migrations SHA-256: <sha>
PostgreSQL image digest: <digest>
Qdrant image digest: <digest>
```

## Zone mapping

| Logical zone | Runtime zone code | Runtime zone ID | Status |
|---|---:|---|---|
| zone-a | 4862 | `<id>` | PASS/FAIL |
| zone-b | 4863 | `<id>` | PASS/FAIL |

Composite identity collision check:

```text
same logical document across zones -> distinct physical IDs: PASS/FAIL
same logical parent across zones -> distinct physical IDs: PASS/FAIL
same logical child across zones -> distinct physical IDs: PASS/FAIL
cross-zone binding anomalies: <count>
```

## Lifecycle state summary

| Version | Expected state | Actual state | Searchable expected | Final result count | Exclusion path | Status |
|---:|---|---|---|---:|---|---|
| 1 | ACTIVE | `<state>` | yes | `<count>` | ALLOWED | PASS/FAIL |
| 2 | INDEXING | `<state>` | no | `<count>` | `<classification>` | PASS/FAIL |
| 3 | DELETED | `<state>` | no | `<count>` | `<classification>` | PASS/FAIL |
| 4 | EXPIRED | `<state>` | no | `<count>` | `<classification>` | PASS/FAIL |

Recorded test clock:

```text
Clock source: <source>
Clock UTC: <timestamp>
Runtime timezone: <timezone>
Version 4 expires_at: <timestamp>
Expiry comparison: PASS/FAIL
```

## Legal-hold audit

```text
Active v1 legal hold present: PASS/FAIL
Active v1 retrievable: PASS/FAIL
Cleanup protection effective: PASS/FAIL
v2 visibility bypasses: 0
v3 visibility bypasses: 0
v4 visibility bypasses: 0
Warm state preserved: PASS/FAIL
Restart state preserved: PASS/FAIL
```

## Static gates

| Gate | Result |
|---|---|
| Python compile | PASS/FAIL |
| Shell syntax | PASS/FAIL |
| cargo fmt | PASS/FAIL |
| cargo check | PASS/FAIL |
| cargo clippy | PASS/FAIL |
| cargo test all targets/features | PASS/FAIL |
| Phase A contracts | PASS/FAIL |
| Phase C contracts | PASS/FAIL |
| Phase D contracts | PASS/FAIL |
| Phase E contracts | PASS/FAIL |

## Environment and readiness

```text
Clean worktree: PASS/FAIL
Port ownership: PASS/FAIL
Phase-owned Docker: PASS/FAIL
Migrations: PASS/FAIL
Health RPC: PASS/FAIL
Metrics endpoint: PASS/FAIL
Projection completion: PASS/FAIL
```

## Canonical state counts

```text
Documents by zone: <summary>
Versions by zone/state: <summary>
Chunks by zone/version: <summary>
Bindings total: <count>
Bindings SYNCED: <count>
Bindings pending: <count>
Bindings failed: <count>
Completed outbox: <count>
Failed outbox: <count>
Dead letters: <count>
Orphans: <count>
Duplicate chunks: <count>
Duplicate bindings: <count>
```

## Qdrant state

```text
Collection: <name>
Total points: <count>
Zone A points: <count>
Zone B points: <count>
Unexpected zone payloads: <count>
Unexpected inactive/deleted/expired payloads: <count or classification>
Cross-zone payload collisions: <count>
```

## Primary query results

| Query | Zone | Entry point | Expected version | Actual version | Required anchor | Forbidden count | Verdict |
|---|---|---|---:|---:|---|---:|---|
| q-zone-a | zone-a | Search | 1 | `<v>` | ASTRA_CANONICAL_STATE_A1 | `<n>` | PASS/FAIL |
| q-zone-a | zone-a | RetrieveContext | 1 | `<v>` | ASTRA_CANONICAL_STATE_A1 | `<n>` | PASS/FAIL |
| q-zone-b | zone-b | Search | 1 | `<v>` | ZONE_B_SECRET_PARENT_A1 | `<n>` | PASS/FAIL |
| q-zone-b | zone-b | RetrieveContext | 1 | `<v>` | ZONE_B_SECRET_PARENT_A1 | `<n>` | PASS/FAIL |
| q-active-version | zone-a | Search | 1 | `<v>` | parent-a1 | `<n>` | PASS/FAIL |
| q-active-version | zone-a | RetrieveContext | 1 | `<v>` | parent-a1 | `<n>` | PASS/FAIL |

```text
Primary rows: <n>/6
query-results.jsonl non-empty: true/false
```

## Opposite-zone controls

| Control | Executed zone | Entry point | Foreign anchor count | Foreign physical IDs | Result classification | Verdict |
|---|---|---|---:|---:|---|---|
| q-zone-a question | zone-b | Search | `<n>` | `<n>` | `<class>` | PASS/FAIL |
| q-zone-a question | zone-b | RetrieveContext | `<n>` | `<n>` | `<class>` | PASS/FAIL |
| q-zone-b question | zone-a | Search | `<n>` | `<n>` | `<class>` | PASS/FAIL |
| q-zone-b question | zone-a | RetrieveContext | `<n>` | `<n>` | `<class>` | PASS/FAIL |

```text
Opposite-zone rows: <n>/4
```

## Lifecycle trap probes

| Anchor | Expected final count | Actual final count | Exclusion path | Verdict |
|---|---:|---:|---|---|
| ASTRA_INACTIVE_VERSION_TRAP | 0 | `<n>` | `<class>` | PASS/FAIL |
| ASTRA_DELETED_PARENT_TRAP | 0 | `<n>` | `<class>` | PASS/FAIL |
| ASTRA_EXPIRED_PARENT_TRAP | 0 | `<n>` | `<class>` | PASS/FAIL |

## Isolation hard gates

```text
cross_zone_candidates_promoted: <count>
cross_zone_hydrations: <count>
cross_zone_final_contexts: <count>
cross_zone_graph_results: <count>
cross_zone_evidence_leaks: <count>
```

Required value for every counter:

```text
0
```

## Lifecycle hard gates

```text
wrong_version_results: <count>
inactive_version_results: <count>
deleted_version_results: <count>
expired_version_results: <count>
legal_hold_visibility_bypasses: <count>
```

Required value for every counter:

```text
0
```

## Parity and repeatability

```text
Search/RetrieveContext parity: PASS/FAIL
Warm repeat: PASS/FAIL
Restart Health: PASS/FAIL
Restart metrics: PASS/FAIL
Restart mandatory queries: PASS/FAIL
Restart opposite-zone controls: PASS/FAIL
Restart lifecycle audit: PASS/FAIL
Restart legal-hold audit: PASS/FAIL
```

## Evidence leak scan

```text
Zone B anchors in Zone A final/hydrated evidence: <count>
Zone A anchors in Zone B final/hydrated evidence: <count>
Unredacted foreign candidate content: <count>
Wrong-zone physical IDs selected: <count>
Verdict: PASS/FAIL
```

## Cleanup

```text
Runtime stopped: PASS/FAIL
Containers removed: PASS/FAIL
Network removed: PASS/FAIL
Phase volumes removed: PASS/FAIL
Phase database/schema removed: PASS/FAIL
Phase Qdrant collection removed: PASS/FAIL
Ports released: PASS/FAIL
Models preserved: PASS/FAIL
target preserved: PASS/FAIL
Previous evidence preserved: PASS/FAIL
Unrelated resources preserved: PASS/FAIL
Cleanup leaks: <count>
```

## Defects discovered

| Defect ID | Severity | Category | Root cause | Fix commit | Regression | Status |
|---|---|---|---|---|---|---|
| `<id>` | P0/P1/P2 | runner/evidence/setup/production | `<cause>` | `<sha>` | `<test>` | OPEN/FIXED |

## Terminal result

```text
Exit code: <code>
Signal: <signal or NONE>
Primary failure stage: <stage or NONE>
Primary failure code: <code or NONE>
Cleanup status: PASS/FAIL
```

## Final verdict

PASS form:

```text
FIX486E official runtime proof completed

Tested source SHA: <sha>
Evidence run: <run-id>
Manifest SHA-256: <sha>
Primary results: 6/6 PASS
Opposite-zone controls: 4/4 PASS
Search/RetrieveContext parity: PASS
Active-version proof: PASS
Legal-hold audit: PASS
Warm repeat: PASS
Restart proof: PASS
Isolation hard-gate violations: 0
Lifecycle hard-gate violations: 0
Cleanup leaks: 0
Evidence integrity: PASS

Verdict:
FIX486_ISOLATION_LIFECYCLE_RUNTIME_PROOF_PASS
```

BLOCKED form:

```text
FIX486E official runtime proof blocked

Current SHA: <sha>
Evidence run: <run-id>
Last completed stage: <stage>
Failing stage: <stage>
Failure code: <code>
Primary rows: <n>/6
Opposite-zone rows: <n>/4
Isolation hard-gate violations: <summary>
Lifecycle hard-gate violations: <summary>
Evidence preserved: true

Verdict:
FIX486_ISOLATION_LIFECYCLE_RUNTIME_PROOF_BLOCKED
```