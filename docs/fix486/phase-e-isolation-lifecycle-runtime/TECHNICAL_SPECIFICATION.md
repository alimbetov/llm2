# FIX486E Technical Specification

## 1. Purpose

`fix486e` proves that AstraVector enforces access-zone isolation and canonical document lifecycle semantics through the real ingestion and retrieval paths.

The phase must demonstrate two properties simultaneously:

1. content from one access zone cannot influence candidates, hydration, graph expansion, final contexts, telemetry, or evidence produced for another zone;
2. only the canonical searchable document version can produce a final context, even when inactive, deleted, or expired versions contain stronger exact identifiers.

The allowed final verdicts are:

```text
FIX486_ISOLATION_LIFECYCLE_RUNTIME_PROOF_PASS
FIX486_ISOLATION_LIFECYCLE_RUNTIME_PROOF_BLOCKED
```

## 2. Lineage

The phase starts from merged `main`:

```text
377852cc6d7ff315b8d7eb27762672d794fd7a9c
```

Required predecessors:

```text
fix486a  DONE
fix486b  DONE
fix486c  DONE / FROZEN BANK 1.0.0
fix486d  DONE / CHILD-PARENT RUNTIME PROOF PASS
```

The Phase E branch is:

```text
codex/fix486e-isolation-lifecycle-runtime-proof
```

## 3. Frozen-bank identity

The proof consumes the existing immutable bank:

```text
bank_id: fix486-hierarchical-bank
bank_version: 1.0.0
status: FROZEN
aggregate_sha256: cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff
```

The following files are immutable:

```text
benchmarks/hierarchical/fix486/corpus/hierarchical-fixture-v1.json
benchmarks/hierarchical/fix486/queries/hierarchical-queries-v1.jsonl
benchmarks/hierarchical/fix486/qrels/hierarchical-qrels-v1.jsonl
benchmarks/hierarchical/fix486/graph-relations/hierarchical-graph-v1.json
benchmarks/hierarchical/fix486/lifecycle/hierarchical-lifecycle-v1.json
```

Do not rewrite inner fixture status fields. The manifest and its hashes define the frozen contract.

## 4. In-scope frozen cases

### 4.1 FIX486-03 — access-zone isolation

Mandatory frozen queries:

```text
q-zone-a
q-zone-b
```

`q-zone-a` executes in `zone-a` and must return the Zone A canonical parent containing:

```text
ASTRA_CANONICAL_STATE_A1
```

It must not expose:

```text
ZONE_B_SECRET_PARENT_A1
ZONE_B_PRIVATE_SOURCE
```

`q-zone-b` executes in `zone-b` and must return the Zone B parent containing:

```text
ZONE_B_SECRET_PARENT_A1
```

It must not expose:

```text
ASTRA_CANONICAL_STATE_A1
ASTRA_LEGAL_HOLD_A2
```

Both zones intentionally reuse logical identifiers such as:

```text
doc-hierarchy
parent-a1
child-a1-180
```

Therefore identity must be composite and zone-scoped.

### 4.2 FIX486-04 — canonical active-version filtering

Mandatory frozen query:

```text
q-active-version
```

Expected result:

```text
zone: zone-a
document: doc-hierarchy
version: 1
parent: parent-a1
status: FOUND
```

Forbidden versions:

```text
2  INDEXING
3  DELETED
4  EXPIRED
```

Forbidden anchors:

```text
ASTRA_INACTIVE_VERSION_TRAP
ASTRA_DELETED_PARENT_TRAP
ASTRA_EXPIRED_PARENT_TRAP
```

The inactive v2 text deliberately has stronger lexical attraction. Ranking strength must never bypass lifecycle visibility.

## 5. Supplemental Phase E controls

Supplemental controls strengthen the frozen proof but do not modify frozen queries or qrels.

### 5.1 Opposite-zone negative replay

Execute the frozen question text under the opposite access zone as runner-owned negative controls:

```text
q-zone-a question under zone-b
q-zone-b question under zone-a
```

Run each control through both:

```text
Search
RetrieveContext
```

