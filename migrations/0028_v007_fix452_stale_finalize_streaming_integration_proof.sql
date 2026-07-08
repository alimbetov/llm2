-- AstraVector v007 fix4.5.2
-- Finalize Stale-State Safety, bounded streaming contract and integration proof schema.
-- Additive migration only; safe for rolling upgrade and rollback.

ALTER TABLE astravector.ingestion_sessions_v004
    ADD COLUMN IF NOT EXISTS finalizing_started_at TIMESTAMPTZ;

ALTER TABLE astravector.ingestion_sessions_v004
    ADD COLUMN IF NOT EXISTS finalizing_heartbeat_at TIMESTAMPTZ;

ALTER TABLE astravector.ingestion_sessions_v004
    ADD COLUMN IF NOT EXISTS last_error_code TEXT;

ALTER TABLE astravector.ingestion_sessions_v004
    ADD COLUMN IF NOT EXISTS last_error_message TEXT;

ALTER TABLE astravector.ingestion_sessions_v004
    ADD COLUMN IF NOT EXISTS last_error_at TIMESTAMPTZ;

UPDATE astravector.ingestion_sessions_v004
SET finalizing_started_at = COALESCE(finalizing_started_at, updated_at, now()),
    finalizing_heartbeat_at = COALESCE(finalizing_heartbeat_at, updated_at, now())
WHERE status = 'FINALIZING';

CREATE INDEX IF NOT EXISTS ix_ingestion_finalizing_stale
    ON astravector.ingestion_sessions_v004(finalizing_heartbeat_at, finalizing_started_at)
    WHERE status = 'FINALIZING';

CREATE INDEX IF NOT EXISTS ix_ingestion_last_error
    ON astravector.ingestion_sessions_v004(last_error_at)
    WHERE last_error_code IS NOT NULL;
