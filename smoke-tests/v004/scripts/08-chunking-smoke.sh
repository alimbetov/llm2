#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/assertions.sh"
source "$SMOKE_ROOT/lib/grpc.sh"
load_smoke_env
command -v grpcurl >/dev/null 2>&1 || blocked "grpcurl not found"
command -v psql >/dev/null 2>&1 || blocked "psql not found"
grpc_assert_service "astravector.embedding.v1.AstraVectorV004Control"
document_suffix="$(printf '%012d' "$((RANDOM + 1))")"
document_id="aaaaaaaa-aaaa-4aaa-8aaa-${document_suffix}"
psql "$(postgres_url)" -v ON_ERROR_STOP=1 \
  -c "DELETE FROM astravector.vector_outbox o USING astravector.vector_bindings_v004 b WHERE o.binding_access_zone_id=b.access_zone_id AND o.binding_id=b.id AND b.access_zone_id='${SMOKE_ACCESS_ZONE_A}'::uuid AND b.document_id='${document_id}'::uuid" \
  -c "DELETE FROM astravector.vector_bindings_v004 WHERE access_zone_id='${SMOKE_ACCESS_ZONE_A}'::uuid AND document_id='${document_id}'::uuid" >/dev/null \
  || fail "failed to clear smoke rows for document"
source_text="$(cat "$SMOKE_ROOT/fixtures/smoke-document-medium.txt")"
content_hash="$(printf '%s' "$source_text" | shasum -a 256 | awk '{print $1}')"
register_body="$(jq -n --arg zone "$SMOKE_ACCESS_ZONE_A" --arg doc "$document_id" --arg hash "$content_hash" '{
  accessZoneId:$zone,
  documentId:$doc,
  documentVersion:1,
  contentHash:$hash,
  activationPolicy:"ACTIVE_LATEST_ONLY"
}')"
grpc_plain -d "$register_body" astravector.embedding.v1.AstraVectorV004Control/RegisterDocumentVersion >"$LOGS_DIR/chunking-register-response.json" 2>"$LOGS_DIR/chunking-register.err" || fail "RegisterDocumentVersion precondition failed"

body="$(jq -n --arg zone "$SMOKE_ACCESS_ZONE_A" --arg doc "$document_id" --arg text "$source_text" '{
  accessZoneId:$zone,
  documentId:$doc,
  documentVersion:1,
  sourceText:$text,
  accessLevel:"PUBLIC",
  profile:{preserveHeadings:true,preserveParagraphs:true,preserveSentences:true,profileVersion:"smoke-v004-profile-v1"},
  idempotencyKey:("chunking-smoke-" + $doc)
}')"
grpc_plain -d "$body" astravector.embedding.v1.AstraVectorV004Control/CreateMultiGranularityChunks >"$LOGS_DIR/chunking-response.json" 2>"$LOGS_DIR/chunking.err" || fail "CreateMultiGranularityChunks call failed"
jq -e '.status == "INDEXING" and (.rootChunkId|length > 0) and (.totalChunks >= 4) and (.parentChunks|length >= 1) and (.subChunks180|length >= 1) and (.subChunks260|length >= 1)' "$LOGS_DIR/chunking-response.json" >/dev/null || fail "chunking response does not contain expected hierarchy"

first_root="$(jq -r '.rootChunkId' "$LOGS_DIR/chunking-response.json")"
grpc_plain -d "$body" astravector.embedding.v1.AstraVectorV004Control/CreateMultiGranularityChunks >"$LOGS_DIR/chunking-idempotent-response.json" 2>"$LOGS_DIR/chunking-idempotent.err" || fail "idempotent CreateMultiGranularityChunks call failed"
second_root="$(jq -r '.rootChunkId' "$LOGS_DIR/chunking-idempotent-response.json")"
assert_equals "$first_root" "$second_root" "chunking root id changed across idempotent call"

psql "$(postgres_url)" -At -F $'\t' -c "SELECT granularity,count(*) FROM astravector.content_chunks_v004 WHERE access_zone_id='${SMOKE_ACCESS_ZONE_A}'::uuid AND document_id='${document_id}'::uuid AND document_version=1 GROUP BY granularity ORDER BY granularity" >"$LOGS_DIR/chunking-counts.tsv" || fail "chunking SQL counts failed"
grep -q $'SOURCE\t1' "$LOGS_DIR/chunking-counts.tsv" || fail "SOURCE chunk count mismatch"
grep -q $'PARENT\t' "$LOGS_DIR/chunking-counts.tsv" || fail "PARENT chunks missing"
grep -q $'SUB_180\t' "$LOGS_DIR/chunking-counts.tsv" || fail "SUB_180 chunks missing"
grep -q $'SUB_260\t' "$LOGS_DIR/chunking-counts.tsv" || fail "SUB_260 chunks missing"

status="$(psql "$(postgres_url)" -Atqc "SELECT status FROM astravector.document_versions WHERE access_zone_id='${SMOKE_ACCESS_ZONE_A}'::uuid AND document_id='${document_id}'::uuid AND document_version=1")" || fail "document version status query failed"
assert_equals "INDEXING" "$status" "document version was not moved to INDEXING"
