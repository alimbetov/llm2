SELECT status, count(*)::text FROM astravector.vector_outbox GROUP BY status ORDER BY status;
