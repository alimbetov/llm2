-- AstraVector v007/fix460: RAG/GraphRAG/TTL consistency hardening.
-- Safe additive migration only.

CREATE INDEX IF NOT EXISTS idx_content_chunks_visibility_lookup_fix460
ON astravector.content_chunks_v004(access_zone_id, id, lifecycle_status, access_level, expires_at)
WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_content_chunks_document_visibility_fix460
ON astravector.content_chunks_v004(access_zone_id, document_id, document_version, lifecycle_status, expires_at)
WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_document_versions_visibility_lookup_fix460
ON astravector.document_versions(access_zone_id, document_id, document_version, status, lifecycle_status, expires_at);

CREATE INDEX IF NOT EXISTS idx_vector_bindings_delete_lookup_fix460
ON astravector.vector_bindings_v004(access_zone_id, document_id, document_version, lifecycle_status)
WHERE qdrant_point_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_vector_bindings_point_visibility_fix460
ON astravector.vector_bindings_v004(access_zone_id, qdrant_point_id, lifecycle_status, expires_at)
WHERE qdrant_point_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_vector_bindings_chunk_visibility_fix460
ON astravector.vector_bindings_v004(access_zone_id, chunk_id, lifecycle_status, expires_at);
