# FIX491 Persistence Recovery Result

Top-level verdict: `FIX491_PERSISTENCE_RECOVERY_PASS`

Tested run:

```text
run_id = fix491-20260811-003559
evidence_dir = docs/fix491/evidence/fix491-20260811-003559
```

## Final Gates

```text
cargo fmt --all --check                                            PASS
cargo check --locked --all-targets --all-features                  PASS
fix491_projection_contracts                                        PASS
fix491_postgres_recovery_contracts                                 PASS
fix491_recovery_testcontainers                                     PASS
postgres-audit                                                     PASS
qdrant-compatibility                                               PASS
qdrant-audit                                                       PASS
retrieval-before                                                   PASS
qdrant-rebuild                                                     PASS
retrieval-after                                                    PASS
```

## PostgreSQL Recovery

```text
repository_migration_count = 39
applied_migration_count    = 39
failed_migrations          = 0
unknown_migrations         = 0
pending_migrations         = 0
checksum_mismatches        = 0
partial_active_documents   = 0
orphan_chunks              = 0
orphan_bindings            = 0
orphan_outbox              = 0
duplicate_chunks           = 0
duplicate_bindings         = 0
duplicate_outbox_events    = 0
schema_inventory_sha256    = f54e3e78a5465986034cdb4fd1936757368490b745fc868d6aef7fe6cb3b82ca
canonical_fingerprint      = a2cb2c30cd982649c26d29035498f0e20aca8339f54e8c15c14cc904e7b44d19
```

`active_searchable_bindings_missing_sparse = 24795` is reported as diagnostic because the tested `local-demo` profile has `sparse.required=false`. Missing dense vectors remain blocking and were zero.

## Qdrant Recovery

```text
collection_compatibility = QDRANT_COLLECTION_COMPATIBLE
dense_dimension          = 1024
dense_distance           = Cosine
sparse_vector_present    = true
required_payload_indexes = 16
missing_payload_indexes  = 0
mismatched_payload_indexes = 0

expected_eligible_bindings = 19776
actual_points              = 19776
missing_points             = 0
orphan_points              = 0
payload_mismatches         = 0
scan_completed             = true
```

## Retrieval Parity

Search worked before and after Qdrant rebuild. The proof returned the same canonical local-demo parent:

```text
document_id       = 175f2d7c-a5b8-573b-903f-f64eaaea903c
document_version  = 1
parent_chunk_id   = 47e5cf3c-16a3-5894-8997-697b78c599af
matched_chunk_id  = 47e5cf3c-16a3-5894-8997-697b78c599af
matched_granularity = PARENT_V004
```

Returned evidence text:

```text
AstraVector хранит каноническое состояние документов в PostgreSQL. В PostgreSQL находятся версии документов, исходные chunks, bindings, lifecycle status и transactional outbox. Это источник истины, который можно проверять обычными SQL-запросами.
```

## Defects Fixed

- Replaced version-only migration audit with SQLx-compatible SHA-384 checksum verification.
- Added semantic PostgreSQL schema inventory and canonical data fingerprint.
- Added complete canonical integrity counters for partial active documents, orphan/duplicate chunks, bindings and outbox, graph orphans, dead outbox and missing dense vectors.
- Added read-only Qdrant collection compatibility audit for dense vector schema, sparse vector schema and required payload indexes.
- Made `full-proof` fail closed instead of returning a partial PASS when lanes are `NOT_RUN`.
- Added executable FIX491 proof runner and Make target.
- Fixed proof-runner status capture so failed commands cannot be reported as `status=0`.
- Corrected optional sparse classification: sparse gaps are blocking only when `sparse.required=true`.
- Optimized the partial-active-document audit from correlated checks to a set-based query.

## Notes

The final proof required a running local runtime on `127.0.0.1:50051`. Earlier retrieval parity attempts were `BLOCKED` only because that endpoint was not running.
