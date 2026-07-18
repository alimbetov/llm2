# FIX486E Isolation and Lifecycle Runtime Proof Result

## Verdict

```text
FIX486_ISOLATION_LIFECYCLE_RUNTIME_PROOF_PASS
```

This result closes Phase E only. It does not declare the whole project
production-ready; the remaining FIX486 phases retain their own acceptance gates.

## Identity

| Field | Value |
|---|---|
| Repository | `alimbetov/llm2` |
| Branch | `codex/fix486e-isolation-lifecycle-runtime-proof` |
| Tested source SHA | `5a6edf484b36142601b3000e9c941fe328bc428a` |
| Run ID | `fix486e-20260718T142323Z` |
| Frozen bank | `1.0.0 / FROZEN` |
| Frozen aggregate SHA-256 | `cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff` |
| Evidence manifest file SHA-256 | `fc4904cc1b4bedc37a6fcc958e2341e9a0e281af8d202bc6ae78b45f2888f3c6` |
| Evidence manifest internal aggregate | `c9c0477c1d2e9b2c0df0baab4e4ec68a5e95e2f15525aa78322e67e828fccac5` |
| Manifest verification | `PASS (252 files)` |

The complete evidence bundle is external to Git:

```text
/Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix486e/fix486e-20260718T142323Z
```

## Result Summary

| Assertion | Result |
|---|---:|
| Locked fmt/check/clippy (`-D warnings`) | `PASS` |
| Locked all-target/all-feature tests | `PASS` |
| Phase A/C/D/E contract tests | `PASS` |
| Production ingestion and projection | `PASS` |
| Canonical PostgreSQL audit | `PASS` |
| Qdrant audit | `PASS` |
| Search proof | `3/3 PASS` |
| RetrieveContext proof | `3/3 PASS` |
| Opposite-zone controls | `4/4 PASS` |
| Entry-point parity | `PASS` |
| Warm repeat | `PASS` |
| Restart repeat | `PASS` |
| Cleanup | `PASS` |
| Failure codes | `[]` |

## Isolation and Lifecycle Evidence

All cross-zone promotion, hydration, final-context, graph-result, and evidence-leak
counters are zero. Wrong-version, inactive, deleted, expired, and legal-hold bypass
counters are also zero.

Canonical state contained one Zone A version for each required state: ACTIVE v1,
INDEXING v2, DELETED v3, and EXPIRED v4. Zone B had one ACTIVE v1. Lifecycle probes
returned no forbidden final contexts and produced no unknown classification.

Legal hold protected all 60 held bindings from cleanup. PostgreSQL reported zero
orphan children, cross-zone bindings, cross-document bindings, cross-version
bindings, duplicate chunks, failed outbox events, and dead letters.

Qdrant contained 78 points for 78 searchable synchronized bindings. The remaining
six bindings were canonically deleted, with six completed delete operations.

## Resolved Production Defect

Document-vector deletion previously created `DELETE_POINT.operation_version` from
`payload_version`, while stale-worker fencing validates deletion against
`ttl_generation`. The scheduler now advances `ttl_generation` atomically, moves the
binding to deletion-pending state, and emits the returned generation in the outbox
event. Legal-hold and already-deleting/deleted bindings are excluded before
scheduling.

The Testcontainers regression exercises the production gRPC deletion path, outbox
publishing, final binding state, and generation consistency. It proves that stale
completion cannot produce a false synchronized or deleted state.

## Access-Zone Identity Contract

Clients and operators should address a zone by stable `access_zone_code` together
with `document_id` or `external_document_id`. The zone UUID remains an internal,
immutable partition and foreign-key identity used for authorization joins, Qdrant
payload filtering, and collision-resistant storage; it is not required as the
human-facing API identifier.

The proof used frozen codes `4862` and `4863`. TTL was resolved independently for
each code through the production access-zone registry. Both currently resolve to
1,460 days; equality of these values is data-driven and not a global TTL shortcut.

## Preserved Failed Attempts

Earlier BLOCKED runs were retained as separate evidence bundles. They document the
protobuf metadata mismatch, deletion-generation fencing defect, invalid auxiliary
child requirement, non-zone-scoped document identity, and Phase D-only child
normalization assumption. None was overwritten or reclassified as PASS.
