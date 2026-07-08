#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/grpc.sh"
source "$SMOKE_ROOT/lib/processes.sh"
load_smoke_env

REPORT="$REPORTS_DIR/DEAD_LETTER_QDRANT_FAILURE_REPORT.md"
EVIDENCE="$REPORTS_DIR/dead-letter-qdrant-failure-evidence.jsonl"
: > "$EVIDENCE"
ZONE="${SMOKE_ACCESS_ZONE_A:-11111111-1111-4111-8111-111111111111}"

die() { fail "$1"; exit "$FAIL_STATUS"; }
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
emit() {
  jq -nc --arg test_id "$1" --arg status "$2" --arg doc "$3" --arg expected "$4" --arg actual "$5" --arg error "${6:-}" \
    '{test_id:$test_id,status:$status,document_id:$doc,access_zone_id:"'"$ZONE"'",expected:$expected,actual:$actual,error:(if $error=="" then null else $error end)}' >> "$EVIDENCE"
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
start_runtime() {
  local mode="$1"
  local count="${2:-0}"
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
    export ASTRAVECTOR_SMOKE_QDRANT_FAIL_MODE="$mode"
    export ASTRAVECTOR_SMOKE_QDRANT_FAIL_COUNT="$count"
    "$PROJECT_DIR/target/debug/astravector-runtime" >"$LOGS_DIR/dead-letter-${mode}.runtime.log" 2>&1
  ) &
  echo "$!" > "$RUNTIME_DIR/runtime.pid"
  for _ in $(seq 1 60); do grpc_plain list >/dev/null 2>&1 && return 0; sleep 1; done
  return 1
}
register_and_chunk() {
  local doc="$1"
  local text="$2"
  local key="$3"
  local hash
  hash="$(sha256_text "$text")"
  local reg
  reg="$(jq -n --arg zone "$ZONE" --arg doc "$doc" --arg hash "$hash" '{accessZoneId:$zone,documentId:$doc,documentVersion:1,contentHash:$hash,activationPolicy:"ACTIVE_LATEST_ONLY"}')"
  grpc_plain -d "$reg" astravector.embedding.v1.AstraVectorV004Control/RegisterDocumentVersion >/dev/null 2>"$LOGS_DIR/dead-letter-${key}.register.err" || return 1
  local chunk
  chunk="$(jq -n --arg zone "$ZONE" --arg doc "$doc" --arg text "$text" --arg key "$key" '{accessZoneId:$zone,documentId:$doc,documentVersion:1,sourceText:$text,accessLevel:"PUBLIC",profile:{profileVersion:"dead-letter-v1"},metadata:{smoke:"dead-letter"},idempotencyKey:$key,correlationId:"dead-letter"}')"
  grpc_plain -d "$chunk" astravector.embedding.v1.AstraVectorV004Control/CreateMultiGranularityChunks >/dev/null 2>"$LOGS_DIR/dead-letter-${key}.chunk.err"
}
counts_for_doc() {
  local doc="$1"
  local outbox bindings qdrant attempts
  outbox="$(psql "$(postgres_url)" -Atqc "SELECT COALESCE(string_agg(status||':'||count,',' ORDER BY status),'none') FROM (SELECT status,count(*) FROM astravector.vector_outbox o JOIN astravector.vector_bindings_v004 b ON b.access_zone_id=o.binding_access_zone_id AND b.id=o.binding_id WHERE b.access_zone_id='${ZONE}'::uuid AND b.document_id='${doc}'::uuid GROUP BY status)s")"
  bindings="$(psql "$(postgres_url)" -Atqc "SELECT COALESCE(string_agg(qdrant_sync_status||':'||count,',' ORDER BY qdrant_sync_status),'none') FROM (SELECT qdrant_sync_status,count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id='${ZONE}'::uuid AND document_id='${doc}'::uuid GROUP BY qdrant_sync_status)s")"
  attempts="$(psql "$(postgres_url)" -Atqc "SELECT COALESCE(max(o.attempt_count),0) FROM astravector.vector_outbox o JOIN astravector.vector_bindings_v004 b ON b.access_zone_id=o.binding_access_zone_id AND b.id=o.binding_id WHERE b.access_zone_id='${ZONE}'::uuid AND b.document_id='${doc}'::uuid")"
  qdrant="$(qdrant_doc_count "$doc")"
  printf 'outbox=%s bindings=%s max_attempts=%s qdrant=%s' "$outbox" "$bindings" "$attempts" "$qdrant"
}
wait_for_status() {
  local doc="$1"
  local wanted="$2"
  local actual
  for _ in $(seq 1 90); do
    actual="$(counts_for_doc "$doc")"
    if [[ "$actual" == *"$wanted"* ]]; then
      printf '%s' "$actual"
      return 0
    fi
    sleep 1
  done
  counts_for_doc "$doc"
  return 1
}
wait_for_full_recovery() {
  local doc="$1"
  local actual
  for _ in $(seq 1 90); do
    actual="$(counts_for_doc "$doc")"
    if [[ "$actual" == *"outbox=COMPLETED:"* && "$actual" == *"bindings=SYNCED:"* && "$actual" != *"PENDING:"* && "$actual" != *"RETRY_PENDING:"* && "$actual" != *"qdrant=0"* ]]; then
      printf '%s' "$actual"
      return 0
    fi
    sleep 1
  done
  counts_for_doc "$doc"
  return 1
}

