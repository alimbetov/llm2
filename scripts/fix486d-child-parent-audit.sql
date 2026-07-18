-- Read-only canonical child/parent integrity check for Phase D.
WITH children AS (
  SELECT * FROM astravector.content_chunks_v004 WHERE granularity IN ('SUB_180', 'SUB_260')
)
SELECT json_build_object(
  'orphan_children', count(*) FILTER (WHERE parent.id IS NULL),
  'cross_document_bindings', count(*) FILTER (WHERE parent.id IS NOT NULL AND parent.document_id <> child.document_id),
  'cross_version_bindings', count(*) FILTER (WHERE parent.id IS NOT NULL AND parent.document_version <> child.document_version)
)
FROM children child
LEFT JOIN astravector.content_chunks_v004 parent
  ON parent.access_zone_id = child.access_zone_id AND parent.id = child.parent_chunk_id;
