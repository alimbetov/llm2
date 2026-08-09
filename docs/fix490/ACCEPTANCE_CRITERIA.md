# FIX490 Acceptance Criteria — Internal REST Retrieval Boundary

Normative sources:

```text
docs/fix490/TECHNICAL_SPECIFICATION.md
docs/fix490/REST_BOUNDARY_HARDENING_ADDENDUM.md
```

The addendum supersedes any older wording that treats REST as an authentication/security boundary.

## A. Scope

- [ ] `POST /api/v1/retrieve` implemented;
- [ ] `GET /health` implemented;
- [ ] `GET /ready` implemented;
- [ ] REST is internal-only;
- [ ] no REST auth middleware/API-key/JWT/gateway-role mechanism added;
- [ ] no REST ingestion/admin/control-plane endpoints added;
- [ ] no localhost/self-gRPC loopback;
- [ ] no retrieval architecture rewrite.

## B. Retrieval parity

REST must execute the existing authoritative retrieval core and preserve the existing facade mappings.

Equivalent REST/gRPC requests must agree on:

- [ ] context count and order;
- [ ] access zone;
- [ ] document id/version;
- [ ] source block id;
- [ ] matched/parent chunk id;
- [ ] matched/parent text;
- [ ] dense/sparse/fusion/final scores within serialization tolerance;
- [ ] evidence status;
- [ ] degraded flag/codes;
- [ ] Graph-derived result presence/provenance when enabled.

Profiles:

- [ ] BALANCED mapping parity;
- [ ] TECHNICAL mapping parity;
- [ ] LEGAL mapping parity;
- [ ] SEMANTIC mapping parity;
- [ ] LEXICAL_STRICT mapping parity.

## C. Retrieval visibility semantics

`callerAccessLevel` is retrieval input, not HTTP authentication.

- [ ] PUBLIC/INTERNAL/CONFIDENTIAL/RESTRICTED mapping tested;
- [ ] existing access-level filtering remains unchanged;
- [ ] single-zone resolution unchanged;
- [ ] multi-zone resolution/limits unchanged;
- [ ] inactive/missing/invalid zones behave as existing core specifies;
- [ ] DELETED excluded;
- [ ] EXPIRED excluded;
- [ ] INDEXING/non-active excluded;
- [ ] final PostgreSQL visibility recheck remains authoritative.

## D. Frozen core invariants

No changes to:

- [ ] CanonicalTokenizer/model identities;
- [ ] BGE-M3/ONNX ownership;
- [ ] token-aware chunking ownership;
- [ ] SOURCE/PARENT/SUB_180/SUB_260;
- [ ] Dense/Sparse/Lexical/Hybrid behavior;
- [ ] fusion/RRF tuning;
- [ ] no-answer behavior;
- [ ] parent hydration;
- [ ] GraphRAG;
- [ ] MMR;
- [ ] token budget;
- [ ] final visibility;
- [ ] document/version/lifecycle/TTL;
- [ ] PostgreSQL canonical state;
- [ ] outbox/reconciliation;
- [ ] Qdrant projection;
- [ ] deadline/backpressure/degradation policy;
- [ ] frozen banks/qrels/thresholds.

If any are required to change, verdict is `FIX490_BLOCKED_BY_ARCHITECTURE_CHANGE`.

## E. HTTP protocol

- [ ] valid JSON retrieval works;
- [ ] malformed JSON -> 400;
- [ ] unsupported content type -> 415;
- [ ] oversized body -> 413;
- [ ] bounded request body configured;
- [ ] correlation id propagated/generated;
- [ ] stable error body contains `code`, `message`, `correlationId`;
- [ ] no stack traces/SQL/secrets/raw backend payloads exposed.

## F. Status mapping

- [ ] InvalidArgument/OutOfRange -> 400;
- [ ] NotFound -> 404;
- [ ] AlreadyExists/FailedPrecondition/Aborted -> 409;
- [ ] ResourceExhausted -> 429;
- [ ] Cancelled -> 499 where response is possible;
- [ ] Unavailable -> 503;
- [ ] DeadlineExceeded -> 504;
- [ ] Internal/unclassified -> 500;
- [ ] any Unauthenticated/PermissionDenied propagated by existing core is mapped deterministically, without adding REST auth.

## G. Deadlines/degradation

- [ ] same configured query deadline used;
- [ ] same bounded Search/retrieval downstream path used;
- [ ] deadline -> 504, not semantic no-answer;
- [ ] backpressure -> 429;
- [ ] unavailable backend -> 503;
- [ ] valid degraded response with surviving contexts remains 200;
- [ ] total required-backend failure does not become empty 200 response.

## H. Probes and runtime

- [ ] `/health` -> 200 while HTTP listener is alive;
- [ ] `/ready` uses existing shared `Readiness` only;
- [ ] `/ready` -> 200 when ready;
- [ ] `/ready` -> 503 when not ready;
- [ ] no authentication on internal probes;
- [ ] HTTP/gRPC share process shutdown token/lifecycle;
- [ ] unexpected HTTP failure cannot silently leave half-alive runtime;
- [ ] port collision with gRPC/metrics rejected;
- [ ] HTTP can be disabled by configuration.

## I. Observability

- [ ] HTTP request count/duration metrics exist or equivalent transport metrics documented;
- [ ] labels are bounded (`route`, `method`, `status_class`);
- [ ] no query/document/access-zone/access-level/correlation-id labels.

## J. Regression tests

Must pass after lockfile is current:

```bash
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

Focused verification:

- [ ] REST unit/protocol tests PASS;
- [ ] REST vs gRPC semantic parity smoke PASS;
- [ ] visibility/lifecycle cases PASS;
- [ ] Graph enabled/disabled parity PASS;
- [ ] no-answer/degraded behavior parity PASS;
- [ ] deadline/backpressure mapping PASS where harness supports it.

## K. Smoke

- [ ] `GET /health` smoke;
- [ ] `GET /ready` smoke;
- [ ] `POST /api/v1/retrieve` smoke;
- [ ] direct REST-vs-gRPC comparison;
- [ ] short REST concurrency smoke at 1 and 2 where practical;
- [ ] no new capacity claim;
- [ ] 60-minute FIX489-R3 soak not repeated solely for transport adapter.

## L. Documentation/evidence

- [ ] `docs/api/rest-api.md` added and says internal-only/no REST auth;
- [ ] README/current readiness docs reconciled to latest authoritative evidence;
- [ ] `docs/fix490/CURRENT_HEAD_STATUS.md` added;
- [ ] `docs/fix490/RESULT.md` added after tests;
- [ ] exact tested SHA recorded;
- [ ] FIX489-R3 remains `LOCAL_MAC_CPU`;
- [ ] no production-capacity inference;
- [ ] no `PRODUCTION_READY` claim without all independent gates.

## M. Independent Codex verification

After implementation Codex must execute:

```text
docs/fix490/CODEX_REST_VERIFICATION_PROMPT.md
```

and produce:

```text
docs/fix490/REST_VERIFICATION_RESULT.md
```

Allowed verification verdicts:

```text
FIX490_REST_VERIFICATION_PASS
FIX490_REST_VERIFICATION_FAIL
FIX490_REST_VERIFICATION_BLOCKED
```
