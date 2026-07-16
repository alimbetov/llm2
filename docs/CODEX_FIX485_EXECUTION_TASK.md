# Codex execution task — FIX485

## 1. Assignment

Implement and verify `v010/fix485 — RAG Search Reliability, Performance and Quality Hardening` on branch:

```text
codex/fix485-rag-reliability-smoke-hardening
```

Primary specifications:

```text
docs/FIX485_RAG_RELIABILITY_PERFORMANCE_QUALITY_HARDENING.md
docs/FIX485_SMOKE_TEST_COVERAGE_PLAN.md
```

Local paths:

```text
SOURCE_REPOSITORY=/Users/ruslanalimbetov/Documents/llm2/astravector
EXTERNAL_EVIDENCE_ROOT=/Users/ruslanalimbetov/Documents/llm2/astravector-evidence
```

Base commit:

```text
c302a8341f64f6b31e0cf7aee97a966f554b3902
```

## 2. Safety rules

1. Do not work in or alter another dirty worktree.
2. Do not reset, clean or stash user changes without explicit permission.
3. Fetch origin and verify the remote branch before editing.
4. Use a dedicated worktree for the fix485 branch when the primary checkout is not already safely on that branch.
5. Never commit model files, tokenizer files, generated evidence, logs, local databases or secrets.
6. Keep all evidence under `/Users/ruslanalimbetov/Documents/llm2/astravector-evidence`.
7. Never merge automatically.
8. Keep the PR draft until mandatory CI and required model-backed evidence are green.
9. All Cargo verification/build commands use `--locked`.
10. Do not delete or weaken tests to achieve a green build.
11. Do not change ranking weights or no-answer thresholds without baseline/candidate A/B evidence.
12. Do not weaken access-zone, access-level, lifecycle or active-version filtering.
13. Extended query processing remains opt-in in production.

## 3. Initial local workflow

From the existing repository:

```bash
cd /Users/ruslanalimbetov/Documents/llm2/astravector

git status -sb
git remote -v
git fetch --all --prune
git rev-parse origin/main
git rev-parse origin/codex/fix485-rag-reliability-smoke-hardening
```

Preferred isolated worktree:

```bash
WORKTREE=/Users/ruslanalimbetov/Documents/llm2/astravector-fix485

git worktree add "$WORKTREE" origin/codex/fix485-rag-reliability-smoke-hardening
cd "$WORKTREE"
git switch -c codex/fix485-rag-reliability-smoke-hardening --track origin/codex/fix485-rag-reliability-smoke-hardening 2>/dev/null || \
  git switch codex/fix485-rag-reliability-smoke-hardening
```

If the branch is already checked out in another worktree, use that worktree rather than forcing the ref.

Record before changes:

```bash
git status -sb
git rev-parse HEAD
git rev-parse origin/main
rustc --version
cargo --version
docker --version
docker compose version
```

## 4. Required implementation order

### Phase 1 — Baseline and architecture audit

- read both fix485 specifications;
- inspect current planner, segmenter, intent, fusion, coverage, no-answer, GraphRAG, MMR, deadlines, admission and diagnostics code;
- inspect existing smoke scripts and quality profiles;
- run static baseline before changing code;
- write a short implementation plan in the issue/PR description;
- identify where current code already satisfies the specification and avoid duplicate abstractions.

### Phase 2 — Query semantic correctness

Implement or harden:

- one canonical `NormalizedQuery` representation;
- shared offsets for segmentation and intent extraction;
- UTF-8-safe mappings;
- candidate-to-intent evidence attribution;
- independent multi-intent coverage;
- explicit branch/segment failure statuses;
- deterministic multilingual classifier ordering;
- model-backed tokenizer boundary tests.

Run focused tests after each logical commit.

### Phase 3 — Executable retrieval smoke

Replace the obsolete static-only hybrid proof with runtime assertions for Dense, Sparse, FTS and Hybrid branches.

Requirements:

- prove branch execution using diagnostics/metrics, not source grep;
- validate expected document/block identities;
- verify access and lifecycle filters;
- verify no-answer and hard negatives;
- preserve backward-compatible public API.

### Phase 4 — Long-query model-backed smoke

Add exact canonical-token boundary generation and scenarios for:

- 256/257/1024/1025/2048/2049;
- CRLF/blank-line normalization;
- two intents in one segment;
- overlap without double counting;
- tail evidence near token 2048;
- stack trace/SQL plus final question;
- long hard negative;
- RU/KZ/EN mixed input.

### Phase 5 — Failure/recovery and observability

Add deterministic tests/failpoints for partial backend failures, timeout, cancellation, admission overload, Graph/MMR skip, shutdown, reconciliation and permit cleanup.

Verify metrics and privacy rules.

### Phase 6 — Performance/fairness and evidence tooling

- fix hardcoded hardware metadata in Mac load evidence;
- use actual `system_profiler`/`sysctl` values;
- add `--locked` everywhere;
- record all release identities;
- add isolated and mixed-tier load profiles;
- add spike/recovery and soak checks;
- keep evidence outside the repo.

### Phase 7 — Packaging, CI and documentation

