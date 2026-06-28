SELECT 'content_chunks_v004_partitions', count(*)::text
FROM pg_inherits i JOIN pg_class p ON p.oid=i.inhparent JOIN pg_namespace n ON n.oid=p.relnamespace
WHERE n.nspname='astravector' AND p.relname='content_chunks_v004';
SELECT 'vector_bindings_v004_partitions', count(*)::text
FROM pg_inherits i JOIN pg_class p ON p.oid=i.inhparent JOIN pg_namespace n ON n.oid=p.relnamespace
WHERE n.nspname='astravector' AND p.relname='vector_bindings_v004';
