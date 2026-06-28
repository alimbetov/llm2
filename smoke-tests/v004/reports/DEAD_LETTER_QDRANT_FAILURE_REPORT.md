# AstraVector_v004 Dead Letter Qdrant Failure Report

## Verdict
DEAD_LETTER_QDRANT_FAILURE_PASS

## Evidence
```json
[
  {
    "test_id": "W3C_qdrant_always_fail_dead_letter",
    "status": "PASS",
    "document_id": "4ba5000c-7c38-5294-a16d-26d3737cf300",
    "access_zone_id": "11111111-1111-4111-8111-111111111111",
    "expected": "outbox reaches DEAD_LETTER without synced Qdrant point",
    "actual": "outbox=DEAD_LETTER:1,RETRY_PENDING:2 bindings=PENDING:3 max_attempts=5 qdrant=0",
    "error": null
  },
  {
    "test_id": "W3C_qdrant_transient_failure_recovers",
    "status": "PASS",
    "document_id": "c8e122fc-c718-5cb2-8ebb-b6a33e8700d2",
    "access_zone_id": "11111111-1111-4111-8111-111111111111",
    "expected": "transient Qdrant failures retry to COMPLETED/SYNCED/Qdrant point",
    "actual": "outbox=COMPLETED:3 bindings=SYNCED:3 max_attempts=2 qdrant=3",
    "error": null
  }
]
```
