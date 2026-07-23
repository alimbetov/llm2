# FIX486G Supplemental Graph Evaluation Bank

## Status

```text
bank_id: fix486g-graph-parent-supplemental
version: 1.0.0
status: FROZEN
```

This bank supplements, but never modifies, the mandatory frozen FIX486 bank:

```text
benchmarks/hierarchical/fix486/
version: 1.0.0
status: FROZEN
aggregate SHA-256: cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff
```

## Purpose

The mandatory frozen case `FIX486-08 / q-graph-repair` proves one canonical Graph parent chain. This supplemental bank estimates whether the behavior generalizes across:

- Russian, Kazakh and English wording;
- lexical and semantic paraphrases;
- short and multi-clause queries;
- Graph-required and Graph-disabled requests;
- no-answer and distractor requests;
- wrong-parent, cross-zone, lifecycle, hop-limit and cycle attacks.

## Composition

```text
positive Graph-required queries: 30
negative/constrained-negative queries: 15
Graph-disabled controls: 6
adversarial fault scenarios: 20
total: 71
```

The bank references the frozen corpus and graph baseline. Adversarial scenarios use phase-owned fault overlays generated only inside the FIX486G environment. They do not mutate frozen payloads.

## Files

```text
bank-manifest.json
queries/graph-parent-queries-v1.jsonl
qrels/qrel-profiles-v1.json
qrels/query-qrel-assignments-v1.jsonl
faults/graph-fault-plans-v1.json
```

## Lifecycle

The reviewed candidate completed structural validation and is frozen at
`1.0.0`. Every query has one materialized qrel assignment. Canonical hashes are
stored in `bank-manifest.json` and verified by `scripts/fix486g_proof.py`.

Any change after freeze requires a new bank version. Failed official results must not be repaired by changing qrels in place.

## Evaluation model

Each accepted Graph result is graded by canonical identity:

```text
3 = required parent-a3 with complete Graph provenance
2 = relevant reconciliation parent with incomplete/non-required provenance
1 = supporting direct parent-a1 evidence
0 = irrelevant
-1 = forbidden identity, zone, lifecycle or attribution
```

Text similarity alone cannot convert a wrong canonical parent into a relevant result.

## Statistical contract

See:

```text
docs/fix486/phase-g-graph-parent-runtime/STATISTICAL_EVALUATION_CONTRACT.md
```

The official report must include raw per-query results, aggregate and sliced metrics, confidence intervals, safety hard gates and latency distributions.
