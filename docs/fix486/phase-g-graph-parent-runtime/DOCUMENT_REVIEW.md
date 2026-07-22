# FIX486G Document Review

## Review scope

Reviewed documents:

```text
TECHNICAL_SPECIFICATION.md
GRAPH_PARENT_PROOF_CONTRACT.md
EXECUTION_AND_EVIDENCE_CONTRACT.md
STATISTICAL_EVALUATION_CONTRACT.md
ACCEPTANCE_CRITERIA.md
CODEX_EXECUTION_TASK.md
RESULT_TEMPLATE.md
benchmarks/hierarchical/fix486g-supplemental/
```

The review also incorporates the expert runtime-proof assessment supplied after the initial planning PR was opened.

## Executive conclusion

The Phase G contract correctly defines the central GraphRAG invariant:

```text
seed child A1
-> REPAIRED_BY
-> related child A3
-> related child's canonical binding
-> related child's canonical parent A3
```

The direct seed parent remains `parent-a1`. A Graph-derived A3 child that reuses `parent-a1` is a correctness defect even if the returned text appears relevant.

The documents are sufficiently precise to authorize a production capability audit. They are not sufficient to authorize a runtime PASS claim because no phase-owned runner, physical identity map, provenance trace, fault campaign or verified evidence manifest exists yet.

## Findings

### FIX486G-DR-P0-001 — Missing executable runtime proof package

Status: `OPEN`

Required implementation surface:

```text
scripts/fix486g-graph-parent-runtime-proof.sh
scripts/fix486g_proof.py
scripts/fix486g-audit.sql
docker-compose.fix486g.yml
config/application-fix486g.yaml
tests/fix486g_graph_parent_contracts.rs
```

Impact: Phase G cannot claim runtime proof without an executable, repeatable, fail-closed campaign.

### FIX486G-DR-P0-002 — Missing physical identity and provenance evidence

Status: `OPEN`

Required evidence:

```text
identity-map.json
graph-provenance-trace.json
graph-identity-chain.json
```

The evidence must map logical frozen IDs to runtime UUIDs and preserve:

```text
seed zone/document/version
seed child
seed parent
relation identity/type/score
related zone/document/version
related child
related parent
hop index
origin
```

Impact: without the map and trace, `parent-a3` cannot be independently distinguished from a semantically similar or incorrectly reused parent.

### FIX486G-DR-P0-003 — Missing fault and negative-control runtime stages

Status: `OPEN`

Mandatory controls:

```text
graph-disabled
wrong-parent
cross-zone
binding-invalid
inactive target
deleted target
expired target
hop-limit
cycle/duplicate-edge
invalid-candidate non-interference
```

Impact: a happy-path query alone cannot prove authorization, lifecycle or canonical-binding safety.

### FIX486G-DR-P1-001 — Missing Search/RetrieveContext normalized parity artifact

Status: `OPEN`

Required artifact:

```text
NORMALIZED_COMPARISON_SUMMARY.json
```

It must compare the normalized identity chain, not only response ordering or text similarity.

### FIX486G-DR-P1-002 — Missing no-tuning guard

Status: `OPEN`

The official campaign must prove that Graph, RRF, MMR and token-budget weights were not changed to force the frozen answer. Query IDs, fixture anchors, `parent-a3` and `REPAIRED_BY` must not be hardcoded in production logic.

### FIX486G-DR-P1-003 — Missing no-N+1 proof

Status: `OPEN`

The focused contracts and runtime evidence must show bounded batch hydration for related Graph candidates and no per-candidate canonical parent query.

### FIX486G-DR-P1-004 — Supplemental bank not ready for statistical certification

Status: `OPEN`

Current bank status:

```text
0.1.0-analysis-seed
CHANGES_REQUIRED_BEFORE_FREEZE
```

Before official use it must contain the corrected 71-query set, resolved per-query qrels, corrected metadata, canonical hashes and an immutable frozen version.

### FIX486G-DR-P1-005 — Missing repository result package

Status: `OPEN`

Required compact artifacts:

```text
results/RESULT.md
results/MANIFEST_POINTER.json
results/STAGE_RESULTS_SUMMARY.json
results/DEFECT_REGISTER.json
results/NORMALIZED_COMPARISON_SUMMARY.json
results/STATISTICAL_SUMMARY.json
```

Generated full evidence remains outside Git and is referenced by immutable hashes.

## Approved execution order

```text
1. production capability audit
2. design review
3. focused red contracts
4. minimal repair of reproduced in-scope P0/P1 defects
5. supplemental-bank correction and independent freeze
6. phase-owned runner and evidence tooling
7. official runtime campaign
8. statistical campaign
9. compact repository result package
```

Runner design is P0, but production Graph changes must not precede the capability audit and red contracts.

## Current truthful verdict

```text
FIX486_GRAPH_PARENT_RUNTIME_PROOF_BLOCKED
blocking_stage=missing_runtime_evidence_package
evidence_preserved=true
```

This verdict describes missing proof, not a confirmed GraphRAG functional failure.

## Review verdict

```text
APPROVED_FOR_CAPABILITY_AUDIT
```

Production Graph repairs remain prohibited until the capability audit and design review identify a reproduced in-scope defect.