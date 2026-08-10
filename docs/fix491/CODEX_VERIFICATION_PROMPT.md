# FIX491 Codex Verification Prompt

Work in `alimbetov/llm2` on branch `agent/fix491-persistence-recovery`.

This is an independent verification task. Do not assume the implementation is correct because tests exist or because recovery commands return zero.

Read fully:

1. `docs/fix491/TECHNICAL_SPECIFICATION.md`
2. `docs/fix491/ACCEPTANCE_CRITERIA.md`
3. `docs/fix491/CODEX_EXECUTION_TASK.md`
4. implementation diff versus FIX490 base
5. `src/outbox/mod.rs`
6. `src/reconciliation/mod.rs`
7. `src/qdrant/mod.rs`
8. `src/recovery/mod.rs`
9. current persistence code and all migrations

## Verification objective

Prove or disprove both statements on the tested SHA:

```text
A. AstraVector PostgreSQL schema/canonical state is reproducible/auditable without hidden working-DB dependencies.
B. AstraVector Qdrant can be completely lost and deterministically rebuilt from PostgreSQL canonical state without retrieval/safety regression.
```

## Baseline

Record:

- branch;
- tested SHA;
- merge-base with FIX490 base;
- git status;
- exact changed files;
- PostgreSQL image/server version;
- pgvector version;
- Qdrant version;
- model/tokenizer identity used by runtime proof.

## Static architecture audit

Verify implementation does not:

- make Qdrant canonical;
- re-embed persisted representations during rebuild;
- introduce duplicate collection configuration;
- introduce a third handwritten Qdrant payload/projection builder;
- create a parallel repair engine instead of reusing/extending reconciler;
- rewrite historical completed outbox events;
- alter retrieval/chunking/BGE-M3/Graph/MMR/access/lifecycle semantics;
- silently auto-repair a working PostgreSQL DB during drift audit;
- silently destroy a non-empty Qdrant collection;
- run destructive/full rebuild concurrently with normal publisher without a proven fence.

## Shared projection contract verification

This is mandatory.

Inspect outbox, reconciliation and rebuild code. Prove they use one canonical projection builder/path, or an equivalent shared implementation that cannot drift independently.

For the same canonical PostgreSQL binding/embedding input compare:

- point ID;
- dense vector;
- sparse indices/values;
- complete payload field set and values;
- version fields;
- expiry/lifecycle/access fields;
- provenance/quality fields that are part of current payload contract.

Fail verification if recovery uses a separate handwritten payload contract.

## PostgreSQL verification

On disposable PostgreSQL 16 + pgvector:

1. Confirm there are no AstraVector schema objects.
2. Apply all repository migrations.
3. Verify `_sqlx_migrations` versions, success and checksums.
4. Execute semantic schema inventory/audit.
5. Verify partitions, indexes/partial predicates, constraints, defaults/identity, extensions and all runtime-required objects.
6. Verify extension/server versions are recorded.
7. Introduce controlled material drift and prove MATERIAL_DRIFT.
8. Introduce unknown/pending/checksum-mismatch migration cases and prove fail-closed behavior.
9. Verify audit is read-only by fingerprinting/catalog state before and after audit.
10. Run canonical-data integrity checks over document/chunk/cache/binding/outbox/access-zone/lifecycle relationships.
11. Verify failed/dead outbox and deletion-fenced/in-progress states are surfaced rather than silently repaired.
12. If possible, compare against developer working DB read-only; never mutate it.

Do not treat successful migration alone as PASS.

## PostgreSQL restore boundary verification

Review the runbook and ensure it distinguishes:

```text
migrations -> schema recovery
backup/PITR -> canonical row recovery
```

RPO/RTO boundary must be explicit. AstraVector must not claim to implement PostgreSQL backup/PITR itself.

## Qdrant collection-schema verification

Prove `collection_exists=true` is not treated as compatibility proof.

Verify audit covers:

- collection name;
- dense vector dimension/names;
- distance metric;
- sparse config;
- payload indexes and their types;
- materially relevant collection config;
- Qdrant server version.

Create an intentionally incompatible disposable collection and prove fail-closed behavior.

## Qdrant rebuild verification

Using known-good canonical PostgreSQL data:

1. Capture collection configuration and retrieval baseline.
2. Capture deterministic PostgreSQL canonical fingerprint/counters.
3. Delete only the target Qdrant collection.
4. Prove PostgreSQL fingerprint is unchanged immediately after loss.
5. Run FIX491 rebuild.
6. Verify no inference/model execution was used to regenerate persisted vectors.
7. Verify missing required persisted embedding is classified/fails rather than triggering re-embedding.
8. Verify recreated collection config and payload indexes.
9. Run projection audit.
10. Require missing eligible points=0, orphan points=0 after clean destructive rebuild, payload mismatch=0, representation/version mismatch=0.
11. Rerun identical retrieval query set.
12. Confirm PostgreSQL canonical fingerprint remains unchanged except explicitly normalized operational fields.

## Eligibility/lifecycle verification

Prove expected-set calculation and rebuild reuse shared lifecycle/searchability semantics rather than a recovery-only handwritten rule set.

