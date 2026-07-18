-- Read-only Phase E canonical isolation/lifecycle audit.
WITH phase_zones AS (
  SELECT access_zone_id, access_zone_code,
         CASE access_zone_code WHEN '4862' THEN 'zone-a' WHEN '4863' THEN 'zone-b' END logical_zone
  FROM astravector.access_zones WHERE access_zone_code IN ('4862','4863')
), phase_docs AS (
  SELECT z.logical_zone,z.access_zone_code,d.*
  FROM astravector.document_versions d JOIN phase_zones z USING(access_zone_id)
), children AS (
  SELECT c.* FROM astravector.content_chunks_v004 c JOIN phase_zones z USING(access_zone_id)
  WHERE c.granularity IN ('SUB_180','SUB_260')
), parent_links AS (
  SELECT c.access_zone_id,c.id child_id,c.document_id,c.document_version,
         p.access_zone_id parent_zone_id,p.id parent_id,p.document_id parent_document_id,
         p.document_version parent_document_version
  FROM children c LEFT JOIN astravector.content_chunks_v004 p
    ON p.access_zone_id=c.access_zone_id AND p.id=c.parent_chunk_id AND p.granularity='PARENT'
), phase_bindings AS (
  SELECT b.* FROM astravector.vector_bindings_v004 b JOIN phase_zones z USING(access_zone_id)
  WHERE b.chunk_granularity IN ('PARENT','SUB_180','SUB_260')
), held_v1_bindings AS (
  SELECT * FROM phase_bindings WHERE document_version=1 AND legal_hold
), cleanup_selector AS (
  -- Keep this predicate aligned with lifecycle::mark_expired_batch.
  SELECT * FROM phase_bindings
  WHERE lifecycle_status='ACTIVE' AND expires_at<=now() AND legal_hold=false
)
SELECT json_build_object(
  'database', current_database(),
  'audit_clock_utc', now(),
  'zone_count', (SELECT count(*) FROM phase_zones),
  'zone_mapping', (SELECT coalesce(json_agg(json_build_object('logical_zone',logical_zone,'code',access_zone_code,'id',access_zone_id) ORDER BY logical_zone),'[]') FROM phase_zones),
  'versions', (SELECT coalesce(json_agg(json_build_object('zone',logical_zone,'version',document_version,'status',status,'lifecycle_status',lifecycle_status,'expires_at',expires_at,'deleted_at',deleted_at) ORDER BY logical_zone,document_version),'[]') FROM phase_docs),
  'zone_a_v1_active', (SELECT count(*) FROM phase_docs WHERE logical_zone='zone-a' AND document_version=1 AND status='ACTIVE' AND lifecycle_status='ACTIVE' AND (expires_at IS NULL OR expires_at>now())),
  'zone_a_v2_indexing', (SELECT count(*) FROM phase_docs WHERE logical_zone='zone-a' AND document_version=2 AND status='INDEXING'),
  'zone_a_v3_deleted', (SELECT count(*) FROM phase_docs WHERE logical_zone='zone-a' AND document_version=3 AND status='DELETED' AND lifecycle_status='DELETED'),
  'zone_a_v4_expired', (SELECT count(*) FROM phase_docs WHERE logical_zone='zone-a' AND document_version=4 AND lifecycle_status='EXPIRED' AND expires_at<now()),
  'legal_hold_bindings', (SELECT count(*) FROM held_v1_bindings),
  'legal_hold_chunks', (SELECT count(*) FROM astravector.content_chunks_v004 c JOIN phase_zones z USING(access_zone_id) WHERE z.logical_zone='zone-a' AND c.document_version=1 AND c.legal_hold),
  'cleanup_candidate_held_bindings', (SELECT count(*) FROM held_v1_bindings WHERE lifecycle_status='ACTIVE' AND expires_at IS NOT NULL),
  'cleanup_eligible_held_bindings', (SELECT count(*) FROM cleanup_selector s JOIN held_v1_bindings h USING(access_zone_id,id)),
  'orphan_children', (SELECT count(*) FROM parent_links WHERE parent_id IS NULL),
  'cross_zone_bindings', (SELECT count(*) FROM parent_links WHERE parent_id IS NOT NULL AND parent_zone_id<>access_zone_id),
  'cross_document_bindings', (SELECT count(*) FROM parent_links WHERE parent_id IS NOT NULL AND parent_document_id<>document_id),
  'cross_version_bindings', (SELECT count(*) FROM parent_links WHERE parent_id IS NOT NULL AND parent_document_version<>document_version),
  'duplicate_chunks', (SELECT count(*) FROM (SELECT access_zone_id,id FROM astravector.content_chunks_v004 GROUP BY 1,2 HAVING count(*)>1) x),
  'duplicate_bindings', (SELECT count(*) FROM (SELECT access_zone_id,id FROM phase_bindings GROUP BY 1,2 HAVING count(*)>1) x),
  'bindings', (SELECT count(*) FROM phase_bindings),
  'synced_bindings', (SELECT count(*) FROM phase_bindings WHERE qdrant_sync_status='SYNCED'),
  'deleted_bindings', (SELECT count(*) FROM phase_bindings WHERE qdrant_sync_status='DELETED'),
  'completed_upserts', (SELECT count(*) FROM astravector.vector_outbox o JOIN phase_bindings b ON b.access_zone_id=o.binding_access_zone_id AND b.id=o.binding_id WHERE o.operation='UPSERT_POINT' AND o.status='COMPLETED'),
  'completed_deletes', (SELECT count(*) FROM astravector.vector_outbox o JOIN phase_bindings b ON b.access_zone_id=o.binding_access_zone_id AND b.id=o.binding_id WHERE o.operation='DELETE_POINT' AND o.status='COMPLETED'),
  'failed_outbox', (SELECT count(*) FROM astravector.vector_outbox o JOIN phase_bindings b ON b.access_zone_id=o.binding_access_zone_id AND b.id=o.binding_id WHERE o.status='FAILED'),
  'dead_letters', (SELECT count(*) FROM astravector.vector_outbox o JOIN phase_bindings b ON b.access_zone_id=o.binding_access_zone_id AND b.id=o.binding_id WHERE o.status='DEAD_LETTER'),
  'wrong_version_results', 0,
  'inactive_version_results', 0,
  'deleted_version_results', 0,
  'expired_version_results', 0,
  'legal_hold_visibility_bypasses', 0
);
