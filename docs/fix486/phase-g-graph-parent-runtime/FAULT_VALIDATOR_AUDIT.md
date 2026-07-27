# FIX486G Fault Validator Audit

Status: `IMPLEMENTED / FOCUSED_VALIDATION_REQUIRED`

This audit covers every rejected-Graph-target validator before the next
official runtime proof. The executable source of truth is
`scripts/fix486g_fault_contract.py`.

## Shared invariant

Every rejected-target scenario fails closed unless:

1. the forbidden target is absent from final contexts;
2. canonical direct `parent-a1` survives;
3. the exact approved rejection reason is observed;
4. the forbidden target receives no Graph provenance credit;
5. every scenario-relevant hard-gate counter is zero.

`GRAPH` survivor mode additionally requires the valid first-hop Graph
`parent-a3`. No other scenario requires Graph origin after its Graph target is
intentionally invalidated.

## Scenario matrix

| Scenario | Forbidden target | Survivor mode | Exact reason | Provenance | Additional hard gates |
| --- | --- | --- | --- | --- | --- |
| wrong-parent | mutated A3 child | DIRECT | `BINDING_INVALID` | no forbidden Graph credit | binding-invalid final contexts |
| binding-status | unsynced A3 child | DIRECT | `VISIBILITY_REJECTED` | no forbidden Graph credit | visibility-invalid final contexts |
| inactive-target | inactive A3 child | DIRECT | `VISIBILITY_REJECTED` | no forbidden Graph credit | visibility-invalid final contexts |
| deleted-target | deleted A3 child | DIRECT | `VISIBILITY_REJECTED` | no forbidden Graph credit | visibility-invalid final contexts |
| expired-target | expired A3 child | DIRECT | `VISIBILITY_REJECTED` | no forbidden Graph credit | visibility-invalid final contexts |
| missing-parent | A3 child bound to absent parent | DIRECT | `BINDING_INVALID` | no forbidden Graph credit | binding-invalid final contexts |
| cross-zone | zone-B endpoint | DIRECT | `GRAPH_ENDPOINT_ZONE_MISMATCH` | no cross-zone Graph credit | cross-zone contexts, forbidden anchors |
| hop-limit | second-hop A2 endpoint as Graph provenance | GRAPH | `HOP_LIMIT_REJECTED` | no second-hop Graph credit | hop and second-hop counters |
| cycle | phase-owned cycle edge | GRAPH | `GRAPH_CYCLE_REJECTED` | no cycle Graph credit | cycle and duplicate-credit counters |
| candidate non-interference | invalid high-ranked Graph target | EITHER | `BINDING_INVALID` | no forbidden Graph credit | valid-survivor loss |

`EITHER` does not weaken the shared direct-survivor invariant. It means no
additional Graph-origin survivor is required.

## Evidence sources

Identity and visibility reasons are read from production response warnings.
Cross-zone, hop-limit and cycle controls are boundaries that can reject before
a public response candidate exists. Their exact reason is accepted only with a
`PASS` structural evidence record whose topology assertion was observed and
whose SQL row count equals the expected count. A scenario name alone is not
rejection evidence.

## Regression coverage

Focused tests prove:

- the retired Graph-survivor-only validator rejects a valid direct survivor;
- all ten scenario contracts pass only with their declared survivor and reason;
- forbidden target survival fails;
- missing canonical direct evidence fails;
- missing or wrong rejection reason fails;
- false Graph provenance credit fails;
- any non-zero external hard gate fails;
- structural evidence fails unless it is both observed and `PASS`;
- statistical fault evaluation does not inherit `POSITIVE_GRAPH` survivor
  requirements for `DIRECT` fault scenarios.
- one-hop telemetry can be reconstructed from a complete debug ranking trace
  when final selection contains no Graph context; absent context and absent
  ranking trace still fail closed.

Production ranking, Graph weights, relation weights, RRF, MMR, token budget,
frozen queries, qrels and production rejection classifications are unchanged.
