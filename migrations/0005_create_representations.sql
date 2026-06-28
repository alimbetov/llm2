CREATE TABLE IF NOT EXISTS astravector.embedding_dense (
    id uuid PRIMARY KEY,
    cache_entry_id uuid NOT NULL
        REFERENCES astravector.embedding_cache_entries(id) ON DELETE CASCADE,
    representation_name varchar(64) NOT NULL,
    representation_version varchar(128) NOT NULL,
    dimension integer NOT NULL,
    normalized boolean NOT NULL,
    distance varchar(32) NOT NULL,
    vector_value vector(1024) NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT uq_embedding_dense_cache UNIQUE (
        cache_entry_id, representation_name, representation_version
    )
);

CREATE TABLE IF NOT EXISTS astravector.embedding_sparse (
    id uuid PRIMARY KEY,
    cache_entry_id uuid NOT NULL
        REFERENCES astravector.embedding_cache_entries(id) ON DELETE CASCADE,
    representation_name varchar(64) NOT NULL,
    representation_version varchar(128) NOT NULL,
    indices integer[] NOT NULL,
    values real[] NOT NULL,
    non_zero_count integer NOT NULL,
    min_weight real,
    max_non_zero integer,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT uq_embedding_sparse_cache UNIQUE (
        cache_entry_id, representation_name, representation_version
    ),
    CONSTRAINT chk_sparse_lengths CHECK (
        cardinality(indices) = cardinality(values)
        AND cardinality(indices) = non_zero_count
    )
);
