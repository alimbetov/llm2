# AstraVector_v004 Full Power Smoke Report

## 1. Verdict
SECURE_RAG_CORE_CANDIDATE + CONSISTENCY_PARTIAL

## 2. Wave Summary
| Wave | Status | Evidence |
|---|---|---|
| Wave 1 RAG Core | PASS | /Users/ruslanalimbetov/Documents/llm2/AstraVector_v004/smoke-tests/v004/reports/full-power-smoke-results.json |
| Wave 2 Access Security | ACCESS_SECURITY_PASS | /Users/ruslanalimbetov/Documents/llm2/AstraVector_v004/smoke-tests/v004/reports/ACCESS_SECURITY_REPORT.md |
| Wave 3 Consistency | CONSISTENCY_PARTIAL | /Users/ruslanalimbetov/Documents/llm2/AstraVector_v004/smoke-tests/v004/reports/CONSISTENCY_REPORT.md |

## 3. Consistency Metrics
```json
{
  "verdict": "CONSISTENCY_PARTIAL",
  "register_parallel_requests": 50,
  "register_rows_created": 1,
  "register_idempotent_responses": 50,
  "register_conflict_rejected": true,
  "chunking_parallel_requests": 50,
  "chunking_success": 50,
  "chunking_conflict_rejected": true,
  "duplicate_chunks": 0,
  "duplicate_bindings": 0,
  "duplicate_outbox_logical_events": 0,
  "activation_parallel_requests": 10,
  "activation_success": 10,
  "active_versions": 1,
  "concurrent_search_requests": 100,
  "concurrent_search_success": 100,
  "concurrent_search_transport_errors": 0,
  "cross_zone_leakage_count": 0,
  "empty_parent_context_count": 0,
  "atomicity_failpoints_total": 8,
  "atomicity_failpoints_passed": 0,
  "atomicity_failpoints_status": "NOT_READY",
  "atomicity_failpoints_reason": "failpoint strings present but no Wave 3 test hook implemented",
  "outbox_double_claim_status": "NOT_READY",
  "outbox_stale_completion_status": "NOT_READY",
  "outbox_fencing_reason": "fencing column exists but smoke helper is not implemented",
  "qdrant_idempotent_upsert_pass": true,
  "dead_letter_test_status": "BLOCKED",
  "dead_letter_reason": "no controllable Qdrant failure mechanism",
  "data_integrity_violations_after_wave3": 0
}
```

## 4. Remaining Blockers
- lifecycle TTL/legal-hold/delete not yet full-power tested
- reconciliation/rebuild not yet full-power tested
- smoke-failpoints are not implemented
- outbox lock_generation/fencing_token is not implemented
- outbox dead-letter requires controllable Qdrant failure hook
- overload/backpressure not yet full-power tested
- observability not yet full-power tested
