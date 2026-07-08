-- v007 fix4.5.5: Access Zone Auto-Provisioning, Bind Fix & Registry Lifecycle Hardening
-- Adds audit fields required for controlled ingestion-time auto-create of access zones.

ALTER TABLE astravector.access_zones
    ADD COLUMN IF NOT EXISTS auto_created BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS created_by TEXT,
    ADD COLUMN IF NOT EXISTS created_reason TEXT,
    ADD COLUMN IF NOT EXISTS first_seen_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_seen_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS ix_access_zones_auto_created
    ON astravector.access_zones(auto_created);

CREATE INDEX IF NOT EXISTS ix_access_zones_last_seen_at
    ON astravector.access_zones(last_seen_at);

-- Backfill audit timestamps for existing registry rows without changing their semantics.
UPDATE astravector.access_zones
SET
    first_seen_at = COALESCE(first_seen_at, created_at, now()),
    last_seen_at = COALESCE(last_seen_at, updated_at, created_at, now()),
    created_reason = COALESCE(created_reason, 'REGISTRY_EXISTING')
WHERE first_seen_at IS NULL
   OR last_seen_at IS NULL
   OR created_reason IS NULL;
