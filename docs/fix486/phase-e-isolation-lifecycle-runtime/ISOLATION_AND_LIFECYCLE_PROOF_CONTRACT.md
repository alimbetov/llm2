# FIX486E Isolation and Lifecycle Proof Contract

## 1. Contract purpose

This document defines the assertions that convert Phase E runtime artifacts into an objective verdict.

The proof is fail closed. Missing data, ambiguous identity, incomplete stages, or unclassified foreign-zone content cannot be treated as success.

## 2. Contract inputs

The evaluator consumes:

```text
frozen bank 1.0.0
runtime identity map
PostgreSQL audit
Qdrant audit
request and response artifacts
normalized result rows
candidate and hydration telemetry
warm-repeat comparison
restart comparison
manifest and checksums
```

Qrels are an independent expected-result oracle. They must not be used to construct actual runtime identities.

## 3. Zone identity contract

The runtime identity map must contain exactly one mapping for each logical zone:

```text
zone-a -> 4862
zone-b -> 4863
```

For every zone mapping record, preserve:

```text
logical_zone_id
runtime_zone_code
runtime_zone_id if present
source of mapping
created_at or setup stage
```

The evaluator must reject:

```text
missing logical zone
multiple runtime mappings for one logical zone
one runtime zone mapped to both logical zones
zone inferred only from response content
```

## 4. Composite identity contract

The frozen fixture deliberately reuses logical IDs across zones. Therefore no identity may be keyed only by:

```text
document name
parent logical ID
child logical ID
source_block_id
```

The minimum logical key is:

```text
logical_zone_id
+ logical_document_id
+ document_version
+ chunk role
+ logical chunk ID
```

The minimum canonical runtime key is:

```text
runtime zone
+ runtime document ID
+ runtime version ID
+ runtime chunk ID
```

The proof must show that Zone A and Zone B instances of `doc-hierarchy`, `parent-a1`, and `child-a1-180` are distinct physical objects.

## 5. Mandatory result schema

Each normalized primary result row must contain:

```text
run_id
phase
case_id
query_id
entry_point
request_zone_logical
request_zone_runtime
status
logical_document_id
runtime_document_id
expected_version
actual_version
matched_child_logical_id
matched_child_runtime_id
hydrated_parent_logical_id
hydrated_parent_runtime_id
matched_child_anchors
parent_anchors
candidate_zone_codes
hydrated_zone_codes
final_context_zone_codes
warning_codes
trace_flags
verdict
```

Negative-control rows additionally contain:

```text
supplemental = true
source_query_id
executed_in_opposite_zone = true
foreign_anchor_count
foreign_context_count
```

## 6. FIX486-03 positive-zone assertions

### 6.1 q-zone-a

Request:

```text
query_id = q-zone-a
request zone = zone-a / 4862
```

Required:

```text
status = FOUND
actual zone = zone-a
actual version = 1
logical parent = parent-a1
parent contains ASTRA_CANONICAL_STATE_A1
```

Forbidden anywhere in final result or unredacted evidence:

```text
ZONE_B_SECRET_PARENT_A1
ZONE_B_PRIVATE_SOURCE
```

Hard gates:

```text
foreign candidate promoted = 0
foreign hydration = 0
foreign final context = 0
foreign graph result = 0
```

### 6.2 q-zone-b

Request:

```text
query_id = q-zone-b
request zone = zone-b / 4863
```

Required:

```text
status = FOUND
actual zone = zone-b
actual version = 1
logical parent = parent-a1
parent contains ZONE_B_SECRET_PARENT_A1
```

Forbidden:

```text
ASTRA_CANONICAL_STATE_A1
ASTRA_LEGAL_HOLD_A2
```

Hard gates mirror q-zone-a.

## 7. Opposite-zone negative-control assertions

### 7.1 Zone A question in Zone B

The exact q-zone-a question text is executed with Zone B authorization.

The evaluator must confirm:

```text
ASTRA_CANONICAL_STATE_A1 final occurrences = 0
Zone A physical IDs in final result = 0
Zone A hydrated rows = 0
```

### 7.2 Zone B question in Zone A

The exact q-zone-b question text is executed with Zone A authorization.

The evaluator must confirm:

```text
ZONE_B_SECRET_PARENT_A1 final occurrences = 0
ZONE_B_PRIVATE_SOURCE final occurrences = 0
Zone B physical IDs in final result = 0
Zone B hydrated rows = 0
```

A no-answer response is acceptable only when it is explicit and not a transport or normalizer failure.

## 8. Isolation telemetry contract

Candidate telemetry must allow one of the following proofs:

```text
foreign candidates never generated
or
foreign candidates observed internally but rejected before hydration
```

The second behavior is acceptable only if the candidate record is redacted and the rejection reason is explicit. Foreign content text must not be emitted.

Recommended rejection reasons:

```text
ZONE_FILTER_REJECTED
ACCESS_SCOPE_REJECTED
CANONICAL_ZONE_MISMATCH
GRAPH_ZONE_MISMATCH
```

Unknown rejection reasons block the proof.

## 9. FIX486-04 active-version assertions

Request:

```text
query_id = q-active-version
request zone = zone-a / 4862
```

