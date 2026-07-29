# FIX486G Current-SHA Graph Parent Verification and Repair

## Recommended model

```text
GPT-5.4
Reasoning: High
```

## Repository

```text
https://github.com/alimbetov/llm2
```

## Working branch

```text
agent/fix486g-current-sha-graph-parent-repair
```

## Base

```text
main
6d2a1b2c9fbbaa62c3ee3faee9ebbfb5d7653c6c
```

Do not work directly on `main`.

Do not merge this branch automatically.

---

# Objective

Determine whether the FIX486G failures observed in the completed 730-observation run are still reproducible on the current `main` SHA, and apply the smallest general production repair only for defects proven on the current SHA.

The previous complete evidence run reported:

```text
NoAnswerSpecificity       = 1.0 PASS
GraphParentRecall@1       = 0.1333 BLOCKED
GraphParentRecall@3       = 0.1333 BLOCKED
GraphParentRecall@5       = 0.1333 BLOCKED
MRR                       = 0.1333 BLOCKED
nDCG@5                    = 0.6108 BLOCKED
DirectPreservationRate    = 0.1 BLOCKED
wrong_parent              = 274
binding_invalid           = 274
valid_survivor_lost       = 188
```

However, that runtime evidence may have been produced before commit:

```text
6d2a1b2c9fbbaa62c3ee3faee9ebbfb5d7653c6c
fix(fix486g): respect directed graph relation traversal
```

That commit restricts reverse traversal to symmetric Graph relations. Therefore, do not assume the old 730-observation result describes current `main`.

---

# Immutable constraints

Do not modify:

```text
frozen queries
qrels
statistical thresholds
bank hashes
expected logical identities
fault fixtures
required language assignments
acceptance criteria
```

Do not add:

```text
query-ID-specific production logic
fixture-ID-specific production logic
hardcoded parent-a1 or parent-a3 behavior
hardcoded quality-run IDs
special handling for one frozen query
```

Do not weaken:

```text
access-zone isolation
access-level filtering
lifecycle validation
canonical binding validation
graph_max_hops = 1
no-answer gates
Graph provenance requirements
```

Do not run the full 730-observation campaign until the focused current-SHA slices and projected metrics pass.

Do not broadly refactor `src/grpc/mod.rs` in this task.

---

# Phase 0 — establish clean current-SHA state

Run:

```bash
git fetch origin --prune
git switch agent/fix486g-current-sha-graph-parent-repair
git status --short
git rev-parse HEAD
git rev-parse origin/main
git rev-parse origin/agent/fix486g-current-sha-graph-parent-repair
```

Required initial state:

```text
branch = agent/fix486g-current-sha-graph-parent-repair
branch base contains 6d2a1b2c9fbbaa62c3ee3faee9ebbfb5d7653c6c
working tree = clean
```

Record the exact tested SHA after every commit.

---

# Phase 1 — prove evidence lineage

Inspect the complete run:

```text
/Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix486g/fix486g-recovery-official-rerun-20260728T2024Z
```

Extract and report:

```text
source SHA
branch
binary SHA-256
run start time
run end time
manifest source identity
raw observation source identity
```

Compare the run source SHA with:

```text
6d2a1b2c9fbbaa62c3ee3faee9ebbfb5d7653c6c
```

Classification:

```text
OLD_SHA_EVIDENCE
CURRENT_SHA_EVIDENCE
SOURCE_IDENTITY_UNKNOWN
```

If the run source SHA differs from current `main`, explicitly mark all old statistical values as historical evidence, not current-SHA verdict.

Add or extend a focused proof-tool contract so statistical evaluation fails closed when:

```text
observation source SHA != manifest source SHA
or
manifest source SHA != expected tested SHA
```

Suggested contract:

```text
statistical_evidence_rejects_source_sha_mismatch
```

Do not change the evaluator's ranking or Graph semantics in this phase.

---

# Phase 2 — current-SHA Graph positive slice

Build and run a targeted model-backed slice on the current branch SHA.

Use the existing FIX486G frozen supplemental bank and production runtime path.

Scope:

```text
all positive Graph queries
Search
RetrieveContext
all required RU/KZ/EN assignments
```