Allowed outcomes:

```text
INSUFFICIENT
NO_ANSWER
explicit empty result
zone-local unrelated result without forbidden anchors
```

Forbidden outcome:

```text
any context, provenance, warning payload, debug trace, or hydrated text from the requested foreign zone
```

These controls must be clearly marked `supplemental=true` and must not be written back into the frozen bank.

### 5.2 Lifecycle anchor probes

The runner may issue supplemental exact probes for the three trap anchors:

```text
ASTRA_INACTIVE_VERSION_TRAP
ASTRA_DELETED_PARENT_TRAP
ASTRA_EXPIRED_PARENT_TRAP
```

Each probe must produce zero final contexts from versions 2, 3, or 4.

The proof must distinguish:

```text
candidate observed and rejected
candidate never generated
```

Both can be valid, but the final visibility decision and reason must be evidenced.

## 6. Out of scope

The following cases belong to later phases:

```text
FIX486-05 stale/orphan Qdrant child proof
FIX486-06 parent hydration timeout and degradation
FIX486-08 graph relation correctness
FIX486-09 multi-intent token-budget coverage
FIX486-10 large-parent pressure and anti-starvation
```

Phase E may observe graph expansion because the frozen zone queries enable it, but only the isolation boundary is evaluated. Graph relation correctness is not a Phase E acceptance criterion.

No stale-point fault injection is required for deleted v3 in this phase. That belongs to `fix486f`.

## 7. Runtime identity model

Logical zone mapping must be captured from runtime setup:

```text
zone-a -> runtime zone code 4862
zone-b -> runtime zone code 4863
```

The mapping must be recorded in evidence and verified against actual canonical rows. It must not be inferred only from query order.

A canonical object identity is at least:

```text
zone
+ document
+ document version
+ chunk role
+ logical chunk identity
```

A runtime object identity additionally includes physical IDs:

```text
runtime document ID
runtime version ID
runtime chunk ID
runtime parent chunk ID
runtime vector binding ID
Qdrant point UUID
```

The same logical identifier in two zones must resolve to different physical objects and different zone-scoped bindings.

## 8. Isolation enforcement layers

Phase E must prove isolation at every relevant layer.

### 8.1 Request boundary

The request carries an explicit authenticated or test-controlled access-zone identity.

Missing or invalid zone identity must fail closed.

### 8.2 Candidate generation

Dense, sparse, lexical, hybrid, and graph-assisted candidate generation must apply zone filtering.

No foreign-zone candidate may be promoted to a final context.

### 8.3 Qdrant projection

Every point used by the proof must carry zone-scoped payload sufficient for filtering and audit.

Qdrant point UUID equality across zones must not be assumed.

### 8.4 Canonical PostgreSQL hydration

PostgreSQL hydration must validate:

```text
zone
document
version
chunk visibility
parent visibility
```

A foreign-zone physical ID must not hydrate under the current request zone.

### 8.5 Graph expansion

If graph expansion executes, every node and edge traversal must remain zone-scoped.

Graph isolation is a hard gate even though graph relevance correctness is deferred.

### 8.6 Final context assembly

Final contexts must contain only zone-local parent text and zone-local matched-child provenance.

### 8.7 Telemetry and evidence

Debug traces, warnings, candidate dumps, normalizer output, and evidence artifacts must not leak foreign-zone text or identifiers beyond explicitly redacted aggregate counts.

## 9. Lifecycle model

Phase E uses the frozen lifecycle scenarios:

```text
active-v1
inactive-v2-higher-score
deleted-v3-stale-qdrant-child
expired-v4
legal-hold-active
```

For Phase E normal execution:

```text
v1 ACTIVE    searchable
v2 INDEXING  not searchable
v3 DELETED   not searchable
v4 EXPIRED   not searchable
```

The `deleted-v3-stale-qdrant-child` scenario is used only for canonical state and final-result exclusion in Phase E. Deliberate stale Qdrant injection is deferred to Phase F.

## 10. Legal-hold audit

Legal hold belongs to lifecycle state and must be audited without broadening the retrieval cases.

