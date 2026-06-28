CREATE TABLE IF NOT EXISTS astravector.reconciliation_runs(
 id uuid PRIMARY KEY,mode varchar(32) NOT NULL,access_zone_id uuid,collection_name varchar(128) NOT NULL,status varchar(32) NOT NULL,
 postgres_checkpoint jsonb,qdrant_checkpoint jsonb,scanned_postgres bigint NOT NULL DEFAULT 0,scanned_qdrant bigint NOT NULL DEFAULT 0,
 mismatch_count bigint NOT NULL DEFAULT 0,repair_count bigint NOT NULL DEFAULT 0,started_at timestamptz NOT NULL DEFAULT now(),updated_at timestamptz NOT NULL DEFAULT now(),completed_at timestamptz
);
CREATE TABLE IF NOT EXISTS astravector.reconciliation_findings(
 id uuid PRIMARY KEY,run_id uuid NOT NULL REFERENCES astravector.reconciliation_runs(id),access_zone_id uuid,binding_id uuid,qdrant_point_id uuid,
 finding_type varchar(64) NOT NULL,action varchar(64),action_status varchar(32),details jsonb NOT NULL DEFAULT '{}'::jsonb,created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS astravector.qdrant_quarantine(
 access_zone_id uuid NOT NULL,point_id uuid NOT NULL,binding_id uuid,reason varchar(128) NOT NULL,status varchar(32) NOT NULL DEFAULT 'QUARANTINED',
 payload jsonb NOT NULL DEFAULT '{}'::jsonb,quarantined_at timestamptz NOT NULL DEFAULT now(),resolved_at timestamptz,resolution varchar(32),resolved_by varchar(128),
 PRIMARY KEY(access_zone_id,point_id)
) PARTITION BY HASH(access_zone_id);
DO $$ DECLARE i int; s text; BEGIN FOR i IN 0..31 LOOP s:=lpad(i::text,2,'0'); EXECUTE format('CREATE TABLE IF NOT EXISTS astravector.qdrant_quarantine_p%s PARTITION OF astravector.qdrant_quarantine FOR VALUES WITH (MODULUS 32, REMAINDER %s)',s,i); END LOOP; END $$;
