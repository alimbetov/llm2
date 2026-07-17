\set ON_ERROR_STOP on
WITH zone AS (
  SELECT access_zone_id FROM astravector.access_zones WHERE access_zone_code = '4861'
)
SELECT jsonb_pretty(jsonb_build_object(
  'access_zones', (SELECT count(*) FROM zone),
  'documents', (SELECT count(*) FROM astravector.document_versions WHERE access_zone_id IN (SELECT access_zone_id FROM zone)),
  'active_documents', (SELECT count(*) FROM astravector.document_versions WHERE access_zone_id IN (SELECT access_zone_id FROM zone) AND status='ACTIVE'),
  'chunks', (SELECT count(*) FROM astravector.content_chunks_v004 WHERE access_zone_id IN (SELECT access_zone_id FROM zone)),
  'source_chunks', (SELECT count(*) FROM astravector.content_chunks_v004 WHERE access_zone_id IN (SELECT access_zone_id FROM zone) AND granularity='SOURCE'),
  'parent_chunks', (SELECT count(*) FROM astravector.content_chunks_v004 WHERE access_zone_id IN (SELECT access_zone_id FROM zone) AND granularity='PARENT'),
  'child_chunks', (SELECT count(*) FROM astravector.content_chunks_v004 WHERE access_zone_id IN (SELECT access_zone_id FROM zone) AND granularity IN ('SUB_180','SUB_260')),
  'orphan_children', (SELECT count(*) FROM astravector.content_chunks_v004 c WHERE c.access_zone_id IN (SELECT access_zone_id FROM zone) AND c.granularity IN ('SUB_180','SUB_260') AND NOT EXISTS (SELECT 1 FROM astravector.content_chunks_v004 p WHERE p.access_zone_id=c.access_zone_id AND p.id=c.parent_chunk_id AND p.granularity='PARENT')),
  'bindings', (SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id IN (SELECT access_zone_id FROM zone)),
  'synced_bindings', (SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id IN (SELECT access_zone_id FROM zone) AND qdrant_sync_status='SYNCED'),
  'completed_outbox', (SELECT count(*) FROM astravector.vector_outbox o WHERE o.binding_access_zone_id IN (SELECT access_zone_id FROM zone) AND o.status='COMPLETED'),
  'dead_letter_outbox', (SELECT count(*) FROM astravector.vector_outbox o WHERE o.binding_access_zone_id IN (SELECT access_zone_id FROM zone) AND o.status='DEAD_LETTER'),
  'duplicate_chunks', (SELECT count(*) FROM (SELECT granularity,representation_type,sequence_no,count(*) FROM astravector.content_chunks_v004 WHERE access_zone_id IN (SELECT access_zone_id FROM zone) GROUP BY granularity,representation_type,sequence_no HAVING count(*)>1) d),
  'duplicate_bindings', (SELECT count(*) FROM (SELECT chunk_id,representation_type,count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id IN (SELECT access_zone_id FROM zone) GROUP BY chunk_id,representation_type HAVING count(*)>1) d),
  'duplicate_outbox_effects', (SELECT count(*) FROM (SELECT binding_access_zone_id,binding_id,operation,operation_version,count(*) FROM astravector.vector_outbox WHERE binding_access_zone_id IN (SELECT access_zone_id FROM zone) GROUP BY binding_access_zone_id,binding_id,operation,operation_version HAVING count(*)>1) d)
))) AS audit;

SELECT jsonb_agg(row_to_json(x) ORDER BY x.granularity, x.sequence_no)
FROM (
  SELECT c.id::text AS chunk_id, c.root_chunk_id::text, c.source_chunk_id::text,
         c.parent_chunk_id::text, c.document_id::text, c.document_version,
         c.granularity, c.sequence_no, c.content_hash::text
  FROM astravector.content_chunks_v004 c
  JOIN astravector.access_zones z USING(access_zone_id)
  WHERE z.access_zone_code='4861'
) x;
