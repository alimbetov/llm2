# Codex execution task — fix486c

## Mission

Implement and prove `fix486c — frozen executable hierarchical bank 1.0.0` from the exact approved baseline:

```text
REPOSITORY=alimbetov/llm2
BASE_BRANCH=main
BASE_SHA=8fd29b2acd166992f953a5020be81b076e581403
WORK_BRANCH=codex/fix486c-frozen-executable-bank-1.0.0
PREVIOUS_VERDICT=FIX486_RUNTIME_BASELINE_PASS
```

Read all files under `docs/fix486/phase-c-frozen-bank/` before modifying code.

## Non-negotiable boundaries

- Do not change query or qrel semantics to match observed runtime output.
- Do not tune ranking, Graph, MMR, RRF, dense, sparse or hybrid behavior.
- Do not claim child/parent, isolation, lifecycle, degradation, Graph, MMR, load or production readiness PASS.
- Do not store runtime-generated IDs inside the frozen bank.
- Do not overwrite bank `1.0.0` after freeze; any semantic bank change requires a new version.
- Keep production fixes in commits separate from bank-freeze content.

## Required work

1. Verify the branch starts from `8fd29b2acd166992f953a5020be81b076e581403` or record and justify an approved newer base.
2. Inspect the existing seed bank and structural tests.
3. Preserve all 10 cases, 11 queries and 11 qrels.
4. Implement canonical byte validation and SHA-256 generation.
5. Update `bank-manifest.json` to `1.0.0 / FROZEN` with complete hashes.
6. Add a fail-closed verifier that detects changed, missing and extra files.
7. Add `fix486c_frozen_bank_contracts.rs`.
8. Add an executable runner/driver that can parse and schedule every query.
9. Add production-path ingestion and external logical-to-runtime identity-map export.
10. Add machine-readable stage and query result schemas.
11. Add `make verify-fix486c-frozen-bank`.
12. Run all mandatory locked gates.
13. Preserve raw evidence outside Git and commit compact summaries only.
14. Produce exactly one final verdict.

## Required execution sequence

```text
source/lineage verification
→ bank structural audit
→ canonical byte audit
→ hash generation
→ manifest freeze
→ hash verification
→ runner dry-run for all queries
→ production-path ingestion preparation
→ runtime identity-map export
→ locked static gates
→ evidence completeness audit
→ final verdict
```

## Mandatory commands

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo test --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo sqlx prepare --check -- --all-targets --all-features
cargo test --locked --test fix486_hierarchical_bank_contracts -- --nocapture
cargo test --locked --test fix486c_frozen_bank_contracts -- --nocapture
python3 scripts/fix486c_verify_frozen_bank.py
make verify-fix486c-frozen-bank
```

## Mandatory evidence

Record:

- source branch/SHA and dirty-worktree result;
- Cargo.lock hash;
- frozen bank per-file hashes;
- bank aggregate hash;
- exact list of frozen files;
- query/case/qrel counts;
- dry-run scheduling result for all 11 queries;
- runner/script hashes;
- release binary, model and tokenizer hashes if a live runtime is started;
- identity-map path and hash;
- exact exit code for every mandatory command;
- defect register;
- stage results and final manifest.

## Defect handling

For every reproducible in-scope P0/P1:

```text
failing evidence
→ failing regression test
→ root cause
→ smallest safe fix in separate commit
→ unchanged bank input
→ rerun failed stage
→ rerun complete Phase C gate
```

If the bank cannot be frozen without changing its intended semantics, stop with:

```text
FIX486_FROZEN_EXECUTABLE_BANK_BLOCKED
```

## Final response format

Report:

1. branch and final SHA;
2. list of changed files;
3. bank version, status and aggregate SHA-256;
4. query/case/qrel counts;
5. mandatory gate matrix;
6. evidence path and manifest hash;
7. unresolved defect count;
8. scope exclusions;
9. exactly one final verdict.
