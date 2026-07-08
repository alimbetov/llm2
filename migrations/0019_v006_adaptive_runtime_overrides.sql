CREATE TABLE IF NOT EXISTS astravector.runtime_tuning_overrides (
    id BIGSERIAL PRIMARY KEY,
    parameter_name TEXT NOT NULL,
    parameter_value TEXT NOT NULL,
    previous_value TEXT NULL,
    scope TEXT NOT NULL DEFAULT 'GLOBAL',
    source TEXT NOT NULL,
    reason TEXT NOT NULL,
    status TEXT NOT NULL,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NULL,
    rollback_of BIGINT NULL REFERENCES astravector.runtime_tuning_overrides(id),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    CONSTRAINT runtime_tuning_overrides_status_chk CHECK (status IN ('ACTIVE','EXPIRED','ROLLED_BACK','REJECTED','DRY_RUN')),
    CONSTRAINT runtime_tuning_overrides_source_chk CHECK (source IN ('AUTO_SAFE','MANUAL','ROLLBACK','DRY_RUN'))
);

CREATE INDEX IF NOT EXISTS idx_runtime_tuning_overrides_active
    ON astravector.runtime_tuning_overrides(parameter_name, scope, status, expires_at)
    WHERE status = 'ACTIVE';

CREATE INDEX IF NOT EXISTS idx_runtime_tuning_overrides_applied_at
    ON astravector.runtime_tuning_overrides(applied_at DESC);
