# FIX486E Implementation Note

## Phase-owned files

Phase E adds its own runner, Python evidence helper, read-only SQL audit,
Docker Compose project, runtime profile, contract test, and Make targets. It
reuses Phase D algorithms only through copied-and-specialized code so evidence
directories, ports, containers, and verdicts cannot collide.

## Runtime identity and setup

Logical zones are resolved from canonical `access_zones` rows by frozen codes:
`zone-a=4862` and `zone-b=4863`. Composite identity includes zone, document,
version, role, logical chunk identity, and all corresponding physical IDs.

TTL is resolved independently from each `access_zone_code` through the production
access-zone registry. The proof records every zone's `default_ttl_days` and verifies
that ordinary document versions inherit that value. Legal hold changes only hold
state and never rewrites TTL. Version 4 alone receives a bounded, recorded
`EXPLICIT_TEST_CLOCK_OVERRIDE` to create the expired-version trap.

The frozen v1 documents are ingested through `IndexLogicalDocument`. Runner-owned
small trap documents for Zone A versions 2, 3, and 4 use the same production API
and external document identity. This avoids direct chunk, embedding, binding, or
outbox inserts.

## Lifecycle preparation

The public API currently exposes activation and deletion but no complete API for
setting document expiry or chunk-group legal hold. Phase E therefore uses
audited, phase-owned SQL only for canonical lifecycle transitions after
production ingestion:

- v2 remains `INDEXING` with its synchronized projection present;
- v3 is scheduled through `DeleteDocumentVectorsFacade`, waits for deletion,
  then canonical document/chunk state is finalized as `DELETED`;
- v4 keeps its projection but canonical document/chunk/binding expiry is set
  before a recorded UTC test clock;
- v1 chunk and binding legal-hold fields are set together with an audit reason.

No stale Qdrant point is injected. All SQL mutations are recorded separately
from the read-only audit and are scoped by runtime zone/document/version IDs.

## Proof strategy

Primary and opposite-zone requests run through Search and RetrieveContext.
Isolation assertions inspect final text, physical IDs, zone identities, ranking
trace identities, and canonical hydration. Lifecycle probes classify exclusion
from observed state as `NOT_PROJECTED`, `FILTERED_AT_CANDIDATE_QUERY`,
`REJECTED_AT_CANONICAL_HYDRATION`, or `REJECTED_AT_FINAL_VISIBILITY`; unknown
classifications fail closed.

The runner records the test clock, startup deadline telemetry, Health, metrics,
binary/config/model/tokenizer/image identities, pre/post warm counts, restart
results, legal-hold state, cleanup, and a verified manifest. Warm and restart
reuse PostgreSQL and Qdrant without re-ingestion.

## Resolved proof defects

`FIX486E-P1-001` preserved the first external BLOCKED evidence run. Its lifecycle
trap request encoded a protobuf `map<string,string>` metadata value as a JSON
boolean. The request now uses the string `"true"`, and a contract regression test
prevents the incompatible encoding from returning.

`FIX486E-P1-002` preserved the next BLOCKED evidence run. Direct document deletion
created `DELETE_POINT.operation_version` from `payload_version`, while the publisher
correctly fences deletion on `ttl_generation`. Scheduling now atomically advances
`ttl_generation`, marks the binding `DELETION_PENDING/DELETE_PENDING`, and writes the
returned generation to outbox. The existing production-path Testcontainers E2E now
asserts delete completion and zero outbox/binding generation mismatches.

`FIX486E-P1-003` preserved the following BLOCKED run. The identity validator
required a Zone B `child-a1-260` that the frozen corpus never declares. Required
child identities now come from the immutable corpus hierarchy; extra production
chunks remain auxiliary, and cross-zone collision checks cover only logical IDs
actually shared by both zones.

`FIX486E-P1-004` was exposed by replaying the failed identity input. The runner
omitted `documentId`, so the facade's external-ID fallback produced the same raw
UUID in both zones. Phase E now supplies a deterministic UUIDv5 derived from phase,
logical zone, and logical document; lifecycle trap versions explicitly reuse the
Zone A physical document ID.