- add Make targets and aggregate smoke profiles;
- update CI with fast mandatory tests;
- keep heavy model-backed/load workflows protected/manual or scheduled;
- update smoke/readiness documentation to remove stale BM25 blocked claims where executable proof now exists;
- build packaged Docker image and validate rollback behavior.

## 5. Mandatory baseline commands

Run before implementation and preserve output in an external evidence run:

```bash
RUN_ID="fix485-baseline-$(date +%Y%m%d-%H%M%S)"
EVIDENCE_DIR="/Users/ruslanalimbetov/Documents/llm2/astravector-evidence/$RUN_ID"
mkdir -p "$EVIDENCE_DIR/static"

cargo fmt --all --check \
  2>&1 | tee "$EVIDENCE_DIR/static/fmt.log"

cargo check --locked --all-targets --all-features \
  2>&1 | tee "$EVIDENCE_DIR/static/check.log"

cargo test --locked query_processing --lib -- --nocapture \
  2>&1 | tee "$EVIDENCE_DIR/static/query-processing.log"

cargo test --locked --test query_processing_contracts -- --nocapture \
  2>&1 | tee "$EVIDENCE_DIR/static/query-contracts.log"

cargo test --locked --all-targets --all-features \
  2>&1 | tee "$EVIDENCE_DIR/static/all-tests.log"

cargo clippy --locked --all-targets --all-features -- -D warnings \
  2>&1 | tee "$EVIDENCE_DIR/static/clippy.log"
```

Do not claim a baseline PASS if shell pipelines hide a failing Cargo exit code. Use `set -o pipefail` or capture `PIPESTATUS`.

## 6. Required validation gates before draft PR review

### Static and integration

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo test --locked query_processing --lib -- --nocapture
cargo test --locked --test query_processing_contracts -- --nocapture
cargo test --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo sqlx prepare --check -- --all-targets --all-features
cargo test --locked --features integration-tests --test smoke_load_retrieve_context_testcontainers -- --nocapture
cargo test --locked --features integration-tests --test e2e_testcontainers -- --nocapture
```

### New fix485 focused gates

Run the new tests added by the task, including:

```text
query_normalization_offsets
query_tokenizer_model_backed
multi_intent_evidence
retrieval_failure_semantics
graph_intent_provenance
explain_search_parity
mixed_tier_fairness
```

### Runtime smoke

Run the new long-query, hybrid runtime, partial failure, observability and packaging smoke scripts according to the smoke plan.

### Quality

Run fresh model-backed quality profiles on the branch head. Do not reuse stale reports from another commit.

### Load/recovery

A draft PR can be opened before the 60-minute soak, but it cannot be marked ready for review or production candidate until mandatory release evidence is complete or explicitly recorded as a blocker.

## 7. Commit strategy

Use small reviewable commits. Recommended sequence:

```text
fix485-01: add canonical query normalization
fix485-02: align segmentation and intent offsets
fix485-03: add candidate intent evidence
fix485-04: add retrieval failure status semantics
fix485-05: expand deterministic multilingual intent rules
fix485-06: add model-backed tokenizer boundary tests
fix485-07: replace static hybrid smoke with runtime proof
fix485-08: add long-query semantic smoke
fix485-09: add failure and recovery smoke
fix485-10: add mixed-tier fairness gate
fix485-11: harden evidence and observability
fix485-12: add packaging deployment rollback smoke
fix485-13: update CI Makefile and readiness docs
```

Do not mix generated evidence into commits.

## 8. GitHub workflow

- work on `codex/fix485-rag-reliability-smoke-hardening`;
- reference the fix485 GitHub issue in commits/PR;
- push incremental commits after focused tests pass;
- open a draft PR targeting `main`;
- include current PASS/BLOCKED matrix in the PR body;
- do not merge;
- do not mark ready for review while mandatory static/security gates are red;
- do not declare production candidate while mandatory model-backed, load, deployment or rollback evidence is missing.

If `gh` is unavailable, continue local work and use the available GitHub connector for issue/PR operations. Absence of `gh` is not a blocker for implementation or testing.

## 9. Required evidence bundle

Final branch evidence should contain:

```text
<evidence-root>/<run-id>/
├── environment-manifest.json
├── resolved-config.json
├── stage-failures.json
├── static/
├── integration/
├── quality/
├── security/
├── long-query/
├── failures/
├── load/
├── packaging/
├── metrics/
├── FIX485-RESULT.json
└── FIX485-RESULT.md
```

No evidence file should be committed unless it is a small intentionally curated schema/example with no machine-specific data.

## 10. Final response format

Report:

1. branch and final SHA;
2. files changed;
3. architectural changes;
4. tests added;
5. exact commands and results;
6. quality deltas versus base commit;
7. latency/fairness results;
8. security results;
9. remaining blockers;
10. draft PR URL.

End with exactly one verdict:

```text
FIX485_PRODUCTION_CANDIDATE
```

or:

```text
FIX485_PRODUCTION_BLOCKED
```

Never use the candidate verdict when any mandatory gate is failed, blocked, skipped or unexecuted.