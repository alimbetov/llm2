-- AstraVector v007/fix462: RetryDocumentDeletion and TTL delete-stage diagnostics.
-- Safe on existing databases and on fresh testcontainers runs.
ALTER TABLE astravector.document_versions
  ADD COLUMN IF NOT EXISTS last_delete_error_stage TEXT;

CREATE INDEX IF NOT EXISTS idx_document_versions_delete_error_stage_fix462
ON astravector.document_versions(last_delete_error_stage)
WHERE last_delete_error_stage IS NOT NULL;
