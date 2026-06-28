-- AstraVector v004 requires PostgreSQL 15+
DO $$ BEGIN
  IF current_setting('server_version_num')::int < 150000 THEN
    RAISE EXCEPTION 'AstraVector v004 requires PostgreSQL 15+';
  END IF;
END $$;

CREATE TABLE IF NOT EXISTS astravector.document_versions (
  access_zone_id uuid NOT NULL,
  document_id uuid NOT NULL,
  document_version bigint NOT NULL CHECK(document_version > 0),
  content_hash char(64) NOT NULL,
  status varchar(32) NOT NULL DEFAULT 'REGISTERED' CHECK(status IN('REGISTERED','INDEXING','ACTIVE','STALE','SUPERSEDED','DELETION_PENDING','DELETED','FAILED')),
  activation_policy varchar(32) NOT NULL DEFAULT 'ACTIVE_LATEST_ONLY',
  created_at timestamptz NOT NULL DEFAULT now(), activated_at timestamptz, updated_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY(access_zone_id,document_id,document_version)
) PARTITION BY HASH(access_zone_id);

CREATE TABLE IF NOT EXISTS astravector.content_chunks_v004 (
  access_zone_id uuid NOT NULL,id uuid NOT NULL,root_chunk_id uuid NOT NULL,source_chunk_id uuid NOT NULL,parent_chunk_id uuid,
  document_id uuid NOT NULL,document_version bigint NOT NULL,granularity varchar(32) NOT NULL,representation_type varchar(32) NOT NULL,
  sequence_no integer NOT NULL,source_start_token integer,source_end_token integer,target_token_count integer,actual_token_count integer NOT NULL,
  overlap_before_tokens integer NOT NULL DEFAULT 0,overlap_after_tokens integer NOT NULL DEFAULT 0,content text NOT NULL,content_hash char(64) NOT NULL,
  tokenizer_version varchar(128) NOT NULL,chunking_profile_version varchar(128) NOT NULL,access_level smallint NOT NULL CHECK(access_level BETWEEN 1 AND 4),
  ttl_days integer CHECK(ttl_days IS NULL OR ttl_days>0),expires_at timestamptz,ttl_generation bigint NOT NULL DEFAULT 0,
  legal_hold boolean NOT NULL DEFAULT false,lifecycle_status varchar(32) NOT NULL DEFAULT 'ACTIVE',metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
  created_at timestamptz NOT NULL DEFAULT now(),updated_at timestamptz NOT NULL DEFAULT now(),deleted_at timestamptz,
  PRIMARY KEY(access_zone_id,id),
  UNIQUE NULLS NOT DISTINCT(access_zone_id,document_id,document_version,root_chunk_id,granularity,representation_type,sequence_no),
  CHECK(granularity IN('SOURCE','PARENT','SUB_180','SUB_260')),
  CHECK(representation_type IN('ORIGINAL','SUMMARY','SYNTHETIC_QUESTION','KEY_FACT','ENTITY','TERM','DEFINITION','FAQ'))
) PARTITION BY HASH(access_zone_id);

CREATE TABLE IF NOT EXISTS astravector.vector_bindings_v004 (
  access_zone_id uuid NOT NULL,id uuid NOT NULL,document_id uuid NOT NULL,document_version bigint NOT NULL,
  root_chunk_id uuid NOT NULL,source_chunk_id uuid NOT NULL,parent_chunk_id uuid,chunk_id uuid NOT NULL,
  chunk_granularity varchar(32) NOT NULL,representation_type varchar(32) NOT NULL,chunk_sequence_no integer,token_count integer,
  cache_entry_id uuid NOT NULL REFERENCES astravector.embedding_cache_entries(id),access_level smallint NOT NULL CHECK(access_level BETWEEN 1 AND 4),
  ttl_days integer CHECK(ttl_days IS NULL OR ttl_days>0),expires_at timestamptz,ttl_generation bigint NOT NULL DEFAULT 0,
  lifecycle_status varchar(32) NOT NULL DEFAULT 'ACTIVE',qdrant_sync_status varchar(32) NOT NULL DEFAULT 'PENDING',
  qdrant_collection varchar(128) NOT NULL,qdrant_point_id uuid NOT NULL,payload_version bigint NOT NULL DEFAULT 1,last_qdrant_sync_version bigint,
  legal_hold boolean NOT NULL DEFAULT false,legal_hold_reason text,legal_hold_until timestamptz,metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
  created_at timestamptz NOT NULL DEFAULT now(),updated_at timestamptz NOT NULL DEFAULT now(),expired_at timestamptz,deleted_at timestamptz,
  PRIMARY KEY(access_zone_id,id),
  FOREIGN KEY(access_zone_id,chunk_id) REFERENCES astravector.content_chunks_v004(access_zone_id,id),
  UNIQUE NULLS NOT DISTINCT(access_zone_id,document_id,document_version,chunk_id,representation_type),
  UNIQUE(access_zone_id,qdrant_point_id)
) PARTITION BY HASH(access_zone_id);

DO $$
DECLARE i int; suffix text;
BEGIN
 FOR i IN 0..31 LOOP
   suffix:=lpad(i::text,2,'0');
   EXECUTE format('CREATE TABLE IF NOT EXISTS astravector.document_versions_p%s PARTITION OF astravector.document_versions FOR VALUES WITH (MODULUS 32, REMAINDER %s)',suffix,i);
   EXECUTE format('CREATE TABLE IF NOT EXISTS astravector.content_chunks_p%s PARTITION OF astravector.content_chunks_v004 FOR VALUES WITH (MODULUS 32, REMAINDER %s)',suffix,i);
   EXECUTE format('CREATE TABLE IF NOT EXISTS astravector.vector_bindings_p%s PARTITION OF astravector.vector_bindings_v004 FOR VALUES WITH (MODULUS 32, REMAINDER %s)',suffix,i);
 END LOOP;
END $$;

CREATE INDEX IF NOT EXISTS idx_v004_chunks_root ON astravector.content_chunks_v004(access_zone_id,root_chunk_id);
CREATE INDEX IF NOT EXISTS idx_v004_chunks_document ON astravector.content_chunks_v004(access_zone_id,document_id,document_version);
CREATE INDEX IF NOT EXISTS idx_v004_bindings_document ON astravector.vector_bindings_v004(access_zone_id,document_id,document_version);
CREATE INDEX IF NOT EXISTS idx_v004_bindings_root ON astravector.vector_bindings_v004(access_zone_id,root_chunk_id);
CREATE INDEX IF NOT EXISTS idx_v004_bindings_cache ON astravector.vector_bindings_v004(cache_entry_id);
CREATE INDEX IF NOT EXISTS idx_v004_bindings_expiration ON astravector.vector_bindings_v004(expires_at,access_zone_id,id) WHERE lifecycle_status='ACTIVE' AND legal_hold=false AND expires_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_v004_bindings_qdrant_pending ON astravector.vector_bindings_v004(updated_at,access_zone_id,id) WHERE qdrant_sync_status IN('PENDING','UPDATE_PENDING','DELETE_PENDING','FAILED');
