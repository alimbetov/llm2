CREATE TABLE IF NOT EXISTS astravector.vector_bindings (
    id uuid PRIMARY KEY,
    tenant_id text NOT NULL,
    workspace_id text NOT NULL,
    document_id uuid,
    document_version bigint,
    chunk_id uuid NOT NULL,
    chunk_type smallint NOT NULL,
    parent_chunk_id uuid,
    source_chunk_id uuid,
    representation_type varchar(32) NOT NULL DEFAULT 'ORIGINAL',
    cache_entry_id uuid NOT NULL REFERENCES astravector.embedding_cache_entries(id) ON DELETE RESTRICT,
    access_level smallint NOT NULL,
    ttl_days integer,
    expires_at timestamptz,
    lifecycle_status varchar(32) NOT NULL DEFAULT 'ACTIVE',
    qdrant_sync_status varchar(32) NOT NULL DEFAULT 'PENDING',
    qdrant_collection varchar(128),
    qdrant_point_id uuid NOT NULL,
    payload_version bigint NOT NULL DEFAULT 1,
    legal_hold boolean NOT NULL DEFAULT false,
    legal_hold_reason text,
    legal_hold_until timestamptz,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    expired_at timestamptz,
    deleted_at timestamptz,
    CONSTRAINT chk_vector_bindings_access_level CHECK (access_level BETWEEN 1 AND 4),
    CONSTRAINT chk_vector_bindings_ttl CHECK (ttl_days IS NULL OR ttl_days > 0),
    CONSTRAINT chk_vector_bindings_lifecycle CHECK (lifecycle_status IN ('ACTIVE','EXPIRED','DELETION_PENDING','SOFT_DELETED','PURGE_PENDING','DELETED','LEGAL_HOLD','DELETE_FAILED')),
    CONSTRAINT chk_vector_bindings_qdrant_status CHECK (qdrant_sync_status IN ('PENDING','SYNCED','UPDATE_PENDING','DELETE_PENDING','DELETED','FAILED','DEAD_LETTER')),
    UNIQUE (tenant_id,workspace_id,document_id,document_version,chunk_id,representation_type)
);
CREATE INDEX IF NOT EXISTS idx_vector_bindings_active_expiration ON astravector.vector_bindings(expires_at) WHERE lifecycle_status='ACTIVE' AND legal_hold=false;
CREATE INDEX IF NOT EXISTS idx_vector_bindings_document ON astravector.vector_bindings(tenant_id,workspace_id,document_id,document_version);
CREATE INDEX IF NOT EXISTS idx_vector_bindings_cache_entry ON astravector.vector_bindings(cache_entry_id);
CREATE INDEX IF NOT EXISTS idx_vector_bindings_qdrant_status ON astravector.vector_bindings(qdrant_sync_status,updated_at);

CREATE TABLE IF NOT EXISTS astravector.vector_outbox (
    id uuid PRIMARY KEY,
    binding_id uuid NOT NULL REFERENCES astravector.vector_bindings(id) ON DELETE CASCADE,
    operation varchar(32) NOT NULL,
    status varchar(32) NOT NULL DEFAULT 'PENDING',
    attempt_count integer NOT NULL DEFAULT 0,
    next_attempt_at timestamptz NOT NULL DEFAULT now(),
    locked_by varchar(128),
    locked_until timestamptz,
    error_code varchar(64),
    error_message text,
    created_at timestamptz NOT NULL DEFAULT now(),
    completed_at timestamptz,
    CONSTRAINT chk_vector_outbox_operation CHECK(operation IN ('UPSERT_POINT','UPDATE_PAYLOAD','DELETE_POINT')),
    CONSTRAINT chk_vector_outbox_status CHECK(status IN ('PENDING','PROCESSING','RETRY_PENDING','COMPLETED','DEAD_LETTER'))
);
CREATE INDEX IF NOT EXISTS idx_vector_outbox_due ON astravector.vector_outbox(status,next_attempt_at,created_at) WHERE status IN ('PENDING','RETRY_PENDING');
