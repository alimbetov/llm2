# fix462 Rollback Plan

## Application rollback

```bash
kubectl rollout undo deployment/astravector
kubectl rollout status deployment/astravector
kubectl logs deployment/astravector --tail=200
```

## Database migration rollback

Migration `0037_v007_fix462_retry_delete_error_stage.sql` is additive. Do not use `DROP COLUMN` in production rollback. Keep `last_delete_error_stage` and rollback only the application image.

## Runtime degradation controls

Disable TTL cleanup:

```bash
ASTRAVECTOR_INDEX_TTL_CLEANUP_ENABLED=false
```

Disable Qdrant extra-point reconciliation:

```bash
ASTRAVECTOR_INDEX_TTL_QDRANT_RECONCILIATION_ENABLED=false
```

Disable GraphRAG/MMR quality features:

```bash
ASTRAVECTOR_GRAPH_RETRIEVAL_ENABLED_BY_DEFAULT=false
ASTRAVECTOR_GRAPH_MMR_ENABLED=false
```

## Pre-deploy safety

```bash
pg_dump --schema=astravector --no-owner --no-privileges > astravector_schema_backup.sql
sqlx migrate run --database-url "$STAGING_DATABASE_URL"
cargo sqlx prepare --check -- --all-targets --all-features
```

Recommended rollout:

1. migrate staging clone;
2. deploy one canary pod;
3. validate `/metrics` and RetrieveContext E2E;
4. roll out remaining pods;
5. keep rollback window open until TTL cleanup runs successfully.
