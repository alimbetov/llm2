# AstraVector Configuration — fix462 operator notes

## Index TTL rollback controls

```yaml
index_ttl:
  enabled: true
  cleanup_enabled: true
  qdrant_reconciliation_enabled: true
```

- `index_ttl.enabled=false` disables the full index TTL subsystem.
- `index_ttl.cleanup_enabled=false` stops TTL cleanup workers while leaving ingestion/search online.
- `index_ttl.qdrant_reconciliation_enabled=false` disables Qdrant extra-point reconciliation and deletes only PostgreSQL-expected binding points. Use this as a rollback switch if legal-hold reconciliation behavior is suspected.

Environment variables:

```bash
ASTRAVECTOR_INDEX_TTL_ENABLED=true
ASTRAVECTOR_INDEX_TTL_CLEANUP_ENABLED=true
ASTRAVECTOR_INDEX_TTL_QDRANT_RECONCILIATION_ENABLED=true
```

## GraphRAG and MMR rollback controls

```yaml
graph_rag:
  retrieval:
    enabled_by_default: true
  rerank:
    mmr_enabled: true
```

Operational fallback:

```bash
ASTRAVECTOR_GRAPH_RETRIEVAL_ENABLED_BY_DEFAULT=false
ASTRAVECTOR_GRAPH_MMR_ENABLED=false
```

Use these only as emergency degradation controls. They reduce context quality but keep basic vector retrieval available.

## Lifecycle fencing

`document_versions.delete_operation_id` is a transient cleanup fencing token. Normal lifecycle updates must not overwrite rows where this value is not null. TTL cleanup finalization must match the exact token that started the delete operation.

## Delete error stage

`document_versions.last_delete_error_stage` records the failing stage, for example:

- `QDRANT_SCROLL`
- `QDRANT_DELETE`
- `POSTGRES_FINALIZE`
- `VECTOR_BINDINGS_UPDATE`
- `TOMBSTONE_PURGE`



## Configuration profiles

AstraVector supports Spring-like profile overlays: `config/application.yaml`, `config/application-dev.yaml`, `config/application-test.yaml`, `config/application-prod.yaml`. See `docs/CONFIG_PROFILES.md`.
