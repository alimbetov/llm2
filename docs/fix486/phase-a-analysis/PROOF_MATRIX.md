# FIX486A proof matrix

Status describes proof available at the end of Phase A. It does not promote a structural contract
or helper test to model-backed runtime proof.

| Case | Invariant | Production path | Existing proof | Missing Phase 1+ proof | Status |
|---|---|---|---|---|---|
| FIX486-01 | Exact child returns canonical parent | Search grouping -> batch hydration | production Testcontainers child/parent retrieval; bank contract | unchanged-bank model-backed assertion for exact child and parent text | IMPLEMENTED_PARTIALLY_PROVEN |
| FIX486-02 | Two children yield one parent | `(zone,parent)` first-winner grouping | grouping code and unit/contract coverage | runtime two-granularity collision with winner trace | IMPLEMENTED_PARTIALLY_PROVEN |
| FIX486-03 | Same logical IDs never cross zones | Qdrant filter -> composite PostgreSQL keys -> final visibility | access security suites; bank collision structurally validated | executable same-label Zone A/B bank run | IMPLEMENTED_PARTIALLY_PROVEN |
| FIX486-04 | Inactive version never returns | Qdrant filter + hydration document join + final visibility | lifecycle/TTL E2E and SQL contracts | higher-attraction inactive v2 bank run | IMPLEMENTED_PARTIALLY_PROVEN |
| FIX486-05 | Orphan/deleted parent forms no context | batch hydration fail-closed | fail-before/PASS Testcontainers regression `FIX486A-P1-001` | stale Qdrant point plus per-candidate drop trace | IMPLEMENTED_PARTIALLY_PROVEN |
| FIX486-06 | Hydration timeout is explicit | PostgreSQL statement timeout -> RPC error | code path and deadline tests | deterministic partial/total hydration failpoints | IMPLEMENTED_NOT_PROVEN |
| FIX486-07 | Exact child evidence survives grouping | matched child content kept separately from parent | E2E matched text and ranking evidence tests | exact identifier bank runtime assertion and trace | IMPLEMENTED_PARTIALLY_PROVEN |
| FIX486-08 | Graph child resolves its own parent | related child -> Graph batch hydration -> own parent | Graph network E2E; P1 fix protects stale parent | bank REPAIRED_BY runtime identity proof | IMPLEMENTED_PARTIALLY_PROVEN |
| FIX486-09 | Unique required intent survives if it fits | intent reservation -> MMR -> token budget -> final coverage | unit tests for coverage and hard budget | constrained canonical-token model-backed bank run | IMPLEMENTED_PARTIALLY_PROVEN |
| FIX486-10 | Large parent does not starve unique aspects | MMR/ranking protection -> token budget | no bank runtime proof | generated 900-token parent and selection assertion | IMPLEMENTED_NOT_PROVEN |

## FIX486-01 exact child to parent

- Entrypoint: `RetrieveContext -> Search`.
- Storage/Qdrant identity: payload child key `(access_zone_id, chunk_id, parent_chunk_id)`;
  PostgreSQL joins the same zone and same document version.
- Positive: `matched_text` contains child anchors while `parent_text` contains broader PARENT.
- Negative: child cannot hydrate a different/missing parent.
- Existing level: Testcontainers with production tonic, Qdrant, PostgreSQL and fixed inference.
- Future executable proof: ingest `q-child-parent-exact`, assert one expected child and
  `parent-a1`; capture hydration SQL count and trace.
- Priority: P1 correctness gate.

## FIX486-02 parent dedup

- Group key: `(access_zone_id, parent_chunk_id)`; first rank-ordered hit wins.
- Exact child evidence: winner's `chunk_id` and matched text survive hydration.
- Positive: SUB_180 and SUB_260 for `parent-a1` yield one final parent.
- Negative: equal parent labels in another zone do not deduplicate together.
- Missing observability: winner/loser reason per parent group.
- Future proof: deterministic dense/sparse scores for both children and final occurrence count 1.
- Priority: P2 duplicate risk, P1 if wrong child evidence is selected.

## FIX486-03 zone collision

