#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/grpc.sh"
source "$SMOKE_ROOT/lib/processes.sh"
load_smoke_env

REPORT="$REPORTS_DIR/ATOMICITY_FAILPOINTS_REPORT.md"
EVIDENCE="$REPORTS_DIR/atomicity-failpoints-evidence.jsonl"
: > "$EVIDENCE"
ZONE="${SMOKE_ACCESS_ZONE_A:-11111111-1111-4111-8111-111111111111}"

die() { fail "$1"; exit "$FAIL_STATUS"; }
emit() {
  jq -nc --arg test_id "$1" --arg status "$2" --arg doc "$3" --arg expected "$4" --arg actual "$5" --arg error "${6:-}" \
    '{test_id:$test_id,status:$status,document_id:$doc,access_zone_id:"'"$ZONE"'",expected:$expected,actual:$actual,sql_evidence:{},qdrant_evidence:{},grpc_evidence:{},error:(if $error=="" then null else $error end)}' >> "$EVIDENCE"
}
uuid_for() { python3 - "$1" <<'PY'
import sys, uuid
print(uuid.uuid5(uuid.NAMESPACE_URL, "astravector:v004:" + sys.argv[1]))
PY
}
sha256_text() { python3 - "$1" <<'PY'
import hashlib, sys
print(hashlib.sha256(sys.argv[1].encode()).hexdigest())
PY
}
qdrant_doc_count() {
  local doc="$1"
  curl -sS -X POST -H 'content-type: application/json' \
    --data "$(jq -n --arg zone "$ZONE" --arg doc "$doc" '{exact:true,filter:{must:[{key:"access_zone_id",match:{value:$zone}},{key:"document_id",match:{value:$doc}}]}}')" \
    "${QDRANT_HTTP_URL}/collections/${QDRANT_COLLECTION}/points/count" | jq -r '.result.count // 0'
}
clear_doc() {
  local doc="$1"
  psql "$(postgres_url)" -v ON_ERROR_STOP=1 \
    -c "DELETE FROM astravector.vector_outbox o USING astravector.vector_bindings_v004 b WHERE o.binding_access_zone_id=b.access_zone_id AND o.binding_id=b.id AND b.access_zone_id='${ZONE}'::uuid AND b.document_id='${doc}'::uuid" \
    -c "DELETE FROM astravector.vector_bindings_v004 WHERE access_zone_id='${ZONE}'::uuid AND document_id='${doc}'::uuid" \
    -c "DELETE FROM astravector.content_chunks_v004 WHERE access_zone_id='${ZONE}'::uuid AND document_id='${doc}'::uuid" \
    -c "DELETE FROM astravector.document_versions WHERE access_zone_id='${ZONE}'::uuid AND document_id='${doc}'::uuid" >/dev/null || die "clear doc failed"
  curl -sS -X POST -H 'content-type: application/json' --data "$(jq -n --arg zone "$ZONE" --arg doc "$doc" '{filter:{must:[{key:"access_zone_id",match:{value:$zone}},{key:"document_id",match:{value:$doc}}]}}')" "${QDRANT_HTTP_URL}/collections/${QDRANT_COLLECTION}/points/delete?wait=true" >/dev/null || true
}
start_fp_runtime() {
  local fp="$1"
  stop_process runtime >/dev/null 2>&1 || true
  pkill -f "$PROJECT_DIR/target/debug/astravector-runtime" >/dev/null 2>&1 || true
  sleep 1
  (
    set -a
    # shellcheck disable=SC1090
    . "$SMOKE_ENV_FILE"
    set +a
    export ASTRAVECTOR_CONFIG="$SMOKE_ROOT/config/application-smoke.yaml"
    export ASTRAVECTOR_DB_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@${POSTGRES_HOST}:${POSTGRES_PORT}/${POSTGRES_DB}"
    export ASTRAVECTOR_QDRANT_URL="$QDRANT_HTTP_URL"
    export ASTRAVECTOR_QDRANT_COLLECTION="$QDRANT_COLLECTION"
    export ASTRAVECTOR_SMOKE_FAILPOINTS_ENABLED=true
    export ASTRA_SMOKE_FAILPOINT="$fp"
    "$PROJECT_DIR/target/debug/astravector-runtime" >"$LOGS_DIR/atomicity-${fp}.runtime.log" 2>&1
  ) &
  echo "$!" > "$RUNTIME_DIR/runtime.pid"
  for _ in $(seq 1 60); do grpc_plain list >/dev/null 2>&1 && return 0; sleep 1; done
  return 1
}
start_clean_runtime() {
  stop_process runtime >/dev/null 2>&1 || true
  pkill -f "$PROJECT_DIR/target/debug/astravector-runtime" >/dev/null 2>&1 || true
  sleep 1
  (
    set -a
    # shellcheck disable=SC1090
    . "$SMOKE_ENV_FILE"
    set +a
    export ASTRAVECTOR_CONFIG="$SMOKE_ROOT/config/application-smoke.yaml"
    export ASTRAVECTOR_DB_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@${POSTGRES_HOST}:${POSTGRES_PORT}/${POSTGRES_DB}"
    export ASTRAVECTOR_QDRANT_URL="$QDRANT_HTTP_URL"
    export ASTRAVECTOR_QDRANT_COLLECTION="$QDRANT_COLLECTION"
    unset ASTRA_SMOKE_FAILPOINT
    unset ASTRAVECTOR_SMOKE_FAILPOINT
    unset ASTRAVECTOR_SMOKE_FAILPOINTS_ENABLED
    "$PROJECT_DIR/target/debug/astravector-runtime" >"$LOGS_DIR/atomicity-clean.runtime.log" 2>&1
  ) &
  echo "$!" > "$RUNTIME_DIR/runtime.pid"
  for _ in $(seq 1 60); do grpc_plain list >/dev/null 2>&1 && return 0; sleep 1; done
  return 1
}
counts_for_doc() {
  local doc="$1"
  local active chunks bindings completed qdrant
  active="$(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM astravector.document_versions WHERE access_zone_id='${ZONE}'::uuid AND document_id='${doc}'::uuid AND status='ACTIVE'")"
  chunks="$(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM astravector.content_chunks_v004 WHERE access_zone_id='${ZONE}'::uuid AND document_id='${doc}'::uuid")"
  bindings="$(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id='${ZONE}'::uuid AND document_id='${doc}'::uuid AND qdrant_sync_status='SYNCED'")"
  completed="$(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM astravector.vector_outbox o JOIN astravector.vector_bindings_v004 b ON b.access_zone_id=o.binding_access_zone_id AND b.id=o.binding_id WHERE b.access_zone_id='${ZONE}'::uuid AND b.document_id='${doc}'::uuid AND o.status='COMPLETED'")"
  qdrant="$(qdrant_doc_count "$doc")"
  printf 'active=%s chunks=%s synced_bindings=%s completed_outbox=%s qdrant=%s' "$active" "$chunks" "$bindings" "$completed" "$qdrant"
}
wait_for_retry_completion() {
  local doc="$1"
  local actual
  for _ in $(seq 1 60); do
    actual="$(counts_for_doc "$doc")"
    if [[ "$actual" != *"synced_bindings=0"* && "$actual" != *"completed_outbox=0"* && "$actual" != *"qdrant=0"* ]]; then
      printf '%s' "$actual"
      return 0
    fi
    sleep 1
  done
  counts_for_doc "$doc"
  return 1
}

