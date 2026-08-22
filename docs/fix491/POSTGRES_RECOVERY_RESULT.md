# FIX491 PostgreSQL Recovery Result

Verdict: `POSTGRES_CANONICAL_AUDIT_PASS`

Evidence:

```text
run_id       = fix491-20260811-003559
evidence     = docs/fix491/evidence/fix491-20260811-003559/postgres-audit.stdout
testcontainers = fix491_recovery_testcontainers PASS
```

## Migration History

The audit now uses SQLx migration semantics: SHA-384 of the migration SQL text compared against `_sqlx_migrations.checksum`.

```text
repository_migration_count = 39
applied_migration_count    = 39
failed_migrations          = 0
unknown_migrations         = 0
pending_migrations         = 0
checksum_mismatches        = 0
```

## Canonical Integrity

```text
partial_active_documents                  = 0
orphan_chunks                             = 0
orphan_bindings                           = 0
orphan_outbox                             = 0
orphan_graph_nodes                        = 0
orphan_graph_edges                        = 0
duplicate_chunks                          = 0
duplicate_bindings                        = 0
duplicate_outbox_events                   = 0
active_searchable_bindings_missing_dense  = 0
dead_or_failed_outbox                     = 0
```

`active_searchable_bindings_missing_sparse = 24795` is diagnostic in this run because the `local-demo` profile has `sparse.required=false`.

## Schema/Fingerprint

```text
schema_inventory_item_count = 6792
schema_inventory_sha256     = f54e3e78a5465986034cdb4fd1936757368490b745fc868d6aef7fe6cb3b82ca
canonical_fingerprint_items = 60
canonical_fingerprint_sha256 = a2cb2c30cd982649c26d29035498f0e20aca8339f54e8c15c14cc904e7b44d19
```

## Clean Bootstrap and Fencing

`fix491_recovery_testcontainers` starts a disposable `pgvector/pgvector:pg16`, applies all repository migrations, runs the PostgreSQL recovery audit, and verifies recovery fencing:

```text
clean PostgreSQL bootstrap + audit              PASS
exclusive recovery rejects stale recovery        PASS
exclusive recovery rejects projection writer     PASS
projection writer succeeds after fence release   PASS
```
