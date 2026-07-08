-- AstraVector v007 fix3 GraphRAG Lite
-- Partitioned structural graph with TTL/access/freshness fields.

CREATE TABLE IF NOT EXISTS astravector.rag_graph_nodes (
    access_zone_id uuid NOT NULL,
    node_id uuid NOT NULL,
    node_type text NOT NULL,
    external_id text NOT NULL,
    document_id uuid,
    document_version bigint,
    chunk_id uuid,
    block_id text,
    label text,
    properties jsonb NOT NULL DEFAULT '{}'::jsonb,
    lifecycle_status text NOT NULL DEFAULT 'ACTIVE',
    expires_at timestamptz NULL,
    quarantined boolean NOT NULL DEFAULT false,
    access_level smallint NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (access_zone_id, node_type, node_id)
) PARTITION BY LIST (node_type);

CREATE TABLE IF NOT EXISTS astravector.rag_graph_nodes_document PARTITION OF astravector.rag_graph_nodes FOR VALUES IN ('DOCUMENT');
CREATE TABLE IF NOT EXISTS astravector.rag_graph_nodes_logical_block PARTITION OF astravector.rag_graph_nodes FOR VALUES IN ('LOGICAL_BLOCK');
CREATE TABLE IF NOT EXISTS astravector.rag_graph_nodes_chunk PARTITION OF astravector.rag_graph_nodes FOR VALUES IN ('CHUNK');
CREATE TABLE IF NOT EXISTS astravector.rag_graph_nodes_entity_tag PARTITION OF astravector.rag_graph_nodes FOR VALUES IN ('ENTITY_TAG');
CREATE TABLE IF NOT EXISTS astravector.rag_graph_nodes_default PARTITION OF astravector.rag_graph_nodes DEFAULT;

CREATE UNIQUE INDEX IF NOT EXISTS ux_rag_graph_nodes_document_external ON astravector.rag_graph_nodes_document(access_zone_id, external_id);
CREATE UNIQUE INDEX IF NOT EXISTS ux_rag_graph_nodes_logical_block_external ON astravector.rag_graph_nodes_logical_block(access_zone_id, external_id);
CREATE UNIQUE INDEX IF NOT EXISTS ux_rag_graph_nodes_chunk_external ON astravector.rag_graph_nodes_chunk(access_zone_id, external_id);
CREATE INDEX IF NOT EXISTS idx_rag_graph_nodes_chunk_lookup ON astravector.rag_graph_nodes_chunk(access_zone_id, chunk_id) WHERE lifecycle_status = 'ACTIVE' AND quarantined = false;
CREATE INDEX IF NOT EXISTS idx_rag_graph_nodes_block_lookup ON astravector.rag_graph_nodes_logical_block(access_zone_id, block_id) WHERE lifecycle_status = 'ACTIVE' AND quarantined = false;
CREATE INDEX IF NOT EXISTS idx_rag_graph_nodes_chunk_doc ON astravector.rag_graph_nodes_chunk(access_zone_id, document_id, document_version);
CREATE INDEX IF NOT EXISTS idx_rag_graph_nodes_block_doc ON astravector.rag_graph_nodes_logical_block(access_zone_id, document_id, document_version);

CREATE TABLE IF NOT EXISTS astravector.rag_graph_edges (
    access_zone_id uuid NOT NULL,
    edge_id uuid NOT NULL,
    source_node_type text NOT NULL,
    source_node_id uuid NOT NULL,
    target_node_type text NOT NULL,
    target_node_id uuid NOT NULL,
    relation_type text NOT NULL,
    relation_score real NOT NULL DEFAULT 1.0,
    relation_source text NOT NULL DEFAULT 'STRUCTURAL',
    relation_rank integer,
    document_id uuid,
    document_version bigint,
    lifecycle_status text NOT NULL DEFAULT 'ACTIVE',
    expires_at timestamptz NULL,
    quarantined boolean NOT NULL DEFAULT false,
    properties jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (access_zone_id, relation_type, edge_id)
) PARTITION BY LIST (relation_type);

