#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/assertions.sh"
source "$SMOKE_ROOT/lib/grpc.sh"
load_smoke_env
command -v grpcurl >/dev/null 2>&1 || blocked "grpcurl not found"
command -v psql >/dev/null 2>&1 || blocked "psql not found"
grpc_assert_service "astravector.embedding.v1.AstraVectorV004Control"
document_suffix="$(printf '%012d' "$((RANDOM + 20000))")"
document_id="aaaaaaaa-aaaa-4aaa-8aaa-${document_suffix}"
content_hash="$(printf 'document-version-smoke-v1' | shasum -a 256 | awk '{print $1}')"
other_hash="$(printf 'document-version-smoke-v1-conflict' | shasum -a 256 | awk '{print $1}')"
body="$(jq -n --arg zone "$SMOKE_ACCESS_ZONE_A" --arg doc "$document_id" --arg hash "$content_hash" '{
  accessZoneId:$zone,
  documentId:$doc,
  documentVersion:1,
  contentHash:$hash,
  activationPolicy:"ACTIVE_LATEST_ONLY"
}')"
grpc_plain -d "$body" astravector.embedding.v1.AstraVectorV004Control/RegisterDocumentVersion >"$LOGS_DIR/document-version-response.json" 2>"$LOGS_DIR/document-version.err" || fail "RegisterDocumentVersion call failed"
jq -e --arg doc "$document_id" '.documentId == $doc and .documentVersion == "1" and .status == "REGISTERED"' "$LOGS_DIR/document-version-response.json" >/dev/null || fail "RegisterDocumentVersion response mismatch"

grpc_plain -d "$body" astravector.embedding.v1.AstraVectorV004Control/RegisterDocumentVersion >"$LOGS_DIR/document-version-idempotent-response.json" 2>"$LOGS_DIR/document-version-idempotent.err" || fail "idempotent RegisterDocumentVersion call failed"
jq -e --arg doc "$document_id" '.documentId == $doc and .documentVersion == "1" and .status == "REGISTERED"' "$LOGS_DIR/document-version-idempotent-response.json" >/dev/null || fail "idempotent RegisterDocumentVersion response mismatch"

conflict_body="$(jq -n --arg zone "$SMOKE_ACCESS_ZONE_A" --arg doc "$document_id" --arg hash "$other_hash" '{
  accessZoneId:$zone,
  documentId:$doc,
  documentVersion:1,
  contentHash:$hash,
  activationPolicy:"ACTIVE_LATEST_ONLY"
}')"
if grpc_plain -d "$conflict_body" astravector.embedding.v1.AstraVectorV004Control/RegisterDocumentVersion >"$LOGS_DIR/document-version-conflict-response.json" 2>"$LOGS_DIR/document-version-conflict.err"; then
  fail "conflicting RegisterDocumentVersion unexpectedly succeeded"
fi
grep -Eq "Code: (AlreadyExists|FailedPrecondition)" "$LOGS_DIR/document-version-conflict.err" || fail "conflicting RegisterDocumentVersion did not return ALREADY_EXISTS/FAILED_PRECONDITION"

psql "$(postgres_url)" -At -F $'\t' -c "SELECT status,content_hash FROM astravector.document_versions WHERE access_zone_id='${SMOKE_ACCESS_ZONE_A}'::uuid AND document_id='${document_id}'::uuid AND document_version=1" >"$LOGS_DIR/document-version-row.tsv" || fail "document_versions SQL assertion failed"
grep -q $'REGISTERED\t'"$content_hash" "$LOGS_DIR/document-version-row.tsv" || fail "document_versions row was not persisted with expected status/hash"

activate_without_chunks="$(jq -n --arg zone "$SMOKE_ACCESS_ZONE_A" --arg doc "$document_id" '{
  accessZoneId:$zone,
  documentId:$doc,
  documentVersion:1
}')"
if grpc_plain -d "$activate_without_chunks" astravector.embedding.v1.AstraVectorV004Control/ActivateDocumentVersion >"$LOGS_DIR/document-version-activate-without-chunks.json" 2>"$LOGS_DIR/document-version-activate-without-chunks.err"; then
  fail "ActivateDocumentVersion unexpectedly succeeded without chunks"
fi
grep -Eq "Code: FailedPrecondition" "$LOGS_DIR/document-version-activate-without-chunks.err" || fail "ActivateDocumentVersion without chunks did not return FAILED_PRECONDITION"

psql "$(postgres_url)" -At -F $'\t' -c "SELECT b.document_id,b.document_version FROM astravector.vector_bindings_v004 b WHERE b.access_zone_id='${SMOKE_ACCESS_ZONE_A}'::uuid AND b.chunk_granularity IN('PARENT','SUB_180','SUB_260') AND b.qdrant_sync_status='SYNCED' AND EXISTS (SELECT 1 FROM astravector.vector_outbox o WHERE o.binding_access_zone_id=b.access_zone_id AND o.binding_id=b.id AND o.operation='UPSERT_POINT' AND o.operation_version=b.payload_version AND o.status='COMPLETED') GROUP BY b.document_id,b.document_version HAVING count(*) FILTER (WHERE b.chunk_granularity='PARENT')>0 AND count(*) FILTER (WHERE b.chunk_granularity='SUB_180')>0 AND count(*) FILTER (WHERE b.chunk_granularity='SUB_260')>0 ORDER BY max(b.updated_at) DESC LIMIT 1" >"$LOGS_DIR/document-version-activation-target.tsv" || fail "activation target SQL failed"
if [[ ! -s "$LOGS_DIR/document-version-activation-target.tsv" ]]; then
  blocked "no synced chunking/outbox document exists to activate"
  exit $?
fi
target_document_id="$(cut -f1 "$LOGS_DIR/document-version-activation-target.tsv")"
target_document_version="$(cut -f2 "$LOGS_DIR/document-version-activation-target.tsv")"
activate_body="$(jq -n --arg zone "$SMOKE_ACCESS_ZONE_A" --arg doc "$target_document_id" --argjson version "$target_document_version" '{
  accessZoneId:$zone,
  documentId:$doc,
  documentVersion:$version
}')"
grpc_plain -d "$activate_body" astravector.embedding.v1.AstraVectorV004Control/ActivateDocumentVersion >"$LOGS_DIR/document-version-activate-response.json" 2>"$LOGS_DIR/document-version-activate.err" || fail "ActivateDocumentVersion call failed"
jq -e --arg doc "$target_document_id" '.documentId == $doc and .status == "ACTIVE"' "$LOGS_DIR/document-version-activate-response.json" >/dev/null || fail "ActivateDocumentVersion response mismatch"
psql "$(postgres_url)" -At -F $'\t' -c "SELECT status FROM astravector.document_versions WHERE access_zone_id='${SMOKE_ACCESS_ZONE_A}'::uuid AND document_id='${target_document_id}'::uuid AND document_version=${target_document_version}" >"$LOGS_DIR/document-version-activated-row.tsv" || fail "activated document SQL assertion failed"
grep -q '^ACTIVE$' "$LOGS_DIR/document-version-activated-row.tsv" || fail "document version was not ACTIVE after activation"
