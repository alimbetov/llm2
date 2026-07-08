# AstraVector Final Readiness Report

Date: 2026-07-08

Final status: `RUNTIME_READY`

This report is based on the live local runtime in `/Users/ruslanalimbetov/Documents/llm2/astravector`. Static checks, skipped profiles, and stale generated reports are not counted as runtime proof.

## Verdict

`make quality-runtime-confidence-remote` executed against `localhost:50051` and passed the mandatory confidence gate.

Current confidence result:

- `runtime_execution`: `CONFIDENCE_GATE_CONFIRMED`
- `verdict`: `PASS`
- `runtime_ready`: `true`
- `runtime_confidence_pass`: `true`
- `production_pass`: `true`
- `production_pass` meaning: confidence gate passed, not `PRODUCTION_CANDIDATE`
- `production_candidate`: `false`
- `production_ready`: `false`
- `data_isolation_mode`: `quality_run_id_namespace`

## Runtime Evidence

Mandatory profiles passed in live ingest-and-retrieve mode:

| Profile | Runtime Execution | Verdict |
|---|---|---|
| Dense | `MODEL_BACKED_E2E_CONFIRMED` | `PASS` |
| Sparse | `MODEL_BACKED_E2E_CONFIRMED` | `PASS` |
| Hybrid | `MODEL_BACKED_E2E_CONFIRMED` | `PASS` |

The final hybrid profile returned:

- access isolation: 15 passed, 0 failed
- distractor: 5 passed, 0 failed
- exact lookup: 12 passed, 0 failed
- hard negative: 15 passed, 0 failed
- hybrid: 2 passed, 0 failed
- lexical sparse: 12 passed, 0 failed
- paraphrase: 3 passed, 0 failed

Security counters:

- cross-zone leakage: `0`
- access-level violations: `0`

Hard-negative counters:

- forbidden total after: `0`
- false positive rate: `0.0`
- weak candidates filtered before MMR: non-zero in the final hybrid run

## Preflight Evidence

The confidence preflight verified:

- gRPC endpoint: `PASS`
- PostgreSQL: `PASS`
- Qdrant: `PASS`
- Qdrant collection: `PASS`
- Qdrant vector schema: `PASS`
- model file: `PASS`
- tokenizer file: `PASS`
- fixture checksum: `COMPUTED`

Model-backed inference is proven by the mandatory runtime profiles. A separate pre-runtime inference probe remains useful diagnostics work, but it is no longer a blocker for `RUNTIME_READY`.

## Static Gates

The following gates passed after the retrieval changes:

- `cargo fmt --check`
- `cargo check --all-targets --all-features`
- `cargo clippy --all-targets --all-features -- -D warnings`

## GraphRAG Policy

- `graph_rag_available = false`
- `graph_rag_required_for_ready = false`
- `graph_rag_required_for_production_candidate = true`

Graph diagnostic or skipped cases are not counted as PASS. GraphRAG remains a blocker for `PRODUCTION_CANDIDATE`, not for `RUNTIME_READY`.

## Distractor Handling

Distractor fixture labels are used for evaluation and reporting only. Production retrieval rejects weak common-overlap candidates based on evidence: lexical coverage, discriminating term matches, leading-term evidence, negative-evidence phrases, no-answer thresholds, and MMR/final context gates. It does not pass or fail production candidates by hardcoded distractor IDs or fixture labels.

## Remaining Blockers To Production Candidate

`RUNTIME_READY` is now reached. `PRODUCTION_CANDIDATE` still requires additional gates:

1. GraphRAG production-candidate proof.
2. Full all-target test suite proof beyond the focused runtime readiness gates.
3. Deployment validation against the packaged image and Kubernetes manifests.
4. Load, soak, recovery, backup/restore, rollback, capacity, and alerting proof.
5. Review and stage the large untracked readiness/fixture file set intentionally.

Generated machine-readable evidence:

- `benchmarks/quality/reports/runtime-confidence-report.json`
- `benchmarks/quality/reports/final-readiness-report.json`
