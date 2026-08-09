# FIX490 Acceptance Criteria

FIX490 is accepted only when every mandatory criterion below is satisfied and the final result records exact tested source identity.

Normative sources:

```text
docs/fix490/TECHNICAL_SPECIFICATION.md
docs/fix490/REST_BOUNDARY_HARDENING_ADDENDUM.md
```

The hardening addendum has priority for REST security, deadlines, cancellation, configuration, probes, HTTP protocol behavior and error mapping.

## A. Baseline and scope

- [ ] implementation branch is based on or currentized to the latest intended `main`;
- [ ] final result records `base_sha`, `tested_sha`, current `main_sha`, model/tokenizer/config identity where applicable;
- [ ] no unrelated architecture/refactor work is included;
- [ ] no frozen retrieval/evaluation semantics are changed;
- [ ] no new REST ingestion/admin/control-plane endpoints are added.

## B. REST boundary

- [ ] `POST /api/v1/retrieve` exists;
- [ ] `GET /health` exists;
- [ ] `GET /ready` exists;
- [ ] REST uses the same in-process retrieval core/application path as gRPC `RetrieveContext`;
- [ ] REST does not loop back through localhost/self gRPC;
- [ ] REST does not implement ingestion/admin/control-plane operations;
- [ ] REST v1 exposes only approved facade fields and STANDARD public retrieval representation;
- [ ] no REST-only score/ranking/coverage/Graph computation exists.

## C. Typed HTTP configuration

- [ ] typed `http` configuration exists;
- [ ] configuration includes `enabled`, `required_on_startup`, `host`, `port`, `max_request_body_bytes`;
- [ ] default HTTP port is `8080` unless explicitly justified otherwise;
- [ ] HTTP port validation rejects collision with gRPC and metrics ports;
- [ ] `http.enabled=false` starts no HTTP listener;
- [ ] required HTTP bind/start failure is fatal.

## D. Health/readiness semantics

- [ ] `/health` returns `200` while process/HTTP adapter is alive;
- [ ] `/ready` uses the existing shared `Readiness` state only;
- [ ] `/ready` returns `200` when ready;
- [ ] `/ready` returns `503` when not ready;
- [ ] REST does not implement an independent PostgreSQL/Qdrant/scheduler readiness algorithm;
- [ ] `security.protect_health` semantics are preserved for HTTP probes.

## E. Security and trusted identity parity

- [ ] authentication is fail-closed when enabled;
- [ ] REST preserves `x-api-key` semantics;
- [ ] REST preserves `trust_forwarded_identity_headers` semantics;
- [ ] REST preserves configured gateway trust header/token semantics;
- [ ] trusted role/access-level semantics are preserved;
- [ ] caller access level is not trusted merely because a public JSON body supplies it;
- [ ] caller identity/service/access context comes from validated security context;
- [ ] access-zone filtering is preserved;
- [ ] multi-zone validation/resolution uses existing rules;
- [ ] deleted/expired/indexing/non-visible contexts cannot leak over REST;
- [ ] final PostgreSQL visibility recheck remains authoritative;
- [ ] gRPC and REST do not maintain independently drifting copies of security policy.

## F. HTTP protocol contract

- [ ] `application/json` request succeeds for valid input;
- [ ] malformed JSON returns `400`;
- [ ] unsupported media type returns `415`;
- [ ] oversized request body returns `413`;
- [ ] request body is bounded by configuration;
- [ ] correlation id is propagated/generated and returned on error;
- [ ] error body contains stable `code`, `message`, `correlationId` fields;
- [ ] error response does not expose stack traces, SQL, credentials/tokens or sensitive backend payloads.

## G. Complete AstraError mapping

Every current variant must be explicitly mapped and tested:

- [ ] `InvalidArgument -> 400`;
- [ ] `OutOfRange -> 400`;
- [ ] `Unauthenticated -> 401`;
- [ ] `PermissionDenied -> 403`;
- [ ] `NotFound -> 404`;
- [ ] `AlreadyExists -> 409`;
- [ ] `FailedPrecondition -> 409`;
- [ ] `OwnershipLost -> 409`;
- [ ] `ResourceExhausted -> 429`;
- [ ] `Cancelled -> 499` classification where a response is possible;
- [ ] `Unavailable -> 503`;
- [ ] `DeadlineExceeded -> 504`;
- [ ] `Internal -> 500`;
- [ ] known variants are not collapsed into generic `500`.

## H. Deadline/cancellation behavior

- [ ] REST retrieval is bounded by the existing AstraVector query/retrieval deadline model;
- [ ] REST reaches the same downstream operation-budget/cancellation-aware path as gRPC;
- [ ] deadline exhaustion maps to `504` and is not semantic no-answer;
- [ ] resource exhaustion/backpressure maps to `429`;
- [ ] backend unavailable maps to `503`;
- [ ] request/client cancellation remains cancellation rather than `Internal` where observable;
- [ ] HTTP request cancellation/disconnect is bridged into per-request cancellation where supported by the framework;
- [ ] global service shutdown cancellation remains shared by HTTP and gRPC.

## I. Positive semantic parity

Equivalent REST/gRPC requests must match on:

- [ ] returned context count;
- [ ] context ordering;
- [ ] `(access_zone_id, document_id, document_version, matched_chunk_id, parent_chunk_id)` identity;
- [ ] `source_block_id`;
- [ ] matched/parent text;
- [ ] dense/sparse/fusion/final scores within serialization tolerance;
- [ ] evidence status;
- [ ] degradation state/codes;
- [ ] Graph-derived context presence/provenance when enabled.

