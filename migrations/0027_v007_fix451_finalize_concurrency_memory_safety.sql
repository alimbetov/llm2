-- AstraVector v007 fix4.5.1
-- Finalize concurrency, bounded staging cleanup, and completed replay retention.

ALTER TABLE astravector.ingestion_sessions_v004
    ADD COLUMN IF NOT EXISTS completed_blocks_cleaned_at TIMESTAMPTZ;

ALTER TABLE astravector.ingestion_sessions_v004
    ADD COLUMN IF NOT EXISTS result_expires_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS ix_ingestion_completed_blocks_cleanup
    ON astravector.ingestion_sessions_v004(finalized_at)
    WHERE status = 'COMPLETED'
      AND completed_blocks_cleaned_at IS NULL;

CREATE INDEX IF NOT EXISTS ix_ingestion_completed_result_cleanup
    ON astravector.ingestion_sessions_v004(result_expires_at)
    WHERE status = 'COMPLETED';
