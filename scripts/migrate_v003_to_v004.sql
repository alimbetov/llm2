-- OFFLINE migration template. Provide a deterministic mapping from legacy tenant/workspace to access_zone_id before execution.
BEGIN;
CREATE TEMP TABLE access_zone_mapping(tenant_id text,workspace_id text,access_zone_id uuid,PRIMARY KEY(tenant_id,workspace_id));
-- INSERT INTO access_zone_mapping VALUES (...);
-- Validate every legacy binding maps exactly once before backfill.
DO $$ BEGIN IF EXISTS(SELECT 1 FROM astravector.vector_bindings b LEFT JOIN access_zone_mapping m USING(tenant_id,workspace_id) WHERE m.access_zone_id IS NULL) THEN RAISE EXCEPTION 'missing access_zone mapping'; END IF; END $$;
-- Backfill content_chunks_v004 and vector_bindings_v004 must be adapted to the exact v003 schema and run after mapping population.
ROLLBACK;
