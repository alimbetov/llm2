# Codex Execution Task — FIX486E Isolation and Lifecycle Runtime Proof

## Objective

Implement and execute Phase E as a source-bound, fail-closed runtime proof for:

```text
FIX486-03 access-zone isolation
FIX486-04 canonical active-version filtering
```

Final allowed verdicts:

```text
FIX486_ISOLATION_LIFECYCLE_RUNTIME_PROOF_PASS
FIX486_ISOLATION_LIFECYCLE_RUNTIME_PROOF_BLOCKED
```

Do not merge the Phase E pull request in the same task that first produces official evidence.

## Repository context

```text
repository: alimbetov/llm2
branch: codex/fix486e-isolation-lifecycle-runtime-proof
base SHA: 377852cc6d7ff315b8d7eb27762672d794fd7a9c
```

Frozen bank:

```text
version: 1.0.0
status: FROZEN
aggregate SHA-256: cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff
```

Read all Phase E documents before implementation:

```text
TECHNICAL_SPECIFICATION.md
ISOLATION_AND_LIFECYCLE_PROOF_CONTRACT.md
EXECUTION_AND_EVIDENCE_CONTRACT.md
ACCEPTANCE_CRITERIA.md
RESULT_TEMPLATE.md
```

## Phase boundary

In scope:

```text
q-zone-a
q-zone-b
q-active-version
opposite-zone negative controls
inactive/deleted/expired final-result exclusion
legal-hold audit
warm repeat
runtime restart proof
```

Out of scope:

```text
stale/orphan Qdrant injection
hydration timeout failpoints
graph relation correctness
MMR and token-budget tuning
large-parent anti-starvation
ranking-weight tuning
```

## Step 1 — Inspect current repository

```bash
cd /Users/ruslanalimbetov/Documents/llm2/astravector

git branch --show-current
git status -sb
git rev-parse HEAD
git log --oneline --decorate -10
```

Confirm the branch is:

```text
codex/fix486e-isolation-lifecycle-runtime-proof
```

Inspect Phase D implementation for reusable patterns, but do not copy Phase D assumptions blindly.

Relevant existing files may include:

```text
scripts/fix486d-child-parent-runtime-proof.sh
scripts/fix486d_proof.py
scripts/fix486d-child-parent-audit.sql
docker-compose.fix486d.yml
config/application-fix486d.yaml
tests/fix486d_child_parent_contracts.rs
```

Reuse generic infrastructure only when isolation between phases remains explicit.

## Step 2 — Design before coding

Produce a short implementation note covering:

```text
Phase E file list
runtime zone mapping source
lifecycle setup mechanism
recorded test clock strategy
opposite-zone control mechanism
candidate/hydration/final isolation evidence
legal-hold audit mechanism
warm and restart sequence
cleanup ownership model
```

Do not modify frozen payload to simplify setup.

## Step 3 — Implement Phase E runner

Expected files:

```text
scripts/fix486e-isolation-lifecycle-runtime-proof.sh
scripts/fix486e_proof.py
scripts/fix486e-isolation-lifecycle-audit.sql
docker-compose.fix486e.yml
config/application-fix486e.yaml
tests/fix486e_isolation_lifecycle_contracts.rs
Makefile
```

Add canonical Make target:

```text
verify-fix486e-isolation-lifecycle-runtime
```

Add compatibility alias if useful:

```text
verify-fix486e-isolation-lifecycle-runtime-proof
```

Both must invoke the same official `--execute-all` path.

## Step 4 — Runner bootstrap and failure semantics

Before preflight, create:

```text
bootstrap.json
stage-results.json
terminal-result.json placeholder
runner stdout/stderr logs
```

Support:

```text
FIX486E_RUN_ID
```

Install traps for:

```text
EXIT ERR INT TERM HUP
```

Preserve original exit code and signal through finalization and cleanup.

A missing Make target, dirty worktree, port collision, or early setup failure must still leave evidence.

## Step 5 — Phase-owned environment

Use a unique Docker Compose project and unique:

```text
PostgreSQL database/schema
Qdrant collection
network
volumes
runtime ports
temporary configuration
```

Do not reuse Phase D state.

Before startup, inspect port ownership for gRPC and metrics. Abort on foreign owner.

## Step 6 — Static and contract tests

Add focused contracts for at least:

1. canonical and alias Make targets;
2. bootstrap and terminal evidence;
3. frozen hash enforcement;
4. two-zone composite identity;
5. same logical IDs produce distinct physical IDs per zone;
6. q-zone-a forbidden Zone B anchors;
7. q-zone-b forbidden Zone A anchors;
8. opposite-zone control completeness;
9. active v1 allowed;
10. v2 INDEXING forbidden;
11. v3 DELETED forbidden;
12. v4 EXPIRED forbidden;
13. recorded test clock requirement;
14. legal hold cannot bypass visibility;
15. exact primary row count of six;
16. exact opposite-zone row count of four;
17. Search/RetrieveContext parity;
18. evidence leak scan;
19. warm and restart comparison completeness;
20. fail-closed aggregate verdict.

