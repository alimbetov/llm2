CREATE TABLE IF NOT EXISTS astravector.enrichment_jobs(
 id uuid PRIMARY KEY,access_zone_id uuid NOT NULL,root_chunk_id uuid NOT NULL,source_chunk_id uuid NOT NULL,status varchar(32) NOT NULL DEFAULT 'PENDING',
 provider_type varchar(32) NOT NULL,provider_version varchar(128),prompt_version varchar(64),attempt_count integer NOT NULL DEFAULT 0,next_attempt_at timestamptz NOT NULL DEFAULT now(),
 locked_by varchar(128),locked_until timestamptz,error_code varchar(64),error_message text,created_at timestamptz NOT NULL DEFAULT now(),updated_at timestamptz NOT NULL DEFAULT now(),completed_at timestamptz,
 UNIQUE(access_zone_id,root_chunk_id,source_chunk_id,provider_type,prompt_version)
);
ALTER TABLE astravector.relevance_evaluations ADD COLUMN IF NOT EXISTS access_zone_id uuid;
ALTER TABLE astravector.relevance_evaluations ADD COLUMN IF NOT EXISTS root_chunk_id uuid;
ALTER TABLE astravector.relevance_evaluations ADD COLUMN IF NOT EXISTS parent_chunk_id uuid;
ALTER TABLE astravector.relevance_evaluations ADD COLUMN IF NOT EXISTS sparse_score real;
ALTER TABLE astravector.relevance_evaluations ADD COLUMN IF NOT EXISTS lexical_score real;
ALTER TABLE astravector.relevance_evaluations ADD COLUMN IF NOT EXISTS fused_score real;
ALTER TABLE astravector.relevance_evaluations ADD COLUMN IF NOT EXISTS reranker_score real;
ALTER TABLE astravector.relevance_evaluations ADD COLUMN IF NOT EXISTS evaluator_version varchar(128);
CREATE TABLE IF NOT EXISTS astravector.relevance_feedback_v004(
 id uuid PRIMARY KEY,access_zone_id uuid NOT NULL,evaluation_id uuid NOT NULL REFERENCES astravector.relevance_evaluations(id),feedback_type varchar(64) NOT NULL,
 actor_hash varchar(128),comment text,created_at timestamptz NOT NULL DEFAULT now(),updated_at timestamptz NOT NULL DEFAULT now(),
 UNIQUE(access_zone_id,evaluation_id,actor_hash)
);
