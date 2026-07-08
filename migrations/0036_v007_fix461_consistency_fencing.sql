-- fix461: consistency fencing and multi-zone visibility hardening
-- Adds document delete fencing columns and indexes for TTL cleanup, legal hold, and compound visibility lookups.

ALTER TABLE astravector.document_versions
  ADD COLUMN IF NOT EXISTS delete_operation_id UUID,
  ADD COLUMN IF NOT EXISTS delete_fencing_started_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_document_versions_delete_operation_id_fix461
ON astravector.document_versions(delete_operation_id)
WHERE delete_operation_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_vector_bindings_v004_doc_lifecycle_legal_hold_fix461
ON astravector.vector_bindings_v004(
  access_zone_id,
  document_id,
  document_version,
  lifecycle_status,
  legal_hold
)
WHERE qdrant_point_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_content_chunks_v004_zone_chunk_visibility_fix461
ON astravector.content_chunks_v004(
  access_zone_id,
  id,
  lifecycle_status,
  access_level,
  expires_at
)
WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_document_versions_visibility_fix461
ON astravector.document_versions(
  access_zone_id,
  document_id,
  document_version,
  status,
  lifecycle_status,
  expires_at
);

CREATE INDEX IF NOT EXISTS idx_rag_graph_expansion_doc_visibility_fix461
ON astravector.content_chunks_v004(
  access_zone_id,
  document_id,
  document_version,
  id,
  lifecycle_status,
  access_level,
  expires_at
)
WHERE deleted_at IS NULL;
