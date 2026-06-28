SELECT lifecycle_status, qdrant_sync_status, count(*)::text
FROM astravector.vector_bindings_v004
GROUP BY lifecycle_status, qdrant_sync_status
ORDER BY lifecycle_status, qdrant_sync_status;
