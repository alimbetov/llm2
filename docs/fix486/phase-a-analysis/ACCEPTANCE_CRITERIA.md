# FIX486A acceptance criteria

## Mandatory outputs

```text
[ ] Analyzed source SHA recorded
[ ] Epic base SHA recorded
[ ] Worktree cleanliness recorded
[ ] Existing tests, smokes, failpoints and quality banks inventoried
[ ] Parent/child production path mapped
[ ] Parent hydration SQL and batching behavior identified
[ ] Parent grouping key identified
[ ] Winning child policy identified
[ ] Exact child evidence preservation path identified
[ ] Search/RetrieveContext/Explain parity mapped
[ ] Zone, access, version, lifecycle and TTL filters mapped by stage
[ ] Cross-zone logical-ID fixture feasibility confirmed
[ ] Stale Qdrant and orphan-child setup defined
[ ] Hydration failure injection points defined
[ ] Partial and total hydration failure policies defined
[ ] Graph child-to-own-parent path mapped
[ ] Direct/Graph parent dedup policy identified
[ ] MMR parent/child semantics identified
[ ] Token-budget ordering identified
[ ] Final coverage recomputation status identified
[ ] Large-parent domination risk classified
[ ] MacBook load methodology defined
[ ] All 10 critical cases completed in proof matrix
[ ] Bank 1.0.0 freeze plan defined
[ ] Implementation backlog created
[ ] External evidence manifest generated
```

## Mandatory hard blockers

The analysis cannot return READY while any of these remain unknown:

```text
parent hydration identity scope
parent grouping identity scope
final access-zone validation
active-version validation
orphan-child behavior
hydration failure versus no-answer semantics
Graph child parent resolution
final coverage after token budget
```

## Allowed final verdicts

```text
FIX486_ANALYSIS_READY
FIX486_ANALYSIS_BLOCKED
```

## Meaning of READY

`FIX486_ANALYSIS_READY` means:

- the real production path is understood;
- the data bank can be implemented without inventing identity semantics;
- each critical scenario has an executable proof design;
- missing testability/observability work is explicitly specified;
- the next phase can proceed without weakening production correctness.

It does not mean hierarchical retrieval itself has passed functional or load validation.