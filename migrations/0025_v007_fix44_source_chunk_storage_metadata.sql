-- AstraVector v007 fix4.4-rev2: reserved additive migration for SOURCE chunk storage mode metadata.
-- Existing chunk metadata JSON can store has_full_text/content_hash/original lengths without schema rewrite.
-- This migration intentionally remains additive and documents the schema version boundary.
CREATE TABLE IF NOT EXISTS astravector.runtime_feature_flags_v004 (
    feature_name TEXT PRIMARY KEY,
    feature_version TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT false,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO astravector.runtime_feature_flags_v004(feature_name, feature_version, enabled, metadata)
VALUES ('source_chunk_storage_mode', 'fix4.4-rev2', true, jsonb_build_object('default', 'METADATA_ONLY'))
ON CONFLICT (feature_name) DO UPDATE SET feature_version=EXCLUDED.feature_version, enabled=EXCLUDED.enabled, metadata=EXCLUDED.metadata, updated_at=now();
