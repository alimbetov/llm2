SELECT 'bindings_without_chunks', count(*)::text
FROM astravector.vector_bindings_v004 b
LEFT JOIN astravector.content_chunks_v004 c ON c.access_zone_id=b.access_zone_id AND c.id=b.chunk_id
WHERE c.id IS NULL;
SELECT 'bindings_without_cache', count(*)::text
FROM astravector.vector_bindings_v004 b
LEFT JOIN astravector.embedding_cache_entries c ON c.id=b.cache_entry_id
WHERE c.id IS NULL;
