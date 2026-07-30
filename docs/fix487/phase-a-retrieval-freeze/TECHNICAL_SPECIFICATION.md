# FIX487A Retrieval Freeze Technical Specification

## Purpose

FIX487 is the production operations readiness phase. Phase A establishes a fail-closed retrieval freeze before operational changes begin. The guard protects the already validated retrieval semantics from accidental quality tuning while allowing later work on metrics, tracing, cancellation, bounded timeouts, concurrency controls, cleanup and operational runbooks.

## Fixed Baseline

- repository: `https://github.com/alimbetov/llm2`
- local project: `/Users/ruslanalimbetov/Documents/llm2/astravector`
- base branch: `main`
- frozen baseline SHA: `4843ce624724eceb865f64c6282d2841a69fcb88`
- Phase A branch: `agent/fix487a-retrieval-freeze`

If `origin/main` no longer resolves to the frozen baseline when Phase A starts, the phase must stop with `BASE_SHA_MOVED`.

## Protected Retrieval Surface

The freeze covers:

- chunking profile and token-boundary behavior;
- dense, sparse and hybrid candidate generation;
- RRF, fusion and MMR ranking semantics;
- GraphRAG seed selection, expansion admission, provenance, survivor preservation and final admission;
- no-answer and hard-negative semantics;
- frozen FIX486 and quality benchmark fixtures, queries, qrels, profiles and bank hashes;
- retrieval thresholds and config defaults that affect ranking, filtering or final context admission.

## Allowed Operational Surface

Future FIX487 changes may touch operational behavior only when the retrieval guard stays green:

- metrics and tracing;
- deadline, timeout and cancellation plumbing;
- bounded concurrency and backpressure controls;
- cleanup, shutdown and resource ownership;
- documentation, runbooks and evidence tooling.

Allowed operational changes must not alter retrieval scores, candidate ordering, qrels, frozen fixtures, threshold values, chunk boundaries or query interpretation.

## Guard Contract

`scripts/fix487_retrieval_freeze_guard.py` compares the current branch and working tree against the frozen baseline SHA. It fails when it detects:

- `protected_config_changed > 0`;
- `protected_fixture_changed > 0`;
- `protected_qrel_changed > 0`;
- `unapproved_retrieval_symbol_changed > 0`;
- missing Phase A manifest files.

The guard emits machine-readable JSON and exits non-zero on violations.

## Validation Commands

```bash
python3 -m py_compile scripts/fix487_retrieval_freeze_guard.py
python3 -m unittest -v tests/test_fix487_retrieval_freeze_guard.py
python3 scripts/fix487_retrieval_freeze_guard.py --repo .
make verify-fix487a-retrieval-freeze
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

Phase A does not run a runtime proof campaign.

## Acceptance Criteria

```text
retrieval_freeze_manifest_complete = true
protected_config_changed = 0
protected_fixture_changed = 0
protected_qrel_changed = 0
unapproved_retrieval_symbol_changed = 0
verdict = FIX487A_RETRIEVAL_FREEZE_PASS
```
