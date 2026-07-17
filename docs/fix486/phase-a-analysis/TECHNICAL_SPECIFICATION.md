# v011/fix486a — Hierarchical Retrieval Analysis and Evidence Readiness

## 1. Status

```text
DOCUMENT_TYPE=ANALYSIS_AND_EVIDENCE_READINESS_SPECIFICATION
BASE_BRANCH=epic/fix486-hierarchical-retrieval-validation
WORK_BRANCH=codex/fix486a-analysis-readiness
BASE_COMMIT=cfa01b2d615582ac736f1ef844d8fc79280e3ff1
LOCAL_PROJECT=/Users/ruslanalimbetov/Documents/llm2/astravector
EVIDENCE_ROOT=/Users/ruslanalimbetov/Documents/llm2/astravector-evidence
PRODUCTION_STATUS=BLOCKED
```

Codex must verify the actual branch and SHA before work. The recorded SHA is a branch creation anchor, not a substitute for runtime identity capture.

## 2. Purpose

This phase prepares objective proof for AstraVector hierarchical retrieval. It must determine:

- what is implemented on the production path;
- what is covered by unit, contract, integration, Testcontainers, model-backed and load tests;
- what remains unproven;
- which fixtures, failpoints, metrics and evidence artifacts are required;
- which defects or testability gaps must be fixed in later phases.

This phase does not declare hierarchical retrieval correct and does not declare production readiness.

## 3. Architecture under analysis

```text
SOURCE
└── PARENT
    ├── SUB_180
    └── SUB_260
```

Production path to map:

```text
logical ingestion
→ deterministic multi-granularity chunks
→ PostgreSQL canonical hierarchy
→ Qdrant dense/sparse projection
→ query planning
→ Dense/Sparse/FTS retrieval
→ fusion
→ child candidate identity
→ parent hydration
→ parent grouping/deduplication
→ no-answer/coverage
→ GraphRAG
→ MMR
→ token budget
→ final visibility recheck
→ final coverage and citations
```

## 4. Allowed status values

Every analyzed capability must receive exactly one status:

```text
IMPLEMENTED_AND_PROVEN
IMPLEMENTED_PARTIALLY_PROVEN
IMPLEMENTED_NOT_PROVEN
DECLARED_NOT_IMPLEMENTED
BLOCKED_BY_OBSERVABILITY
BLOCKED_BY_ENVIRONMENT
```

Statements such as “should work”, “probably works” or “appears implemented” are prohibited in the final report.

## 5. Scope

In scope:

- structured ingestion and logical blocks;
- SOURCE/PARENT/SUB_180/SUB_260 identity and linkage;
- deterministic IDs and trace inheritance;
- PostgreSQL parent hydration and grouping;
- Qdrant payload identity;
- access-zone/access-level/version/TTL/deletion filtering;
- GraphRAG child-to-parent resolution;
- MMR and token budget;
- candidate-to-intent evidence and final coverage;
- diagnostics, ranking trace and metrics;
- MacBook performance methodology;
- immutable evidence identity.

Out of scope unless a P0/P1 defect blocks analysis:

- model replacement;
- vector dimension changes;
- reindexing production data;
- ranking weight tuning;
- threshold weakening;
- public protobuf breaking changes;
- fixture-specific production logic;
- automatic merge;
- production-ready claims.

## 6. Required analysis phases

### Phase A0 — Baseline inventory

Capture source SHA, Cargo.lock SHA, dirty-worktree status, Rust/Cargo, macOS/hardware, Docker/PostgreSQL/Qdrant identities, model/tokenizer/config hashes and all existing tests/smokes/quality profiles/failpoints.

Classify existing failures as:

```text
PRODUCT_DEFECT
TEST_DEFECT
ENVIRONMENT_BLOCKER
MISSING_MODEL
MISSING_SERVICE
FLAKY_TEST
STALE_EXPECTATION
```

### Phase A1 — Hierarchy and fixture feasibility

Map:

- actual chunk identity inputs;
- parent/child persistence keys;
- Qdrant point identity;
- logical-block mapping;
- feasibility of same logical UUIDs in different access zones;
- lifecycle states required by the bank;
- deterministic Graph relation construction.

### Phase A2 — Child/parent production path

Determine:

