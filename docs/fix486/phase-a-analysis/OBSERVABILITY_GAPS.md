# FIX486A observability gaps

| ID | Gap | Risk | Required evidence | Priority / phase |
|---|---|---|---|---|
| OBS-486-01 | Parent grouping trace lacks group key, winning child and rejected siblings | Cannot explain exact-child winner or dedup | bounded debug records keyed by hashed zone/parent/chunk | P2 / fix486d |
| OBS-486-02 | Missing hydration is only an aggregate metric | Stale child drop cannot be tied to a candidate | ranking drop reason `HYDRATION_MISSING`, no content/secret IDs in normal logs | P2 / fix486e |
| OBS-486-03 | Explain uses a separate candidate-only path | Operators may read Explain output as final Search result | explicit `explain_scope=CANDIDATE_ONLY` until shared pipeline trace exists | P2 / fix486d |
| OBS-486-04 | Graph context lookup misses are silently continued | Related child/parent loss is not attributable | `GRAPH_CONTEXT_HYDRATION_MISSING` count and bounded trace identity | P2 / fix486g |
| OBS-486-05 | No dedicated parent hydration SQL-call counter | N+1 regression is inferred, not directly gated | request-scoped query count or repository operation counter | P2 / fix486i |
| OBS-486-06 | Token-budget trace identifies dropped chunks but not intent/token efficiency | Large-parent domination diagnosis is incomplete | tokens, protected intents, coverage delta and drop reason per candidate | P2 / fix486h |
| OBS-486-07 | Partial hydration has no status vocabulary because hydration is one batch | Partial degradation cannot be proven | failpoint outcome plus surviving/missing counts and explicit warning | P1 testability / fix486f |

Existing useful signals include parent hydration duration/candidate/missing counters, Graph seed and
related-row debug logs, Graph candidate metrics by relation, MMR stage diagnostics, token-budget
drop counters, final visibility drop counters, and coverage ratios after each destructive stage.

Debug output must remain bounded and authorization-safe. Production metrics should use counts and
low-cardinality reason labels; raw zone/chunk identities belong only in authorized debug traces.
