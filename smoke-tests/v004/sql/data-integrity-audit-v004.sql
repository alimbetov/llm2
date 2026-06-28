SELECT 'bindings_without_chunk' AS check_name, count(*)::bigint AS violations
FROM astravector.vector_bindings_v004 vb
WHERE NOT EXISTS (
  SELECT 1 FROM astravector.content_chunks_v004 c
  WHERE c.id = vb.chunk_id AND c.access_zone_id = vb.access_zone_id
);

SELECT 'active_searchable_chunks_without_document_version', count(*)::bigint
FROM astravector.content_chunks_v004 c
WHERE c.lifecycle_status='ACTIVE'
AND NOT EXISTS (
  SELECT 1 FROM astravector.document_versions dv
  WHERE dv.document_id=c.document_id AND dv.document_version=c.document_version AND dv.access_zone_id=c.access_zone_id
);

SELECT 'synced_bindings_without_completed_outbox', count(*)::bigint
FROM astravector.vector_bindings_v004 vb
WHERE vb.qdrant_sync_status='SYNCED'
AND NOT EXISTS (
  SELECT 1 FROM astravector.vector_outbox o
  WHERE o.binding_access_zone_id=vb.access_zone_id
    AND o.binding_id=vb.id
    AND o.status='COMPLETED'
    AND o.operation='UPSERT_POINT'
    AND o.operation_version=vb.last_qdrant_sync_version
);

SELECT 'active_document_without_searchable_synced_bindings', count(*)::bigint
FROM astravector.document_versions dv
WHERE dv.status='ACTIVE'
AND NOT EXISTS (
  SELECT 1 FROM astravector.vector_bindings_v004 vb
  WHERE vb.access_zone_id=dv.access_zone_id
    AND vb.document_id=dv.document_id
    AND vb.document_version=dv.document_version
    AND vb.lifecycle_status='ACTIVE'
    AND vb.qdrant_sync_status='SYNCED'
    AND vb.chunk_granularity IN('PARENT','SUB_180','SUB_260')
);

SELECT 'orphan_parent_chunk_id', count(*)::bigint
FROM astravector.content_chunks_v004 c
WHERE c.parent_chunk_id IS NOT NULL
AND NOT EXISTS (
  SELECT 1 FROM astravector.content_chunks_v004 p
  WHERE p.id=c.parent_chunk_id AND p.access_zone_id=c.access_zone_id
);

SELECT 'orphan_source_chunk_id', count(*)::bigint
FROM astravector.content_chunks_v004 c
WHERE c.source_chunk_id IS NOT NULL
AND NOT EXISTS (
  SELECT 1 FROM astravector.content_chunks_v004 s
  WHERE s.id=c.source_chunk_id AND s.access_zone_id=c.access_zone_id
);

SELECT 'duplicate_searchable_binding_logical_keys', count(*)::bigint
FROM (
  SELECT access_zone_id,document_id,document_version,chunk_id,representation_type,count(*)
  FROM astravector.vector_bindings_v004
  GROUP BY access_zone_id,document_id,document_version,chunk_id,representation_type
  HAVING count(*) > 1
) d;

SELECT 'permanently_processing_outbox_events', count(*)::bigint
FROM astravector.vector_outbox
WHERE status='PROCESSING' AND updated_at < now() - interval '5 minutes';

SELECT 'failed_dead_letter_events_for_active_documents', count(*)::bigint
FROM astravector.vector_outbox o
JOIN astravector.vector_bindings_v004 vb
  ON vb.access_zone_id=o.binding_access_zone_id AND vb.id=o.binding_id
JOIN astravector.document_versions dv
  ON dv.access_zone_id=vb.access_zone_id
 AND dv.document_id=vb.document_id
 AND dv.document_version=vb.document_version
WHERE dv.status='ACTIVE' AND o.status IN ('FAILED','DEAD_LETTER');

SELECT 'active_expired_chunks_without_legal_hold', count(*)::bigint
FROM astravector.content_chunks_v004
WHERE lifecycle_status='ACTIVE'
  AND expires_at IS NOT NULL
  AND expires_at < now()
  AND coalesce(legal_hold,false)=false;
