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
cargo run --locked --bin astravector-runtime -- recovery qdrant-rebuild --batch-size 500
cargo run --locked --bin astravector-runtime -- recovery qdrant-rebuild --batch-size 500 --replace-existing
cargo run --locked --bin astravector-runtime -- recovery full-proof --batch-size 500
```

`full-proof` currently executes the Qdrant audit/rebuild/audit lane only and reports PostgreSQL bootstrap proof and retrieval parity as `NOT_RUN` until those lanes are implemented and executed.

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
- Do not claim full FIX491 PASS until PostgreSQL bootstrap, schema drift, canonical-data integrity, Qdrant audit, recovery fencing, interruption/resume, PostgreSQL fingerprint and retrieval parity all pass on the tested SHA.
- Never log database passwords, Qdrant API keys or tokens in evidence.