Run during development:

```bash
python3 -m py_compile scripts/fix486e_proof.py
bash -n scripts/fix486e-isolation-lifecycle-runtime-proof.sh
cargo fmt --all --check
cargo test --locked --test fix486e_isolation_lifecycle_contracts -- --nocapture
```

## Step 7 — Commit implementation before official proof

Inspect changes:

```bash
git status -sb
git diff --stat
git diff
```

Stage only Phase E implementation and required Make changes.

Do not use `git add -A` when unrelated files exist.

Suggested commit:

```bash
git commit -m "fix486e: add isolation and lifecycle runtime proof"
```

The official proof requires a clean commit. A dirty-tree failure is expected and correct before commit.

## Step 8 — Full pre-proof gates

From the clean implementation SHA run:

```bash
python3 -m py_compile scripts/fix486e_proof.py
bash -n scripts/fix486e-isolation-lifecycle-runtime-proof.sh
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo test --locked --test fix486_hierarchical_bank_contracts -- --nocapture
cargo test --locked --test fix486c_frozen_bank_contracts -- --nocapture
cargo test --locked --test fix486d_child_parent_contracts -- --nocapture
cargo test --locked --test fix486e_isolation_lifecycle_contracts -- --nocapture
```

All must return exit code zero.

## Step 9 — Frozen-bank verification

Verify before runtime:

```text
bank version = 1.0.0
status = FROZEN
aggregate SHA-256 unchanged
five payload hashes unchanged
```

On mismatch, preserve BLOCKED evidence and stop.

Do not update hashes.

## Step 10 — Runtime zone setup

Create or resolve:

```text
zone-a -> 4862
zone-b -> 4863
```

Capture actual setup responses and canonical rows.

The proof must demonstrate that the same logical IDs across zones have different physical identities.

## Step 11 — Production ingestion

Ingest both zone documents through supported production ingestion paths.

Create the Zone A lifecycle versions:

```text
v1 ACTIVE
v2 INDEXING
v3 DELETED
v4 EXPIRED relative to the official test clock
```

Represent legal hold on active v1 according to supported canonical semantics.

Prefer APIs. Any controlled SQL setup must be explicitly documented and isolated.

Do not inject a stale Qdrant child for v3.

## Step 12 — Wait for readiness

Before query execution wait for:

```text
migrations complete
Health PASS
metrics endpoint PASS
required active bindings SYNCED
required outbox COMPLETED
Qdrant active points present
failed outbox = 0
dead letters = 0
```

Record binary, config, model, tokenizer, image, migration, and Cargo.lock identities.

## Step 13 — Canonical and Qdrant audits

Run the read-only PostgreSQL audit and Qdrant audit.

Canonical audit must show:

```text
both zones
Zone A v1-v4 states
recorded expires_at and test clock
legal hold
chunk/binding/outbox counts
no duplicate or cross-zone bindings
```

Qdrant audit must show zone-scoped payload and point counts.

Use canonical `content_hash`. Do not add `pgcrypto` dependency.

## Step 14 — Mandatory frozen query campaign

Run through Search and RetrieveContext:

```text
q-zone-a in zone-a
q-zone-b in zone-b
q-active-version in zone-a
```

Required primary rows:

```text
6/6
```

### q-zone-a

Required:

```text
zone-a / 4862
version 1
parent-a1
ASTRA_CANONICAL_STATE_A1
```

Forbidden:

```text
ZONE_B_SECRET_PARENT_A1
ZONE_B_PRIVATE_SOURCE
```

### q-zone-b

Required:

```text
zone-b / 4863
version 1
parent-a1 scoped to Zone B
ZONE_B_SECRET_PARENT_A1
```

Forbidden:

```text
ASTRA_CANONICAL_STATE_A1
ASTRA_LEGAL_HOLD_A2
```

### q-active-version

Required:

```text
zone-a / 4862
doc-hierarchy
version 1
parent-a1
```

Forbidden:

```text
versions 2, 3, 4
ASTRA_INACTIVE_VERSION_TRAP
ASTRA_DELETED_PARENT_TRAP
ASTRA_EXPIRED_PARENT_TRAP
```

## Step 15 — Opposite-zone controls

Execute through both entry points:

```text
q-zone-a question under zone-b
q-zone-b question under zone-a
```

Required supplemental rows:

```text
4/4
```

No foreign-zone content or physical identity may appear in final or hydrated evidence.

An explicit no-answer is acceptable. Transport or normalizer failure is not.

## Step 16 — Lifecycle trap probes

Execute exact probes for:

