# FIX486G Graph Parent Runtime Proof Result

## Identity

| Field | Value |
|---|---|
| Repository | `alimbetov/llm2` |
| Branch | `<branch>` |
| Tested source SHA | `<sha>` |
| Remote SHA | `<sha>` |
| Base Phase F SHA | `c5fa4cb41cf9cd57ddf914562723bbe9758110cd` |
| Runtime binary SHA-256 | `<sha256>` |
| Resolved config SHA-256 | `<sha256>` |
| Model SHA-256 | `<sha256>` |
| Tokenizer SHA-256 | `<sha256>` |
| Run ID | `<run-id>` |

## Bank identities

| Bank | Version/status | Aggregate SHA-256 |
|---|---|---|
| Mandatory FIX486 | `1.0.0 / FROZEN` | `cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff` |
| FIX486G supplemental | `<version/status>` | `<sha256>` |
| Holdout | `<not-used/identity>` | `<sha256-or-na>` |

## Mandatory gates

| Gate | Result | Evidence |
|---|---:|---|
| fmt | `<PASS/BLOCKED>` | `<artifact>` |
| locked check | `<PASS/BLOCKED>` | `<artifact>` |
| locked clippy | `<PASS/BLOCKED>` | `<artifact>` |
| locked all-target tests | `<PASS/BLOCKED>` | `<artifact>` |
| SQLx prepare | `<PASS/BLOCKED>` | `<artifact>` |
| prior FIX486 contracts | `<PASS/BLOCKED>` | `<artifact>` |
| Phase G contracts | `<PASS/BLOCKED>` | `<artifact>` |
| frozen bank verification | `<PASS/BLOCKED>` | `<artifact>` |
| supplemental bank verification | `<PASS/BLOCKED>` | `<artifact>` |

## Canonical identity chain

```text
seed zone: <zone>
seed document/version: <document>/<version>
seed child: <logical>/<physical>
seed parent: parent-a1/<physical>
relation: <id> / REPAIRED_BY / <score>
related child: <logical>/<physical>
related parent: parent-a3/<physical>
hop index: 1
origin: GRAPH
```

## Runtime proof matrix

| Scenario | Search | RetrieveContext | Result |
|---|---:|---:|---:|
| Graph disabled | `<status>` | `<status>` | `<PASS/BLOCKED>` |
| Frozen healthy Graph | `<status>` | `<status>` | `<PASS/BLOCKED>` |
| Wrong parent | `<status>` | `<status>` | `<PASS/BLOCKED>` |
| Cross-zone | `<status>` | `<status>` | `<PASS/BLOCKED>` |
| Lifecycle invalid | `<status>` | `<status>` | `<PASS/BLOCKED>` |
| Binding invalid | `<status>` | `<status>` | `<PASS/BLOCKED>` |
| Candidate non-interference | `<status>` | `<status>` | `<PASS/BLOCKED>` |
| Hop limit | `<status>` | `<status>` | `<PASS/BLOCKED>` |
| Cycle/duplicate edge | `<status>` | `<status>` | `<PASS/BLOCKED>` |
| Warm repeat | `<status>` | `<status>` | `<PASS/BLOCKED>` |
| Restart repeat | `<status>` | `<status>` | `<PASS/BLOCKED>` |

## Safety hard gates

| Counter | Required | Actual |
|---|---:|---:|
| cross-zone Graph final contexts | `0` | `<n>` |
| wrong-parent Graph final contexts | `0` | `<n>` |
| seed-parent reuse final contexts | `0` | `<n>` |
| lifecycle-invalid Graph final contexts | `0` | `<n>` |
| binding-invalid Graph final contexts | `0` | `<n>` |
| hop-limit violation final contexts | `0` | `<n>` |
| cycle credit inflation events | `0` | `<n>` |
| false Graph attribution events | `0` | `<n>` |
| forbidden anchor leaks | `0` | `<n>` |
| Graph provenance missing | `0` | `<n>` |

## Statistical quality

