-- Read-only canonical Graph child/parent integrity check for Phase G.
WITH children AS (
  SELECT * FROM astravector.content_chunks_v004
  WHERE granularity IN ('SUB_180', 'SUB_260')
), same_zone_parent AS (
  SELECT child.access_zone_id, child.id child_id, parent.id parent_id,
         parent.document_id parent_document_id,
         parent.document_version parent_document_version
  FROM children child
  LEFT JOIN astravector.content_chunks_v004 parent
    ON parent.access_zone_id=child.access_zone_id
   AND parent.id=child.parent_chunk_id
   AND parent.granularity='PARENT'
), graph_endpoints AS (
  SELECT e.access_zone_id, e.edge_id, e.relation_type, e.relation_source,
         e.properties, source.chunk_id source_chunk_id,
         target.chunk_id target_chunk_id,
         source.access_zone_id source_zone_id,
         target.access_zone_id target_zone_id
  FROM astravector.rag_graph_edges e
  LEFT JOIN astravector.rag_graph_nodes_chunk source
    ON source.access_zone_id=e.access_zone_id AND source.node_id=e.source_node_id
  LEFT JOIN astravector.rag_graph_nodes_chunk target
    ON target.access_zone_id=e.access_zone_id AND target.node_id=e.target_node_id
  WHERE e.source_node_type='CHUNK'
    AND e.target_node_type='CHUNK'
)
SELECT json_build_object(
  'active_documents', (SELECT count(*) FROM astravector.document_versions WHERE status='ACTIVE'),
  'active_versions', (SELECT count(*) FROM astravector.document_versions WHERE status='ACTIVE'),
  'parent_chunks', (SELECT count(*) FROM astravector.content_chunks_v004 WHERE granularity='PARENT'),
  'child_chunks', (SELECT count(*) FROM children),
  'orphan_children', (SELECT count(*) FROM same_zone_parent WHERE parent_id IS NULL),
  'cross_document_bindings', (SELECT count(*) FROM children child JOIN same_zone_parent p ON p.access_zone_id=child.access_zone_id AND p.child_id=child.id WHERE p.parent_id IS NOT NULL AND p.parent_document_id<>child.document_id),
  'cross_version_bindings', (SELECT count(*) FROM children child JOIN same_zone_parent p ON p.access_zone_id=child.access_zone_id AND p.child_id=child.id WHERE p.parent_id IS NOT NULL AND p.parent_document_version<>child.document_version),
  'cross_zone_bindings', (SELECT count(*) FROM children child JOIN astravector.content_chunks_v004 parent ON parent.id=child.parent_chunk_id AND parent.access_zone_id<>child.access_zone_id),
  'duplicate_chunk_ids', (SELECT count(*) FROM (SELECT access_zone_id,id FROM astravector.content_chunks_v004 GROUP BY access_zone_id,id HAVING count(*)>1) d),
  'duplicate_source_provenance_rows', (SELECT count(*) FROM (SELECT access_zone_id,document_id,document_version,source_block_id,granularity,sequence_no FROM astravector.content_chunks_v004 GROUP BY access_zone_id,document_id,document_version,source_block_id,granularity,sequence_no HAVING count(*)>1) d),
  'bindings', (SELECT count(*) FROM astravector.vector_bindings_v004 WHERE chunk_granularity IN ('PARENT','SUB_180','SUB_260')),
  'synced_bindings', (SELECT count(*) FROM astravector.vector_bindings_v004 WHERE chunk_granularity IN ('PARENT','SUB_180','SUB_260') AND qdrant_sync_status='SYNCED'),
  'completed_outbox', (SELECT count(*) FROM astravector.vector_outbox WHERE operation='UPSERT_POINT' AND status='COMPLETED'),
  'dead_letters', (SELECT count(*) FROM astravector.vector_outbox WHERE status='DEAD_LETTER'),
  'graph_edges', (SELECT count(*) FROM graph_endpoints),
  'quality_fixture_edges', (SELECT count(*) FROM graph_endpoints WHERE relation_source='QUALITY_FIXTURE'),
  'repaired_by_edges', (SELECT count(*) FROM graph_endpoints WHERE relation_type='REPAIRED_BY'),
  'orphan_graph_endpoints', (SELECT count(*) FROM graph_endpoints WHERE source_chunk_id IS NULL OR target_chunk_id IS NULL),
  'cross_zone_graph_edges', (SELECT count(*) FROM graph_endpoints WHERE source_zone_id IS DISTINCT FROM access_zone_id OR target_zone_id IS DISTINCT FROM access_zone_id),
  'graph_self_edges', (SELECT count(*) FROM graph_endpoints WHERE source_chunk_id=target_chunk_id)
);