CREATE TABLE IF NOT EXISTS astravector.rag_graph_edges_document_contains_block PARTITION OF astravector.rag_graph_edges FOR VALUES IN ('DOCUMENT_CONTAINS_BLOCK');
CREATE TABLE IF NOT EXISTS astravector.rag_graph_edges_block_contains_block PARTITION OF astravector.rag_graph_edges FOR VALUES IN ('BLOCK_CONTAINS_BLOCK');
CREATE TABLE IF NOT EXISTS astravector.rag_graph_edges_block_produced_chunk PARTITION OF astravector.rag_graph_edges FOR VALUES IN ('BLOCK_PRODUCED_CHUNK');
CREATE TABLE IF NOT EXISTS astravector.rag_graph_edges_chunk_produced_by_block PARTITION OF astravector.rag_graph_edges FOR VALUES IN ('CHUNK_PRODUCED_BY_BLOCK');
CREATE TABLE IF NOT EXISTS astravector.rag_graph_edges_chunk_has_parent PARTITION OF astravector.rag_graph_edges FOR VALUES IN ('CHUNK_HAS_PARENT');
CREATE TABLE IF NOT EXISTS astravector.rag_graph_edges_chunk_previous_sibling PARTITION OF astravector.rag_graph_edges FOR VALUES IN ('CHUNK_PREVIOUS_SIBLING');
CREATE TABLE IF NOT EXISTS astravector.rag_graph_edges_chunk_next_sibling PARTITION OF astravector.rag_graph_edges FOR VALUES IN ('CHUNK_NEXT_SIBLING');
CREATE TABLE IF NOT EXISTS astravector.rag_graph_edges_chunk_same_table PARTITION OF astravector.rag_graph_edges FOR VALUES IN ('CHUNK_SAME_TABLE');
CREATE TABLE IF NOT EXISTS astravector.rag_graph_edges_default PARTITION OF astravector.rag_graph_edges DEFAULT;

CREATE INDEX IF NOT EXISTS idx_edges_chunk_has_parent_source ON astravector.rag_graph_edges_chunk_has_parent(access_zone_id, source_node_id) WHERE lifecycle_status = 'ACTIVE' AND quarantined = false;
CREATE INDEX IF NOT EXISTS idx_edges_chunk_previous_sibling_source ON astravector.rag_graph_edges_chunk_previous_sibling(access_zone_id, source_node_id) WHERE lifecycle_status = 'ACTIVE' AND quarantined = false;
CREATE INDEX IF NOT EXISTS idx_edges_chunk_next_sibling_source ON astravector.rag_graph_edges_chunk_next_sibling(access_zone_id, source_node_id) WHERE lifecycle_status = 'ACTIVE' AND quarantined = false;
CREATE INDEX IF NOT EXISTS idx_edges_chunk_same_table_source ON astravector.rag_graph_edges_chunk_same_table(access_zone_id, source_node_id) WHERE lifecycle_status = 'ACTIVE' AND quarantined = false;
CREATE INDEX IF NOT EXISTS idx_edges_chunk_has_parent_doc ON astravector.rag_graph_edges_chunk_has_parent(access_zone_id, document_id, document_version);
CREATE INDEX IF NOT EXISTS idx_edges_chunk_previous_sibling_doc ON astravector.rag_graph_edges_chunk_previous_sibling(access_zone_id, document_id, document_version);
CREATE INDEX IF NOT EXISTS idx_edges_chunk_next_sibling_doc ON astravector.rag_graph_edges_chunk_next_sibling(access_zone_id, document_id, document_version);
CREATE INDEX IF NOT EXISTS idx_edges_chunk_same_table_doc ON astravector.rag_graph_edges_chunk_same_table(access_zone_id, document_id, document_version);