The slice must prove for every Graph-positive observation:

```text
related Graph child is reached from the declared direction
related child hydrates its own canonical parent
response parent_chunk_id matches canonical binding parent
provenance graph_related_parent_chunk_id matches response parent_chunk_id
required Graph relation is preserved
seed parent is not reused as related parent
```

Collect at minimum:

```text
query_id
entry_point
seed child runtime/logical ID
seed parent runtime/logical ID
edge direction
relation type
related child runtime/logical ID
binding parent runtime/logical ID
response parent runtime/logical ID
graph_related_parent_chunk_id
rank
failure codes
```

Required focused result:

```text
wrong_parent_graph_final_contexts = 0
binding_invalid_graph_final_contexts = 0
seed_parent_reuse = 0
cross_zone = 0
hop_limit = 0
```

Calculate projected:

```text
GraphParentRecall@1
GraphParentRecall@3
GraphParentRecall@5
MRR
nDCG@5
```

Do not apply another Graph SQL fix if the current-SHA slice already passes parent and binding invariants.

---

# Phase 3 — directed traversal red contracts

Confirm current production semantics with executable tests.

Required tests:

```text
directed_repaired_by_relation_never_reverse_expands
reverse_traversal_is_limited_to_symmetric_relations
```

Use a generic topology, not frozen IDs:

```text
source-child --REPAIRED_BY--> repaired-child
```

Required behavior:

```text
source-child can expand to repaired-child
repaired-child cannot reverse-expand to source-child through REPAIRED_BY
```

Symmetric controls must still work in both directions:

```text
RELATED_TO
CHUNK_SEMANTIC_SIMILAR
CHUNK_SAME_TABLE
```

If these tests pass and Phase 2 has zero wrong-parent/binding-invalid results, treat directed traversal as fixed and do not alter it further.

---

# Phase 4 — canonical Graph binding invariant

Add a focused red contract for the production Graph hydration and response construction path:

```text
graph_hydrated_parent_matches_related_child_binding
```

Required invariant:

```text
hydrated related child ID
→ synced ORIGINAL vector binding
→ canonical parent ID
```

and:

```text
response.parent_chunk_id
== binding.parent_chunk_id
== citation.metadata.graph_related_parent_chunk_id
```

Also require:

```text
response.matched_chunk_id or protected graph_related_chunk_id identifies the related child
related child and parent share access zone
document version is active
binding is SYNCED
lifecycle is ACTIVE
```

If any identity differs, reject the Graph candidate before final selection and record a precise rejection reason.

Do not repair metadata alone while leaving an incorrect response parent.

Do not repair response parent alone while preserving stale Graph provenance.

The binding row is the source of truth for the child-to-parent mapping.

---

# Phase 5 — valid survivor preservation

Run only the existing fault-focused cases:

```text
graph_wrong_parent_overlay
graph_cross_zone_overlay
graph_inactive_deleted_expired_overlay
graph_second_hop_overlay
graph_cycle_overlay
```

For every fault observation prove:

```text
invalid Graph candidate is rejected
required valid direct survivor remains
required valid Graph survivor remains only when the declared fault contract requires it
invalid candidate does not consume final capacity
```

Required hard gate:

```text
valid_survivor_lost = 0
```

Add or strengthen:

```text
invalid_graph_candidate_does_not_consume_final_capacity
```

The production order must be logically equivalent to:

```text
retrieve bounded reserve
→ canonical hydration
→ access/lifecycle/binding validation
→ remove invalid Graph candidates
→ refill from valid reserve
→ MMR/final ranking
→ final context limit
```

Do not apply the irreversible final context limit before invalid Graph candidates are removed.

If current code already preserves survivors, do not change final selection.

---

# Phase 6 — minimal production repair

Modify production code only for a current-SHA red contract.

Likely files, depending on the proven divergence:

```text
src/persistence/mod.rs
src/grpc/mod.rs
src/graph/mod.rs
```

Allowed repair classes:

```text
direction-aware Graph expansion
canonical binding-sourced parent identity
parent-scoped Graph provenance
post-validation candidate refill
source-SHA evidence fail-closed validation
```

Forbidden repair classes:

