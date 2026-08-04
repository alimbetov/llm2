# FIX488 Acceptance Criteria

PASS requires:

```text
runtime_started = true
grpc_reflection_pass = true
document_registered = true
chunks_created > 0
bindings_created > 0
outbox_completed > 0
document_activated = true
postgres_document_found = true
postgres_chunk_count > 0
postgres_binding_count > 0
qdrant_collection_found = true
qdrant_point_count > 0
qdrant_document_point_found = true
semantic_search_results > 0
exact_anchor_search_results > 0
returned_document_id = loaded_document_id
cross_zone_leakage_count = 0
wrong_version_count = 0
```

Evidence containing `DRY_RUN_ONLY`, `PLACEHOLDER`, `NOT_IMPLEMENTED` or `SIMULATED` must fail.

