CREATE TABLE IF NOT EXISTS astravector.embedding_items (
    id uuid PRIMARY KEY,
    embedding_request_id uuid NOT NULL
        REFERENCES astravector.embedding_requests(id) ON DELETE CASCADE,
    chunk_id uuid NOT NULL,
    chunk_type smallint NOT NULL,
    parent_chunk_id uuid,
    cache_entry_id uuid
        REFERENCES astravector.embedding_cache_entries(id),
    text_hash varchar(64) NOT NULL,
    text_length integer NOT NULL,
    model_input_token_count integer,
    truncated boolean NOT NULL DEFAULT false,
    status varchar(32) NOT NULL,
    error_code varchar(64),
    error_message text,
    created_at timestamptz NOT NULL DEFAULT now(),
    completed_at timestamptz,
    CONSTRAINT chk_embedding_chunk_type CHECK (chunk_type IN (1, 2)),
    CONSTRAINT chk_child_parent CHECK (chunk_type <> 2 OR parent_chunk_id IS NOT NULL),
    CONSTRAINT uq_embedding_request_chunk UNIQUE (embedding_request_id, chunk_id)
);
