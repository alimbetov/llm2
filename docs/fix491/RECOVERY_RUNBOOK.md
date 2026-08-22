# FIX491 Recovery Runbook

## Authority Boundary

PostgreSQL is the canonical AstraVector state. Qdrant is a rebuildable projection.

```text
empty PostgreSQL + repository migrations
= schema recovery

operator PostgreSQL backup/PITR + pending migrations + postgres-audit
= canonical data disaster recovery
```

AstraVector does not implement PostgreSQL backup/PITR. RPO is determined by the operator backup policy. RTO is restore time plus migrations, PostgreSQL audit, Qdrant rebuild and retrieval parity verification.

## Qdrant Projection Recovery

The recovery path uses the existing reconciliation engine and the shared canonical projection builder. It does not call ONNX/BGE inference and does not reparse source documents.

Commands:

```bash
cargo run --locked --bin astravector-runtime -- recovery qdrant-audit --batch-size 500
cargo run --locked --bin astravector-runtime -- recovery qdrant-compatibility
cargo run --locked --bin astravector-runtime -- recovery qdrant-rebuild --batch-size 500
cargo run --locked --bin astravector-runtime -- recovery qdrant-rebuild --batch-size 500 --replace-existing
cargo run --locked --bin astravector-runtime -- recovery full-proof --batch-size 500
make verify-fix491-persistence-recovery
```

`verify-fix491-persistence-recovery` is the canonical closure runner. It executes static checks, focused FIX491 contracts, disposable PostgreSQL bootstrap/fencing, PostgreSQL canonical audit, Qdrant collection compatibility, Qdrant projection audit, rebuild and retrieval parity when `FIX491_RUN_RETRIEVAL_PARITY=1` is set.

`full-proof` is fail-closed: it must not report top-level PASS when any required lane is `NOT_RUN`.

`--replace-existing` is the explicit destructive opt-in. Without it, rebuild performs idempotent upserts into the existing compatible collection and does not remove orphan points.

## Fencing

Full Qdrant rebuild takes an exclusive PostgreSQL advisory transaction lock:

```text
class_id = 491
object_id = 1
```

Normal outbox and reconciliation projection writes take a shared advisory transaction lock around Qdrant mutation and the associated synced-state update. If exclusive recovery is active, the normal writer fails closed with:

```text
QDRANT_RECOVERY_FENCE_ACTIVE
```

This prevents a destructive/full rebuild from racing with ordinary projection writers.

## Operator Rules

- Do not treat Qdrant collection existence as recovery proof.
- Run `qdrant-audit` after every rebuild.
- Do not claim full FIX491 PASS until PostgreSQL bootstrap, SQLx checksum audit, schema inventory, canonical-data integrity, Qdrant collection compatibility, Qdrant audit, recovery fencing, PostgreSQL fingerprint and retrieval parity all pass on the tested SHA.
- Never log database passwords, Qdrant API keys or tokens in evidence.

## Final Verified Run

```text
run_id = fix491-20260811-003559
verdict = FIX491_PERSISTENCE_RECOVERY_PASS
postgres = POSTGRES_CANONICAL_AUDIT_PASS
qdrant_compatibility = QDRANT_COLLECTION_COMPATIBLE
qdrant_projection = QDRANT_PROJECTION_CONSISTENT
retrieval_parity = PASS
```
