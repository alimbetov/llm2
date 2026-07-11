-- Keep reverse one-hop graph expansion bounded as historical edges accumulate.
CREATE INDEX IF NOT EXISTS ix_rag_graph_edges_zone_target_active_v477
ON astravector.rag_graph_edges(access_zone_id, target_node_id, relation_type, relation_score DESC)
WHERE lifecycle_status='ACTIVE' AND quarantined=false;
