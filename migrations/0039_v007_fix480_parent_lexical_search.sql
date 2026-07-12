ALTER TABLE astravector.content_chunks_v004
    ADD COLUMN IF NOT EXISTS search_vector_simple tsvector
    GENERATED ALWAYS AS (to_tsvector('simple', COALESCE(content, ''))) STORED;

-- SQLx runs migrations transactionally, so the local/test migration uses a regular
-- partitioned GIN index. Production operators may build equivalent child indexes
-- concurrently before attaching them; see docs/FIX480_LEXICAL_INDEX_RUNBOOK.md.
CREATE INDEX IF NOT EXISTS idx_content_chunks_v004_parent_search_vector
ON astravector.content_chunks_v004
USING GIN(search_vector_simple)
WHERE granularity = 'PARENT'
  AND representation_type = 'ORIGINAL'
  AND lifecycle_status = 'ACTIVE'
  AND deleted_at IS NULL;
