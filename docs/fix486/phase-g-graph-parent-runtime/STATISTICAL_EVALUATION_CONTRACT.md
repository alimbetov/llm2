# FIX486G Statistical Evaluation Contract

## 1. Purpose

A runtime proof establishes that mandatory invariants hold for controlled scenarios. A statistical evaluation estimates how reliably the retrieval engine generalizes across query wording, language, distractors, ranking positions and Graph fault classes.

Phase G therefore requires both:

```text
hard safety gates
+
retrieval quality statistics
```

A point estimate of `100%` on a small set is not sufficient by itself. Every reported metric must include numerator, denominator, sample definition and, where meaningful, a 95% confidence interval.

## 2. Evaluation datasets

### 2.1 Frozen mandatory bank

```text
benchmarks/hierarchical/fix486/
version: 1.0.0
status: FROZEN
```

This bank remains the normative acceptance gate and must not be modified.

### 2.2 FIX486G supplemental Graph bank

```text
benchmarks/hierarchical/fix486g-supplemental/
```

The supplemental bank expands query wording and adversarial Graph scenarios. It is versioned and hashed independently from the frozen bank.

Development stages:

```text
0.1.0-analysis-seed
0.2.0-reviewed-candidate
1.0.0-FROZEN
```

The bank must be frozen before production Graph behavior is tuned against its official results.

### 2.3 Aggregate holdout bank

A separate holdout bank is required for FIX486J. It must not be used to tune Graph weights, ranking, MMR, token budgets or thresholds.

The holdout bank may be generated and sealed after Phase G/H implementation freeze. Its qrels must be established independently of runtime output.

## 3. Query families

The supplemental Graph bank must cover at least these families:

1. direct-plus-Graph repair questions;
2. Graph-only reconciliation evidence;
3. paraphrases with no exact fixture wording;
4. exact technical identifier plus Graph relation;
5. Russian queries;
6. Kazakh queries;
7. English queries;
8. short underspecified queries;
9. long multi-clause queries where only one clause requires Graph evidence;
10. strong lexical distractors;
11. semantically close but wrong-parent distractors;
12. Graph-disabled controls;
13. cross-zone edge traps;
14. inactive/deleted/expired target traps;
15. binding-invalid related endpoint traps;
16. duplicate edge controls;
17. one-hop limit controls;
18. cycle controls;
19. candidate displacement controls;
20. semantic no-answer controls.

## 4. Relevance model

Qrels must distinguish relevance grades:

```text
3 = exact required Graph parent and complete provenance
2 = relevant parent but incomplete/non-required provenance
1 = partially relevant supporting context
0 = irrelevant
-1 = forbidden result
```

For FIX486-08, the normative Graph target is `parent-a3`. The direct `parent-a1` may be relevant direct evidence but cannot satisfy the Graph-parent qrel.

A result with the correct text but wrong canonical parent identity is forbidden, not partially correct.

## 5. Primary quality metrics

### 5.1 Parent Recall@K

For positive Graph-required queries:

```text
GraphParentRecall@K = queries with required Graph parent in top K / positive Graph queries
```

Report at:

```text
K = 1, 3, 5
```

### 5.2 Mean Reciprocal Rank

```text
MRR = mean(1 / rank of first exact required Graph parent)
```

A direct seed parent does not count as the required Graph parent.

### 5.3 nDCG@K

Use graded qrels and report:

```text
nDCG@3
nDCG@5
```

Forbidden results must receive zero gain and trigger the applicable hard gate.

### 5.4 Graph parent accuracy

```text
GraphParentAccuracy = accepted Graph contexts with canonical expected parent / all accepted Graph contexts
```

This is an identity metric, not text-similarity accuracy.

### 5.5 Edge precision

```text
GraphEdgePrecision = accepted Graph traversals satisfying canonical edge and endpoint policy / all accepted Graph traversals
```

### 5.6 Provenance completeness

```text
GraphProvenanceCompleteness = accepted Graph contexts with complete mandatory provenance / all accepted Graph contexts
```

### 5.7 Graph contribution rate

For queries whose qrel requires Graph evidence:

```text
GraphContributionRate = queries where Graph adds the required parent beyond direct-only results / Graph-required queries
```

This prevents a PASS where Graph is enabled but contributes nothing.

### 5.8 Direct preservation rate

```text
DirectPreservationRate = degraded Graph queries retaining all valid direct evidence / degraded Graph queries with valid direct evidence
```

### 5.9 No-answer specificity

```text
NoAnswerSpecificity = negative queries returning no forbidden context / negative queries
```

Infrastructure failures are excluded from semantic no-answer and evaluated separately.

### 5.10 Repeatability

```text
NormalizedRepeatability = repeated queries with identical normalized parent/provenance set / repeated queries
```

