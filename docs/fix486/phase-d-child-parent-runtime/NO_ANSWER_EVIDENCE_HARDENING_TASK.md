# Technical Specification - fix486d P1-006 Exact Technical Evidence Preservation

## Status

```text
precondition=FIX486_CHILD_PARENT_RUNTIME_PROOF_BLOCKED
observed_stage=PRE_MMR_NO_ANSWER / POST_MMR_NO_ANSWER
scope=runner diagnostic first, production change only after confirmation
```

## Problem statement

The frozen Phase D corpus is ingested, projected to Qdrant and hydrated from
PostgreSQL. The three positive queries still return empty final contexts:

```text
PRE_MMR_WEAK_CANDIDATE_FILTERED
POST_MMR_NO_ANSWER_TRIGGERED
FINAL_CONTEXT_SET_TOO_WEAK
```

For `q-exact-identifier`, lexical retrieval found candidates, but the current
runner sent every query through a single hybrid request and did not prove the
frozen query profile was honored. The existing evidence does not yet prove
whether the root cause is request orchestration or production no-answer logic.

## Non-goals

Do not:

- modify the frozen corpus, queries, qrels, manifest or hashes;
- tune dense/sparse weights, RRF, GraphRAG or MMR for a query ID;
- disable no-answer globally;
- weaken zone, lifecycle, version or access-level checks;
- protect candidates by fixture label, logical ID or raw query ID;
- replace model-backed retrieval with mocks.

## Phase 1 - Profile-aware runner proof

### Required changes

The Phase D runner must load each query's `profile` from the frozen query file
and map it to an existing public request contract:

```text
TECHNICAL      -> technical supported Search/RetrieveContext mode
LEXICAL_STRICT -> lexical/sparse supported Search/RetrieveContext mode
```

The mapping must be explicit, bounded and covered by tests. It may not use a
single unconditional `SEARCH_MODE_V005_HYBRID` request for all cases.

For diagnostic requests only, `include_debug=true` must capture a bounded
ranking trace with, for each candidate:

```text
chunk ID, parent ID, zone, document, version, source block ID,
dense/sparse/lexical/fusion/final scores, exact_technical_match,
pre/post no-answer presence, and first drop reason.
```

### Acceptance

- Frozen profile is recorded in every request and normalized result.
- Search and RetrieveContext both honor the profile.
- A profile-mapping contract rejects unknown profiles.
- `q-exact-identifier` produces candidate evidence or a precise no-answer
  drop trace; missing trace is BLOCKED, never PASS.
- No production `src/**` changes occur in this phase.

## Phase 2 - Production defect decision gate

Run the entire Phase D campaign after Phase 1 from a clean commit.

### No production change is allowed when

The profile-aware campaign returns a valid positive child/parent pair for all
six primary requests.

### Production defect is confirmed only when

For an active, visible, hydrated original child candidate, all are true:

```text
matched child contains every requested exact technical identifier
sparse or lexical evidence is present
exact_technical_match=true
parent hydration succeeds
zone/document/version/access checks succeed
```

and the candidate is then removed by pre- or post-MMR no-answer filtering.

The fail evidence must record the candidate identity, scores and first drop
stage before any production code changes.

## Phase 3 - Narrow production repair (conditional)

Only after the decision gate confirms a defect, add an evidence-preservation
path to the production no-answer filter.

It may preserve a candidate only if all of these conditions hold:

```text
original representation
ACTIVE lifecycle and active document version
authorized zone and access level
successful canonical PostgreSQL parent hydration
all exact technical query identifiers occur in matched child text
exact_technical_match=true
non-zero sparse or lexical evidence
no forbidden, synthetic, cross-zone or stale evidence
```

The path must preserve existing cancellation, limits and final visibility
checks. It must not bypass no-answer for semantic-only or common-overlap
candidates.

## Required regression tests

1. Profile mapping reads frozen query profile, not query ID.
2. Unknown profile fails closed.
3. Exact technical original child survives pre/post no-answer only after
   canonical hydration and visibility validation.
4. Weak common-overlap candidate remains filtered.
5. Zone B, inactive/deleted version and synthetic evidence receive no
   preservation.
6. Search/RetrieveContext return the same logical child/parent pair.
7. Ranking trace records the first no-answer drop without document-body leak.
8. Existing frozen aggregate remains unchanged.

## Mandatory validation

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo test --locked --test fix486_hierarchical_bank_contracts -- --nocapture
cargo test --locked --test fix486c_frozen_bank_contracts -- --nocapture
cargo test --locked --test fix486d_child_parent_contracts -- --nocapture
make verify-fix486d-child-parent-runtime
```

## Evidence and verdict

Every campaign keeps a new external evidence run. The result must include six
primary rows, pre/post no-answer trace, Search/RetrieveContext parity, warm and
restart comparison, and a hash-verified manifest.

Allowed verdicts:

```text
FIX486_CHILD_PARENT_RUNTIME_PROOF_PASS
FIX486_CHILD_PARENT_RUNTIME_PROOF_BLOCKED
```

No production-ready claim is permitted in this phase.
