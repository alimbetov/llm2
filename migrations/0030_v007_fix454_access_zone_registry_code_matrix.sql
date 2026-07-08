-- AstraVector v007 fix4.5.4
-- Access Zone Registry, Code Matrix TTL Policy & UUID-backed Zone Resolution.
-- External clients may use access_zone_code (0000..9999), while internal storage remains UUID-backed.

CREATE TABLE IF NOT EXISTS astravector.access_zones (
    access_zone_id UUID PRIMARY KEY,
    access_zone_code CHAR(4) NOT NULL UNIQUE,
    access_zone_name TEXT,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'ACTIVE',
    default_ttl_days INTEGER NOT NULL,
    ttl_policy_source TEXT NOT NULL DEFAULT 'CODE_MATRIX',
    allow_never_expire BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

DO $$ BEGIN
    ALTER TABLE astravector.access_zones
        ADD CONSTRAINT chk_access_zone_code_format
        CHECK (access_zone_code ~ '^[0-9]{4}$');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE astravector.access_zones
        ADD CONSTRAINT chk_access_zone_status
        CHECK (status IN ('ACTIVE','DISABLED','DELETED'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE astravector.access_zones
        ADD CONSTRAINT chk_access_zone_ttl_policy_source
        CHECK (ttl_policy_source IN ('CODE_MATRIX','MANUAL_OVERRIDE'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE astravector.access_zones
        ADD CONSTRAINT chk_access_zone_default_ttl
        CHECK (default_ttl_days >= 0);
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

CREATE UNIQUE INDEX IF NOT EXISTS ux_access_zones_code ON astravector.access_zones(access_zone_code);
CREATE INDEX IF NOT EXISTS ix_access_zones_status ON astravector.access_zones(status);
CREATE INDEX IF NOT EXISTS ix_access_zones_ttl_policy ON astravector.access_zones(ttl_policy_source);

-- Store the external code as a diagnostic alias. UUID remains the authoritative key.
ALTER TABLE astravector.ingestion_sessions_v004
    ADD COLUMN IF NOT EXISTS access_zone_code CHAR(4);

ALTER TABLE astravector.document_versions
    ADD COLUMN IF NOT EXISTS access_zone_code CHAR(4);

ALTER TABLE astravector.content_chunks_v004
    ADD COLUMN IF NOT EXISTS access_zone_code CHAR(4);

-- Backfill registry entries for existing UUID zones. Existing data must keep working.
-- Generated codes start at 1000 so the 0000-0999 never-expire band remains available for explicit zones.
WITH distinct_zones AS (
    SELECT access_zone_id, row_number() OVER (ORDER BY access_zone_id) AS rn
    FROM (
        SELECT DISTINCT access_zone_id FROM astravector.document_versions
        UNION
        SELECT DISTINCT access_zone_id FROM astravector.content_chunks_v004
        UNION
        SELECT DISTINCT access_zone_id FROM astravector.ingestion_sessions_v004
    ) z
), generated AS (
    SELECT access_zone_id,
           lpad(((999 + rn)::int)::text, 4, '0') AS access_zone_code
    FROM distinct_zones
    WHERE rn <= 9000
)
INSERT INTO astravector.access_zones(access_zone_id, access_zone_code, access_zone_name, status, default_ttl_days, ttl_policy_source, allow_never_expire)
SELECT access_zone_id,
       access_zone_code,
       'legacy-zone-' || access_zone_code,
       'ACTIVE',
       0,
       'MANUAL_OVERRIDE',
       true
FROM generated
ON CONFLICT(access_zone_id) DO NOTHING;

-- Backfill diagnostic codes into existing rows.
UPDATE astravector.document_versions dv
SET access_zone_code = az.access_zone_code
FROM astravector.access_zones az
WHERE dv.access_zone_id = az.access_zone_id
  AND dv.access_zone_code IS NULL;

UPDATE astravector.content_chunks_v004 c
SET access_zone_code = az.access_zone_code
FROM astravector.access_zones az
WHERE c.access_zone_id = az.access_zone_id
  AND c.access_zone_code IS NULL;

UPDATE astravector.ingestion_sessions_v004 s
SET access_zone_code = az.access_zone_code
FROM astravector.access_zones az
WHERE s.access_zone_id = az.access_zone_id
  AND s.access_zone_code IS NULL;

CREATE INDEX IF NOT EXISTS ix_document_versions_fix454_access_zone_code
ON astravector.document_versions(access_zone_code);

CREATE INDEX IF NOT EXISTS ix_content_chunks_fix454_access_zone_code
ON astravector.content_chunks_v004(access_zone_code);

CREATE INDEX IF NOT EXISTS ix_ingestion_sessions_fix454_access_zone_code
ON astravector.ingestion_sessions_v004(access_zone_code);
