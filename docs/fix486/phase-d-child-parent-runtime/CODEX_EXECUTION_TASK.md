# Codex Execution Task — fix486d Child/Parent Runtime Proof

## Mission

Implement and execute Phase D against the immutable hierarchical bank `1.0.0`.

Return exactly one final verdict:

```text
FIX486_CHILD_PARENT_RUNTIME_PROOF_PASS
```

or:

```text
FIX486_CHILD_PARENT_RUNTIME_PROOF_BLOCKED
```

Do not claim production readiness or completion of phases E–J.

## Approved lineage

```text
repository=alimbetov/llm2
base_branch=main
approved_base_sha=9de9383d6cfef3b1ed32637688907a55429b3cf3
work_branch=codex/fix486d-child-parent-runtime-proof
previous_verdict=FIX486_FROZEN_EXECUTABLE_BANK_PASS
bank_version=1.0.0
bank_status=FROZEN
bank_aggregate_sha256=cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff
```

Resolve actual local and remote identities before implementation. If the branch is based on a newer approved `main`, record the complete lineage and verify that the bank aggregate is unchanged.

## Required reading

Read these documents before editing code:

```text
docs/fix486/phase-d-child-parent-runtime/TECHNICAL_SPECIFICATION.md
docs/fix486/phase-d-child-parent-runtime/CHILD_PARENT_PROOF_CONTRACT.md
docs/fix486/phase-d-child-parent-runtime/EXECUTION_AND_EVIDENCE_CONTRACT.md
docs/fix486/phase-d-child-parent-runtime/ACCEPTANCE_CRITERIA.md
docs/fix486/phase-d-child-parent-runtime/RESULT_TEMPLATE.md
```

Also inspect:

```text
benchmarks/hierarchical/fix486/bank-manifest.json
benchmarks/hierarchical/fix486/queries/hierarchical-queries-v1.jsonl
benchmarks/hierarchical/fix486/qrels/hierarchical-qrels-v1.jsonl
benchmarks/hierarchical/fix486/corpus/hierarchical-fixture-v1.json
scripts/fix486c-frozen-bank.sh
scripts/fix486c_verify_frozen_bank.py
existing quality runtime runner and ranking trace code
Search and RetrieveContext production implementation
PostgreSQL hierarchy persistence and parent hydration code
Qdrant binding/projection code
```

## Scope

Implement a reproducible, fail-closed proof for exactly these frozen queries:

```text
q-child-parent-exact
q-parent-dedup
q-exact-identifier
```

Execute each through:

```text
Search
RetrieveContext
```

Primary result count:

```text
3 queries × 2 entry points = 6 mandatory results
```

## Non-goals

Do not:

- modify frozen bank files;
- change qrels or query text;
- tune dense/sparse/hybrid weights;
- tune RRF, MMR or Graph;
- weaken access-zone filters;
- add failure injection for Phase F;
- certify lifecycle/isolation/Graph/token-budget/load behavior;
- replace model-backed runtime with mocks;
- use direct SQL to create successful fixture hierarchy;
- declare `PRODUCTION_READY`.

## Step 1 — Confirm branch and identities

Run:

```bash
cd /Users/ruslanalimbetov/Documents/llm2/astravector
git fetch origin
git branch --show-current
git status -sb
git rev-parse HEAD
git rev-parse origin/main
git log --oneline --decorate -15
```

Required:

```text
branch=codex/fix486d-child-parent-runtime-proof
working_tree=clean before official evidence
```

Run the frozen-bank verifier and prove the aggregate SHA is unchanged.

## Step 2 — Inspect existing observability before adding fields

Determine whether current Search/RetrieveContext responses and ranking traces already expose:

```text
matched_chunk_id
parent_chunk_id
source_block_id
document/version/zone identity
matched child text
parent text
retrieval sources
exact_technical_match
sparse/lexical scores
parent dedup stage or reason
```

Prefer existing diagnostics. Add narrowly scoped identifier-level diagnostics only when required evidence cannot be derived.

Any diagnostic addition must:

- preserve runtime behavior;
- be bounded;
- avoid leaking unrestricted content;
- have regression contracts;
- be disabled or appropriately sampled outside explicit diagnostics if production cost is material.

## Step 3 — Add Phase D runner

Recommended implementation:

```text
scripts/fix486d-child-parent-runtime-proof.sh
```

Add Make target:

```text
verify-fix486d-child-parent-runtime
```

Required runner modes:

```text
--verify-identities
--prepare
--ingest
--execute-search
--execute-retrieve-context
--repeat
--restart-proof
--execute-all
```

The runner must use `set -Eeuo pipefail`, preserve exit codes and produce explicit stage results.

## Step 4 — Reuse Phase C materialization without changing the bank

Use the production-safe tokenizer-aware materialization implemented in Phase C.

Do not rewrite frozen payload files or hashes.

Ingest through the production facade, wait for document operations and outbox projection, then generate a logical-to-runtime identity map.

## Step 5 — Add canonical read-only audit

Recommended file:

```text
scripts/fix486d-child-parent-audit.sql
```

The audit must prove for each returned pair:

```text
child exists
parent exists
child.parent_chunk_id = parent.id
same zone
same document
same version
visible/active production state
child granularity is expected
parent granularity is PARENT
source-block provenance is available
```

Also report violations:

```text
orphan children
cross-zone bindings
cross-document bindings
cross-version bindings
duplicate identity/provenance rows
```

All violation counts must be zero.

## Step 6 — Add result normalizer

Create a deterministic normalizer in Rust, Python or shell+jq.

It must preserve semantic identity and remove only volatile fields.

Normalized result must contain:

```text
query ID
case ID
entry point
logical zone/document/version
logical matched child
logical parent
runtime matched child ID
runtime parent ID
matched required anchors
parent required anchors
forbidden anchors found
status/failure codes
```

## Step 7 — Implement FIX486-01 proof

For both entry points prove:

```text
matched child is child-a1-180 or child-a1-260
parent is parent-a1
matched text contains ORA-00904
matched text contains content_chunks_v004
parent text contains ASTRA_CANONICAL_STATE_A1
matched and parent IDs are distinct
canonical binding exists
forbidden anchors absent
```

Do not accept the anchors when they appear only in the query echo, logs or unrelated context.

## Step 8 — Implement FIX486-02 dedup proof

For both entry points prove:

```text
at least two eligible child candidates map to parent-a1 before final parent dedup
final parent-a1 occurrence count = 1
final duplicate parent contexts = 0
```

Use current ranking trace if sufficient. Otherwise add a narrow parent-dedup trace stage.

Do not change candidate scores, limits or final ranking merely to make the trace convenient. If existing production diagnostic depth cannot expose the required pre-dedup set, document the blocker before altering behavior.

## Step 9 — Implement FIX486-07 exact-child proof

For both entry points prove:

```text
matched text contains /api/v1/search
matched text contains parent_chunk_id
exact_technical_match=true
sparse_score or lexical_score present
matched_child_evidence_lost=0
parent is parent-a1
```

The exact identifiers must be associated with matched-child evidence.

## Step 10 — Compare Search and RetrieveContext

Create normalized comparison artifacts per query.

Require:

```text
same zone
same document
same version
same expected parent
same required-anchor outcome
same forbidden-anchor outcome
same child, or two qrel-allowed children with equivalent required evidence and explicit explanation
```

A different parent or missing child evidence is a hard failure.

## Step 11 — Warm and restart repeatability

Run all six primary executions twice without reingestion, then restart the runtime and repeat.

Record:

```text
pre-repeat normalized results
warm-repeat normalized results
pre-restart normalized results
post-restart normalized results
comparisons
```

Do not normalize away identity drift.

## Step 12 — Add focused contracts

Recommended test file:

```text
tests/fix486d_child_parent_contracts.rs
```

Required contract coverage:

1. exact three-query selection from frozen bank;
2. exactly six primary results required;
3. identity-map required fields;
4. canonical child-parent binding assertion;
5. FIX486-01 anchor separation between child and parent;
6. FIX486-02 pre/post dedup assertions;
7. FIX486-07 exact-child evidence assertion;
8. Search/RetrieveContext parity;
9. mandatory skip blocks verdict;
10. missing evidence blocks verdict;
11. bank aggregate mismatch blocks before runtime execution;
12. infrastructure/model errors cannot become no-answer PASS.

Add regression tests before production fixes for every discovered P0/P1.

## Step 13 — Run mandatory gates

At minimum:

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo sqlx prepare --check -- --all-targets --all-features
cargo test --locked --test fix486_hierarchical_bank_contracts -- --nocapture
cargo test --locked --test fix486c_frozen_bank_contracts -- --nocapture
cargo test --locked --test fix486d_child_parent_contracts -- --nocapture
```

Preserve exact output and exit codes in external evidence.

## Step 14 — Execute official proof

Use a clean evidence root, for example:

```text
/Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix486d/<run-id>
```

Run the full proof:

```bash
./scripts/fix486d-child-parent-runtime-proof.sh \
  --execute-all \
  --evidence-root <root> \
  --run-id <run-id>
```

The runner must create all mandatory artifacts and a hash-verified manifest.

## Step 15 — Defect handling

For an in-scope P0/P1:

1. stop the final PASS attempt;
2. preserve failing evidence;
3. add a failing regression test;
4. localize root cause from code/metrics/state;
5. implement the smallest production-safe fix in a separate commit;
6. keep frozen bank and qrels unchanged;
7. rerun the failed stage;
8. rerun all static gates and the complete D1–D5 campaign;
9. record before/after evidence.

Do not mask defects by increasing result limits, changing query wording, changing qrels or disabling validation.

## Step 16 — Repository result outputs

On successful official execution, add compact files:

```text
docs/fix486/phase-d-child-parent-runtime/RESULT.md
docs/fix486/phase-d-child-parent-runtime/STAGE_RESULTS_SUMMARY.json
docs/fix486/phase-d-child-parent-runtime/QUERY_RESULTS_SUMMARY.json
docs/fix486/phase-d-child-parent-runtime/IDENTITY_MAP_SUMMARY.json
docs/fix486/phase-d-child-parent-runtime/MANIFEST_POINTER.json
docs/fix486/phase-d-child-parent-runtime/DEFECT_REGISTER.json
```

Do not commit full external evidence.

## Step 17 — Final self-review

Before declaring PASS, inspect:

```bash
git status -sb
git diff --stat <approved-base>..HEAD
git diff <approved-base>..HEAD
git log --oneline <approved-base>..HEAD
```

Confirm:

- no frozen bank payload changed;
- no unrelated files changed;
- no ranking/Graph/MMR tuning occurred;
- all evidence identities match tested HEAD;
- no unresolved P0/P1 remains;
- all six primary results PASS;
- manifest integrity PASS.

## Required final response

### PASS form

```text
FIX486D child/parent runtime proof completed

Repository: alimbetov/llm2
Branch: codex/fix486d-child-parent-runtime-proof
Tested source SHA: <sha>
Bank: 1.0.0 / FROZEN
Aggregate SHA-256: cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff

FIX486-01 Search: PASS
FIX486-01 RetrieveContext: PASS
FIX486-02 Search: PASS
FIX486-02 RetrieveContext: PASS
FIX486-07 Search: PASS
FIX486-07 RetrieveContext: PASS

Search/RetrieveContext parity: PASS
Warm repeatability: PASS
Restart repeatability: PASS
Evidence completeness: PASS
Unresolved P0/P1: 0

Verdict: FIX486_CHILD_PARENT_RUNTIME_PROOF_PASS
Next phase: fix486e — isolation/lifecycle proof
```

### BLOCKED form

```text
FIX486D child/parent runtime proof blocked

Blocking stage: <stage>
Failure code: <code>
Exact failure: <description>
Tested source SHA: <sha>
Bank aggregate unchanged: true|false
Evidence preserved: true
Unresolved P0/P1: <count>

Verdict: FIX486_CHILD_PARENT_RUNTIME_PROOF_BLOCKED
```
