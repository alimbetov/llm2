# FIX486 implementation backlog

The repaired P1 `FIX486A-P1-001` is recorded in the analysis report and is not backlog work.

| ID | Category | Severity | Required work | Proof | Target | Blocking Phase A |
|---|---|---|---|---|---|---|
| FIX486-B01 | FIXTURE_GAP | P2 | Freeze bank 1.0.0; generate `parent-large` with production tokenizer; resolve manifest hashes and physical identity map | immutable manifest + structural test | fix486c | no |
| FIX486-B02 | OBSERVABILITY_GAP | P2 | Trace `(zone,parent)` grouping winner and deduplicated children | exact winner/dedup assertions | fix486d | no |
| FIX486-B03 | OBSERVABILITY_GAP | P2 | Add per-candidate `HYDRATION_MISSING` and Graph hydration missing reasons | stale projection runtime test | fix486e/g | no |
| FIX486-B04 | FAILPOINT_GAP | P1 | Add total and partial parent hydration failpoints and explicit degradation policy | public tonic timeout/partial Testcontainers tests | fix486f | no; executable design complete |
| FIX486-B05 | DOCUMENTATION_GAP | P2 | Make Explain scope explicit or derive Explain from shared Search trace | parity contract and final-result comparison | fix486d | no |
| FIX486-B06 | TESTABILITY_GAP | P1 | Execute all 11 bank queries through model-backed production ingest/retrieval | unchanged-bank machine report | fix486d-h | no; Phase A is readiness |
| FIX486-B07 | OBSERVABILITY_GAP | P2 | Add request-scoped hydration SQL operation count | no-N+1 load assertion | fix486i | no |
| FIX486-B08 | PERFORMANCE_GAP | P2 | Measure coverage-per-token and large-parent monopoly | FIX486-10 plus three-run Mac evidence | fix486h/i | no |
| FIX486-B09 | CI_GATE_GAP | P2 | Add bank structural contract and targeted hierarchy Testcontainers target to CI | locked CI logs | fix486b/c | no |

## Detail

### FIX486-B04 hydration failures

Root cause: direct parent hydration is intentionally one batch SQL statement, so the current code
can prove total SQL failure but cannot deterministically simulate one missing/slow parent. Add
stage-level failpoints, not query IDs. A partial successful response must carry explicit degraded
status and must recompute intent coverage; total failure must not become ordinary no-answer.

### FIX486-B05 Explain scope

Affected path: `explain_search` performs query planning and Qdrant candidate generation but not
canonical hydration, Graph merge, MMR, token budget, final visibility or final coverage. Either
label this candidate-only contract in protobuf/docs, or generate Explain from the same bounded
ranking trace as Search without rerunning divergent retrieval logic.

### FIX486-B08 large parents

No ranking tuning is authorized by Phase A. First generate the canonical-token fixture and collect
same-bank quality, latency, hard-negative and security evidence. Only a demonstrated failure may
justify a general coverage-per-token algorithm change in fix486h.
