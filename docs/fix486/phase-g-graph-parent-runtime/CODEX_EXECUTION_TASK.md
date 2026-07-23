# FIX486G Codex Execution Task

## Mission

Execute Phase G as a contract-first, evidence-driven GraphRAG validation campaign.

The objective is not merely to make `q-graph-repair` return `parent-a3`. The objective is to prove that the production retrieval engine consistently performs this identity chain:

```text
seed child A1
-> frozen Graph edge REPAIRED_BY
-> related child A3
-> canonical A3 binding
-> related child's own parent A3
-> final Graph context with complete provenance
```

## Branch and lineage

```text
branch: codex/fix486g-graph-parent-proof
base branch: codex/fix486f-runtime-proof
base tested SHA: c5fa4cb41cf9cd57ddf914562723bbe9758110cd
prerequisite verdict: FIX486_STALE_ORPHAN_HYDRATION_RUNTIME_PROOF_PASS
```

Do not rebase onto an older main. Preserve the full Phase F implementation and proof lineage.

## Mandatory inputs

Read and follow:

```text
docs/fix486/phase-g-graph-parent-runtime/TECHNICAL_SPECIFICATION.md
docs/fix486/phase-g-graph-parent-runtime/GRAPH_PARENT_PROOF_CONTRACT.md
docs/fix486/phase-g-graph-parent-runtime/EXECUTION_AND_EVIDENCE_CONTRACT.md
docs/fix486/phase-g-graph-parent-runtime/STATISTICAL_EVALUATION_CONTRACT.md
docs/fix486/phase-g-graph-parent-runtime/ACCEPTANCE_CRITERIA.md
benchmarks/hierarchical/fix486/
benchmarks/hierarchical/fix486g-supplemental/
```

## Frozen bank rule

The mandatory frozen bank is immutable:

```text
version: 1.0.0
status: FROZEN
aggregate SHA-256: cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff
```

Do not modify its corpus, queries, qrels, graph relations or lifecycle payloads.

The supplemental bank is not yet an official frozen gate. Before official statistical execution, validate, review, materialize per-query qrels, compute hashes and freeze it under a new immutable version.

## Required sequence

### Step 1 — documentation review

Review all Phase G documents for contradictions, vacuous assertions, unbounded scope and API incompatibility.

Publish:

```text
DOCUMENT_REVIEW.md
```

Allowed verdicts:

```text
APPROVED_FOR_CAPABILITY_AUDIT
CHANGES_REQUIRED
```

No production Graph code changes are allowed while the verdict is `CHANGES_REQUIRED`.

### Step 2 — production capability audit

Map the actual production path from direct seed retrieval through final response.

Publish:

```text
capability-audit.md
```

The audit must identify exact functions, SQL queries, relation storage, endpoint filters, canonical hydration, dedup identities, hop/cycle handling, Graph-disabled behavior, deadlines, metrics and Search/RetrieveContext differences.

Required terminal line:

```text
UNKNOWN_MATERIAL_CAPABILITIES = 0
```

### Step 3 — design review

Based on the audit, publish:

```text
DESIGN_REVIEW.md
```

Allowed verdicts:

```text
APPROVED_FOR_CONTRACT_TEST_IMPLEMENTATION
CHANGES_REQUIRED
```

Do not prescribe new architecture where current production behavior already satisfies the contract.

### Step 4 — red contracts

Add focused contracts before production repairs.

At minimum cover:

1. related child hydrates its own parent;
2. seed parent cannot be reused;
3. canonical binding mismatch is rejected;
4. cross-zone edge cannot traverse;
5. inactive/deleted/expired target cannot return;
6. Graph provenance is complete;
7. Graph-disabled request produces no Graph origin;
8. one-hop maximum is enforced;
9. cycles and duplicate edges cannot inflate credit;
10. rejected Graph candidate cannot displace valid survivor;
11. Search/RetrieveContext normalized parity;
12. no N+1 hydration.

Publish a red-baseline record that distinguishes already-correct behavior from reproduced defects.

### Step 5 — minimal production repair

Repair only reproduced in-scope P0/P1 defects.

Forbidden repair strategies:

- query-ID or fixture-anchor branching;
- hardcoding `parent-a3`;
- hardcoding `REPAIRED_BY` only for the fixture;
- weakening zone/lifecycle checks;
- changing frozen qrels;
- tuning Graph, RRF, MMR or token-budget weights to force PASS;
- adding N+1 SQL;
- introducing public mutable failpoints.