- Qdrant point UUID is physically zone-scoped; logical identity may be equal across zones.
- All hydration, Graph, embedding and final visibility keys include zone.
- Positive: each zone retrieves only its local reused logical labels.
- Negative: forbidden anchors and cross-zone count remain zero.
- Explain caveat: Explain applies one-zone Qdrant filtering but does not execute final hydration.
- Future proof: same request set against both zones and malicious Zone A query for Zone B anchor.
- Priority: P0 security gate.

## FIX486-04 active version

- Qdrant lifecycle is a prefilter; PostgreSQL document version ACTIVE is authoritative.
- Child and parent must share document/version; final visibility rechecks matched chunk state.
- Positive: version 1 wins even when version 2 has stronger lexical evidence.
- Negative: INDEXING, DELETED and expired versions return zero.
- Future proof: stale projection points for v2/v3/v4 with unchanged qrel.
- Priority: P0 lifecycle gate.

## FIX486-05 orphan child

- Repaired functions: both single-zone and multi-zone Graph context hydration queries.
- Before: `LEFT JOIN` plus `COALESCE(parent, child)` returned SUB_180 as parent evidence.
- After: mandatory same-zone/document/version active PARENT `JOIN`; stale child is dropped.
- Regression: `test_e2e_retrieve_context_full_rag_lifecycle_over_tonic_network`.
- Remaining gap: direct hydration metric is aggregate and does not expose candidate drop identity.
- Future proof: insert stale Qdrant child after parent deletion and assert drop trace.
- Priority: repaired P1; trace gap P2.

## FIX486-06 hydration timeout

- Direct hydration is one transaction with a local PostgreSQL `statement_timeout`.
- Insufficient budget or SQL timeout propagates as `DeadlineExceeded`/database error; it is not
  converted to a successful empty context set.
- Partial-parent timeout is not representable by the current one-query hydration implementation.
- Required failpoints: total delay/error before fetch; deterministic omission of selected parent
  rows after fetch for partial degradation policy testing.
- Future policy decision: partial omission may return surviving contexts only with explicit
  degradation; total failure must fail the RPC or explicitly report unavailable.
- Priority: P1 testability/degradation gate.

## FIX486-07 exact evidence preservation

- Winning Qdrant child remains `matched_chunk_id`; SQL selects `m.content` independently from
  `p.content`; response carries both score components and citation metadata.
- Positive: `/api/v1/search` and `parent_chunk_id` remain in matched evidence.
- Negative: a parent-only lexical match cannot be claimed as exact child evidence.
- Future proof: query bank with sparse/FTS enabled and assert trace flag plus original PostgreSQL
  child bytes.
- Priority: P1 evidence correctness.

## FIX486-08 Graph own parent

- Seed key and edge rows contain seed and related zones/chunk IDs.
- Related child contexts are keyed by `(related access_zone_id, related chunk_id)` and hydrate
  their own PARENT before scoring.
- Direct/Graph dedup merges provenance only for equal zone-aware result identity.
- Positive: A1 seed discovers A3 child and returns `parent-a3`.
- Negative: it never reuses `parent-a1`, a foreign-zone parent, or a stale parent.
- Future proof: bank relation `REPAIRED_BY`, debug seed/relation/origin trace, final citation.
- Priority: P1 Graph provenance gate.

## FIX486-09 token budget and intents

- Required segment evidence is marked before MMR; final coverage is recomputed after MMR,
  token budget and visibility.
- Ranking protection changes drop ordering but hard token limits remain absolute.
- If no required intent survives, Search clears results; partial coverage produces warnings.
- Future proof: exact tokenizer-generated chunks where A/B/C physically fit at 900 tokens and B
  is lowest score; assert final coverage 3/3 or explicit degraded if physical fit is false.
- Priority: P1 false-full-coverage/lost-intent gate.

## FIX486-10 large parent pressure

- Current truncation scores candidates and protects selected evidence; it does not optimize
  coverage-per-token globally.
- Positive target: smaller B and C contexts survive instead of one 900-token A-only parent when
  this maximizes required-intent coverage.
- Negative: report `large_parent_budget_monopoly=0` and no false full coverage.
- Future proof requires generated text measured by the production tokenizer and same-bank A/B
  latency/quality evidence before any ranking change.
- Priority: P1 if a required intent is lost and falsely reported; otherwise P2 ranking quality.
