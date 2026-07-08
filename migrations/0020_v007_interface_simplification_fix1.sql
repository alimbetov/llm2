-- AstraVector v007 interface simplification fix1
-- Adds persistence foundation for LogicalBlock -> Chunk trace and async-correct delete state.

ALTER TABLE astravector.content_chunks_v004
  ADD COLUMN IF NOT EXISTS source_block_id text,
  ADD COLUMN IF NOT EXISTS source_location jsonb NOT NULL DEFAULT '{}'::jsonb,
  ADD COLUMN IF NOT EXISTS source_links jsonb NOT NULL DEFAULT '[]'::jsonb;

CREATE INDEX IF NOT EXISTS idx_v007_chunks_source_block
  ON astravector.content_chunks_v004(access_zone_id, document_id, document_version, source_block_id)
  WHERE source_block_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS astravector.logical_block_chunk_mapping (
  access_zone_id uuid NOT NULL,
  document_id uuid NOT NULL,
  document_version bigint NOT NULL CHECK(document_version > 0),
  block_id text NOT NULL,
  chunk_id uuid NOT NULL,
  relation_type text NOT NULL,
  source_location jsonb NOT NULL DEFAULT '{}'::jsonb,
  source_links jsonb NOT NULL DEFAULT '[]'::jsonb,
  created_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY(access_zone_id, document_id, document_version, block_id, chunk_id),
  FOREIGN KEY(access_zone_id, chunk_id) REFERENCES astravector.content_chunks_v004(access_zone_id, id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_v007_logical_block_mapping_chunk
  ON astravector.logical_block_chunk_mapping(access_zone_id, chunk_id);

CREATE INDEX IF NOT EXISTS idx_v007_logical_block_mapping_document
  ON astravector.logical_block_chunk_mapping(access_zone_id, document_id, document_version, block_id);