Exercise at least:

- ACTIVE searchable binding;
- expired chunk/document/binding;
- deleted/soft-deleted state;
- inactive/non-active document;
- representation type filtering where applicable;
- access-zone validity;
- legal-hold cases around deletion/reconciliation.

Do not assume legal_hold means searchable; verify current semantics.

## Destructive-safety verification

Prove:

- missing collection can rebuild;
- empty compatible collection can rebuild;
- non-empty consistent collection is audit/no-op by default;
- non-empty drifted collection is not destroyed by default;
- incompatible collection is refused;
- explicit destructive replacement requires documented opt-in;
- target DB/collection identity/config/counts are logged before destructive action;
- secrets are absent from logs/evidence.

## Recovery fencing verification

Prove the selected design prevents conflicting full/destructive rebuild with normal publisher/reconciliation or another recovery execution.

Execute a concurrency test showing a second/conflicting recovery is rejected or serialized. If the design is explicitly offline-only, prove the command detects/enforces the required fence rather than merely documenting “stop the runtime”.

## Bounded/restartable rebuild verification

Use enough fixture points to exercise multiple batches.

Verify:

- stable deterministic scan ordering;
- configured batch bound is respected;
- memory behavior is bounded by implementation design;
- point upsert is idempotent;
- interruption after partial progress;
- rerun/resume completes without duplicates/corruption;
- cancellation exits cleanly;
- retry loop is bounded;
- final audit is required before PASS.

## Qdrant audit completeness verification

Force or simulate bounded-scroll incomplete conditions (limit/timeout/loop/error as practical).

A partial/incomplete scan MUST NOT yield `QDRANT_PROJECTION_CONSISTENT`.

Read-only audit must not delete/quarantine orphan points.

## Retrieval parity verification

Frozen query set must include when fixture capabilities allow:

- dense;
- sparse;
- hybrid;
- access-zone/visibility;
- no-answer/hard-negative;
- Graph-enabled query.

Compare before/after:

- response classification;
- context count/order;
- document/version;
- matched/parent chunk IDs;
- access zone;
- text identity;
- representation identity;
- scores with justified tolerance;
- Graph/degradation provenance.

## Startup/readiness review

Verify FIX491 does not equate `Qdrant collection exists` with `projection recovered`.

Test/document missing collection + auto-create behavior. An empty auto-created collection with canonical PostgreSQL data MUST NOT be reported as FIX491 recovery PASS merely because readiness/collection existence succeeds.

Do not require a global readiness redesign unless implementation changed it intentionally and justifies it.

## Full isolated proof

Execute:

```text
EMPTY PostgreSQL
-> migrations
-> migration/schema audit
-> ingestion fixture
-> canonical PostgreSQL state
-> Qdrant projection
-> activation/retrieval baseline
-> PostgreSQL canonical fingerprint
-> destroy Qdrant collection
-> verify fingerprint unchanged
-> rebuild from PostgreSQL
-> complete Qdrant audit
-> retrieval parity
-> PostgreSQL fingerprint unchanged
-> PostgreSQL schema/canonical integrity audit
```

## Regression gates

Run exactly:

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo test --features integration-tests --test e2e_testcontainers -- --nocapture
```

Do not rerun long unrelated FIX489 soak unless recovery changed retrieval semantics.

## Evidence review

Verify/update:

```text
docs/fix491/POSTGRES_RECOVERY_RESULT.md
docs/fix491/QDRANT_RECOVERY_RESULT.md
docs/fix491/PERSISTENCE_RECOVERY_RESULT.md
```

Machine-readable JSON counterparts are mandatory.

Every PASS statement must name the command/test/evidence proving it. Evidence must record scan completeness and recovery fence behavior.

## Fix policy

Review/test/report first. You may apply only minimal FIX491-local fixes for concrete defects found during verification and must add/adjust a test proving the defect.

If fixing requires changing a protected architecture/retrieval invariant, stop with:

`FIX491_BLOCKED_BY_ARCHITECTURE_CHANGE`

## Final verdict

Only:

```text
FIX491_PERSISTENCE_RECOVERY_PASS
FIX491_PERSISTENCE_RECOVERY_FAIL
FIX491_PERSISTENCE_RECOVERY_BLOCKED
```

PASS requires:

- PostgreSQL reproducibility/no material drift;
- canonical-data integrity and read-only audit proof;
- shared Qdrant projection-builder parity;
- collection-schema compatibility proof;
- persisted representation reuse/no inference fallback;
- fenced, bounded, resumable rebuild;
- complete Qdrant audit with zero unexplained drift;
- PostgreSQL fingerprint preservation;
- retrieval parity;
- mandatory regression gates.

At the end report:

1. final verdict;
2. tested SHA;
3. files changed during verification;
4. defects found/fixed;
5. PostgreSQL migration/schema/integrity result;
6. Qdrant collection/projection audit counters and scan status;
7. recovery fence and interruption/resume result;
8. PostgreSQL fingerprint result;
9. retrieval parity result;
10. regression gate results;
11. anything BLOCKED/unverified.

Do not merge to main automatically.