cargo build --features smoke-failpoints --bin astravector-runtime >/dev/null || die "smoke-failpoints build failed"
failures=0

doc_dead="$(uuid_for "dead-letter-qdrant-always-fail")"
text_dead="Dead letter Qdrant always fail document."
clear_doc "$doc_dead"
start_runtime "always_fail" 0 || die "runtime did not start for always_fail"
register_and_chunk "$doc_dead" "$text_dead" "dead-letter-always-fail" || die "register/chunk failed for always_fail"
actual="$(wait_for_status "$doc_dead" "outbox=DEAD_LETTER:" || true)"
if [[ "$actual" == *"outbox=DEAD_LETTER:"* && "$actual" != *"bindings=SYNCED:"* && "$actual" == *"qdrant=0"* ]]; then
  emit "W3C_qdrant_always_fail_dead_letter" "PASS" "$doc_dead" "outbox reaches DEAD_LETTER without synced Qdrant point" "$actual"
else
  emit "W3C_qdrant_always_fail_dead_letter" "FAIL" "$doc_dead" "outbox reaches DEAD_LETTER without synced Qdrant point" "$actual"
  failures=$((failures+1))
fi

doc_recover="$(uuid_for "dead-letter-qdrant-fail-n-times")"
text_recover="Dead letter Qdrant transient fail document."
clear_doc "$doc_recover"
start_runtime "fail_n_times" 2 || die "runtime did not start for fail_n_times"
register_and_chunk "$doc_recover" "$text_recover" "dead-letter-transient-recovery" || die "register/chunk failed for fail_n_times"
actual="$(wait_for_full_recovery "$doc_recover" || true)"
if [[ "$actual" == *"outbox=COMPLETED:"* && "$actual" == *"bindings=SYNCED:"* && "$actual" != *"qdrant=0"* ]]; then
  emit "W3C_qdrant_transient_failure_recovers" "PASS" "$doc_recover" "transient Qdrant failures retry to COMPLETED/SYNCED/Qdrant point" "$actual"
else
  emit "W3C_qdrant_transient_failure_recovers" "FAIL" "$doc_recover" "transient Qdrant failures retry to COMPLETED/SYNCED/Qdrant point" "$actual"
  failures=$((failures+1))
fi

stop_process runtime >/dev/null 2>&1 || true
{
  echo "# AstraVector_v004 Dead Letter Qdrant Failure Report"
  echo
  echo "## Verdict"
  [[ "$failures" -eq 0 ]] && echo "DEAD_LETTER_QDRANT_FAILURE_PASS" || echo "DEAD_LETTER_QDRANT_FAILURE_FAIL"
  echo
  echo "## Evidence"
  echo '```json'
  jq -s . "$EVIDENCE"
  echo '```'
} > "$REPORT"
[[ "$failures" -eq 0 ]] || exit "$FAIL_STATUS"
exit "$PASS"
