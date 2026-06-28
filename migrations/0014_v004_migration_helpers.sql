CREATE OR REPLACE FUNCTION astravector.v004_assert_partition_pruning(zone uuid)
RETURNS TABLE(plan_line text) LANGUAGE plpgsql AS $$
BEGIN
 RETURN QUERY EXECUTE format('EXPLAIN (COSTS OFF) SELECT * FROM astravector.vector_bindings_v004 WHERE access_zone_id=%L::uuid',zone);
END $$;
-- Legacy data backfill is intentionally explicit and must be run via scripts/migrate_v003_to_v004.sql after access-zone mapping is provided.
