CREATE TABLE IF NOT EXISTS astravector.search_representations (
    id uuid PRIMARY KEY,
    tenant_id text NOT NULL,
    workspace_id text NOT NULL,
    document_id uuid,
    document_version bigint,
    source_chunk_id uuid NOT NULL,
    representation_type varchar(32) NOT NULL,
    representation_text text NOT NULL,
    representation_hash char(64) NOT NULL,
    cache_entry_id uuid REFERENCES astravector.embedding_cache_entries(id),
    access_level smallint NOT NULL,
    ttl_days integer,
    expires_at timestamptz,
    generation_model varchar(128),
    generation_prompt_version varchar(64),
    validation_status varchar(32) NOT NULL DEFAULT 'GENERATED',
    lifecycle_status varchar(32) NOT NULL DEFAULT 'ACTIVE',
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT chk_search_representation_type CHECK(representation_type IN ('ORIGINAL','SUMMARY','SYNTHETIC_QUESTION','KEY_FACT','ENTITY','TERM','DEFINITION','FAQ')),
    CONSTRAINT chk_search_representation_validation CHECK(validation_status IN ('GENERATED','VALIDATED','REJECTED','STALE')),
    UNIQUE(tenant_id,workspace_id,source_chunk_id,representation_type,representation_hash)
);
CREATE INDEX IF NOT EXISTS idx_search_rep_source ON astravector.search_representations(tenant_id,workspace_id,source_chunk_id);

CREATE TABLE IF NOT EXISTS astravector.relevance_evaluations (
    id uuid PRIMARY KEY,
    tenant_id text NOT NULL,
    workspace_id text NOT NULL,
    evaluation_type varchar(32) NOT NULL,
    question_hash char(64),
    question_text text,
    source_chunk_id uuid,
    answer_id uuid,
    dense_score real,
    sparse_score real,
    fused_score real,
    reranker_score real,
    consistency_score real,
    final_score real,
    decision varchar(32) NOT NULL,
    evaluator_type varchar(32) NOT NULL,
    evaluator_version varchar(128),
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_relevance_scope_created ON astravector.relevance_evaluations(tenant_id,workspace_id,created_at DESC);

CREATE TABLE IF NOT EXISTS astravector.relevance_feedback (
    id uuid PRIMARY KEY,
    evaluation_id uuid REFERENCES astravector.relevance_evaluations(id) ON DELETE SET NULL,
    feedback_type varchar(32) NOT NULL,
    rating smallint,
    accepted boolean,
    actor_type varchar(32),
    actor_id varchar(128),
    comment text,
    created_at timestamptz NOT NULL DEFAULT now()
);
