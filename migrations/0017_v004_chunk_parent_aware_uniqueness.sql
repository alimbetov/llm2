-- Align content chunk idempotency with src/persistence.rs:
-- store_v004_chunks uses ON CONFLICT(access_zone_id, document_id, document_version,
-- root_chunk_id, parent_chunk_id, granularity, representation_type, sequence_no).
--
-- This migration is intentionally idempotent because a previous failed runtime start
-- may have created content_chunks_v004_parent_aware_key before sqlx recorded the
-- migration as successful.

ALTER TABLE astravector.content_chunks_v004
  DROP CONSTRAINT IF EXISTS content_chunks_v004_access_zone_id_document_id_document_ver_key;

ALTER TABLE astravector.content_chunks_v004
  DROP CONSTRAINT IF EXISTS content_chunks_v004_parent_aware_key;

DROP INDEX IF EXISTS astravector.content_chunks_v004_parent_aware_key;

ALTER TABLE astravector.content_chunks_v004
  ADD CONSTRAINT content_chunks_v004_parent_aware_key
  UNIQUE NULLS NOT DISTINCT (
    access_zone_id,
    document_id,
    document_version,
    root_chunk_id,
    parent_chunk_id,
    granularity,
    representation_type,
    sequence_no
  );