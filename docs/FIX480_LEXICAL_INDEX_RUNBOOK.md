# fix480 Parent Lexical Index

The SQLx migration creates the partitioned GIN index transactionally for local and test
environments. On a production-sized PostgreSQL cluster, build matching indexes on every
`content_chunks_v004` child partition with `CREATE INDEX CONCURRENTLY`, then attach them to
the partitioned parent index during a controlled maintenance change.

Rollback disables the runtime path with `ASTRAVECTOR_LEXICAL_SEARCH_ENABLED=false`. Dense
and sparse Qdrant retrieval remain available. The retired latest-updated parent scan is not
enabled as a fallback. Dropping the generated column requires a separate reviewed migration.
