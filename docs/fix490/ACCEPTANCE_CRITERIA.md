# FIX490 Acceptance Criteria

FIX490 is accepted only when every mandatory criterion below is satisfied and the final result records exact tested source identity.

## A. Baseline and scope

- [ ] implementation branch is based on or currentized to the latest intended `main`;
- [ ] final result records `base_sha`, `tested_sha`, model/tokenizer/config identity where applicable;
- [ ] no unrelated architecture/refactor work is included;
- [ ] no frozen retrieval/evaluation semantics are changed.

## B. REST boundary

- [ ] `POST /api/v1/retrieve` exists;
- [ ] `GET /health` exists;
- [ ] `GET /ready` exists;
- [ ] REST uses the same retrieval core/application path as gRPC `RetrieveContext`;
- [ ] REST does not loop back through localhost gRPC;
- [ ] REST does not implement ingestion/admin/control-plane operations;
- [ ] HTTP host/port are typed configuration and do not collide with gRPC/metrics defaults;
- [ ] graceful shutdown uses existing cancellation/drain lifecycle;
- [ ] `/ready` reflects existing `Readiness` state.

## C. Security and semantic parity

- [ ] authentication is fail-closed;
- [ ] caller access level is propagated;
- [ ] access-zone filtering is preserved;
- [ ] deleted/expired/indexing/non-visible contexts cannot leak over REST;
- [ ] REST/gRPC context identities and ordering match for equivalent positive requests;
- [ ] evidence/degradation status matches;
- [ ] Graph-derived context behavior matches when enabled;
- [ ] no REST-only score/ranking/coverage computation exists.

## D. Error/degradation behavior

- [ ] AstraError-to-HTTP status mapping is explicit and tested;
- [ ] partial valid degradation remains a successful retrieval response;
- [ ] total backend failure/deadline is not converted to successful no-answer;
- [ ] overload/backpressure maps without weakening current runtime behavior;
- [ ] cancellation/deadline semantics remain bounded.

## E. Existing invariants

The following must remain unchanged unless the phase is declared blocked:

- [ ] canonical tokenizer/model identity;
- [ ] BGE-M3/ONNX ownership;
- [ ] token-aware hierarchical chunking ownership;
- [ ] `SOURCE/PARENT/SUB_180/SUB_260` hierarchy;
- [ ] dense/sparse/lexical/hybrid semantics;
- [ ] fusion/RRF semantics and tuning;
- [ ] no-answer semantics;
- [ ] child-to-parent hydration;
- [ ] GraphRAG semantics and tuning;
- [ ] MMR semantics and tuning;
- [ ] hard token budget;
- [ ] final visibility recheck;
- [ ] access/version/lifecycle/TTL rules;
- [ ] PostgreSQL canonical-state model;
- [ ] outbox/reconciliation/Qdrant projection model;
- [ ] retry/deadline/backpressure/degradation policy;
- [ ] frozen banks/qrels/thresholds.

## F. Regression gates

- [ ] `cargo fmt --all --check` PASS;
- [ ] `cargo check --locked --all-targets --all-features` PASS;
- [ ] `cargo clippy --locked --all-targets --all-features -- -D warnings` PASS;
- [ ] `cargo test --locked --all-targets --all-features` PASS;
- [ ] focused REST positive parity tests PASS;
- [ ] REST security negative tests PASS;
- [ ] REST degradation/failure tests PASS where supported by existing harness;
- [ ] existing retrieval/security/invariant suites used by the phase remain green and are not weakened.

## G. Documentation/evidence synchronization

- [ ] stale top-level README status is audited and corrected;
- [ ] `docs/ASTRAVECTOR_READINESS_REPORT.md` is reconciled with later authoritative evidence;
- [ ] `docs/02-readiness-and-verdicts.md` is reconciled;
- [ ] `docs/12-roadmap.md` is reconciled;
- [ ] `docs/fix490/CURRENT_HEAD_STATUS.md` is added;
- [ ] every PASS/open claim in the new status report identifies its source phase/file/SHA or is explicitly classified as inherited/historical;
- [ ] FIX489-R3 remains labelled `LOCAL_MAC_CPU`;
- [ ] no production-capacity claim is made from local Mac evidence;
- [ ] no `PRODUCTION_READY` claim is made unless every repository-defined production gate is independently proven.

## H. Final evidence

- [ ] `docs/fix490/RESULT.md` exists;
- [ ] result contains exact `tested_sha`;
- [ ] result lists executed gates and outcomes;
- [ ] result records REST parity evidence;
- [ ] result records whether `main` moved during the phase;
- [ ] result uses one allowed verdict only:

```text
FIX490_REST_BOUNDARY_AND_READINESS_SYNC_PASS
FIX490_REST_BOUNDARY_AND_READINESS_SYNC_BLOCKED
```

## Hard stop rule

If REST implementation requires changing a frozen semantic invariant, stop implementation and record:

```text
FIX490_BLOCKED_BY_ARCHITECTURE_CHANGE
```

Do not widen scope silently.