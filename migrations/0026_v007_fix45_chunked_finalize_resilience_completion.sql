-- AstraVector v007 fix4.5: chunked finalize and resilience completion
CREATE TABLE IF NOT EXISTS astravector.ingestion_session_batches_v004 (
    ingestion_session_id UUID NOT NULL,
    batch_index INT NOT NULL,
    batch_content_hash TEXT NOT NULL,
    block_count INT NOT NULL,
    batch_size_bytes BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (ingestion_session_id, batch_index)
);

ALTER TABLE astravector.ingestion_sessions_v004
    ADD COLUMN IF NOT EXISTS request_fingerprint TEXT,
    ADD COLUMN IF NOT EXISTS result_response_json JSONB,
    ADD COLUMN IF NOT EXISTS finalized_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS ix_ingestion_finalizing
    ON astravector.ingestion_sessions_v004(status, updated_at)
    WHERE status = 'FINALIZING';

CREATE INDEX IF NOT EXISTS ix_ingestion_batches_session
    ON astravector.ingestion_session_batches_v004(ingestion_session_id, batch_index);

CREATE INDEX IF NOT EXISTS ix_ingestion_blocks_session_batch
    ON astravector.ingestion_session_blocks_v004(ingestion_session_id, batch_index, block_index);
