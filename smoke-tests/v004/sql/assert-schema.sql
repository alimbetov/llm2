SELECT 'schema_exists', count(*)::text FROM information_schema.schemata WHERE schema_name='astravector';
SELECT 'pgvector_extension', count(*)::text FROM pg_extension WHERE extname='vector';
SELECT 'document_versions', count(*)::text FROM information_schema.tables WHERE table_schema='astravector' AND table_name='document_versions';
SELECT 'content_chunks_v004', count(*)::text FROM information_schema.tables WHERE table_schema='astravector' AND table_name='content_chunks_v004';
SELECT 'vector_bindings_v004', count(*)::text FROM information_schema.tables WHERE table_schema='astravector' AND table_name='vector_bindings_v004';
SELECT 'vector_outbox', count(*)::text FROM information_schema.tables WHERE table_schema='astravector' AND table_name='vector_outbox';
