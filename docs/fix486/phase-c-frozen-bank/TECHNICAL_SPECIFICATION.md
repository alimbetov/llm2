# v012/fix486c — Frozen Executable Hierarchical Bank 1.0.0

## 1. Document status

```text
DOCUMENT_TYPE=IMPLEMENTATION_AND_PROOF_SPECIFICATION
PHASE=FIX486C_FROZEN_EXECUTABLE_BANK_1_0_0
BASE_BRANCH=main
EXPECTED_BASE_SHA=8fd29b2acd166992f953a5020be81b076e581403
WORK_BRANCH=codex/fix486c-frozen-executable-bank-1.0.0
BANK_ID=fix486-hierarchical-bank
CURRENT_BANK_VERSION=0.1.0-analysis-seed
TARGET_BANK_VERSION=1.0.0
BANK_FREEZE_AUTHORIZED=true
PRODUCTION_STATUS=BLOCKED
```

The actual branch and source identities must be recorded at execution time. `EXPECTED_BASE_SHA` is the mandatory lineage point for this phase unless an explicitly approved newer `main` SHA contains no semantic changes to the bank or runtime contract.

## 2. Preconditions

Phase B completed with:

```text
FIX486_RUNTIME_BASELINE_PASS
```

Official Phase B identities:

```text
RUNTIME_BASELINE_SHA=9e5250becad48583960c888f37c09ad32a6597ad
PHASE_B_EVIDENCE_COMMIT=248c115ddb1c98a693a342cfaaf5292289b6fc2c
PHASE_B_MERGE_SHA=8fd29b2acd166992f953a5020be81b076e581403
PHASE_B_EVIDENCE_FILES=113
PHASE_B_MANIFEST_SHA256=6a4ee479020fc8598ad2096a3e4d2a196e78c9a6ef6cb266680e81b447069ce7
```

Established facts:

- the reproducible runtime baseline passes from a clean state;
- PostgreSQL is canonical and Qdrant is a projection;
- Search is the authoritative retrieval path and RetrieveContext delegates to it;
- runtime startup, restart, persistence and dependency-readiness recovery are proven;
- deterministic access-zone identity is implemented;
- the hierarchical bank exists as `0.1.0-analysis-seed` and is structurally valid;
- the bank is not yet immutable or executable as the certification source for phases D–I.

## 3. Objective

Freeze the hierarchical validation bank as immutable version `1.0.0` and make it mechanically executable without changing retrieval behavior or tuning ranking parameters.

The phase answers:

> Can one exact, hash-locked bank version be used as the unchanged source of truth for all later child/parent, isolation/lifecycle, failure/degradation, Graph, MMR/token-budget and Mac-load proofs?

## 4. Allowed final verdicts

Exactly one:

```text
FIX486_FROZEN_EXECUTABLE_BANK_PASS
```

or:

```text
FIX486_FROZEN_EXECUTABLE_BANK_BLOCKED
```

A PASS means only that bank `1.0.0` is structurally complete, immutable, hash-verifiable and executable. It does not mean any query quality or production-readiness verdict has passed.

## 5. Scope

### In scope

- freeze `benchmarks/hierarchical/fix486` as bank `1.0.0`;
- preserve the existing ten cases and eleven queries;
- preserve corpus, query and qrel semantics unless a structural contradiction makes freezing impossible;
- compute per-file and aggregate SHA-256 identities;
- define canonical byte and aggregate-hash rules;
- add a fail-closed bank integrity verifier;
- add an executable bank runner contract;
- add production-path fixture ingestion and logical-to-runtime identity capture;
- add machine-readable execution result schema;
- add immutable-bank contract tests;
- add a Makefile verification target;
- publish compact freeze evidence in Git;
- record any blocker without silently weakening qrels.

### Out of scope

- changing Search/RetrieveContext ranking behavior;
- changing dense, sparse, hybrid, RRF, Graph or MMR weights;
- changing token budgets to force a PASS;
- changing qrels after observing runtime output;
- certifying child/parent runtime correctness;
- certifying isolation/lifecycle behavior;
- executing failure/degradation proof;
- certifying Graph parent selection;
- certifying MMR or token-budget quality;
- running Mac load or soak tests;
- declaring production candidate or production ready.

## 6. Frozen bank contents

The frozen manifest must reference exactly:

