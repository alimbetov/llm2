ALTER TABLE astravector.vector_outbox
  ADD COLUMN IF NOT EXISTS lock_generation bigint NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_vector_outbox_fencing_v004
  ON astravector.vector_outbox(id, locked_by, lock_generation)
  WHERE status='PROCESSING';
