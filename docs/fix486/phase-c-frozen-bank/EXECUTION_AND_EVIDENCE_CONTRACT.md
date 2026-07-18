# fix486c execution and evidence contract

## Execution modes

The Phase C runner must support at least:

```text
VERIFY_ONLY
DRY_RUN
PREPARE_RUNTIME
INGEST_ONLY
EXECUTE_ALL
```

`VERIFY_ONLY` and `DRY_RUN` must not require PostgreSQL, Qdrant or the ONNX model.

## Stage model

Every mandatory stage emits:

```json
{
  "stage_id": "bank-hash-verification",
  "status": "PASS|FAIL|BLOCKED|SKIPPED",
  "started_at_utc": "<timestamp>",
  "finished_at_utc": "<timestamp>",
  "exit_code": 0,
  "failure_code": null,
  "evidence": []
}
```

Mandatory stages may not be represented as PASS when skipped.

## Query dry-run

Dry-run must prove all 11 query rows:

- parse successfully;
- map to exactly one qrel;
- map to a known case;
- reference a known access zone;
- reference a supported profile;
- contain a valid positive `max_contexts`;
- contain valid optional token and Graph settings;
- can be converted into an executable request plan.

Dry-run does not assert that runtime retrieval satisfies the qrel.

## Runtime preparation

When runtime mode is used:

1. start from clean phase-owned state;
2. apply migrations;
3. validate the expected model/tokenizer;
4. build or identify a locked release binary;
5. ingest through production paths;
6. capture physical identities externally;
7. apply supported Graph relations;
8. preserve exact configuration and binary identities.

## Query execution evidence

Each query row must record:

- query and case ID;
- request parameters after resolution;
- access zone identity;
- runtime response status;
- matched and parent chunk IDs;
- source block IDs;
- document/version identities;
- warnings and degradation markers;
- ranking/graph traces when available;
- hard-gate evaluation results;
- PASS/FAIL/BLOCKED/SKIPPED classification.

## Evidence identity

Every evidence manifest must bind:

```text
source SHA
Cargo.lock SHA-256
bank version
bank aggregate SHA-256
runner SHA-256
resolved config SHA-256
release binary SHA-256 when used
model SHA-256 when used
tokenizer SHA-256 when used
PostgreSQL image identity when used
Qdrant image identity when used
```

Identity drift blocks the run.

## Evidence storage

Raw evidence stays under:

```text
<ASTRAVECTOR_EVIDENCE_ROOT>/fix486c/<run-id>/
```

Git stores only:

- compact result summary;
- manifest pointer;
- stage summary;
- defect register;
- hashes required to independently locate and verify the bundle.

## Completeness gate

The finalizer must enumerate all mandatory evidence files and fail with:

```text
EVIDENCE_INCOMPLETE
```

when any file is missing, empty, malformed or not represented in the evidence manifest.

## Allowed Phase C classifications

```text
FIX486_FROZEN_EXECUTABLE_BANK_PASS
FIX486_FROZEN_EXECUTABLE_BANK_BLOCKED
```

Do not emit downstream functional verdicts from Phase C.