Required positive scenarios:

- [ ] single-zone retrieval;
- [ ] multi-zone retrieval when enabled;
- [ ] Graph disabled;
- [ ] Graph enabled;
- [ ] insufficient/no-answer response;
- [ ] successful degraded response with surviving contexts.

## J. Security/lifecycle negative parity

Where supported by existing fixtures/harness:

- [ ] missing/bad API key;
- [ ] untrusted forwarded identity;
- [ ] bad gateway trust token;
- [ ] insufficient caller access level;
- [ ] wrong/inactive/missing zone;
- [ ] multi-zone disabled;
- [ ] too many zones;
- [ ] invalid zone id;
- [ ] DELETED document/version;
- [ ] EXPIRED context;
- [ ] INDEXING/non-active context;
- [ ] REST exposes no context that authoritative gRPC/security policy rejects.

## K. Failure/degradation parity

Where supported by existing harness:

- [ ] partial optional branch degradation is tested;
- [ ] PostgreSQL hydration timeout/failure is tested;
- [ ] Qdrant unavailable/timeout is tested;
- [ ] request deadline exhaustion is tested;
- [ ] overload/backpressure is tested;
- [ ] request cancellation is tested where practical;
- [ ] valid partial degradation with surviving contexts remains successful;
- [ ] total required-backend failure is not converted to successful empty/no-answer response.

## L. Existing invariants

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

## M. Server lifecycle and observability

- [ ] HTTP and gRPC share the existing process shutdown/drain lifecycle;
- [ ] unexpected termination of required HTTP triggers global failure/shutdown;
- [ ] required gRPC termination cannot leave HTTP reporting healthy indefinitely;
- [ ] no separate REST microservice/reverse-proxy container is introduced;
- [ ] minimal HTTP request count/duration metrics exist or equivalent transport observability is documented;
- [ ] HTTP metrics use bounded labels only;
- [ ] query/user/document/access-zone/correlation values are not metric labels.

## N. Regression gates

- [ ] `cargo fmt --all --check` PASS;
- [ ] `cargo check --locked --all-targets --all-features` PASS;
- [ ] `cargo clippy --locked --all-targets --all-features -- -D warnings` PASS;
- [ ] `cargo test --locked --all-targets --all-features` PASS;
- [ ] focused REST positive parity tests PASS;
- [ ] REST security negative tests PASS;
- [ ] REST HTTP protocol tests PASS;
- [ ] REST degradation/failure tests PASS where supported by existing harness;
- [ ] existing retrieval/security/invariant suites used by the phase remain green and are not weakened.

## O. REST smoke

- [ ] executable `/health` smoke documented/run;
- [ ] executable `/ready` smoke documented/run;
- [ ] executable `/api/v1/retrieve` curl smoke documented/run;
- [ ] short REST transport concurrency smoke at local concurrency 1 and 2 run where practical;
- [ ] REST smoke is not misrepresented as capacity evidence;
- [ ] full 60-minute FIX489-R3 soak is not rerun solely for the thin adapter unless core semantics changed or evidence was invalidated.

## P. Documentation/evidence synchronization

- [ ] stale top-level README status is audited and corrected;
- [ ] `docs/README.md` links REST API documentation;
- [ ] `docs/ASTRAVECTOR_READINESS_REPORT.md` is reconciled with later authoritative evidence;
- [ ] `docs/02-readiness-and-verdicts.md` is reconciled;
- [ ] `docs/12-roadmap.md` is reconciled;
- [ ] `docs/api/rest-api.md` is added;
- [ ] REST API doc covers config, request, response, auth/trusted identity, errors, probes and curl examples;
- [ ] `docs/fix490/CURRENT_HEAD_STATUS.md` is added;
- [ ] every PASS/open claim identifies source phase/file/SHA or is explicitly inherited/historical;
- [ ] FIX489-R3 remains labelled `LOCAL_MAC_CPU`;
- [ ] no production-capacity claim is made from local Mac evidence;
- [ ] no `PRODUCTION_READY` claim is made unless every repository-defined production gate is independently proven.

## Q. Final evidence

- [ ] `docs/fix490/RESULT.md` exists;
- [ ] result contains exact `tested_sha` and current `main_sha`;
- [ ] result lists executed gates and outcomes;
- [ ] result records REST/gRPC parity evidence;
- [ ] result records security negative evidence;
- [ ] result records complete error-mapping evidence;
- [ ] result records deadline/backpressure/probe evidence;
- [ ] result records whether `main` moved during the phase;
- [ ] result uses one allowed implementation verdict only:

```text
FIX490_REST_BOUNDARY_AND_READINESS_SYNC_PASS
FIX490_REST_BOUNDARY_AND_READINESS_SYNC_BLOCKED
```

## R. Independent Codex verification

After implementation:

- [ ] execute the review/test procedure in `docs/fix490/CODEX_REST_VERIFICATION_PROMPT.md`;
- [ ] produce `docs/fix490/REST_VERIFICATION_RESULT.md`;
- [ ] independent verification uses one allowed verdict:

```text
FIX490_REST_VERIFICATION_PASS
FIX490_REST_VERIFICATION_FAIL
FIX490_REST_VERIFICATION_BLOCKED
```

## Hard stop rule

If REST implementation requires changing a frozen semantic invariant, stop implementation and record:

```text
FIX490_BLOCKED_BY_ARCHITECTURE_CHANGE
```

Do not widen scope silently.
