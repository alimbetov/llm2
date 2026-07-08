# AstraVector v007 interface simplification fix1

## Goal

`v007 fix1` closes release-gate issues found in `v007-interface-simplification` without removing the internal reliability mechanisms: outbox, sync, activation gate, Qdrant reconciliation, TTL lifecycle, diagnostics, and adaptive runtime.

## Implemented in fix1

- `RetrieveContext` no longer defaults missing access level to `INTERNAL`.
- `RetrieveContext` can derive access level from trusted metadata headers:
  - `x-astravector-access-level`
  - `x-astravector-caller-access-level`
- `LogicalBlock[]` validation now checks:
  - exactly one `BLOCK_TYPE_DOCUMENT` root;
  - non-empty and unique `block_id`;
  - non-empty text;
  - no `BLOCK_TYPE_UNSPECIFIED`;
  - parent exists;
  - no self-parent;
  - no cycles;
  - parent-child type compatibility;
  - source location sanity;
  - source link safety.
- `TTL_MODE_ABSOLUTE` is explicitly rejected until absolute TTL persistence/lifecycle is fully implemented.
- `AUTO_WHEN_READY` is explicitly rejected until lifecycle auto-activation worker is implemented.
- `DeleteDocumentVectorsFacade` returns `DELETE_SCHEDULED`, not `DELETED`, after scheduling `DELETE_POINT` outbox events.
- `GetRuntimeHealth` checks live PostgreSQL ping and Qdrant collection availability.
- Migration `0020_v007_interface_simplification_fix1.sql` adds persistence foundation for `LogicalBlock -> Chunk` trace.

## Intentionally deferred

- Full absolute TTL lifecycle worker.
- Full `AUTO_WHEN_READY` worker.
- Runtime insertion into `logical_block_chunk_mapping` for every generated chunk.
- Exact block-level source link selection for matched chunks.

These are planned for a follow-up fix after local `cargo check/test` and E2E smoke.
