-- AstraVector v007 fix4.4-rev2: durable chunked ingestion staging.
CREATE TABLE IF NOT EXISTS astravector.ingestion_sessions_v004 (
    ingestion_session_id UUID PRIMARY KEY,
    access_zone_id UUID NOT NULL,
    document_id UUID NOT NULL,
    document_version BIGINT NOT NULL,
    source_uri TEXT,
    file_name TEXT,
    content_hash TEXT,
    idempotency_key TEXT NOT NULL,
    status TEXT NOT NULL,
    total_bytes_estimate BIGINT,
    total_blocks_estimate BIGINT,
    received_batches INTEGER NOT NULL DEFAULT 0,
    received_blocks BIGINT NOT NULL DEFAULT 0,
    received_bytes BIGINT NOT NULL DEFAULT 0,
    final_content_hash TEXT,
    result_idempotency_key TEXT,
    error_code TEXT,
    error_message TEXT,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT ck_ingestion_sessions_status CHECK (status IN ('ACTIVE','FINALIZING','COMPLETED','ABORTED','EXPIRED','FAILED'))
);

CREATE UNIQUE INDEX IF NOT EXISTS ux_ingestion_idempotency
    ON astravector.ingestion_sessions_v004(access_zone_id, idempotency_key);

CREATE INDEX IF NOT EXISTS ix_ingestion_status_expires
    ON astravector.ingestion_sessions_v004(status, expires_at);

CREATE INDEX IF NOT EXISTS ix_ingestion_document
    ON astravector.ingestion_sessions_v004(access_zone_id, document_id, document_version);

CREATE TABLE IF NOT EXISTS astravector.ingestion_session_blocks_v004 (
    ingestion_session_id UUID NOT NULL REFERENCES astravector.ingestion_sessions_v004(ingestion_session_id) ON DELETE CASCADE,
    batch_index INTEGER NOT NULL,
    block_index INTEGER NOT NULL,
    block_json JSONB NOT NULL,
    batch_content_hash TEXT NOT NULL,
    block_size_bytes INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (ingestion_session_id, batch_index, block_index)
);

CREATE INDEX IF NOT EXISTS ix_ingestion_blocks_session
    ON astravector.ingestion_session_blocks_v004(ingestion_session_id, batch_index);
