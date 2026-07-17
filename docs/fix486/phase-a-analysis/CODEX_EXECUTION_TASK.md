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
2. Verify branch head and base lineage.
3. Use a separate worktree.
4. Record `git status -sb`, `git rev-parse HEAD`, `git rev-parse origin/main` and the epic branch SHA.
5. Verify the data-bank manifest before using any fixture.
6. Read every file under `docs/fix486/phase-a-analysis/` and `benchmarks/hierarchical/fix486/`.
7. Freeze source, environment, Cargo.lock, model, tokenizer, config and bank identities before running tests.

## Working rules

- Analyze the real production entrypoints, not only helper functions.
- Distinguish unit proof, integration proof, model-backed proof and load proof.
- Do not change ranking weights, thresholds, qrels or expected identities merely to obtain green results.
- Do not weaken access-zone, access-level, active-version, lifecycle, TTL or no-answer policies.
- Do not hardcode fixture IDs, anchors or expected labels in production logic.
- Do not commit models, tokenizer binaries, credentials, large logs, database volumes or Qdrant storage.
- Do not erase failing evidence after a repair.
- Keep bank changes, regression tests, production fixes and evidence reports in separate commits.
- Do not merge.
- Update draft PR #15 targeting `epic/fix486-hierarchical-retrieval-validation`.

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

Classify every baseline failure before modifying production code.

## Mandatory defect-repair loop

After analysis and baseline, repair every reproducible in-scope P0/P1 defect found in the hierarchical retrieval path.

For each defect:

1. preserve the original failing evidence;
2. record source SHA, bank version/SHA and scenario ID;
3. add a regression test that fails before the fix;
4. document the root cause and affected production path;
5. implement the smallest safe production fix in a separate commit;
6. keep queries, qrels and expected identities unchanged;
7. rerun the exact same scenario;
8. preserve after-fix evidence;
9. publish a direct before/after comparison;
10. update the defect register and Proof Matrix.

A reproducible P2 correctness or resource-leak defect should also be fixed when the repair is local, low-risk and does not broaden scope. Otherwise record it in `IMPLEMENTATION_BACKLOG.md` with the reason, risk and target phase. P3 items remain backlog work.

## Mandatory final rerun

After the last repair, rerun:

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo test --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
```

Also rerun:

- every targeted regression test added by this phase;
- every relevant integration/Testcontainers test;
- every executable bank scenario used by this phase;
- every available model-backed gate required to validate an affected production path.

A missing prerequisite must be reported as `BLOCKED`; it must not be silently skipped or represented as PASS.

## Required final report

The draft PR must include:

- analyzed source SHA;
- final candidate SHA;
- bank version and SHA identities;
- architecture map;
- proof status for all ten critical cases;
- complete defect register;
- failing tests and evidence before fixes;
- production fix commits;
- passing tests and evidence after fixes;
- testability and observability gaps;
- fixture feasibility;
- failure-injection design;
- Mac performance methodology;
- implementation backlog;
- mandatory final-gate matrix;
- external evidence path and manifest hash;
- exactly one final verdict.

## READY rule

`FIX486_ANALYSIS_READY` is allowed only when:

- the real production path is mapped;
- all ten critical scenarios have executable proof designs;
- no reproducible in-scope P0/P1 defect remains unresolved;
- every repaired defect has an unchanged-bank failing regression and before/after evidence;
- mandatory final gates pass;
- remaining P2/P3 items are explicitly documented with risk and target phase.

Any unresolved P0/P1 defect, mandatory failed gate, missing evidence, identity mismatch, or mandatory `BLOCKED/SKIPPED` stage requires:

```text
FIX486_ANALYSIS_BLOCKED
```

Allowed final verdicts:

```text
FIX486_ANALYSIS_READY
FIX486_ANALYSIS_BLOCKED
```