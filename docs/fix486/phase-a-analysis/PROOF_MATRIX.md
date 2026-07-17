# FIX486A proof matrix

Codex must replace every `TBD` with repository-backed evidence. A protobuf field or struct definition alone is not proof that the production path uses it.

| Case | Invariant | Production entrypoint | Code path | Existing proof | Missing proof | Required fixture | Required observability/failpoint | Status |
|---|---|---|---|---|---|---|---|---|
| FIX486-01 | Exact child returns its canonical parent | RetrieveContext/Search | TBD | TBD | Model-backed runtime assertion | `q-child-parent-exact` | hydration trace | TBD |
| FIX486-02 | Two children of one parent produce one final context | RetrieveContext | TBD | TBD | runtime dedup assertion | `q-parent-dedup` | group key, winner reason | TBD |
| FIX486-03 | Same logical UUIDs in different zones never mix | RetrieveContext/Explain | TBD | TBD | deliberate collision fixture | `q-zone-a`, `q-zone-b` | zone rejection counters | TBD |
| FIX486-04 | Inactive version cannot hydrate or return | RetrieveContext/Search/Explain | TBD | TBD | stale higher-score version | `q-active-version` | lifecycle drop trace | TBD |
| FIX486-05 | Deleted or orphan parent never forms context | RetrieveContext | TBD | TBD | stale Qdrant child | `q-orphan-child` | HYDRATION_MISSING | TBD |
| FIX486-06 | Hydration timeout is explicit and not false no-answer | RetrieveContext | TBD | TBD | partial/full timeout paths | `q-hydration-timeout` | hydration failpoint | TBD |
| FIX486-07 | Exact child evidence survives grouping | RetrieveContext/Explain | TBD | TBD | exact identifier runtime proof | `q-exact-identifier` | exact-match trace | TBD |
| FIX486-08 | Graph child resolves its own parent | RetrieveContext with Graph | TBD | TBD | graph-to-parent runtime proof | `q-graph-repair` | seed/relation/origin trace | TBD |
| FIX486-09 | Unique required-intent context survives token budget | RetrieveContext | TBD | TBD | constrained budget proof | `q-multi-intent-budget` | final coverage trace | TBD |
| FIX486-10 | Large parent cannot starve multiple unique aspects | RetrieveContext | TBD | TBD | A vs B+C selection proof | `q-large-parent-pressure` | drop reason/token efficiency | TBD |

## Status values

```text
IMPLEMENTED_AND_PROVEN
IMPLEMENTED_PARTIALLY_PROVEN
IMPLEMENTED_NOT_PROVEN
DECLARED_NOT_IMPLEMENTED
BLOCKED_BY_OBSERVABILITY
BLOCKED_BY_ENVIRONMENT
```

## Required per-case detail

For every row create a subsection containing:

- exact files and functions;
- PostgreSQL tables and composite keys;
- Qdrant collection/payload keys;
- stage ordering;
- expected positive result;
- expected negative result;
- existing tests and their level;
- required future unit/integration/runtime/load tests;
- known defect or uncertainty;
- P0/P1/P2 priority.