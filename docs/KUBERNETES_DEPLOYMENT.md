# Kubernetes deployment notes for fix463

## Required runtime config

Production runtime requires Qdrant when index TTL cleanup is enabled:

```yaml
ASTRAVECTOR_QDRANT_ENABLED: "true"
ASTRAVECTOR_QDRANT_URL: "http://qdrant:6333"
ASTRAVECTOR_QDRANT_COLLECTION: "astravector_v004"
ASTRAVECTOR_INDEX_TTL_CLEANUP_ENABLED: "true"
```

## NetworkPolicy

Runtime, publisher, lifecycle and reconciliation workloads need egress to Qdrant ports `6333` and `6334`.

## Migration job

Use `/usr/local/bin/astravector-runtime migrate` from the same image tag as runtime.

## Image tag

Use one tag across runtime, migration, lifecycle and publisher:

```text
astravector-runtime:0.4.1-fix465-p2-production-hardening
```