cargo build --features smoke-failpoints --bin astravector-runtime >/dev/null || die "smoke-failpoints build failed"
failpoints=(required_after_document_version_update required_after_chunk_insert required_after_embedding_cache_insert required_after_dense_insert required_after_binding_insert required_after_outbox_insert required_before_commit required_after_commit_before_response)
failures=0
for fp in "${failpoints[@]}"; do
  doc="$(uuid_for "atomicity-${fp}")"
  text="Atomicity failpoint ${fp} document."
  hash="$(sha256_text "$text")"
  clear_doc "$doc"
  if [[ "$fp" == "required_after_document_version_update" ]]; then
    start_fp_runtime "$fp" || die "runtime did not start for $fp"
    body="$(jq -n --arg zone "$ZONE" --arg doc "$doc" --arg hash "$hash" '{accessZoneId:$zone,documentId:$doc,documentVersion:1,contentHash:$hash,activationPolicy:"ACTIVE_LATEST_ONLY"}')"
    if grpc_plain -d "$body" astravector.embedding.v1.AstraVectorV004Control/RegisterDocumentVersion >"$LOGS_DIR/atomicity-${fp}.json" 2>"$LOGS_DIR/atomicity-${fp}.err"; then
      emit "W3B_${fp}" "FAIL" "$doc" "controlled gRPC error" "success"
      failures=$((failures+1))
    else
      actual="$(counts_for_doc "$doc")"
      if [[ "$actual" == *"active=0"* && "$actual" == *"chunks=0"* && "$actual" == *"synced_bindings=0"* && "$actual" == *"completed_outbox=0"* && "$actual" == *"qdrant=0"* ]]; then
        emit "W3B_${fp}" "PASS" "$doc" "rollback/no completed state" "$actual"
      else
        emit "W3B_${fp}" "FAIL" "$doc" "rollback/no completed state" "$actual"
        failures=$((failures+1))
      fi
    fi
  else
    start_clean_runtime || die "clean runtime did not start"
    reg="$(jq -n --arg zone "$ZONE" --arg doc "$doc" --arg hash "$hash" '{accessZoneId:$zone,documentId:$doc,documentVersion:1,contentHash:$hash,activationPolicy:"ACTIVE_LATEST_ONLY"}')"
    grpc_plain -d "$reg" astravector.embedding.v1.AstraVectorV004Control/RegisterDocumentVersion >/dev/null 2>"$LOGS_DIR/atomicity-${fp}.register.err" || die "register failed before $fp"
    start_fp_runtime "$fp" || die "runtime did not start for $fp"
    chunk="$(jq -n --arg zone "$ZONE" --arg doc "$doc" --arg text "$text" --arg key "atomicity-${fp}" '{accessZoneId:$zone,documentId:$doc,documentVersion:1,sourceText:$text,accessLevel:"PUBLIC",profile:{profileVersion:"atomicity-v1"},metadata:{smoke:"atomicity"},idempotencyKey:$key,correlationId:"atomicity"}')"
    if grpc_plain -d "$chunk" astravector.embedding.v1.AstraVectorV004Control/CreateMultiGranularityChunks >"$LOGS_DIR/atomicity-${fp}.json" 2>"$LOGS_DIR/atomicity-${fp}.err"; then
      if [[ "$fp" != "required_after_commit_before_response" ]]; then
        emit "W3B_${fp}" "FAIL" "$doc" "controlled gRPC error" "success"
        failures=$((failures+1))
        continue
      fi
    fi
    start_clean_runtime || die "clean retry runtime did not start"
    grpc_plain -d "$chunk" astravector.embedding.v1.AstraVectorV004Control/CreateMultiGranularityChunks >/dev/null 2>"$LOGS_DIR/atomicity-${fp}.retry.err" || die "retry failed for $fp"
    actual="$(wait_for_retry_completion "$doc")"
    dup_chunks="$(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM (SELECT access_zone_id,document_id,document_version,source_chunk_id,parent_chunk_id,granularity,sequence_no,content_hash,count(*) FROM astravector.content_chunks_v004 WHERE access_zone_id='${ZONE}'::uuid AND document_id='${doc}'::uuid GROUP BY access_zone_id,document_id,document_version,source_chunk_id,parent_chunk_id,granularity,sequence_no,content_hash HAVING count(*)>1)d")"
    if [[ "$dup_chunks" -eq 0 && "$actual" != *"synced_bindings=0"* ]]; then
      emit "W3B_${fp}" "PASS" "$doc" "retry completes without duplicate canonical state" "$actual duplicate_chunks=$dup_chunks"
    else
      emit "W3B_${fp}" "FAIL" "$doc" "retry completes without duplicate canonical state" "$actual duplicate_chunks=$dup_chunks"
      failures=$((failures+1))
    fi
  fi
done
stop_process runtime >/dev/null 2>&1 || true
{
  echo "# AstraVector_v004 Atomicity Failpoints Report"
  echo
  echo "## Verdict"
  [[ "$failures" -eq 0 ]] && echo "ATOMICITY_FAILPOINTS_PASS" || echo "ATOMICITY_FAILPOINTS_FAIL"
  echo
  echo "## Evidence"
  echo '```json'
  jq -s . "$EVIDENCE"
  echo '```'
} > "$REPORT"
[[ "$failures" -eq 0 ]] || exit "$FAIL_STATUS"
exit "$PASS"
