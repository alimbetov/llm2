# Codex execution task — fix486b

## Repository

```text
https://github.com/alimbetov/llm2
```

## Work branch

```text
codex/fix486b-reproducible-runtime-baseline
```

## PR base

```text
epic/fix486-hierarchical-retrieval-validation
```

## Local paths

```text
PROJECT=/Users/ruslanalimbetov/Documents/llm2/astravector
WORKTREE=/Users/ruslanalimbetov/Documents/llm2/astravector-fix486b
EVIDENCE_ROOT=/Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix486b
```

## Mandatory first actions

1. Fetch and prune remote refs.
2. Verify work branch lineage from the current epic SHA.
3. Create and use the separate worktree.
4. Record clean `git status -sb` and all source identities.
5. Read every file under:

```text
docs/fix486/phase-a-analysis/
docs/fix486/phase-b-runtime-baseline/
benchmarks/hierarchical/fix486/
```

6. Confirm Phase A verdict is `FIX486_ANALYSIS_READY`.
7. Confirm the Phase A bank remains `0.1.0-analysis-seed` and is not frozen by this task.

## Mission

Implement and execute the reproducible runtime baseline exactly as defined in `TECHNICAL_SPECIFICATION.md`.

The required closeout is:

```text
R1 clean cold start
R2 independent clean repetition
R1/R2 normalized comparison
R3 persistence and dependency recovery
mandatory locked gates
compact committed result
external evidence bundle
```

## Mandatory commands

Preserve exact exit codes and stdout/stderr:

```bash
set -o pipefail
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo test --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo sqlx prepare --check -- --all-targets --all-features
cargo test --locked --features integration-tests --test e2e_testcontainers -- --nocapture
cargo test --locked --features integration-tests --test smoke_load_retrieve_context_testcontainers -- --nocapture
cargo test --locked --test fix486_hierarchical_bank_contracts -- --nocapture
```

## Implementation rules

- Use production ingestion, Search and RetrieveContext paths.
- Build release binaries with `--locked`.
- Record the resolved configuration rather than only source YAML.
- Use phase-owned containers/volumes and do not destroy unrelated user data.
- Verify ports before startup and after shutdown.
- Keep the Phase A hierarchical bank unchanged.
- Do not freeze bank 1.0.0.
- Do not tune ranking, RRF, no-answer, Graph or MMR.
- Do not claim performance or retrieval-quality superiority.
- Missing mandatory capability is `BLOCKED`, not `SKIPPED` or `PASS`.
- Do not commit model/tokenizer binaries, credentials, volumes or large logs.
- Do not merge.

## Defect repair

For every reproducible in-scope P0/P1:

```text
preserve FAIL evidence
→ add failing regression
→ document root cause
→ smallest safe fix in separate commit
→ rerun same control input
→ rerun R1/R2/R3 and mandatory gates
→ publish before/after evidence
```

Do not alter control expectations or Phase A qrels to hide a defect.

## Required repository changes

At minimum complete and update:

```text
CONTROL_FIXTURE_SPECIFICATION.md
RUN_MATRIX.md
EVIDENCE_CONTRACT.md
DEFECT_POLICY.md
ACCEPTANCE_CRITERIA.md
RESULT_TEMPLATE.md
```

Add the phase-owned runner, test driver, audit SQL, configuration and Makefile/CI target needed to make the baseline executable and reproducible.

## Required final report

Include:

```text
baseline source SHA
final candidate SHA
origin/main and epic SHA
Cargo.lock/config/binary/model/tokenizer hashes
container identities
migration head
R1/R2/R3 stage matrix
normalized reproducibility diff
control fixture physical identity map
Search/RetrieveContext control responses
dependency recovery evidence
defect register and fix commits
all gate results
external evidence path
manifest SHA-256
handoff blockers for fix486c
```

## Final verdict

Exactly one:

```text
FIX486_RUNTIME_BASELINE_PASS
```

or:

```text
FIX486_RUNTIME_BASELINE_BLOCKED
```

Update the branch and its draft PR. Do not merge.