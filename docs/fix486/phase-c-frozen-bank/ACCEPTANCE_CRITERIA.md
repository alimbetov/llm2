# fix486c acceptance criteria

## Lineage

- [ ] Base lineage resolves to `8fd29b2acd166992f953a5020be81b076e581403` or an approved descendant.
- [ ] Worktree is clean before official evidence creation.
- [ ] Source SHA is recorded in every final manifest.

## Bank structure

- [ ] Bank ID is `fix486-hierarchical-bank`.
- [ ] Version is exactly `1.0.0`.
- [ ] Status is exactly `FROZEN`.
- [ ] Exactly 10 cases exist.
- [ ] Exactly 11 queries exist.
- [ ] Exactly 11 qrels exist.
- [ ] Query IDs are unique.
- [ ] Every query has exactly one qrel.
- [ ] Every qrel references an existing query.
- [ ] All logical parent and child references resolve within their zone.
- [ ] All Graph endpoints resolve.
- [ ] All lifecycle scenarios are named and addressable.

## Immutability and hashes

- [ ] Exactly five payload files are frozen.
- [ ] Every payload follows canonical byte rules.
- [ ] Every per-file SHA-256 is non-null and correct.
- [ ] Aggregate SHA-256 is non-null and correct.
- [ ] No untracked payload files exist below the bank root.
- [ ] Hash verifier fails on a controlled byte mutation.
- [ ] Hash verifier fails on a missing payload file.
- [ ] Hash verifier fails on an extra payload file.

## Executability

- [ ] Runner supports verify-only and dry-run modes.
- [ ] All 11 queries parse in dry-run.
- [ ] All 11 queries produce executable request plans.
- [ ] Every plan preserves access zone, profile, context limit and optional Graph/token settings.
- [ ] Production-path ingestion is implemented or invoked.
- [ ] Runtime-generated identities are written outside the frozen bank.
- [ ] Query results use PASS/FAIL/BLOCKED/SKIPPED.
- [ ] Mandatory skipped stages cannot become PASS.

## Gates

- [ ] `cargo fmt --all --check` PASS.
- [ ] locked all-target check PASS.
- [ ] locked all-target tests PASS.
- [ ] locked clippy PASS.
- [ ] SQLx prepare check PASS.
- [ ] existing fix486 bank contracts PASS.
- [ ] new fix486c frozen-bank contracts PASS.
- [ ] Python frozen-bank verifier PASS.
- [ ] Makefile aggregate gate PASS.

## Evidence

- [ ] External evidence directory is created.
- [ ] Evidence manifest contains every required identity.
- [ ] Stage results are machine-readable.
- [ ] Query dry-run results are machine-readable.
- [ ] Defect register is present.
- [ ] Evidence completeness audit PASS.
- [ ] Compact result documents are committed.

## Boundaries

- [ ] No qrel was adjusted to match runtime output.
- [ ] No retrieval tuning was introduced.
- [ ] No downstream Phase D–I verdict was claimed.
- [ ] No unresolved in-scope P0/P1 remains.

## Final verdict

All items must pass for:

```text
FIX486_FROZEN_EXECUTABLE_BANK_PASS
```

Any mandatory failure, omission, hash mismatch, skipped gate, evidence gap or unresolved P0/P1 produces:

```text
FIX486_FROZEN_EXECUTABLE_BANK_BLOCKED
```
