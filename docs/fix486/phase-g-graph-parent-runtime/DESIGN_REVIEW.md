# FIX486G Design Review

## Decision

The existing architecture is retained. Phase G will strengthen the canonical
validation boundary and provenance record without changing ranking policy,
Graph weights, relation weights, MMR, RRF, or token budgets.

## Approved minimal repair

1. Extend `RelatedChunk` with immutable edge identity and endpoint provenance
   selected by the existing one-hop SQL.
2. Reject self-edges before Graph candidate construction.
3. Make Graph hydration require one canonical searchable binding whose zone,
   document, version, chunk and parent relationship agree with PostgreSQL and
   whose Qdrant state is `SYNCED`.
4. Request a bounded pre-hydration reserve using the existing hydration reserve
   configuration, while preserving the configured final Graph context limit and
   global Graph maximum.
5. Emit complete protected citation/debug metadata for seed, edge, related child,
   related parent and hop identity.
6. Add bounded rejection/provenance metrics with enum-only labels.

## Rejected alternatives

- No fixture IDs, query IDs, anchors, `parent-a3`, or relation-specific branches
  in production code.
- No direct SQL parent fallback.
- No N+1 binding or parent lookup.
- No unbounded refill or recursive traversal.
- No public failpoint API.
- No ranking or threshold adjustment.

## Contract strategy

Red contracts first assert:

- canonical Graph hydration requires a synced binding;
- binding parent identity agrees with the related chunk;
- stable edge identity survives SQL and result metadata;
- self-edges are rejected;
- expansion uses a bounded reserve and retains the configured final cap;
- Search/RetrieveContext continue to share one Graph implementation;
- Graph parent hydration remains one batch query.

The same contracts become regression gates after the repair.

## Review verdict

```text
APPROVED_FOR_CONTRACT_TEST_IMPLEMENTATION
```
