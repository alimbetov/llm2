# Codex execution task — fix486a

## Repository

```text
https://github.com/alimbetov/llm2
```

## Work branch

```text
codex/fix486a-analysis-readiness
```

## PR base

```text
epic/fix486-hierarchical-retrieval-validation
```

## Local paths

```text
PROJECT=/Users/ruslanalimbetov/Documents/llm2/astravector
WORKTREE=/Users/ruslanalimbetov/Documents/llm2/astravector-fix486a
EVIDENCE_ROOT=/Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix486a
```

## Mandatory first actions

1. Fetch remote refs.
2. Verify the branch head and base lineage.
3. Use a separate worktree.
4. Record `git status -sb`, `git rev-parse HEAD`, `git rev-parse origin/main` and the epic branch SHA.
5. Verify the data-bank manifest before using any fixture.
6. Read every file under `docs/fix486/phase-a-analysis/` and `benchmarks/hierarchical/fix486/`.

## Working rules

- Analyze the real production entrypoints, not only helper functions.
- Distinguish unit proof, integration proof, model-backed proof and load proof.
- Do not change ranking weights, thresholds, qrels or expected identities to obtain green results.
- Do not commit models, tokenizer binaries, credentials, large logs, database volumes or Qdrant storage.
- Do not merge.
- Create a draft PR to `epic/fix486-hierarchical-retrieval-validation`.

## Analysis requirements

For every critical scenario provide:

```text
invariant
production entrypoint
code path
storage path
Qdrant identity
existing tests
existing runtime evidence
missing observability
required fixture
required failpoint
required future test
current proof status
priority
```

## Baseline commands

Run with `set -o pipefail` and preserve exact exit codes:

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo test --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
```

Do not fix unrelated baseline failures in the same commit as analysis documentation. Classify them first.

## Required final report

The draft PR must include:

- analyzed source SHA;
- bank version and SHA identities;
- architecture map;
- proof status for all ten critical cases;
- current defects;
- testability and observability gaps;
- fixture feasibility;
- failure-injection design;
- Mac performance methodology;
- implementation backlog;
- external evidence path and manifest hash;
- one final verdict.

Allowed verdicts:

```text
FIX486_ANALYSIS_READY
FIX486_ANALYSIS_BLOCKED
```

## Important defect policy

A P0/P1 defect may be fixed in this branch only when it blocks truthful analysis. In that case:

1. preserve FAIL evidence;
2. add a regression test;
3. commit the fix separately;
4. keep the data bank and qrels unchanged;
5. rerun the same inputs;
6. document before/after results.

All other fixes belong in later phase branches.