-- AstraVector v007 fix4.5.7
-- Production readiness hardening indexes for TTL cleanup and access-zone registry consistency.

CREATE INDEX IF NOT EXISTS ix_access_zones_status_code
ON astravector.access_zones(status, access_zone_code);

CREATE INDEX IF NOT EXISTS ix_document_versions_delete_failed_retry
ON astravector.document_versions(lifecycle_status, last_delete_error_at)
WHERE lifecycle_status = 'DELETE_FAILED';

CREATE INDEX IF NOT EXISTS ix_document_versions_deleting_stale_v457
ON astravector.document_versions(lifecycle_status, deleting_started_at)
WHERE lifecycle_status = 'DELETING';

CREATE INDEX IF NOT EXISTS ix_document_versions_ttl_worker_v457
ON astravector.document_versions(lifecycle_status, expires_at, updated_at)
WHERE lifecycle_status IN ('ACTIVE', 'EXPIRED', 'SUPERSEDED', 'DELETE_FAILED');
