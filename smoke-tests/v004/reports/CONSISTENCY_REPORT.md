# AstraVector_v004 Consistency Report

## 1. Verdict
CONSISTENCY_PARTIAL

## 2. Summary
| Check | Status | Evidence |
|---|---|---|
| Register idempotency | PASS | /Users/ruslanalimbetov/Documents/llm2/AstraVector_v004/smoke-tests/v004/reports/consistency-evidence.jsonl |
| Register conflicting idempotency | PASS | /Users/ruslanalimbetov/Documents/llm2/AstraVector_v004/smoke-tests/v004/logs/consistency/register-conflict.err |
| Chunking idempotency | PASS | /Users/ruslanalimbetov/Documents/llm2/AstraVector_v004/smoke-tests/v004/reports/consistency-evidence.jsonl |
| Chunking conflicting idempotency | PASS | /Users/ruslanalimbetov/Documents/llm2/AstraVector_v004/smoke-tests/v004/logs/consistency/chunk-conflict.err |
| Activation idempotency | PASS | /Users/ruslanalimbetov/Documents/llm2/AstraVector_v004/smoke-tests/v004/reports/consistency-evidence.jsonl |
| Concurrent Search | PASS | /Users/ruslanalimbetov/Documents/llm2/AstraVector_v004/smoke-tests/v004/logs/consistency/search |
| Atomicity failpoints | NOT_READY | failpoint strings present but no Wave 3 test hook implemented |
| Outbox double claim | NOT_READY | fencing column exists but smoke helper is not implemented |
| Outbox stale completion | NOT_READY | fencing column exists but smoke helper is not implemented |
| Qdrant idempotent upsert | PASS | /Users/ruslanalimbetov/Documents/llm2/AstraVector_v004/smoke-tests/v004/reports/consistency-evidence.jsonl |
| Dead letter | BLOCKED | no controllable Qdrant failure mechanism |
| Data integrity audit after Wave 3 | PASS | /Users/ruslanalimbetov/Documents/llm2/AstraVector_v004/smoke-tests/v004/reports/full-power-data-integrity.tsv |

## 3. Metrics
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

## 4. Duplicate Checks
| Check | Count |
|---|---:|
| duplicate_chunks | 0 |
| duplicate_bindings | 0 |
| duplicate_outbox_logical_events | 0 |

## 5. Atomicity Findings
| Failpoint | Expected | Actual | Status |
|---|---|---|---|
| smoke-failpoints | runtime hooks | not present | BLOCKED |

## 6. Outbox Fencing Findings
| Scenario | Expected | Actual | Status |
|---|---|---|---|
| double claim | lock generation/fencing token | schema has lease but no generation token | BLOCKED |
| stale completion | stale generation rejected | cannot prove without generation token | BLOCKED |
| qdrant idempotent upsert | count by binding_id = 1 | pass=true | PASS |

## 7. Remaining Blockers
- smoke-failpoints are not implemented in runtime
- vector_outbox has no lock_generation/fencing_token
- no controllable Qdrant failure hook for dead-letter proof
