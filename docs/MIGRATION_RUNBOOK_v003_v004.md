# v003 → v004 migration runbook
1. Back up PostgreSQL and verify restore.
2. Apply migrations 0010–0014 on PostgreSQL 15+.
3. Populate a deterministic `(tenant_id, workspace_id) → access_zone_id` mapping.
4. Backfill document versions, content chunks and bindings in batches.
5. Run row counts, FK orphan checks, cache-entry checks and sample vector checksums.
6. Enable dual write or pause writes for final delta.
7. Switch runtime to v004 tables.
8. Validate partition pruning with `EXPLAIN`.
9. Rebuild/reconcile Qdrant projection.
10. Retain legacy tables until acceptance is signed off.
