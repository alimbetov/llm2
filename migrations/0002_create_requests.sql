CREATE TABLE IF NOT EXISTS astravector.embedding_requests (
    id uuid PRIMARY KEY,
    emb_task_id uuid NOT NULL,
    correlation_id uuid NOT NULL UNIQUE,
    idempotency_key varchar(256),
    tenant_id varchar(128) NOT NULL,
    workspace_id varchar(128) NOT NULL,
    caller_service varchar(100) NOT NULL,
    purpose varchar(32) NOT NULL,
    access_level varchar(32) NOT NULL,
    persistence_mode varchar(32) NOT NULL,
    requested_representations varchar(64)[] NOT NULL,
    request_hash varchar(64) NOT NULL,
    status varchar(32) NOT NULL,
    item_count integer NOT NULL,
    contract_version varchar(128) NOT NULL,
    tokenizer_version varchar(128) NOT NULL,
    model_version varchar(128) NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    started_at timestamptz,
    completed_at timestamptz,
    error_code varchar(64),
    error_message text
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_embedding_requests_idempotency
    ON astravector.embedding_requests (tenant_id, workspace_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
