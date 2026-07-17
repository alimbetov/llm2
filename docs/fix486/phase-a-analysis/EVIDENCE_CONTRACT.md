# FIX486A evidence contract

## External evidence root

```text
/Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix486a/<run-id>/
```

## Required identity

Every official run must record:

```text
source branch and SHA
origin/main SHA
epic branch SHA
clean-worktree assertion
Cargo.lock SHA-256
binary SHA-256
model SHA-256
tokenizer SHA-256
config SHA-256
bank ID, version and aggregate SHA-256
corpus/query/qrel/graph/lifecycle hashes
macOS and hardware values
Docker/PostgreSQL/Qdrant versions and image digests
stage commands, timestamps, durations and exact exit codes
```

## Status semantics

```text
PASS     assertion executed and satisfied
FAIL     assertion executed and failed
BLOCKED  mandatory prerequisite or observability unavailable
SKIPPED  intentionally not executed
```

`BLOCKED` and `SKIPPED` are not PASS.

## Required layout

```text
<run-id>/
├── environment/
├── baseline/
├── repository-analysis/
├── test-inventory/
├── fixture-analysis/
├── child-parent/
├── isolation-lifecycle/
├── failures/
├── graph-parent/
├── mmr-token-budget/
├── performance-methodology/
├── logs/
├── proof-matrix.json
├── observability-gap-matrix.json
├── implementation-backlog.json
├── stage-results.json
├── manifest.json
└── FIX486A-ANALYSIS-REPORT.md
```

## Stage result schema

```json
{
  "stage": "A2_CHILD_PARENT_ANALYSIS",
  "status": "PASS",
  "mandatory": true,
  "source_sha": "",
  "bank_version": "0.1.0-analysis-seed",
  "bank_sha256": "",
  "started_at": "",
  "finished_at": "",
  "duration_ms": 0,
  "commands": [],
  "assertions_total": 0,
  "assertions_passed": 0,
  "failure_codes": [],
  "evidence_files": []
}
```

## Invalid evidence conditions

An official run is `BLOCKED` when:

- the worktree is dirty before execution;
- source SHA changes during execution;
- bank/query/qrel hashes change;
- model/tokenizer/config identity changes;
- Docker image identity changes without a new run;
- expected labels are generated from runtime output;
- a mandatory stage has missing evidence;
- shell pipelines hide a failing exit code;
- the final report claims more than the executed stages prove.

## Git policy

Commit only compact reports, manifests, schemas, fixtures and hashes. Do not commit models, database volumes, Qdrant storage, heap dumps, credentials or large raw logs.

## Analysis verdict

```text
FIX486_ANALYSIS_READY
FIX486_ANALYSIS_BLOCKED
```

The verdict must be machine-readable in `analysis-verdict.json` and repeated verbatim in the Markdown report.