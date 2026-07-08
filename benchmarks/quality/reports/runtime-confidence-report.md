# Runtime Confidence Report

- started_at: `2026-07-08T20:29:20+05:00`
- finished_at: `2026-07-08T20:30:45+05:00`
- quality_run_id: `fix474f-20260708-202920`
- verdict: `PASS`
- runtime_execution: `CONFIDENCE_GATE_CONFIRMED`
- production_pass: `true`
- astraVector version: `UNKNOWN`
- git commit: `d1ae47bd12d66f1029cf1d153f4a602282548b5e`
- model version: `UNKNOWN`

## Mandatory Profiles

| Profile | Verdict | Runtime Execution | Available/Blocked |
|---|---:|---|---|
| dense | PASS | MODEL_BACKED_E2E_CONFIRMED | sparse=n/a, hybrid=n/a, blocked=false |
| sparse | PASS | MODEL_BACKED_E2E_CONFIRMED | sparse=true, hybrid=n/a, blocked=false |
| hybrid | PASS | MODEL_BACKED_E2E_CONFIRMED | sparse=n/a, hybrid=true, blocked=false |

## Hard-Negative Before/After

| Metric | Before | After |
|---|---:|---:|
| forbidden_document_returned | 12 | 0 |
| forbidden_phrase_returned | 3 | 0 |
| forbidden_total | 15 | 0 |
| failed | 15 | 0 |

## No-Answer Thresholds

- enabled: `true`
- min_dense_score: `0.25`
- min_sparse_score: `0.1`
- min_hybrid_score: `0.3`
- exact_technical_boost: `0.5`

## Security Gates

- cross_zone_leakage_count: `0`
- access_level_violation_count: `0`

## Reasons

- none

## Preflight

- endpoint_available: `true`
- postgres_available: `true`
- qdrant_available: `true`
- qdrant_collection_available: `true`
- qdrant_vector_schema_available: `true`
- model_file_found: `true`
- tokenizer_file_found: `true`
- model_inference_verified: `true`
- model_inference_reason: `MODEL_INFERENCE_VERIFIED_BY_DENSE_RUNTIME_PROFILE`

## Warnings

- none