```text
benchmarks/hierarchical/fix486/
├── bank-manifest.json
├── corpus/hierarchical-fixture-v1.json
├── queries/hierarchical-queries-v1.jsonl
├── qrels/hierarchical-qrels-v1.jsonl
├── graph-relations/hierarchical-graph-v1.json
└── lifecycle/hierarchical-lifecycle-v1.json
```

No generated runtime IDs, timestamps, host paths, process IDs or evidence outputs may be stored inside the frozen bank.

## 7. Bank identity contract

The manifest must contain at least:

```json
{
  "bank_id": "fix486-hierarchical-bank",
  "bank_version": "1.0.0",
  "schema_version": "1",
  "status": "FROZEN",
  "freeze_source_sha": "<commit-sha>",
  "frozen_at_utc": "<RFC3339 timestamp>",
  "query_count": 11,
  "case_count": 10,
  "hash_algorithm": "SHA-256",
  "hashes": {
    "corpus_sha256": "<64-lowercase-hex>",
    "queries_sha256": "<64-lowercase-hex>",
    "qrels_sha256": "<64-lowercase-hex>",
    "graph_sha256": "<64-lowercase-hex>",
    "lifecycle_sha256": "<64-lowercase-hex>",
    "aggregate_sha256": "<64-lowercase-hex>",
    "status": "RESOLVED"
  }
}
```

### Canonical hashing rules

1. Per-file hashes are computed over exact committed bytes.
2. JSON/JSONL files must end with exactly one LF and contain no CRLF.
3. Aggregate input order is fixed:

```text
corpus
queries
qrels
graph
lifecycle
```

4. Aggregate bytes are the UTF-8 lines:

```text
<relative-path>\t<sha256>\n
```

5. `bank-manifest.json` is excluded from the aggregate hash to avoid recursive identity.
6. The verifier must recompute every hash and fail on missing, extra, reordered or modified files.

## 8. Executable bank contract

The phase must introduce a phase-owned executable interface with these logical operations:

```text
verify
prepare
clean
start-runtime
migrate
ingest
apply-graph
apply-lifecycle-scenario
execute-query
execute-all
export-identity-map
export-results
stop-runtime
```

A shell runner may orchestrate Rust test drivers or binaries, but all mandatory stages must preserve exact exit codes and emit machine-readable stage results.

Recommended entry point:

```bash
make verify-fix486c-frozen-bank
```

Recommended execution entry point:

```bash
./scripts/fix486c-frozen-bank.sh --verify-only
./scripts/fix486c-frozen-bank.sh --prepare-runtime
./scripts/fix486c-frozen-bank.sh --execute-all
```

## 9. Production-path ingestion contract

1. Corpus ingestion must use the public ingestion facade or the same production service path used by it.
2. Do not insert document/chunk hierarchy directly with ad hoc SQL.
3. Do not precompute physical chunk IDs in frozen fixtures.
4. Capture runtime-generated IDs in an external identity map.
5. Repeating ingestion with the same request identity must not create duplicates.
6. Graph relation setup must use the supported graph persistence path; direct SQL is allowed only if no production API exists and the exception is documented.
7. Lifecycle fault setup belongs to later proof phases; Phase C verifies scenario definitions are executable and addressable, not that their expected runtime outcomes already pass.

## 10. Logical-to-runtime identity map

The runner must emit a separate file similar to:

```json
{
  "schema_version": 1,
  "bank_id": "fix486-hierarchical-bank",
  "bank_version": "1.0.0",
  "bank_aggregate_sha256": "<hash>",
  "source_sha": "<sha>",
  "access_zones": {},
  "documents": {},
  "versions": {},
  "blocks": {},
  "parents": {},
  "children": {},
  "graph_relations": {}
}
```

The map is runtime evidence and must remain outside the frozen bank directory.

## 11. Query execution result contract

Every query execution must produce a row containing at least:

```json
{
  "query_id": "q-child-parent-exact",
  "case_id": "FIX486-01",
  "status": "PASS|FAIL|BLOCKED|SKIPPED",
  "runtime_status": "<service status>",
  "matched_contexts": [],
  "warnings": [],
  "hard_gate_results": {},
  "bank_aggregate_sha256": "<hash>",
  "source_sha": "<sha>",
  "runtime_binary_sha256": "<hash>",
  "model_sha256": "<hash>",
  "tokenizer_sha256": "<hash>"
}
```

Phase C does not require all functional qrels to PASS. It requires every query to be loadable, dispatchable, traceable to one qrel and exportable without modifying the bank.

## 12. Mandatory structural assertions