Report warm and restart repeatability separately.

## 6. Safety metrics and hard gates

These are not averaged into a quality score. Any non-zero final-context violation blocks PASS:

```text
cross_zone_graph_final_contexts = 0
wrong_parent_graph_final_contexts = 0
seed_parent_reuse_final_contexts = 0
inactive_deleted_expired_graph_final_contexts = 0
binding_invalid_graph_final_contexts = 0
hop_limit_violation_final_contexts = 0
cycle_credit_inflation_events = 0
false_graph_attribution_events = 0
forbidden_anchor_leaks = 0
```

The report may count rejected attacks separately. Rejected attacks are evidence of protection, not safety failures.

## 7. Degradation truthfulness metrics

Report:

```text
GraphFailureDetectionRecall
GraphFailureClassificationAccuracy
FalseSemanticNoAnswerRate
FalseFullCoverageRate
HealthyRequestContaminationRate
```

Definitions:

```text
GraphFailureDetectionRecall = injected Graph failures explicitly detected / injected Graph failures
GraphFailureClassificationAccuracy = correctly classified Graph failures / detected Graph failures
FalseSemanticNoAnswerRate = infrastructure failures labeled semantic no-answer / infrastructure failures
FalseFullCoverageRate = partial Graph evidence reported as full / partial Graph scenarios
HealthyRequestContaminationRate = concurrent healthy requests affected by request-scoped Graph faults / concurrent healthy controls
```

All four error rates must equal zero for official Phase G scenarios.

## 8. Latency and resource metrics

Report separately for Graph-disabled and Graph-enabled requests:

```text
p50 latency
p95 latency
p99 latency
mean latency
max latency
Graph expansion duration
canonical Graph hydration duration
candidate count before/after validation
SQL statement count
Qdrant request count
Graph relation query count
```

Phase G does not certify production capacity. Latency is used to detect pathological regression and unbounded fan-out.

Required boundedness gates:

```text
no N+1 SQL hydration
candidate count <= configured maximum
hop count <= configured maximum
request duration <= request deadline + fixed jitter allowance
```

Mac load SLOs remain Phase I.

## 9. Statistical uncertainty

For proportions, report a Wilson 95% confidence interval.

For zero observed safety failures, also report the one-sided upper confidence bound. The report must not present `0/N` as proof that the true failure probability is zero.

For latency, report sample count and percentile calculation method. Bootstrap intervals may be added but are not mandatory for Phase G.

## 10. Minimum sample sizes

Phase G supplemental official evaluation requires at least:

```text
positive Graph-required queries: 30
negative/no-answer queries: 15
security/lifecycle adversarial executions: 20
warm repeats: 3 per mandatory query
restart repeats: 2 per mandatory query
concurrent fault/healthy pairs: 10
```

These are minimums, not targets. FIX486J should use a larger independent holdout set.

## 11. Required slices

Every primary metric must be reported overall and sliced by:

```text
language: RU / KZ / EN
query family
profile
entry point: Search / RetrieveContext
Graph enabled/disabled
relation type
fault class
warm/restart run
```

A high aggregate score cannot hide a failed critical slice.

## 12. Thresholds for Phase G

Mandatory frozen query:

```text
GraphParentRecall@5 = 1.0
Graph parent identity correct = true
Graph provenance complete = true
all safety hard gates = 0
```

Supplemental bank release thresholds:

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
all degradation truthfulness error rates = 0
```

A metric below threshold produces BLOCKED even if the single frozen mandatory query passes.

## 13. Anti-overfitting controls

The implementation must not:

- branch on query IDs;
- branch on fixture anchors;
- special-case `parent-a3`;
- special-case `REPAIRED_BY` solely for the bank;
- derive qrels from retrieved output;
- modify bank data after observing official results without version increment and renewed review;
- tune on the FIX486J holdout bank.

A source scan and focused contract must verify absence of fixture-ID logic in production retrieval code.

## 14. Machine-readable report

Required output:

```text
statistical-report.json
statistical-report.md
per-query-results.jsonl
per-slice-metrics.json
latency-distribution.json
safety-hard-gates.json
confidence-intervals.json
```

`statistical-report.json` must include:

```text
source identities
bank identities
run identities
metric definitions and versions
raw numerators and denominators
point estimates
confidence intervals
thresholds
per-slice outcomes
hard-gate outcomes
excluded/blocked rows with reasons
final statistical verdict
```

## 15. Statistical verdict

Allowed values:

```text
FIX486G_STATISTICAL_QUALITY_PASS
FIX486G_STATISTICAL_QUALITY_BLOCKED
```

The overall Phase G PASS requires both:

```text
FIX486_GRAPH_PARENT_RUNTIME_PROOF_PASS
FIX486G_STATISTICAL_QUALITY_PASS
```
