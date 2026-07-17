# FIX486B evidence contract

## External root

```text
/Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix486b/<run-id>
```

## Required layout

```text
<run-id>/
├── environment/
│   ├── os.txt
│   ├── hardware.json
│   ├── tools.txt
│   └── ports-before-after.json
├── source/
│   ├── git-identity.json
│   ├── cargo-lock.sha256
│   └── worktree-status.txt
├── static/
├── infrastructure/
│   ├── compose-config.txt
│   ├── postgres-inspect.json
│   ├── qdrant-inspect.json
│   └── readiness.json
├── migrations/
│   ├── clean-apply.log
│   ├── reapply.log
│   ├── migration-head.json
│   └── schema-audit.json
├── model-tokenizer/
│   ├── identities.json
│   ├── tokenizer-offsets.json
│   └── warmup.json
├── build/
│   ├── release-build.log
│   ├── binary.sha256
│   └── resolved-config.sha256
├── runtime/
│   ├── reflection.json
│   ├── health.json
│   ├── services.json
│   └── runtime.log
├── fixture/
│   ├── control-input.json
│   ├── control-input.sha256
│   └── control-fixture-identity.json
├── ingestion/
├── retrieval/
├── restart/
├── dependency-recovery/
├── comparisons/
│   ├── r1-r2-normalized.json
│   └── r2-r3-normalized.json
├── metrics/
├── logs/
├── defect-register.json
├── stage-results.json
├── manifest.json
└── FIX486B-RUNTIME-BASELINE-RESULT.md
```

## Manifest requirements

`manifest.json` must contain:

```text
schema version
run ID
phase
source branch/SHA
origin/main SHA
epic SHA
clean-worktree assertion
Cargo.lock SHA-256
model/tokenizer/config/binary/control-fixture hashes
PostgreSQL and Qdrant image references and IDs/digests
OS/hardware/tool versions
stage commands, timestamps, durations and exit codes
R1/R2/R3 verdicts
final verdict
failure codes
evidence file hashes
```

## Stage result schema

```json
{
  "stage": "R1_RUNTIME_READINESS",
  "run": "R1",
  "mandatory": true,
  "status": "PASS",
  "started_at": "...",
  "finished_at": "...",
  "duration_ms": 0,
  "command": "...",
  "exit_code": 0,
  "assertions": [],
  "failure_codes": [],
  "evidence_files": []
}
```

## Integrity rules

- Hash all compact evidence files.
- Record evidence creation after command completion, not before.
- Do not rewrite the initial failing evidence after a repair.
- Before/after defect bundles must reference different source SHAs and the same control input hash.
- Missing required file blocks the final verdict.
- Raw credentials and secrets must be redacted before any compact summary is committed.
- Absolute local paths are allowed only in external evidence and compact pointers, never as portable runtime defaults.

## Git-committed evidence

Commit only:

```text
phase specification and runner
compact final Markdown result
compact machine-readable stage summary
manifest hash pointer
defect register summary
normalized comparison summary
```

Do not commit:

```text
models
tokenizer binaries
container volumes
large logs
Prometheus dumps
heap/core dumps
credentials
full database or Qdrant snapshots
```

## Failure policy

An evidence bundle is invalid when:

```text
worktree was dirty at run start
source/config/model/tokenizer/binary identity changed mid-run
control input changed between R1 and R2
stage status is inferred only from process exit without assertions
required logs or manifests are missing
hash verification fails
```

Invalid evidence produces `FIX486_RUNTIME_BASELINE_BLOCKED`.