```text
ASTRA_INACTIVE_VERSION_TRAP
ASTRA_DELETED_PARENT_TRAP
ASTRA_EXPIRED_PARENT_TRAP
```

Prove zero final contexts and classify the exclusion path for each version:

```text
NOT_PROJECTED
FILTERED_AT_CANDIDATE_QUERY
REJECTED_AT_CANONICAL_HYDRATION
REJECTED_AT_FINAL_VISIBILITY
```

No UNKNOWN classification is allowed.

## Step 17 — Isolation evidence at every layer

Collect evidence for:

```text
candidate generation
Qdrant filters
canonical hydration
graph expansion when invoked
final context assembly
telemetry and logs
```

All hard-gate counters must be zero:

```text
cross_zone_candidates_promoted
cross_zone_hydrations
cross_zone_final_contexts
cross_zone_graph_results
cross_zone_evidence_leaks
```

Do not log unredacted foreign-zone content in wrong-zone traces.

## Step 18 — Search/RetrieveContext parity

Normalize and compare all three mandatory queries.

Zone, version, logical document, logical parent, required anchors, and forbidden counts must match.

Differences in timings, trace IDs, request IDs, and floating-point score representation are allowed.

## Step 19 — Warm repeat

Without re-ingestion, rerun all six mandatory requests.

Compare logical outputs and canonical/Qdrant counts.

No new document versions, chunks, bindings, outbox effects, or points may be created.

## Step 20 — Runtime restart proof

Restart only AstraVector. Preserve PostgreSQL and Qdrant.

After Health and metrics pass, rerun:

```text
six mandatory requests
four opposite-zone controls
lifecycle audit
legal-hold audit
```

Isolation and lifecycle semantics must remain unchanged.

## Step 21 — Legal-hold proof

Show:

```text
active v1 hold state present
active v1 retrievable
cleanup protection effective
v2/v3/v4 remain non-searchable
state survives warm and restart
```

Do not implement hold release in Phase E.

## Step 22 — Evidence integrity

Generate the artifact set required by `EXECUTION_AND_EVIDENCE_CONTRACT.md`.

Verify:

```text
six primary rows
four opposite-zone rows
all mandatory stages present
all checksums valid
manifest complete
terminal exit code zero
```

Record the external evidence path and separate manifest SHA-256.

## Step 23 — Handle defects

On any failure:

1. preserve BLOCKED evidence;
2. identify the exact last completed and failing stages;
3. classify runner/evidence/setup/production defect;
4. inspect canonical and Qdrant partial state;
5. do not modify frozen bank or qrels;
6. implement the smallest fix;
7. add regression coverage;
8. commit;
9. obtain clean worktree;
10. repeat all gates and the full official proof.

No previous evidence run can certify a later source SHA.

## Step 24 — Publish after PASS only

After a complete PASS and clean worktree:

```bash
git push -u origin codex/fix486e-isolation-lifecycle-runtime-proof
```

Update the draft pull request with:

```text
tested source SHA
evidence run ID
manifest SHA-256
frozen aggregate SHA
zone mapping
lifecycle state summary
primary rows 6/6
opposite-zone controls 4/4
Search/RetrieveContext parity
warm proof
restart proof
legal-hold audit
hard-gate counters
final verdict
```

Add an evidence comment. Keep the PR draft and do not merge.

## Expected PASS response

```text
FIX486E official runtime proof completed

Repository:
alimbetov/llm2

Branch:
codex/fix486e-isolation-lifecycle-runtime-proof

Tested source SHA:
<sha>

Evidence run:
<run-id>

Manifest SHA-256:
<sha>

Frozen bank:
1.0.0 / FROZEN

Zone mapping:
zone-a=4862
zone-b=4863

Primary results:
6/6 PASS

Opposite-zone controls:
4/4 PASS

Search/RetrieveContext parity:
PASS

Active-version proof:
PASS

Legal-hold audit:
PASS

Warm repeat:
PASS

Restart proof:
PASS

Hard-gate violations:
0

Evidence integrity:
PASS

Branch pushed:
true

PR updated:
true

PR merged:
false

Verdict:
FIX486_ISOLATION_LIFECYCLE_RUNTIME_PROOF_PASS
```

## Expected BLOCKED response

```text
FIX486E official runtime proof blocked

Current SHA:
<sha>

Evidence run:
<run-id>

Last completed stage:
<stage>

Failing stage:
<stage>

Failure code:
<code>

Zone/lifecycle state:
<summary>

Primary rows:
<n>/6

Opposite-zone rows:
<n>/4

Hard-gate violations:
<counts>

Evidence directory:
<path>

Evidence preserved:
true

Branch pushed:
false

PR updated:
false

Verdict:
FIX486_ISOLATION_LIFECYCLE_RUNTIME_PROOF_BLOCKED
```