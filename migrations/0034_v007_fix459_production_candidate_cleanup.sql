-- fix459 production-candidate cleanup.
-- Adds schema compatibility and indexes required by the fix459 GraphRAG/TTL hardening.

ALTER TABLE astravector.document_versions
    ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}'::jsonb;

CREATE INDEX IF NOT EXISTS ix_document_versions_metadata_gin_v459
ON astravector.document_versions USING GIN(metadata);

CREATE INDEX IF NOT EXISTS ix_rag_graph_nodes_chunk_zone_node_active_v459
ON astravector.rag_graph_nodes_chunk(access_zone_id, node_id)
WHERE lifecycle_status='ACTIVE' AND quarantined=false;

CREATE INDEX IF NOT EXISTS ix_rag_graph_edges_zone_source_active_v459
ON astravector.rag_graph_edges(access_zone_id, source_node_id, relation_type, relation_score DESC)
WHERE lifecycle_status='ACTIVE' AND quarantined=false;

CREATE INDEX IF NOT EXISTS ix_content_chunks_parent_visibility_v459
ON astravector.content_chunks_v004(access_zone_id, id, lifecycle_status, access_level, expires_at)
WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS ix_document_versions_ttl_visibility_v459
ON astravector.document_versions(access_zone_id, document_id, document_version, lifecycle_status, expires_at);

CREATE INDEX IF NOT EXISTS ix_document_versions_delete_failed_next_attempt_v459
ON astravector.document_versions(lifecycle_status, next_delete_attempt_at, delete_attempts)
WHERE lifecycle_status='DELETE_FAILED';