- indexed granularities;
- child candidate identity;
- parent hydration SQL;
- batching versus N+1;
- parent group key;
- winning child policy;
- exact child evidence preservation;
- parity between Search, RetrieveContext and Explain.

### Phase A3 — Isolation and lifecycle

Build a stage matrix for Dense, Sparse, FTS, hydration, Graph, MMR, token budget, final visibility and Explain. For each stage identify zone, access-level, version, lifecycle and TTL filters.

### Phase A4 — Failure/degradation

Map parent hydration timeout/unavailable/missing/partial/cancelled behavior. Define failpoints and verify that infrastructure failure cannot become false no-answer.

### Phase A5 — Graph parent resolution

Map:

```text
direct child
→ graph seed
→ edge
→ related child
→ related child visibility
→ related parent hydration
→ graph/direct dedup
→ MMR
→ citation
```

### Phase A6 — MMR, token budget and multi-intent

Determine the exact order of:

```text
candidate-intent evidence
→ parent grouping
→ Graph merge
→ MMR
→ token budget
→ visibility recheck
→ final coverage
```

Identify whether coverage is recomputed after every destructive stage and whether unique required-intent contexts are protected.

### Phase A7 — Mac methodology

Define correctness, medium and stress corpora; warmup; concurrency; arrival model; p50/p95/p99; CPU/RAM/swap; PostgreSQL/Qdrant latency; hydration SQL count; Graph/MMR/token-budget timings; three-run repeatability.

### Phase A8 — Evidence contract and implementation backlog

Produce the proof matrix, observability gap matrix, failure injection matrix and phase-by-phase implementation backlog.

## 7. Critical cases to prepare

The analysis must produce an executable proof design for:

1. exact child returns correct parent;
2. two children of one parent produce one final context;
3. same logical UUIDs in different zones never mix;
4. inactive parent version never returns;
5. deleted/orphan parent never forms a context;
6. parent hydration timeout produces explicit degradation;
7. exact child evidence survives parent grouping;
8. Graph-expanded child resolves its own parent;
9. token budget preserves the only context for a required intent;
10. a large parent cannot starve several unique aspects.

## 8. Mandatory deliverables

```text
docs/fix486/phase-a-analysis/ANALYSIS_REPORT.md
docs/fix486/phase-a-analysis/ARCHITECTURE_MAP.md
docs/fix486/phase-a-analysis/PROOF_MATRIX.md
docs/fix486/phase-a-analysis/OBSERVABILITY_GAPS.md
docs/fix486/phase-a-analysis/FAILURE_INJECTION_PLAN.md
docs/fix486/phase-a-analysis/MAC_PERFORMANCE_METHODOLOGY.md
docs/fix486/phase-a-analysis/IMPLEMENTATION_BACKLOG.md
```

Machine-readable output outside tracked source where appropriate:

```text
target/fix486a-analysis/repository-inventory.json
target/fix486a-analysis/existing-test-inventory.json
target/fix486a-analysis/hierarchy-code-map.json
target/fix486a-analysis/proof-matrix.json
target/fix486a-analysis/observability-gap-matrix.json
target/fix486a-analysis/failure-injection-matrix.json
target/fix486a-analysis/analysis-verdict.json
```

## 9. Defect handling

This analysis phase should not silently fix production behavior. If a P0/P1 defect prevents truthful analysis, Codex may fix it only after:

1. capturing the failing behavior;
2. recording the root cause;
3. adding a regression test;
4. preserving the bank and qrels unchanged;
5. committing the fix separately;
6. rerunning the same proof input;
7. reporting before/after evidence.

## 10. Definition of Ready

The next phase may begin only when:

- all ten cases have a proof-matrix row;
- production call paths are mapped;
- parent hydration query and batching are identified;
- parent group key and winner policy are identified;
- all security/lifecycle filters are mapped;
- cross-zone fixtures are feasible;
- failpoint designs exist;
- Graph child-to-parent path is mapped;
- MMR/token-budget/final-coverage order is known;
- Mac methodology is reproducible;
- bank and evidence schemas are versioned;
- implementation backlog is concrete.

## 11. Verdict

Allowed verdicts:

```text
FIX486_ANALYSIS_READY
FIX486_ANALYSIS_BLOCKED
```

`FIX486_ANALYSIS_READY` means only that the project is ready for evidence-producing hierarchical tests. It does not mean the search path has passed those tests.