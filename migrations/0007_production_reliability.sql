ALTER TABLE astravector.embedding_cache_entries
    ADD COLUMN IF NOT EXISTS lease_token bigint NOT NULL DEFAULT 0;
ALTER TABLE astravector.embedding_cache_entries
    ADD COLUMN IF NOT EXISTS lease_expires_at timestamptz;
ALTER TABLE astravector.embedding_cache_entries
    ADD COLUMN IF NOT EXISTS model_input_token_count integer;
ALTER TABLE astravector.embedding_cache_entries
    ADD COLUMN IF NOT EXISTS truncated boolean NOT NULL DEFAULT false;
ALTER TABLE astravector.embedding_cache_entries
    ADD COLUMN IF NOT EXISTS retry_count integer NOT NULL DEFAULT 0;

ALTER TABLE astravector.embedding_items DROP CONSTRAINT IF EXISTS chk_embedding_item_status;
ALTER TABLE astravector.embedding_requests DROP CONSTRAINT IF EXISTS chk_embedding_request_status;

CREATE INDEX IF NOT EXISTS idx_cache_processing_lease
    ON astravector.embedding_cache_entries (lease_expires_at)
    WHERE status = 'PROCESSING';
CREATE INDEX IF NOT EXISTS idx_requests_idempotency_lookup
    ON astravector.embedding_requests (tenant_id, workspace_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;

ALTER TABLE astravector.embedding_items
    DROP CONSTRAINT IF EXISTS embedding_items_cache_entry_id_fkey;
ALTER TABLE astravector.embedding_items
    ADD CONSTRAINT embedding_items_cache_entry_id_fkey
    FOREIGN KEY (cache_entry_id)
    REFERENCES astravector.embedding_cache_entries(id)
    ON DELETE SET NULL;
