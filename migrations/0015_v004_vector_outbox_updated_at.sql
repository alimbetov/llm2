ALTER TABLE astravector.vector_outbox
  ADD COLUMN IF NOT EXISTS updated_at timestamptz NOT NULL DEFAULT now();

UPDATE astravector.vector_outbox
SET updated_at = COALESCE(completed_at, created_at, now())
WHERE updated_at IS NULL;
