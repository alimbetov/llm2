CREATE INDEX IF NOT EXISTS idx_embedding_requests_tenant_workspace_status
    ON astravector.embedding_requests (tenant_id, workspace_id, status);
CREATE INDEX IF NOT EXISTS idx_embedding_requests_task
    ON astravector.embedding_requests (tenant_id, workspace_id, emb_task_id);
CREATE INDEX IF NOT EXISTS idx_embedding_requests_created_at
    ON astravector.embedding_requests (created_at);
CREATE INDEX IF NOT EXISTS idx_embedding_items_request_status
    ON astravector.embedding_items (embedding_request_id, status);
CREATE INDEX IF NOT EXISTS idx_embedding_items_chunk
    ON astravector.embedding_items (chunk_id);
CREATE INDEX IF NOT EXISTS idx_embedding_items_parent
    ON astravector.embedding_items (parent_chunk_id)
    WHERE parent_chunk_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_embedding_items_cache_entry
    ON astravector.embedding_items (cache_entry_id);
CREATE INDEX IF NOT EXISTS idx_cache_text_hash_purpose
    ON astravector.embedding_cache_entries
       (tenant_id, workspace_id, text_hash, purpose);
CREATE INDEX IF NOT EXISTS idx_cache_status_processing_started
    ON astravector.embedding_cache_entries (status, processing_started_at)
    WHERE status = 'PROCESSING';
CREATE INDEX IF NOT EXISTS idx_cache_last_accessed
    ON astravector.embedding_cache_entries (last_accessed_at);
