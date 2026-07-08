-- AstraVector v007 fix4.5.8
-- Critical production defect remediation: ingestion/indexing ownership, TTL terminal delete state,
-- access-zone cache freshness metadata and versioned embedding cache keys.

ALTER TABLE astravector.document_versions
    ADD COLUMN IF NOT EXISTS processing_owner_id TEXT,
    ADD COLUMN IF NOT EXISTS processing_started_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS processing_heartbeat_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS next_delete_attempt_at TIMESTAMPTZ;

ALTER TABLE astravector.access_zones
    ADD COLUMN IF NOT EXISTS status_version BIGINT NOT NULL DEFAULT 1;

-- Allow terminal delete failure state introduced by fix4.5.8.
DO $$ BEGIN
    ALTER TABLE astravector.document_versions
        DROP CONSTRAINT IF EXISTS chk_document_versions_fix453_lifecycle_status;
EXCEPTION WHEN undefined_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE astravector.document_versions
        ADD CONSTRAINT chk_document_versions_fix458_lifecycle_status
        CHECK (lifecycle_status IN ('ACTIVE','EXPIRED','SUPERSEDED','DELETING','DELETE_FAILED','DELETE_PERMANENTLY_FAILED','DELETED'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

CREATE INDEX IF NOT EXISTS ix_document_versions_processing_owner_v458
ON astravector.document_versions(processing_owner_id)
WHERE processing_owner_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS ix_document_versions_indexing_stale_v458
ON astravector.document_versions(processing_heartbeat_at)
WHERE status='INDEXING' AND processing_owner_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS ix_document_versions_delete_failed_next_attempt_v458
ON astravector.document_versions(lifecycle_status, next_delete_attempt_at, delete_attempts)
WHERE lifecycle_status = 'DELETE_FAILED';

CREATE INDEX IF NOT EXISTS ix_document_versions_delete_permanently_failed_v458
ON astravector.document_versions(lifecycle_status, updated_at)
WHERE lifecycle_status = 'DELETE_PERMANENTLY_FAILED';

CREATE INDEX IF NOT EXISTS ix_access_zones_status_version_v458
ON astravector.access_zones(access_zone_id, status, status_version);

-- Version-aware lookup aid. Existing unique(cache_key) remains valid because new runtime
-- includes model/tokenizer/dense/sparse versions in generated cache_key values.
CREATE INDEX IF NOT EXISTS ix_embedding_cache_entries_version_lookup_v458
ON astravector.embedding_cache_entries(
  tenant_id,
  workspace_id,
  text_hash,
  tokenizer_version,
  model_version,
  dense_version,
  sparse_version
);
