-- AstraVector v007 fix4.5.3
-- Index TTL Lifecycle, Multi-Zone Access Contract & Batch Deletion
-- PostgreSQL remains source of truth. Qdrant is a searchable projection.

ALTER TABLE astravector.ingestion_sessions_v004
    ADD COLUMN IF NOT EXISTS ttl_days INTEGER;

ALTER TABLE astravector.document_versions
    ADD COLUMN IF NOT EXISTS indexed_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS ttl_days INTEGER,
    ADD COLUMN IF NOT EXISTS expires_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS lifecycle_status TEXT NOT NULL DEFAULT 'ACTIVE',
    ADD COLUMN IF NOT EXISTS delete_after_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS deleting_started_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS delete_attempts INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS last_delete_error_code TEXT,
    ADD COLUMN IF NOT EXISTS last_delete_error_message TEXT,
    ADD COLUMN IF NOT EXISTS last_delete_error_at TIMESTAMPTZ;

ALTER TABLE astravector.content_chunks_v004
    ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;

ALTER TABLE astravector.rag_graph_nodes
    ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;

ALTER TABLE astravector.rag_graph_edges
    ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;

-- Backfill legacy metadata safely. Existing data must not disappear after the TTL search filter is enabled.
UPDATE astravector.document_versions
SET indexed_at = COALESCE(indexed_at, activated_at, created_at, now()),
    ttl_days = COALESCE(ttl_days, 0),
    expires_at = CASE WHEN COALESCE(ttl_days, 0) = 0 THEN NULL ELSE COALESCE(expires_at, COALESCE(indexed_at, activated_at, created_at, now()) + (ttl_days * interval '1 day')) END,
    lifecycle_status = COALESCE(lifecycle_status, CASE WHEN status='DELETED' THEN 'DELETED' WHEN status='SUPERSEDED' THEN 'SUPERSEDED' ELSE 'ACTIVE' END)
WHERE indexed_at IS NULL OR ttl_days IS NULL OR lifecycle_status IS NULL;

UPDATE astravector.content_chunks_v004
SET lifecycle_status = COALESCE(lifecycle_status, 'ACTIVE')
WHERE lifecycle_status IS NULL;

UPDATE astravector.rag_graph_nodes
SET lifecycle_status = COALESCE(lifecycle_status, 'ACTIVE')
WHERE lifecycle_status IS NULL;

UPDATE astravector.rag_graph_edges
SET lifecycle_status = COALESCE(lifecycle_status, 'ACTIVE')
WHERE lifecycle_status IS NULL;

DO $$ BEGIN
    ALTER TABLE astravector.document_versions
        ADD CONSTRAINT chk_document_versions_fix453_lifecycle_status
        CHECK (lifecycle_status IN ('ACTIVE','EXPIRED','SUPERSEDED','DELETING','DELETE_FAILED','DELETED'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE astravector.document_versions
        ADD CONSTRAINT chk_document_versions_fix453_ttl_days
        CHECK (ttl_days IS NULL OR ttl_days >= 0);
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

CREATE INDEX IF NOT EXISTS ix_document_versions_fix453_ttl_cleanup
ON astravector.document_versions (lifecycle_status, expires_at, updated_at)
WHERE lifecycle_status IN ('ACTIVE','EXPIRED','SUPERSEDED','DELETE_FAILED');

CREATE INDEX IF NOT EXISTS ix_document_versions_fix453_deleting_stale
ON astravector.document_versions (deleting_started_at)
WHERE lifecycle_status = 'DELETING';

CREATE INDEX IF NOT EXISTS ix_document_versions_fix453_access_doc_version
ON astravector.document_versions (access_zone_id, document_id, document_version);

CREATE INDEX IF NOT EXISTS ix_content_chunks_fix453_search_lifecycle
ON astravector.content_chunks_v004 (access_zone_id, lifecycle_status, expires_at, access_level);

CREATE INDEX IF NOT EXISTS ix_content_chunks_fix453_doc_version
ON astravector.content_chunks_v004 (access_zone_id, document_id, document_version);

CREATE INDEX IF NOT EXISTS ix_rag_graph_nodes_fix453_lifecycle
ON astravector.rag_graph_nodes (access_zone_id, lifecycle_status, expires_at);

CREATE INDEX IF NOT EXISTS ix_rag_graph_edges_fix453_lifecycle
ON astravector.rag_graph_edges (access_zone_id, lifecycle_status, expires_at);
