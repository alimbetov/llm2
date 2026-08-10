# FIX491 Codex Verification Prompt

Work in `alimbetov/llm2` on branch `agent/fix491-persistence-recovery`.

This is an independent verification task. Do not assume the implementation is correct because tests exist or because recovery commands return zero.

Read fully:

1. `docs/fix491/TECHNICAL_SPECIFICATION.md`
2. `docs/fix491/ACCEPTANCE_CRITERIA.md`
3. `docs/fix491/CODEX_EXECUTION_TASK.md`
4. implementation diff versus FIX490 base
5. current migrations, persistence, outbox, recovery, reconciliation and Qdrant code

## Verification objective

Prove or disprove both statements on the tested SHA:

```text
A. AstraVector PostgreSQL schema is reproducible from repository migrations and the working DB has no material hidden schema drift.
B. AstraVector Qdrant can be completely lost and deterministically rebuilt from PostgreSQL canonical state without retrieval/safety regression.
```

## Baseline

Record:

- branch;
- tested SHA;
- merge-base with main/FIX490 base;
- git status;
- exact changed files;
- PostgreSQL image/version;
- pgvector version;
- Qdrant version;
- model/tokenizer identity used by runtime proof.

## Static architecture audit

Verify implementation does not:

- make Qdrant canonical;
- re-embed persisted representations during rebuild when stored vectors exist;
- introduce duplicate collection configuration;
- rewrite historical completed outbox events;
- alter retrieval/chunking/BGE-M3/Graph/MMR/access/lifecycle semantics;
- silently auto-repair a working PostgreSQL DB during drift audit;
- silently destroy a non-empty Qdrant collection.

## PostgreSQL verification

On disposable PostgreSQL 16 + pgvector:

1. Confirm there are no AstraVector schema objects.
2. Apply all repository migrations.
3. Verify `_sqlx_migrations` versions, success and checksums.
4. Execute schema inventory/audit.
5. Verify partitions, indexes/partial predicates, constraints, defaults, extensions and all runtime-required objects.
6. Introduce a controlled material drift in a disposable copy and prove audit fails/classifies it as MATERIAL_DRIFT.
7. If possible, compare against the developer's current working DB read-only; never mutate it.
8. Prove an unknown/checksum-mismatched migration history is rejected in a disposable DB.

Do not treat a successful `cargo sqlx migrate run` alone as PASS.

## Qdrant verification

Using known-good canonical PostgreSQL data:

1. Capture collection configuration and retrieval baseline.
2. Capture PostgreSQL canonical counters/hashes sufficient to prove PostgreSQL is unchanged by Qdrant loss.
3. Delete only the target Qdrant collection in disposable/local proof environment.
4. Run FIX491 rebuild.
5. Verify recreated collection config including dense/sparse and payload indexes.
6. Run projection audit.
7. Require:
   - missing eligible points = 0;
   - orphan points = 0;
   - payload mismatch = 0;
   - representation-version mismatch = 0.
8. Rerun the same retrieval query set and compare identity/order/scores with documented tolerance.
9. Verify access-zone/access-level/version/lifecycle correctness is unchanged.

## Destructive-safety verification

Prove:

- missing collection can rebuild;
- empty compatible collection can rebuild;
- non-empty existing collection is refused by default;
- explicit destructive replacement requires the documented opt-in;
- target database/collection identity is logged before destructive action.

## Outbox/reconciliation verification

Inspect PostgreSQL before and after rebuild.

Verify recovery does not falsify old completed outbox history and does not create orphan bindings. Verify the selected recovery strategy is consistent with existing reconciliation semantics.

## Full proof

Execute the full isolated path:

```text
EMPTY PostgreSQL
-> migrations
-> ingestion fixture
-> PostgreSQL canonical state
-> Qdrant projection
-> activation/retrieval baseline
-> destroy Qdrant collection
-> rebuild
-> projection audit
-> retrieval parity
-> PostgreSQL schema audit
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

Do not rerun long unrelated FIX489 soak unless recovery changes retrieval semantics.

## Evidence review

Verify and update as needed:

```text
docs/fix491/POSTGRES_RECOVERY_RESULT.md
docs/fix491/QDRANT_RECOVERY_RESULT.md
docs/fix491/PERSISTENCE_RECOVERY_RESULT.md
```

Every PASS statement must name the command/test/evidence that proves it.

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

PASS requires both PostgreSQL reproducibility/no material drift and Qdrant destruction/rebuild retrieval parity, plus all mandatory regression gates.

At the end report:

1. final verdict;
2. tested SHA;
3. files changed during verification;
4. defects found/fixed;
5. actual PostgreSQL drift result;
6. actual Qdrant projection audit counters;
7. actual retrieval parity result;
8. regression gate results;
9. anything BLOCKED/unverified.

Do not merge to main automatically.