Every repair requires:

```text
defect ID
root cause
red reproducer
fix commit
regression test
rerun evidence
```

### Step 6 — supplemental bank review and freeze

Validate the multilingual/adversarial bank independently of runtime output.

Required checks:

- unique query IDs;
- exact query counts by family/language;
- qrel profile resolution for every query;
- no unresolved qrels;
- no contradictory expected/forbidden parent;
- no runtime-derived qrels;
- fault plan provenance and cleanup;
- source scan for fixture-specific production logic.

Promote only after review:

```text
0.1.0-analysis-seed
-> 0.2.0-reviewed-candidate
-> 1.0.0-FROZEN
```

Record canonical per-file and aggregate SHA-256.

### Step 7 — phase-owned runner

Implement the runner and evidence tooling defined by the execution contract.

Expected files:

```text
scripts/fix486g-graph-parent-runtime-proof.sh
scripts/fix486g_proof.py
scripts/fix486g-audit.sql
docker-compose.fix486g.yml
config/application-fix486g.yaml
tests/fix486g_graph_parent_contracts.rs
```

Canonical target:

```text
make verify-fix486g-graph-parent-runtime
```

### Step 8 — official runtime campaign

Run from a clean tested SHA with isolated PostgreSQL, Qdrant, ports, network and volumes.

Mandatory proof tracks:

- frozen FIX486-08 Search;
- frozen FIX486-08 RetrieveContext;
- Graph-disabled control;
- wrong-parent rejection;
- cross-zone rejection;
- lifecycle-invalid target rejection;
- candidate non-interference;
- one-hop enforcement;
- cycle/duplicate-edge control;
- warm repeat;
- restart repeat;
- metrics/trace consistency;
- cleanup and evidence verification.

### Step 9 — statistical campaign

Execute the frozen supplemental bank and generate:

```text
statistical-report.json
statistical-report.md
per-query-results.jsonl
per-slice-metrics.json
latency-distribution.json
safety-hard-gates.json
confidence-intervals.json
```

Report raw numerators and denominators. Do not report percentages without sample counts.

Mandatory statistical verdict:

```text
FIX486G_STATISTICAL_QUALITY_PASS
FIX486G_STATISTICAL_QUALITY_BLOCKED
```

### Step 10 — repository result package

Publish compact result artifacts:

```text
RESULT.md
MANIFEST_POINTER.json
STAGE_RESULTS_SUMMARY.json
DEFECT_REGISTER.json
NORMALIZED_COMPARISON_SUMMARY.json
STATISTICAL_SUMMARY.json
```

The full evidence bundle remains outside Git and is referenced by immutable hashes.

## Required quality thresholds

Frozen mandatory query:

```text
GraphParentRecall@5 = 1.0
Graph parent identity = parent-a3
complete provenance = true
all safety hard gates = 0
```

Supplemental bank:

```text
GraphParentRecall@1 >= 0.90
GraphParentRecall@3 >= 0.97
GraphParentRecall@5 >= 0.99
MRR >= 0.94
nDCG@5 >= 0.95
GraphParentAccuracy = 1.0
GraphEdgePrecision = 1.0
GraphProvenanceCompleteness = 1.0
GraphContributionRate >= 0.95
DirectPreservationRate = 1.0
NoAnswerSpecificity = 1.0
WarmNormalizedRepeatability = 1.0
RestartNormalizedRepeatability = 1.0
all safety hard gates = 0
all degradation-truthfulness error rates = 0
```

## Defect policy

Severity:

```text
P0: security boundary violation, cross-zone leakage, forbidden lifecycle context, fabricated Graph provenance
P1: wrong canonical parent, lost valid survivor, false full coverage, Graph-disabled false attribution, hop/cycle correctness failure
P2: diagnostics or non-blocking ranking/latency issue without correctness violation
```

Official PASS requires unresolved in-scope P0 = 0 and P1 = 0.

## Final verdict

Overall PASS requires both:

```text
FIX486_GRAPH_PARENT_RUNTIME_PROOF_PASS
FIX486G_STATISTICAL_QUALITY_PASS
```

Otherwise publish:

```text
FIX486_GRAPH_PARENT_RUNTIME_PROOF_BLOCKED
blocking_stage=<stage>
failure_code=<code>
evidence_preserved=true
```
