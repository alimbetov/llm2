# FIX486A analysis and repair report

## Identity

```text
SOURCE_BRANCH=codex/fix486a-analysis-readiness
BASELINE_SOURCE_SHA=bb6fd6781623cbe0a84f91a204f59da2a32e5c55
RUNTIME_FIX_CANDIDATE_SHA=f160a76c78fc775e633d5d17760219eb8af8f40b
ORIGIN_MAIN_SHA=cfa01b2d615582ac736f1ef844d8fc79280e3ff1
EPIC_SHA=cfa01b2d615582ac736f1ef844d8fc79280e3ff1
CARGO_LOCK_SHA256=624a1767cc1748d81e3b4baeeeb2c734d46ed6bfd553a93ca18e37501edbf38a
CONFIG_SHA256=c8f0263e559547373407711fe00fae7bc32c33185132e5d16582732652808234
MODEL_SHA256=f84251230831afb359ab26d9fd37d5936d4d9bb5d1d5410e66442f630f24435b
TOKENIZER_SHA256=21106b6d7dab2952c1d496fb21d5dc9db75c28ed361a05f5020bbba27810dd08
BANK_ID=fix486-hierarchical-bank
BANK_VERSION=0.1.0-analysis-seed
BANK_FILES_SHA256=cb5b80c25f30f20a2e68a70952be2223905ad9bb2c731ab58603a670d57e4933
BASELINE_EVIDENCE=/Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix486a/fix486a-baseline-20260717T142647Z
```

The seed manifest is deliberately `NOT_FROZEN`; its null embedded hashes are not represented as a
1.0.0 bank. External evidence freezes the actual seed file hashes used by this analysis.

## Baseline

| Gate on baseline SHA | Result | Assertion |
|---|---|---|
| `cargo fmt --all --check` | PASS | exit 0 |
| `cargo check --locked --all-targets --all-features` | PASS | exit 0 |
| `cargo test --locked --all-targets --all-features` | PASS | exit 0; unit, integration and Testcontainers assertions executed |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS | exit 0 |

Exact baseline stdout/stderr, commands, timestamps and exit codes are in the external path above.

## Architecture conclusion

The production path is mapped in `ARCHITECTURE_MAP.md`. The key conclusions are:

- Search is authoritative; RetrieveContext delegates to it.
- Explain is a separate candidate-only path and is not proof of final Search parity.
- Qdrant filters early, while PostgreSQL hydration and final visibility are authoritative.
- Parent grouping and all canonical lookups are zone-scoped and batched.
- Exact child text and broader parent text are separate response evidence.
- Graph expansion uses composite seed identity and one-hop zone-scoped SQL.
- coverage is recomputed after MMR, token budget and final visibility.

## Bank feasibility

The structural contract loads 11/11 queries and 11/11 qrels covering all ten critical cases.
FIX486-03 intentionally has two requests for the two zones. Reused logical parent/child labels are
feasible because production physical IDs include the access zone. Lifecycle and Graph inputs are
structurally present. The bank is feasible for a Phase 1 immutable 1.0.0 freeze after generated
large-parent text, tokenizer counts, physical identity mapping and manifest hashes are resolved.

## Defect register

| ID | Severity/category | Scenario | Root cause | Regression/fix | Status |
|---|---|---|---|---|---|
| FIX486A-P1-001 | P1 wrong parent / lifecycle | FIX486-05 and FIX486-08 | Graph context hydration used `LEFT JOIN` and `COALESCE(p,c)`, substituting a child when its canonical parent was unavailable | production tonic Testcontainers assertion; commit `388d7fd` | RESOLVED |

### FIX486A-P1-001 before/after

```text
EXPECTED=visible graph child with deleted parent produces no hydrated context
BEFORE_SHA=bb6fd6781623cbe0a84f91a204f59da2a32e5c55
BEFORE=FAIL: GraphChunkContextRecord.parent_record.id equalled SUB_180 child id and child content
ASSERTION=stale_child_contexts.is_empty()
ROOT_CAUSE=LEFT JOIN parent plus COALESCE(parent fields, child fields)
FIX=mandatory same-zone/document/version ACTIVE PARENT join in single and multi Graph hydration SQL
FIX_COMMIT=388d7fd
AFTER=PASS: targeted Testcontainers test, 1 passed / 0 failed
QUERIES_CHANGED=false
QRELS_CHANGED=false
REMAINING_RISK=per-candidate hydration-missing trace is still absent (P2)
```

No other reproducible in-scope P0/P1 production defect was established during source analysis and
the executed regression. Missing runtime proof or failpoint capability is not mislabeled as a
production defect; it is recorded in the backlog.

## Proof readiness

All ten cases have executable designs in `PROOF_MATRIX.md`. Two cases remain
`IMPLEMENTED_NOT_PROVEN` at Phase A because their dedicated failpoint/generated-token fixture is
scheduled for later phases. This is consistent with analysis readiness; it is not a claim that the
full Phase 0-8 validation bank has passed.

## Test and evidence inventory

- Production tonic/PostgreSQL/Qdrant Testcontainers: `tests/e2e_testcontainers.rs`.
- Query planning, ranking evidence, Graph intent and Explain contracts under `tests/`.
- Access, lifecycle, atomicity, consistency, outbox, recovery and retrieval scripts under
  `smoke-tests/v004/scripts/`.
- Existing backend failpoints cover dense/sparse Qdrant and ingestion transaction boundaries.
- Existing quality corpora cover dense, sparse, hybrid, Graph, MMR, long document, access and TTL.
- FIX486 structural bank contract: `tests/fix486_hierarchical_bank_contracts.rs`.

## Remaining lower-severity work

The actionable list is in `IMPLEMENTATION_BACKLOG.md`. The highest-risk next work is deterministic
hydration failure injection, Explain scope/parity, per-candidate missing-parent trace, immutable
bank 1.0.0 freeze and model-backed execution of all 11 requests.

## Final gate policy

Final gate results are recorded after the report commit in external `stage-results.json` and in
`RESULT_TEMPLATE.md`. A failed, blocked or skipped mandatory final gate changes the verdict to
`FIX486_ANALYSIS_BLOCKED`.
