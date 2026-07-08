# AstraVector Migration Guide — fix462

## Required migration

`migrations/0037_v007_fix462_retry_delete_error_stage.sql` adds:

```sql
ALTER TABLE astravector.document_versions
ADD COLUMN IF NOT EXISTS last_delete_error_stage TEXT;
```

and index:

```sql
CREATE INDEX IF NOT EXISTS idx_document_versions_delete_error_stage_fix462
ON astravector.document_versions(last_delete_error_stage)
WHERE last_delete_error_stage IS NOT NULL;
```

## Migration validation

Run on a clean/staging database:

```bash
sqlx migrate run
sqlx migrate info
cargo sqlx prepare --check -- --all-targets --all-features
```

## Rollback policy

The fix462 migration is additive. In production, do not rollback with `DROP COLUMN`; instead rollback the application image. The extra column is safe for older application versions to ignore.
