# AstraVector_v004 Atomicity Failpoints Report

## Verdict
ATOMICITY_FAILPOINTS_PASS

## Evidence
```json
[
  {
    "test_id": "W3B_required_after_document_version_update",
    "status": "PASS",
    "document_id": "6e6cc90d-d434-5f86-9faf-9e2b76a437b3",
    "access_zone_id": "11111111-1111-4111-8111-111111111111",
    "expected": "rollback/no completed state",
    "actual": "active=0 chunks=0 synced_bindings=0 completed_outbox=0 qdrant=0",
    "sql_evidence": {},
    "qdrant_evidence": {},
    "grpc_evidence": {},
    "error": null
  },
  {
    "test_id": "W3B_required_after_chunk_insert",
    "status": "PASS",
    "document_id": "b1297c39-2f9e-5f43-9924-289e911f1208",
    "access_zone_id": "11111111-1111-4111-8111-111111111111",
    "expected": "retry completes without duplicate canonical state",
    "actual": "active=0 chunks=4 synced_bindings=3 completed_outbox=3 qdrant=3 duplicate_chunks=0",
    "sql_evidence": {},
    "qdrant_evidence": {},
    "grpc_evidence": {},
    "error": null
  },
  {
    "test_id": "W3B_required_after_embedding_cache_insert",
    "status": "PASS",
    "document_id": "f3b10fb4-168f-5481-8852-97ff35e4d1af",
    "access_zone_id": "11111111-1111-4111-8111-111111111111",
    "expected": "retry completes without duplicate canonical state",
    "actual": "active=0 chunks=4 synced_bindings=1 completed_outbox=1 qdrant=1 duplicate_chunks=0",
    "sql_evidence": {},
    "qdrant_evidence": {},
    "grpc_evidence": {},
    "error": null
  },
  {
    "test_id": "W3B_required_after_dense_insert",
    "status": "PASS",
    "document_id": "c50dea88-3103-555f-a1ac-fcf16d73c5d8",
    "access_zone_id": "11111111-1111-4111-8111-111111111111",
    "expected": "retry completes without duplicate canonical state",
    "actual": "active=0 chunks=4 synced_bindings=1 completed_outbox=1 qdrant=1 duplicate_chunks=0",
    "sql_evidence": {},
    "qdrant_evidence": {},
    "grpc_evidence": {},
    "error": null
  },
  {
    "test_id": "W3B_required_after_binding_insert",
    "status": "PASS",
    "document_id": "b500a144-463d-5dd8-89f9-8e4e2d004660",
    "access_zone_id": "11111111-1111-4111-8111-111111111111",
    "expected": "retry completes without duplicate canonical state",
    "actual": "active=0 chunks=4 synced_bindings=3 completed_outbox=3 qdrant=3 duplicate_chunks=0",
    "sql_evidence": {},
    "qdrant_evidence": {},
    "grpc_evidence": {},
    "error": null
  },
  {
    "test_id": "W3B_required_after_outbox_insert",
    "status": "PASS",
    "document_id": "2dcc44d8-b70f-5198-9082-509edcf41b8c",
    "access_zone_id": "11111111-1111-4111-8111-111111111111",
    "expected": "retry completes without duplicate canonical state",
    "actual": "active=0 chunks=4 synced_bindings=3 completed_outbox=3 qdrant=3 duplicate_chunks=0",
    "sql_evidence": {},
    "qdrant_evidence": {},
    "grpc_evidence": {},
    "error": null
  },
  {
    "test_id": "W3B_required_before_commit",
    "status": "PASS",
    "document_id": "574958f6-cb6c-5976-9bcd-10b40085fb03",
    "access_zone_id": "11111111-1111-4111-8111-111111111111",
    "expected": "retry completes without duplicate canonical state",
    "actual": "active=0 chunks=4 synced_bindings=3 completed_outbox=3 qdrant=3 duplicate_chunks=0",
    "sql_evidence": {},
    "qdrant_evidence": {},
    "grpc_evidence": {},
    "error": null
  },
  {
    "test_id": "W3B_required_after_commit_before_response",
    "status": "PASS",
    "document_id": "c232ffe9-c7de-54f9-b90a-561613733a81",
    "access_zone_id": "11111111-1111-4111-8111-111111111111",
    "expected": "retry completes without duplicate canonical state",
    "actual": "active=0 chunks=4 synced_bindings=1 completed_outbox=1 qdrant=1 duplicate_chunks=0",
    "sql_evidence": {},
    "qdrant_evidence": {},
    "grpc_evidence": {},
    "error": null
  }
]
```