| Metric | Numerator/denominator | Result | Threshold | 95% CI | Verdict |
|---|---:|---:|---:|---:|---:|
| GraphParentRecall@1 | `<n>/<N>` | `<value>` | `>= 0.90` | `<low,high>` | `<PASS/BLOCKED>` |
| GraphParentRecall@3 | `<n>/<N>` | `<value>` | `>= 0.97` | `<low,high>` | `<PASS/BLOCKED>` |
| GraphParentRecall@5 | `<n>/<N>` | `<value>` | `>= 0.99` | `<low,high>` | `<PASS/BLOCKED>` |
| MRR | `<N>` | `<value>` | `>= 0.94` | `<optional>` | `<PASS/BLOCKED>` |
| nDCG@5 | `<N>` | `<value>` | `>= 0.95` | `<optional>` | `<PASS/BLOCKED>` |
| GraphParentAccuracy | `<n>/<N>` | `<value>` | `1.0` | `<low,high>` | `<PASS/BLOCKED>` |
| GraphEdgePrecision | `<n>/<N>` | `<value>` | `1.0` | `<low,high>` | `<PASS/BLOCKED>` |
| GraphProvenanceCompleteness | `<n>/<N>` | `<value>` | `1.0` | `<low,high>` | `<PASS/BLOCKED>` |
| GraphContributionRate | `<n>/<N>` | `<value>` | `>= 0.95` | `<low,high>` | `<PASS/BLOCKED>` |
| DirectPreservationRate | `<n>/<N>` | `<value>` | `1.0` | `<low,high>` | `<PASS/BLOCKED>` |
| NoAnswerSpecificity | `<n>/<N>` | `<value>` | `1.0` | `<low,high>` | `<PASS/BLOCKED>` |
| WarmNormalizedRepeatability | `<n>/<N>` | `<value>` | `1.0` | `<low,high>` | `<PASS/BLOCKED>` |
| RestartNormalizedRepeatability | `<n>/<N>` | `<value>` | `1.0` | `<low,high>` | `<PASS/BLOCKED>` |

## Statistical slices

| Slice | Query count | Recall@5 | MRR | nDCG@5 | Safety failures |
|---|---:|---:|---:|---:|---:|
| RU | `<N>` | `<v>` | `<v>` | `<v>` | `<n>` |
| KZ | `<N>` | `<v>` | `<v>` | `<v>` | `<n>` |
| EN | `<N>` | `<v>` | `<v>` | `<v>` | `<n>` |
| Search | `<N>` | `<v>` | `<v>` | `<v>` | `<n>` |
| RetrieveContext | `<N>` | `<v>` | `<v>` | `<v>` | `<n>` |
| adversarial | `<N>` | `<v>` | `<v>` | `<v>` | `<n>` |
| negative | `<N>` | `n/a` | `n/a` | `n/a` | `<n>` |

## Latency and boundedness

| Metric | Graph disabled | Graph enabled |
|---|---:|---:|
| sample count | `<N>` | `<N>` |
| p50 | `<ms>` | `<ms>` |
| p95 | `<ms>` | `<ms>` |
| p99 | `<ms>` | `<ms>` |
| max | `<ms>` | `<ms>` |
| max candidate count | `<n>` | `<n>` |
| SQL statements per request | `<n>` | `<n>` |
| Graph relation queries per request | `<n>` | `<n>` |

Confirm:

```text
N+1 hydration introduced = false
candidate limit exceeded = false
hop limit exceeded = false
request deadline exceeded beyond allowed jitter = false
```

## Defects

| ID | Severity | Root cause | Red reproducer | Fix SHA | Rerun |
|---|---|---|---|---|---:|
| `<id>` | `<P0/P1/P2>` | `<cause>` | `<test/evidence>` | `<sha>` | `<PASS>` |

Unresolved in-scope:

```text
P0=<n>
P1=<n>
```

## Evidence

```text
external evidence root: <path>
manifest SHA-256: <sha256>
evidence aggregate SHA-256: <sha256>
verified files: <count>
missing mandatory files: <count>
hash mismatches: <count>
cleanup status: <PASS/BLOCKED>
```

## Scope boundary

Confirm:

```text
frozen bank changed = false
qrels derived from runtime = false
Graph weights tuned to fixtures = false
RRF/MMR/token-budget tuned = false
access-zone filters weakened = false
multi-hop Graph certified = false
Mac capacity certified = false
production readiness claimed = false
```

## Verdicts

Runtime verdict:

```text
<FIX486_GRAPH_PARENT_RUNTIME_PROOF_PASS | FIX486_GRAPH_PARENT_RUNTIME_PROOF_BLOCKED>
```

Statistical verdict:

```text
<FIX486G_STATISTICAL_QUALITY_PASS | FIX486G_STATISTICAL_QUALITY_BLOCKED>
```

Overall Phase G verdict:

```text
<FIX486G_PASS | FIX486G_BLOCKED>
```
