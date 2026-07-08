# Reconciliation runbook

`astravector-reconciliation` now supports active modes:

```bash
astravector-reconciliation --full --batch-size 500 --interval-seconds 30
astravector-reconciliation --binding <zone_uuid>:<binding_uuid>
```

Repaired Qdrant points must be built with the same retrieval payload contract as outbox-created points. Legal-hold points are never deleted by reconciliation.

## Metrics

- `reconciliation_bindings_scanned_total`
- `reconciliation_bindings_repaired_total`
- `reconciliation_bindings_deleted_total`
- `reconciliation_skipped_legal_hold_total`
- `reconciliation_errors_total`