```text
global Graph weight tuning
RRF/MMR threshold tuning
query-specific score boosts
fixture-specific identities
qrel changes
frozen-bank changes
```

Before editing production code, publish a checkpoint in the Codex response:

```text
root cause
affected current-SHA observations
first divergence stage
red contract name
red assertion
production file/function
minimal repair
why the repair is general
```

---

# Phase 7 — validation sequence

For every repair:

```text
red contract FAIL
→ minimal production change
→ red contract PASS
→ related focused suites PASS
→ targeted live slice PASS
```

Run at minimum:

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo test --locked --test fix486g_graph_parent_contracts -- --nocapture
cargo test --locked --test fix486g_runner_hardening_contracts -- --nocapture
cargo test --locked --test fix486g_statistical_capture_contracts -- --nocapture
cargo test --locked --test fix486g_statistical_proof_contracts -- --nocapture
cargo clippy --locked --all-targets --all-features -- -D warnings
```

Also run the exact focused unit/integration tests added for:

```text
directed traversal
canonical binding parent identity
survivor refill
source-SHA evidence validation
```

Do not start the full official campaign while any focused gate is non-zero or projected metric is below threshold.

---

# Phase 8 — projected acceptance

Before a full run, required projected state:

```text
NoAnswerSpecificity = 1.0
GraphParentRecall@1 >= 0.90
GraphParentRecall@3 >= 0.97
GraphParentRecall@5 >= 0.99
MRR >= 0.94
nDCG@5 >= 0.95
DirectPreservationRate = 1.0
```

Required safety gates:

```text
wrong parent = 0
binding invalid = 0
valid survivor lost = 0
cross-zone = 0
seed-parent reuse = 0
lifecycle invalid = 0
hop-limit violations = 0
false Graph attribution = 0
```

If projected metrics remain blocked, stop and report the exact blocker. Do not run 730 observations.

---

# Phase 9 — official run policy

Only after all focused and projected criteria pass may Codex run one complete official FIX486G campaign on the final published branch SHA.

Required official result:

```text
FIX486_GRAPH_PARENT_RUNTIME_PROOF_PASS
FIX486G_STATISTICAL_QUALITY_PASS
FIX486G_PASS
```

If not achieved:

```text
FIX486G_BLOCKED
```

with one exact reproducible blocker and preserved evidence.

Do not merge the branch automatically.

---

# Commit discipline

Use separate commits:

```text
1. test(fix486g): reject statistical source SHA mismatch
2. test(fix486g): cover current-SHA graph parent and survivor invariants
3. fix(fix486g): <minimal general production repair>, only if red contract proves it
4. docs(fix486g): publish focused current-SHA evidence
```

Push after every verified commit.

Do not leave a proven fix only in the local working tree.

---

# Required result artifacts

Publish compact branch-owned summaries under:

```text
docs/fix486/phase-g-graph-parent-runtime/current-sha-repair/
```

Required files:

```text
CURRENT_SHA_DIAGNOSIS.md
CURRENT_SHA_TARGETED_RESULTS.json
CURRENT_SHA_BLOCKER_MATRIX.json
```

Include:

```text
old evidence source SHA
current tested SHA
whether old run was stale
current-SHA parent/binding counters
current-SHA survivor counters
projected metrics
changed production functions
focused tests
remaining blockers
merge recommendation
```

Do not commit large raw runtime evidence bundles.

---

# Final Codex response

Report:

```text
repository
branch
base SHA
final tested SHA
working tree
commit SHAs
PR URL

old evidence source SHA
old evidence current/stale classification

current-SHA positive Graph observations
wrong-parent count
binding-invalid count
valid-survivor-lost count

GraphParentRecall@1 projected
GraphParentRecall@3 projected
GraphParentRecall@5 projected
MRR projected
nDCG@5 projected
DirectPreservationRate projected

red contracts
production root cause
production files changed
focused test results
CI result

official run ID, only if executed
overall verdict
merge recommendation
```

Allowed final classifications:

```text
CURRENT_SHA_ALREADY_FIXED
CURRENT_SHA_REPAIR_PASS
CURRENT_SHA_REPAIR_BLOCKED
```

Do not claim `FIX486G_PASS` unless the complete current-SHA official campaign passes.