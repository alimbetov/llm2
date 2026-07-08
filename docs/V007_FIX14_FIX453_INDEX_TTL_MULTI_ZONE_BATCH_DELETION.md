# AstraVector v007 fix4.5.3 — Index TTL Lifecycle, Multi-Zone Access Contract & Batch Deletion

This patch adds the production lifecycle contract for indexed RAG data.

## Scope

- `access_zone_id` remains mandatory on ingestion/indexing.
- `ttl_days` is accepted by `StartLogicalDocumentIngestionRequest`.
- `SearchRequestV004` and `RetrieveContextRequest` accept `repeated string access_zone_ids` for multi-zone retrieval.
- Qdrant search filtering now supports multi-zone `access_zone_id` matching.
- Qdrant search filtering uses numeric `expires_at_epoch > now_epoch`.
- Never-expire documents use a far-future Qdrant epoch instead of an OR/should filter.
- PostgreSQL remains the lifecycle source of truth.
- PostgreSQL context fetch rechecks access zone, lifecycle status and TTL after Qdrant candidate search.
- GraphRAG expansion must use the same access-zone and TTL/lifecycle filters as direct retrieval.
- `IndexTtlCleanupWorker` logic claims expired/superseded/delete-failed document versions with `FOR UPDATE SKIP LOCKED`.
- Qdrant points are deleted by `(access_zone_id, document_id, document_version)` in batches.
- Tombstone retention and tombstone purge hooks are included.
- Payload drift is mitigated by PostgreSQL recheck and sync-failure metrics; full reconciliation remains a future hardening item.

## Required acceptance points

1. Ingestion without `access_zone_id` fails.
2. `ttl_days` is validated by configured min/max.
3. `ttl_days=0` uses PostgreSQL `expires_at=NULL` and Qdrant far-future `expires_at_epoch`.
4. Multi-zone search never returns chunks outside the request's allowed zones.
5. Expired/superseded/deleting/deleted chunks are excluded from Search/RetrieveContext.
6. GraphRAG expansion cannot pull foreign-zone or expired graph neighbors.
7. Cleanup is idempotent and safe in multi-pod environments.
8. Old document versions become `SUPERSEDED` on new-version activation.
9. Metrics expose TTL cleanup backlog, failures, deleted documents, deleted points and tombstones.

## Known implementation note

The existing PostgreSQL schema uses `uuid` for `access_zone_id`; therefore fix4.5.3 validates safe string shape but still requires UUID-compatible zone identifiers at runtime. A future schema-breaking version may widen `access_zone_id` to text if non-UUID zone names become mandatory.
