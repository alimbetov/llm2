# FIX486A acceptance criteria

## Mandatory analysis outputs

```text
[ ] Analyzed source SHA recorded
[ ] Final candidate SHA recorded
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

## Mandatory defect closeout

```text
[ ] Every reproducible in-scope P0/P1 defect has a defect record
[ ] Every reproducible in-scope P0/P1 defect has preserved FAIL evidence
[ ] Every repaired defect has a failing regression test created before the fix
[ ] Every repaired defect has a documented root cause
[ ] Every repaired defect has a separate production-fix commit
[ ] Queries, qrels and expected identities remained unchanged during repair
[ ] Every repaired defect has after-fix evidence from the same scenario
[ ] Every repaired defect has a direct before/after comparison
[ ] No reproducible in-scope P0/P1 defect remains unresolved
[ ] Remaining P2/P3 defects have risk, reason and target phase in backlog
```

## Mandatory final rerun

```text
[ ] cargo fmt --all --check PASS
[ ] cargo check --locked --all-targets --all-features PASS
[ ] cargo test --locked --all-targets --all-features PASS
[ ] cargo clippy --locked --all-targets --all-features -- -D warnings PASS
[ ] All targeted regression tests PASS
[ ] All required integration/Testcontainers tests PASS
[ ] All available affected model-backed gates PASS
[ ] No mandatory stage is BLOCKED or SKIPPED
[ ] Final run uses recorded source, bank, model, tokenizer and config identities
```

## Mandatory hard blockers

The phase cannot return READY while any of these remain unknown or unresolved:

```text
parent hydration identity scope
parent grouping identity scope
final access-zone validation
active-version validation
orphan-child behavior
hydration failure versus no-answer semantics
Graph child parent resolution
final coverage after token budget
reproducible in-scope P0/P1 defect
missing regression test for a repaired defect
missing before/after evidence
mandatory failed/BLOCKED/SKIPPED gate
identity mismatch between baseline and rerun
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
- all reproducible in-scope P0/P1 defects found in this phase are repaired;
- every repair is protected by an unchanged-bank regression proof;
- the mandatory final project rerun is green;
- remaining lower-severity work is explicitly scheduled.

It does not mean the complete Phase 0–8 hierarchical retrieval validation program or Mac load certification has already passed.