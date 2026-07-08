-- GraphRAG Lite fix2 hardening patch: semantic relation, uniqueness, and expansion indexes.

CREATE TABLE IF NOT EXISTS astravector.rag_graph_edges_chunk_semantic_similar
PARTITION OF astravector.rag_graph_edges
FOR VALUES IN ('CHUNK_SEMANTIC_SIMILAR');

CREATE INDEX IF NOT EXISTS idx_edges_chunk_semantic_similar_source
ON astravector.rag_graph_edges_chunk_semantic_similar(access_zone_id, source_node_id)
WHERE lifecycle_status = 'ACTIVE' AND quarantined = false;

CREATE INDEX IF NOT EXISTS idx_edges_chunk_semantic_similar_doc
ON astravector.rag_graph_edges_chunk_semantic_similar(access_zone_id, document_id, document_version);

CREATE INDEX IF NOT EXISTS idx_rag_graph_edges_doc_source_relation
ON astravector.rag_graph_edges(access_zone_id, document_id, document_version, source_node_id, relation_type)
WHERE lifecycle_status = 'ACTIVE' AND quarantined = false;

CREATE UNIQUE INDEX IF NOT EXISTS uq_rag_graph_edges_unique
ON astravector.rag_graph_edges(
    access_zone_id,
    relation_type,
    document_id,
    document_version,
    source_node_id,
    target_node_id
);