```text
BANK_ID=fix486-hierarchical-bank
BANK_VERSION=1.0.0
BANK_STATUS=FROZEN
CASE_COUNT=10
QUERY_COUNT=11
QREL_COUNT=11
DUPLICATE_QUERY_IDS=0
DUPLICATE_CASE_QUERY_PAIRS=0
ORPHAN_QRELS=0
QUERIES_WITHOUT_QRELS=0
UNRESOLVED_LOGICAL_PARENTS=0
UNRESOLVED_LOGICAL_CHILDREN=0
UNKNOWN_GRAPH_ENDPOINTS=0
UNKNOWN_LIFECYCLE_SCENARIOS=0
NULL_HASH_FIELDS=0
HASH_MISMATCHES=0
UNTRACKED_BANK_FILES=0
```

## 13. Mandatory static gates

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo test --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo sqlx prepare --check -- --all-targets --all-features
cargo test --locked --test fix486_hierarchical_bank_contracts -- --nocapture
cargo test --locked --test fix486c_frozen_bank_contracts -- --nocapture
python3 scripts/fix486c_verify_frozen_bank.py
```

If a separate executable runner is implemented, add a dry-run contract test proving all 11 queries can be parsed and scheduled without a live runtime.

## 14. Required repository outputs

```text
benchmarks/hierarchical/fix486/bank-manifest.json
scripts/fix486c_verify_frozen_bank.py
scripts/fix486c-frozen-bank.sh
tests/fix486c_frozen_bank_contracts.rs
Makefile

docs/fix486/phase-c-frozen-bank/
├── TECHNICAL_SPECIFICATION.md
├── CODEX_EXECUTION_TASK.md
├── BANK_FREEZE_CONTRACT.md
├── EXECUTION_AND_EVIDENCE_CONTRACT.md
├── ACCEPTANCE_CRITERIA.md
└── RESULT_TEMPLATE.md
```

Implementation may add a small Rust driver or integration test, but it must not redesign production retrieval.

## 15. Evidence layout

Raw evidence must remain outside Git:

```text
<ASTRAVECTOR_EVIDENCE_ROOT>/fix486c/<run-id>/
├── environment/
├── source/
├── bank/
├── static/
├── build/
├── runtime/
├── ingestion/
├── identity-map/
├── query-dry-run/
├── execution/
├── comparisons/
├── logs/
├── stage-results.json
├── manifest.json
└── FIX486C-FROZEN-BANK-RESULT.md
```

Compact hashes, result summaries and manifest pointers may be committed after a successful run.

## 16. Defect policy

For any reproducible P0/P1 blocker:

1. preserve failing evidence;
2. add a failing regression test;
3. document root cause;
4. fix production code separately from qrels and bank content;
5. rerun the same input;
6. rerun all mandatory Phase C gates;
7. do not change qrels to follow the corrected or existing runtime output.

If the defect proves that the seed bank is internally contradictory, stop with `FIX486_FROZEN_EXECUTABLE_BANK_BLOCKED`. Do not silently repair the bank and call it `1.0.0` in the same unreviewed commit.

## 17. Definition of Done

```text
[ ] Work started from approved base SHA
[ ] Frozen bank contains exactly five payload files plus manifest
[ ] Bank version is 1.0.0 and status is FROZEN
[ ] Ten cases and eleven queries preserved
[ ] Eleven qrels preserved and resolvable
[ ] Per-file hashes populated and verified
[ ] Aggregate hash populated and verified
[ ] Canonical byte rules verified
[ ] Fail-closed integrity verifier implemented
[ ] Executable runner or driver implemented
[ ] All eleven queries parse and schedule
[ ] Production-path ingestion plan implemented
[ ] Runtime identity map exported outside bank
[ ] Result schema includes PASS/FAIL/BLOCKED/SKIPPED
[ ] Mandatory locked gates PASS
[ ] No unresolved in-scope P0/P1
[ ] External evidence manifest complete
[ ] Compact result documents committed
[ ] No Phase D–I quality verdict claimed
```

## 18. Final gate

`FIX486_FROZEN_EXECUTABLE_BANK_PASS` requires every mandatory Phase C assertion and evidence item to be present and PASS.

Any hash mismatch, missing qrel, unresolvable logical identity, extra bank file, dirty worktree, source drift, skipped mandatory gate, incomplete evidence or unresolved P0/P1 produces:

```text
FIX486_FROZEN_EXECUTABLE_BANK_BLOCKED
```