For active v1:

```text
legal_hold = true
cleanup protection = effective
retrieval policy = follows canonical ACTIVE state
```

The proof must show:

1. legal hold does not make inactive, deleted, or expired versions searchable;
2. legal hold protects the canonical active representation from destructive cleanup;
3. legal hold state survives warm repeat and runtime restart;
4. retrieval still follows active-version filtering.

No legal-hold release workflow is required in Phase E.

## 11. Execution matrix

### 11.1 Mandatory frozen requests

| Query | Zone | Search | RetrieveContext |
|---|---|---:|---:|
| q-zone-a | zone-a | required | required |
| q-zone-b | zone-b | required | required |
| q-active-version | zone-a | required | required |

Mandatory primary result count:

```text
3 queries x 2 entry points = 6
```

### 11.2 Supplemental negative isolation requests

| Probe | Executed zone | Search | RetrieveContext |
|---|---|---:|---:|
| q-zone-a question | zone-b | required | required |
| q-zone-b question | zone-a | required | required |

Supplemental isolation result count:

```text
2 probes x 2 entry points = 4
```

### 11.3 Supplemental lifecycle probes

At minimum one exact probe per forbidden trap anchor. The runner may choose Search, RetrieveContext, or both, but must record the chosen matrix before execution.

## 12. Repeatability campaign

### 12.1 Warm repeat

Repeat all six mandatory frozen requests without re-ingestion.

Prove:

```text
same logical zone/document/version
same allowed parent
same forbidden counts = 0
no additional versions/chunks/bindings/outbox effects
```

### 12.2 Runtime restart

Restart only AstraVector, preserving PostgreSQL and Qdrant state.

Repeat all six mandatory requests and the four opposite-zone controls.

Prove that isolation and lifecycle semantics survive process restart.

### 12.3 Optional second clean run

A second clean-environment run is recommended if a production fix is introduced during Phase E. It is mandatory when the fix changes zone filtering, lifecycle predicates, Qdrant payload filters, or hydration authorization.

## 13. Hard gates

All values must be zero:

```text
cross_zone_candidates_promoted
cross_zone_hydrations
cross_zone_final_contexts
cross_zone_graph_results
cross_zone_evidence_leaks
wrong_version_results
inactive_version_results
deleted_version_results
expired_version_results
legal_hold_visibility_bypasses
```

Additional required conditions:

```text
zone-a positive result present
zone-b positive result present
active v1 positive result present
mandatory primary results = 6/6
opposite-zone controls = 4/4 executed
Search/RetrieveContext parity = PASS
warm repeat = PASS
restart repeat = PASS
```

## 14. Production-path rule

Documents and lifecycle transitions must be created through supported production APIs and production persistence paths.

Direct SQL is allowed only for:

```text
read-only audit
phase-owned deterministic test clock setup when no supported API exists
explicitly documented lifecycle fixture preparation that cannot be expressed through production APIs
```

Any direct mutation must be isolated, recorded, and must not bypass the retrieval authorization logic being tested.

## 15. Evidence rule

External evidence must be stored outside Git. Git may contain only:

```text
runner code
contracts
compact result summary
manifest hashes
evidence index
```

Every official run must bind evidence to:

```text
source commit SHA
binary SHA-256
configuration SHA-256
frozen aggregate SHA-256
model and tokenizer hashes
container image identities
runtime zone mapping
```

## 16. Defect policy

When a failure is discovered:

1. preserve the full `BLOCKED` evidence;
2. classify runner, evidence, fixture interpretation, or production defect;
3. do not change frozen payload or qrels;
4. implement the smallest root-cause fix;
5. add a regression contract;
6. commit the fix;
7. obtain a clean worktree;
8. repeat all static gates;
9. repeat the full official runtime proof on the new SHA.

## 17. Exit criteria

Phase E is complete only when the official result is:

```text
FIX486_ISOLATION_LIFECYCLE_RUNTIME_PROOF_PASS
```

Phase E completion does not imply whole-project production readiness. Phases `fix486f` through `fix486j` remain required.