Required:

```text
status = FOUND
logical document = doc-hierarchy
actual version = 1
logical parent = parent-a1
```

Forbidden versions:

```text
2
3
4
```

Forbidden anchors:

```text
ASTRA_INACTIVE_VERSION_TRAP
ASTRA_DELETED_PARENT_TRAP
ASTRA_EXPIRED_PARENT_TRAP
```

Hard gates:

```text
wrong_version_results = 0
inactive_version_results = 0
deleted_version_results = 0
expired_version_results = 0
```

## 10. Candidate-versus-final lifecycle evidence

For each forbidden version, the runner must classify the path:

```text
NOT_PROJECTED
FILTERED_AT_CANDIDATE_QUERY
REJECTED_AT_CANONICAL_HYDRATION
REJECTED_AT_FINAL_VISIBILITY
```

The classification must be grounded in runtime evidence.

The evaluator must reject:

```text
UNKNOWN
NOT_CHECKED
missing lifecycle reason
```

A forbidden version may appear in canonical audit metadata, but must never become a final context.

## 11. Lifecycle state audit contract

The PostgreSQL audit must identify the intended states:

| Version | Expected state | Searchable |
|---:|---|---|
| 1 | ACTIVE | yes |
| 2 | INDEXING | no |
| 3 | DELETED | no |
| 4 | EXPIRED or active-with-past-expiry as implemented | no |

For every version record, capture:

```text
runtime version ID
document state
activation state
deleted marker
expires_at
effective test clock
legal_hold
chunk count
binding count
outbox state
```

## 12. Test clock contract

Expiry evaluation must use a recorded test clock.

Evidence must include:

```text
clock source
clock value
runtime timezone
expired version expires_at
comparison result
```

Wall-clock assumptions without recorded values block the proof.

The runner must not modify the frozen fixture timestamp text.

## 13. Legal-hold contract

For active v1, prove:

```text
legal hold state present
canonical version remains ACTIVE
retrieval remains allowed
cleanup does not delete protected canonical state
```

Legal hold must not cause any of the following:

```text
v2 becomes searchable
v3 becomes searchable
v4 becomes searchable
expired state ignored for another version
cross-zone visibility
```

The audit must distinguish cleanup protection from retrieval authorization.

## 14. Search/RetrieveContext parity

For each mandatory query, compare normalized logical results across both entry points.

Required equality:

```text
request zone
logical document
actual version
logical parent
allowed anchors
forbidden anchor counts
visibility verdict
```

Differences allowed:

```text
trace IDs
request IDs
timings
floating-point scores
non-semantic ordering of rejected candidates
```

Any difference in zone or version semantics blocks the proof.

## 15. Warm-repeat contract

After the first successful campaign, rerun all six mandatory requests without ingestion.

Required stability:

```text
same logical result
same zone and version
same forbidden counts = 0
same active version identity
```

Canonical counts must not increase:

```text
document versions
chunks
bindings
completed outbox effects
Qdrant points
```

## 16. Restart contract

Restart only the AstraVector runtime.

Preserve PostgreSQL and Qdrant.

After readiness:

1. rerun all six mandatory requests;
2. rerun four opposite-zone controls;
3. repeat lifecycle and legal-hold audits.

Required:

```text
isolation unchanged
active-version filtering unchanged
legal-hold state unchanged
no duplicate canonical records
no duplicate projection effects
```

## 17. Evidence-leak contract

The evaluator scans result and log artifacts for forbidden foreign anchors.

Foreign anchors may appear only in:

```text
frozen fixture copies or hash-verified source references
explicit expected-forbidden lists
redacted audit labels
```

They must not appear in:

```text
final contexts
hydrated text for the wrong zone
user-facing response text
normalizer-selected evidence
unredacted foreign candidate dumps
```

## 18. Health and telemetry contract

The official proof requires:

```text
Health RPC PASS
metrics endpoint PASS
runtime binary hash recorded
configuration hash recorded
model hash recorded
tokenizer hash recorded
```

Metrics must include enough information to show requests completed and isolation/lifecycle rejection counters did not indicate bypass.

Missing telemetry is `BLOCKED`, not PASS.

## 19. Cleanup contract

Cleanup must remove only phase-owned:

```text
runtime process
Docker containers
networks
volumes
phase database or schema
Qdrant collection
temporary configuration
```

Cleanup must preserve:

```text
models
target/
frozen bank
previous evidence
unrelated containers and volumes
```

Cleanup failure must not overwrite the primary failure code.

## 20. Aggregate verdict algorithm

Return `PASS` only when:

```text
mandatory primary rows = 6/6
opposite-zone controls = 4/4
zone-a positive proof = PASS
zone-b positive proof = PASS
active-version proof = PASS
Search/RetrieveContext parity = PASS
warm repeat = PASS
restart proof = PASS
legal-hold audit = PASS
all hard-gate counters = 0
evidence integrity = PASS
terminal exit code = 0
unresolved Phase E P0/P1 defects = 0
```

Otherwise return:

```text
FIX486_ISOLATION_LIFECYCLE_RUNTIME_PROOF_BLOCKED
```

The result must never be inferred only from process exit code.