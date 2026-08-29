# CODEX-09 Session Finalize Activation Remediation

## Baseline

| Field | Value |
| --- | --- |
| BASE_SHA | `2142e2bd328c8964dbb3c2eff52caf8a350c5ddf` |
| BRANCH | `codex/fix-session-finalize-activation-policy` |
| OLD_IMAGE | `registry.astrabase.asia/astravector:sha-1cb6065` |
| OLD_IMAGE_DIGEST | `sha256:b0567810b5ea3df752ff8ba559fcf16bc46b245878e798b8888dcf93426ee6ad` |

## Pre-Change Code Trace

| Concern | File | Function/type | Current behavior |
| --- | --- | --- | --- |
| session Finalize | `src/grpc/mod.rs` | `AstraVectorIngestionFacade::finalize_logical_document_ingestion` | Acquires `ACTIVE -> FINALIZING`, rehydrates staged blocks, validates final hash, builds an internal `IndexLogicalDocumentRequest`, calls `index_logical_document`, then stores replayable response and marks session `COMPLETED`. |
| policy construction | `src/grpc/mod.rs` | internal request in `finalize_logical_document_ingestion` | Sets `VectorIndexingOptions.activation_policy` to `ActivationPolicy::AutoWhenReady`. |
| AUTO validator | `src/grpc/mod.rs` | `reject_unsupported_activation_policy` | Rejects `ActivationPolicy::AutoWhenReady` with `UNSUPPORTED_ACTIVATION_POLICY_AUTO_WHEN_READY` because durable auto-activation lifecycle is not supported. |
| MANUAL single-call path | `src/grpc/mod.rs` | `AstraVectorIngestionFacade::index_logical_document` | Uses caller/default indexing options, rejects unsupported AUTO, registers document version, creates chunks/vectors/outbox and returns an indexing operation. MANUAL is the validated supported lifecycle. |
| readiness status | `src/grpc/mod.rs` | `get_document_vector_status`, `compute_document_sync_status` | Reports `READY_TO_ACTIVATE` when expected bindings are synced, no outbox failures remain, and Qdrant points are present; reports `FAILED` on failed/dead-letter bindings/outbox; otherwise reports syncing/indexing state. |
| activation | `src/grpc/mod.rs` | `AstraVectorV004Control::activate_document_version` | Requires internal/admin metadata, checks `ready_to_activate`, then calls repository activation to mark document `ACTIVE`. |

## Proto Contract

| Item | Result |
| --- | --- |
| START_HAS_ACTIVATION_POLICY | `NO` |
| FINALIZE_HAS_ACTIVATION_POLICY | `NO` |
| Proto wire change required | `NO` |

`StartLogicalDocumentIngestionRequest` and `FinalizeLogicalDocumentIngestionRequest` do not expose activation policy. Session ingestion policy is therefore server-owned.

## Root Cause

`FinalizeLogicalDocumentIngestion` selected `AUTO_WHEN_READY` internally while the same runtime explicitly rejects `AUTO_WHEN_READY` as unsupported. Normal clients cannot request this policy through the session API, so the server rejected its own server-owned selection.

## Selected Remediation

Session Finalize uses `MANUAL` activation policy. `AUTO_WHEN_READY` remains defined and rejected until a real durable auto-activation lifecycle worker is implemented.

`Finalize success` means the session was accepted/finalized and indexing/vector publication work was created or completed. It does not imply the document is searchable. The supported flow is:

```text
StartLogicalDocumentIngestion
AppendLogicalDocumentBlocks
FinalizeLogicalDocumentIngestion
GetDocumentVectorStatus -> READY_TO_ACTIVATE
AstraVectorV004Control.ActivateDocumentVersion
retrieval
```

## Public Activation Boundary

`AstraVectorV004Control.ActivateDocumentVersion` is treated as a supported public compatibility path for AstraIndexator in this release because it is implemented in the current runtime, documented in `docs/api/grpc-api.md` and `docs/api/grpcurl-examples.md`, and is already used by the validated single-call lifecycle.

## Changed Files

| File | Change | Reason |
| --- | --- | --- |
| `src/grpc/mod.rs` | Changed | Session finalize now selects the server-owned `MANUAL` activation policy through `session_finalize_activation_policy()`. Added regression tests proving session finalize uses `MANUAL` and `AUTO_WHEN_READY` remains rejected. |
| `tests/session_finalize_activation_contracts.rs` | Added | Compiled source-contract guard proving session finalize does not select unsupported `AUTO_WHEN_READY` internally. |
| `docs/INGESTION_FINALIZE_RECOVERY.md` | Changed | Documented Finalize -> READY_TO_ACTIVATE -> explicit activation semantics for session ingestion consumers. |
| `docs/qualification/CODEX-09-SESSION-FINALIZE-ACTIVATION-REMEDIATION.md` | Added | Required remediation evidence and final report. |
| `proto/astravector_embedding.proto` | UNCHANGED | No wire change required. |

## Test Matrix

| Test | Result |
| --- | --- |
| fmt | PASS: `cargo fmt --check` |
| clippy | PASS: `cargo clippy --all-targets -- -D warnings` |
| unit | PASS: `cargo test session_finalize_selects_manual_activation_policy`; PASS: `cargo test auto_when_ready_remains_rejected_until_lifecycle_worker_exists` |
| full cargo test | PASS: `cargo test` |
| session Start | NOT RUN in source-remediation stage |
| session Append | NOT RUN in source-remediation stage |
| replay | NOT RUN in runtime/source-remediation stage |
| conflict | NOT RUN in runtime/source-remediation stage |
| session Finalize | PASS: source-contract `cargo test session_finalize_uses_manual_server_owned_activation_policy_contract` proves Finalize no longer constructs unsupported `AUTO_WHEN_READY` |
| readiness | NOT RUN in source-remediation stage |
| explicit activation | NOT RUN in source-remediation stage |
| retrieval | NOT RUN in source-remediation stage |
| restart after Append | NOT RUN in source-remediation stage |
| restart after Finalize | NOT RUN in source-remediation stage |
| post-restart activation | NOT RUN in source-remediation stage |
| single-call regression | No proto or single-call indexing behavior changed |
| AccessZone regression | No access-zone code changed; covered by existing `cargo test` suite |
| TTL regression | No TTL behavior changed; existing TTL tests passed in `cargo test` |
| hash regression | No finalize hash-validation behavior changed |

## Final Qualification

| Field | Value |
| --- | --- |
| FIX_SHA | `PENDING_COMMIT` |
| MERGE_SHA | `PENDING` |
| POST_MERGE_CI_RUN | `PENDING` |
| NEW_IMAGE_TAG | `PENDING` |
| NEW_IMAGE_DIGEST | `PENDING` |
| IMAGE_SOURCE_SHA | `PENDING` |
| PORTABLE_RESULT | `PENDING` |
| SESSION_SMOKE_RESULT | `PENDING` |
| ASTRAINDEXATOR_COMPATIBILITY_RESULT | `PENDING` |

## Verdict

CODEX-09_SOURCE_REMEDIATION_PASS
