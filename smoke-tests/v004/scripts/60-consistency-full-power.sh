#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/grpc.sh"
load_smoke_env

LOG_DIR="$LOGS_DIR/consistency"
EVIDENCE="$REPORTS_DIR/consistency-evidence.jsonl"
METRICS="$REPORTS_DIR/consistency-metrics.json"
REPORT="$REPORTS_DIR/CONSISTENCY_REPORT.md"
mkdir -p "$LOG_DIR"
rm -f "$LOG_DIR"/* "$LOG_DIR"/search/* 2>/dev/null || true
: > "$EVIDENCE"

ZONE="${SMOKE_ACCESS_ZONE_A:-11111111-1111-4111-8111-111111111111}"
CIVIL_DOC="${CIVIL_CODE_DOCUMENT_ID:-72fd8953-9f11-5eef-a03c-ef47c3d40daa}"
runtime_pid=""

register_parallel_requests=50
register_rows_created=0
register_idempotent_responses=0
register_conflict_rejected=false
register_conflict_status="UNKNOWN"
chunking_parallel_requests=50
chunking_success=0
chunking_conflict_rejected=false
chunking_conflict_status="UNKNOWN"
duplicate_chunks=0
duplicate_bindings=0
duplicate_outbox_logical_events=0
activation_parallel_requests=10
activation_success=0
active_versions=0
concurrent_search_requests=100
concurrent_search_success=0
concurrent_search_transport_errors=0
cross_zone_leakage_count=0
empty_parent_context_count=0
atomicity_failpoints_total=8
atomicity_failpoints_passed=0
atomicity_failpoints_status="BLOCKED"
atomicity_failpoints_reason="runtime has no smoke-failpoints support"
outbox_double_claim_status="BLOCKED"
outbox_stale_completion_status="BLOCKED"
outbox_fencing_reason="outbox has no fencing token/generation"
qdrant_idempotent_upsert_pass=false
dead_letter_test_status="BLOCKED"
dead_letter_reason="no controllable Qdrant failure mechanism"
data_integrity_violations_after_wave3=0

record_evidence() {
  jq -nc --arg type "$1" --arg status "$2" --argjson details "$3" \
    '{type:$type,status:$status,details:$details}' >> "$EVIDENCE"
}

grpc_json_ok() {
  local body="$1" method="$2" out="$3" err="$4" max_time="${5:-30}"
  grpcurl -plaintext -max-time "$max_time" -d "$body" "$(grpc_addr)" "$method" >"$out" 2>"$err"
}

die() {
  fail "$1"
  write_reports "CONSISTENCY_FAIL"
  exit "$FAIL_STATUS"
}

uuid_for() {
  python3 - "$1" <<'PY'
import sys, uuid
print(uuid.uuid5(uuid.NAMESPACE_URL, "astravector:v004:" + sys.argv[1]))
PY
}

sha256_text() {
  python3 - "$1" <<'PY'
import hashlib, sys
print(hashlib.sha256(sys.argv[1].encode("utf-8")).hexdigest())
PY
}

ensure_runtime() {
  if grpc_plain list >/dev/null 2>&1; then
    return 0
  fi
  [[ -x "$PROJECT_DIR/target/debug/astravector-runtime" ]] || return 1
  (
    set -a
    # shellcheck disable=SC1090
    . "$SMOKE_ENV_FILE"
    set +a
    export ASTRAVECTOR_CONFIG="$SMOKE_ROOT/config/application-smoke.yaml"
    export ASTRAVECTOR_DB_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@${POSTGRES_HOST}:${POSTGRES_PORT}/${POSTGRES_DB}"
    export ASTRAVECTOR_QDRANT_URL="$QDRANT_HTTP_URL"
    export ASTRAVECTOR_QDRANT_COLLECTION="$QDRANT_COLLECTION"
    "$PROJECT_DIR/target/debug/astravector-runtime" >"$LOGS_DIR/consistency-runtime.log" 2>&1
  ) &
  runtime_pid="$!"
  for _ in $(seq 1 60); do
    grpc_plain list >/dev/null 2>&1 && return 0
    sleep 1
  done
  return 1
}

cleanup_runtime() {
  [[ -n "$runtime_pid" ]] && kill "$runtime_pid" >/dev/null 2>&1 || true
}
trap cleanup_runtime EXIT

clear_doc() {
  local zone="$1" doc="$2"
  psql "$(postgres_url)" -v ON_ERROR_STOP=1 \
    -c "DELETE FROM astravector.vector_outbox o USING astravector.vector_bindings_v004 b WHERE o.binding_access_zone_id=b.access_zone_id AND o.binding_id=b.id AND b.access_zone_id='${zone}'::uuid AND b.document_id='${doc}'::uuid" \
    -c "DELETE FROM astravector.vector_bindings_v004 WHERE access_zone_id='${zone}'::uuid AND document_id='${doc}'::uuid" \
    -c "DELETE FROM astravector.content_chunks_v004 WHERE access_zone_id='${zone}'::uuid AND document_id='${doc}'::uuid" \
    -c "DELETE FROM astravector.document_versions WHERE access_zone_id='${zone}'::uuid AND document_id='${doc}'::uuid" >/dev/null || die "failed to clear consistency doc"
  curl -sS -X POST -H 'content-type: application/json' \
    --data "$(jq -n --arg zone "$zone" --arg doc "$doc" '{filter:{must:[{key:"access_zone_id",match:{value:$zone}},{key:"document_id",match:{value:$doc}}]}}')" \
    "${QDRANT_HTTP_URL}/collections/${QDRANT_COLLECTION}/points/delete?wait=true" >/dev/null || die "failed to clear consistency qdrant points"
}

grpc_error_code() {
  local err="$1"
  if grep -q "Code: AlreadyExists" "$err"; then printf "ALREADY_EXISTS"
  elif grep -q "Code: FailedPrecondition" "$err"; then printf "FAILED_PRECONDITION"
  elif grep -q "Code: Internal" "$err"; then printf "INTERNAL"
  elif grep -q "Code: Unavailable" "$err"; then printf "UNAVAILABLE"
  else printf "TRANSPORT_OR_OTHER"
  fi
}

wait_indexed() {
  local doc="$1" deadline searchable synced completed qdrant
  deadline=$((SECONDS + 120))
  while (( SECONDS < deadline )); do
    searchable="$(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id='${ZONE}'::uuid AND document_id='${doc}'::uuid AND chunk_granularity IN('PARENT','SUB_180','SUB_260')")" || searchable=0
    synced="$(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id='${ZONE}'::uuid AND document_id='${doc}'::uuid AND chunk_granularity IN('PARENT','SUB_180','SUB_260') AND qdrant_sync_status='SYNCED'")" || synced=0
    completed="$(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM astravector.vector_outbox o JOIN astravector.vector_bindings_v004 b ON b.access_zone_id=o.binding_access_zone_id AND b.id=o.binding_id WHERE b.access_zone_id='${ZONE}'::uuid AND b.document_id='${doc}'::uuid AND o.operation='UPSERT_POINT' AND o.status='COMPLETED'")" || completed=0
    qdrant="$(curl -sS -X POST -H 'content-type: application/json' --data "$(jq -n --arg zone "$ZONE" --arg doc "$doc" '{exact:true,filter:{must:[{key:"access_zone_id",match:{value:$zone}},{key:"document_id",match:{value:$doc}},{key:"chunk_granularity",match:{any:["PARENT","SUB_180","SUB_260"]}}]}}')" "${QDRANT_HTTP_URL}/collections/${QDRANT_COLLECTION}/points/count" | jq -r '.result.count // 0')" || qdrant=0
    [[ "$searchable" -gt 0 && "$synced" -eq "$searchable" && "$completed" -eq "$searchable" && "$qdrant" -eq "$searchable" ]] && return 0
    sleep 2
  done
  return 1
}

write_reports() {
  local verdict="$1"
  jq -n \
    --arg verdict "$verdict" \
    --argjson register_parallel_requests "$register_parallel_requests" \
    --argjson register_rows_created "$register_rows_created" \
    --argjson register_idempotent_responses "$register_idempotent_responses" \
    --argjson register_conflict_rejected "$register_conflict_rejected" \
    --argjson chunking_parallel_requests "$chunking_parallel_requests" \
    --argjson chunking_success "$chunking_success" \
    --argjson chunking_conflict_rejected "$chunking_conflict_rejected" \
    --argjson duplicate_chunks "$duplicate_chunks" \
    --argjson duplicate_bindings "$duplicate_bindings" \
    --argjson duplicate_outbox_logical_events "$duplicate_outbox_logical_events" \
    --argjson activation_parallel_requests "$activation_parallel_requests" \
    --argjson activation_success "$activation_success" \
    --argjson active_versions "$active_versions" \
    --argjson concurrent_search_requests "$concurrent_search_requests" \
    --argjson concurrent_search_success "$concurrent_search_success" \
    --argjson concurrent_search_transport_errors "$concurrent_search_transport_errors" \
    --argjson cross_zone_leakage_count "$cross_zone_leakage_count" \
    --argjson empty_parent_context_count "$empty_parent_context_count" \
    --argjson atomicity_failpoints_total "$atomicity_failpoints_total" \
    --argjson atomicity_failpoints_passed "$atomicity_failpoints_passed" \
    --arg atomicity_failpoints_status "$atomicity_failpoints_status" \
    --arg atomicity_failpoints_reason "$atomicity_failpoints_reason" \
    --arg outbox_double_claim_status "$outbox_double_claim_status" \
    --arg outbox_stale_completion_status "$outbox_stale_completion_status" \
    --arg outbox_fencing_reason "$outbox_fencing_reason" \
    --argjson qdrant_idempotent_upsert_pass "$qdrant_idempotent_upsert_pass" \
    --arg dead_letter_test_status "$dead_letter_test_status" \
    --arg dead_letter_reason "$dead_letter_reason" \
    --argjson data_integrity_violations_after_wave3 "$data_integrity_violations_after_wave3" \
    '{verdict:$verdict,register_parallel_requests:$register_parallel_requests,register_rows_created:$register_rows_created,register_idempotent_responses:$register_idempotent_responses,register_conflict_rejected:$register_conflict_rejected,chunking_parallel_requests:$chunking_parallel_requests,chunking_success:$chunking_success,chunking_conflict_rejected:$chunking_conflict_rejected,duplicate_chunks:$duplicate_chunks,duplicate_bindings:$duplicate_bindings,duplicate_outbox_logical_events:$duplicate_outbox_logical_events,activation_parallel_requests:$activation_parallel_requests,activation_success:$activation_success,active_versions:$active_versions,concurrent_search_requests:$concurrent_search_requests,concurrent_search_success:$concurrent_search_success,concurrent_search_transport_errors:$concurrent_search_transport_errors,cross_zone_leakage_count:$cross_zone_leakage_count,empty_parent_context_count:$empty_parent_context_count,atomicity_failpoints_total:$atomicity_failpoints_total,atomicity_failpoints_passed:$atomicity_failpoints_passed,atomicity_failpoints_status:$atomicity_failpoints_status,atomicity_failpoints_reason:$atomicity_failpoints_reason,outbox_double_claim_status:$outbox_double_claim_status,outbox_stale_completion_status:$outbox_stale_completion_status,outbox_fencing_reason:$outbox_fencing_reason,qdrant_idempotent_upsert_pass:$qdrant_idempotent_upsert_pass,dead_letter_test_status:$dead_letter_test_status,dead_letter_reason:$dead_letter_reason,data_integrity_violations_after_wave3:$data_integrity_violations_after_wave3}' > "$METRICS"
  {
    echo "# AstraVector_v004 Consistency Report"
    echo
    echo "## 1. Verdict"
    echo "$verdict"
    echo
    echo "## 2. Summary"
    echo "| Check | Status | Evidence |"
    echo "|---|---|---|"
    echo "| Register idempotency | $([[ "$register_rows_created" -eq 1 && "$register_idempotent_responses" -gt 0 ]] && echo PASS || echo FAIL) | $EVIDENCE |"
    echo "| Register conflicting idempotency | $([[ "$register_conflict_rejected" == true ]] && echo PASS || echo FAIL) | $LOG_DIR/register-conflict.err |"
    echo "| Chunking idempotency | $([[ "$duplicate_chunks" -eq 0 && "$duplicate_bindings" -eq 0 && "$duplicate_outbox_logical_events" -eq 0 ]] && echo PASS || echo FAIL) | $EVIDENCE |"
    echo "| Chunking conflicting idempotency | $([[ "$chunking_conflict_rejected" == true ]] && echo PASS || echo FAIL) | $LOG_DIR/chunk-conflict.err |"
    echo "| Activation idempotency | $([[ "$active_versions" -eq 1 ]] && echo PASS || echo FAIL) | $EVIDENCE |"
    echo "| Concurrent Search | $([[ "$concurrent_search_transport_errors" -eq 0 && "$cross_zone_leakage_count" -eq 0 && "$empty_parent_context_count" -eq 0 ]] && echo PASS || echo FAIL) | $LOG_DIR/search |"
    echo "| Atomicity failpoints | $atomicity_failpoints_status | $atomicity_failpoints_reason |"
    echo "| Outbox double claim | $outbox_double_claim_status | $outbox_fencing_reason |"
    echo "| Outbox stale completion | $outbox_stale_completion_status | $outbox_fencing_reason |"
    echo "| Qdrant idempotent upsert | $([[ "$qdrant_idempotent_upsert_pass" == true ]] && echo PASS || echo FAIL) | $EVIDENCE |"
    echo "| Dead letter | $dead_letter_test_status | $dead_letter_reason |"
    echo "| Data integrity audit after Wave 3 | $([[ "$data_integrity_violations_after_wave3" -eq 0 ]] && echo PASS || echo FAIL) | $REPORTS_DIR/full-power-data-integrity.tsv |"
    echo
    echo "## 3. Metrics"
    echo '```json'
    jq . "$METRICS"
    echo '```'
    echo
    echo "## 4. Duplicate Checks"
    echo "| Check | Count |"
    echo "|---|---:|"
    echo "| duplicate_chunks | $duplicate_chunks |"
    echo "| duplicate_bindings | $duplicate_bindings |"
    echo "| duplicate_outbox_logical_events | $duplicate_outbox_logical_events |"
    echo
    echo "## 5. Atomicity Findings"
    echo "| Failpoint | Expected | Actual | Status |"
    echo "|---|---|---|---|"
    echo "| smoke-failpoints | runtime hooks | not present | BLOCKED |"
    echo
    echo "## 6. Outbox Fencing Findings"
    echo "| Scenario | Expected | Actual | Status |"
    echo "|---|---|---|---|"
    echo "| double claim | lock generation/fencing token | schema has lease but no generation token | BLOCKED |"
    echo "| stale completion | stale generation rejected | cannot prove without generation token | BLOCKED |"
    echo "| qdrant idempotent upsert | count by binding_id = 1 | pass=$qdrant_idempotent_upsert_pass | $([[ "$qdrant_idempotent_upsert_pass" == true ]] && echo PASS || echo FAIL) |"
    echo
    echo "## 7. Remaining Blockers"
    echo "- smoke-failpoints are not implemented in runtime"
    echo "- vector_outbox has no lock_generation/fencing_token"
    echo "- no controllable Qdrant failure hook for dead-letter proof"
  } > "$REPORT"
  update_full_power_report "$verdict"
}

update_full_power_report() {
  local consistency_verdict="$1"
  local system_verdict="SECURE_RAG_CORE_CANDIDATE + ${consistency_verdict}"
  local metrics_tmp results_tmp
  metrics_tmp="$REPORTS_DIR/full-power-smoke-metrics.tmp.json"
  results_tmp="$REPORTS_DIR/full-power-smoke-results.tmp.json"
  jq -n \
    --arg verdict "$system_verdict" \
    --slurpfile consistency "$METRICS" \
    --slurpfile current "$REPORTS_DIR/full-power-smoke-metrics.json" \
    '{verdict:$verdict,previous:($current[0] // null),consistency:$consistency[0]}' > "$metrics_tmp"
  mv "$metrics_tmp" "$REPORTS_DIR/full-power-smoke-metrics.json"
  jq -n \
    --arg verdict "$system_verdict" \
    --slurpfile metrics "$REPORTS_DIR/full-power-smoke-metrics.json" \
    '{verdict:$verdict,metrics:$metrics[0]}' > "$results_tmp"
  mv "$results_tmp" "$REPORTS_DIR/full-power-smoke-results.json"
  {
    echo "# AstraVector_v004 Full Power Smoke Report"
    echo
    echo "## 1. Verdict"
    echo "$system_verdict"
    echo
    echo "## 2. Wave Summary"
    echo "| Wave | Status | Evidence |"
    echo "|---|---|---|"
    echo "| Wave 1 RAG Core | PASS | $REPORTS_DIR/full-power-smoke-results.json |"
    echo "| Wave 2 Access Security | ACCESS_SECURITY_PASS | $REPORTS_DIR/ACCESS_SECURITY_REPORT.md |"
    echo "| Wave 3 Consistency | $consistency_verdict | $REPORT |"
    echo
    echo "## 3. Consistency Metrics"
    echo '```json'
    jq . "$METRICS"
    echo '```'
    echo
    echo "## 4. Remaining Blockers"
    echo "- lifecycle TTL/legal-hold/delete not yet full-power tested"
    echo "- reconciliation/rebuild not yet full-power tested"
    echo "- smoke-failpoints are not implemented"
    echo "- outbox lock_generation/fencing_token is not implemented"
    echo "- outbox dead-letter requires controllable Qdrant failure hook"
    echo "- overload/backpressure not yet full-power tested"
    echo "- observability not yet full-power tested"
  } > "$REPORTS_DIR/FULL_POWER_SMOKE_REPORT.md"
}

ensure_runtime || die "runtime gRPC service did not become ready"
grpc_assert_service "astravector.embedding.v1.AstraVectorV004Control" || exit "$BLOCKED_STATUS"

doc="$(uuid_for "consistency-document-v1")"
text="AstraVector v004 consistency smoke document. CONSISTENCY_IDEMPOTENCY_SECRET_AST_VECTOR_004."
hash="$(sha256_text "$text")"
clear_doc "$ZONE" "$doc"

register_body="$(jq -n --arg zone "$ZONE" --arg doc "$doc" --arg hash "$hash" '{accessZoneId:$zone,documentId:$doc,documentVersion:1,contentHash:$hash,activationPolicy:"ACTIVE_LATEST_ONLY",idempotencyKey:"consistency-register-v1"}')"
pids=""
for i in $(seq 1 "$register_parallel_requests"); do
  (grpc_json_ok "$register_body" astravector.embedding.v1.AstraVectorV004Control/RegisterDocumentVersion "$LOG_DIR/register-$i.json" "$LOG_DIR/register-$i.err" 20) &
  pids="$pids $!"
done
for pid in $pids; do wait "$pid" || true; done
register_success="$(ls "$LOG_DIR"/register-*.json 2>/dev/null | xargs -n1 jq -r '.documentId? // empty' 2>/dev/null | grep -c "^$doc$" || true)"
register_idempotent_responses="$register_success"
register_rows_created="$(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM astravector.document_versions WHERE access_zone_id='${ZONE}'::uuid AND document_id='${doc}'::uuid AND document_version=1")" || register_rows_created=0
[[ "$register_rows_created" -eq 1 ]] || die "register idempotency row count is $register_rows_created"
record_evidence "register_parallel" "PASS" "$(jq -n --argjson requests "$register_parallel_requests" --argjson success "$register_success" --argjson rows "$register_rows_created" '{requests:$requests,success:$success,rows:$rows}')"

conflict_hash="$(sha256_text "different consistency text")"
conflict_body="$(jq -n --arg zone "$ZONE" --arg doc "$doc" --arg hash "$conflict_hash" '{accessZoneId:$zone,documentId:$doc,documentVersion:1,contentHash:$hash,activationPolicy:"ACTIVE_LATEST_ONLY",idempotencyKey:"consistency-register-v1"}')"
if grpc_plain -d "$conflict_body" astravector.embedding.v1.AstraVectorV004Control/RegisterDocumentVersion >"$LOG_DIR/register-conflict.json" 2>"$LOG_DIR/register-conflict.err"; then
  register_conflict_rejected=false
else
  register_conflict_status="$(grpc_error_code "$LOG_DIR/register-conflict.err")"
  [[ "$register_conflict_status" == "ALREADY_EXISTS" || "$register_conflict_status" == "FAILED_PRECONDITION" ]] && register_conflict_rejected=true
fi
[[ "$register_conflict_rejected" == true ]] || die "register conflicting idempotency was not rejected"

chunk_body="$(jq -n --arg zone "$ZONE" --arg doc "$doc" --arg text "$text" '{
  accessZoneId:$zone, documentId:$doc, documentVersion:1, sourceText:$text, accessLevel:"PUBLIC",
  profile:{preserveHeadings:true,preserveParagraphs:true,preserveSentences:true,profileVersion:"consistency-v1",parent:{granularity:"PARENT_V004",targetTokens:60,minTokens:1,maxTokens:120,overlapTokens:0},granularities:[{granularity:"SUB_180_V004",targetTokens:30,minTokens:1,maxTokens:80,overlapTokens:0},{granularity:"SUB_260_V004",targetTokens:45,minTokens:1,maxTokens:100,overlapTokens:0}]},
  metadata:{smoke:"consistency"}, idempotencyKey:"consistency-chunk-v1", correlationId:"consistency-smoke"
}')"
pids=""
for i in $(seq 1 "$chunking_parallel_requests"); do
  (grpc_json_ok "$chunk_body" astravector.embedding.v1.AstraVectorV004Control/CreateMultiGranularityChunks "$LOG_DIR/chunk-$i.json" "$LOG_DIR/chunk-$i.err" 45) &
  pids="$pids $!"
done
for pid in $pids; do wait "$pid" || true; done
chunking_success="$(ls "$LOG_DIR"/chunk-*.json 2>/dev/null | xargs -n1 jq -r '.status? // empty' 2>/dev/null | grep -c "^INDEXING$" || true)"
wait_indexed "$doc" || die "consistency document did not index completely"

conflict_chunk_body="$(jq -n --arg zone "$ZONE" --arg doc "$doc" '{accessZoneId:$zone, documentId:$doc, documentVersion:1, sourceText:"different text for conflicting idempotency", accessLevel:"PUBLIC", profile:{profileVersion:"consistency-v1"}, metadata:{smoke:"consistency"}, idempotencyKey:"consistency-chunk-v1", correlationId:"consistency-smoke-conflict"}')"
if grpc_plain -d "$conflict_chunk_body" astravector.embedding.v1.AstraVectorV004Control/CreateMultiGranularityChunks >"$LOG_DIR/chunk-conflict.json" 2>"$LOG_DIR/chunk-conflict.err"; then
  chunking_conflict_rejected=false
else
  chunking_conflict_status="$(grpc_error_code "$LOG_DIR/chunk-conflict.err")"
  [[ "$chunking_conflict_status" == "ALREADY_EXISTS" || "$chunking_conflict_status" == "FAILED_PRECONDITION" ]] && chunking_conflict_rejected=true
fi
[[ "$chunking_conflict_rejected" == true ]] || die "chunking conflicting idempotency was not rejected"

duplicate_chunks="$(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM (SELECT access_zone_id,document_id,document_version,source_chunk_id,parent_chunk_id,granularity,sequence_no,content_hash,count(*) FROM astravector.content_chunks_v004 WHERE access_zone_id='${ZONE}'::uuid AND document_id='${doc}'::uuid AND document_version=1 GROUP BY access_zone_id,document_id,document_version,source_chunk_id,parent_chunk_id,granularity,sequence_no,content_hash HAVING count(*)>1) d")" || duplicate_chunks=1
duplicate_bindings="$(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM (SELECT access_zone_id,document_id,document_version,chunk_id,representation_type,count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id='${ZONE}'::uuid AND document_id='${doc}'::uuid AND document_version=1 GROUP BY access_zone_id,document_id,document_version,chunk_id,representation_type HAVING count(*)>1) d")" || duplicate_bindings=1
duplicate_outbox_logical_events="$(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM (SELECT binding_id,operation,operation_version,count(*) FROM astravector.vector_outbox WHERE binding_id IN (SELECT id FROM astravector.vector_bindings_v004 WHERE access_zone_id='${ZONE}'::uuid AND document_id='${doc}'::uuid AND document_version=1) GROUP BY binding_id,operation,operation_version HAVING count(*)>1) d")" || duplicate_outbox_logical_events=1
[[ "$duplicate_chunks" -eq 0 && "$duplicate_bindings" -eq 0 && "$duplicate_outbox_logical_events" -eq 0 ]] || die "duplicate canonical state detected"

activate_body="$(jq -n --arg zone "$ZONE" --arg doc "$doc" '{accessZoneId:$zone,documentId:$doc,documentVersion:1}')"
pids=""
for i in $(seq 1 "$activation_parallel_requests"); do
  (grpc_json_ok "$activate_body" astravector.embedding.v1.AstraVectorV004Control/ActivateDocumentVersion "$LOG_DIR/activate-$i.json" "$LOG_DIR/activate-$i.err" 20) &
  pids="$pids $!"
done
for pid in $pids; do wait "$pid" || true; done
activation_success="$(ls "$LOG_DIR"/activate-*.json 2>/dev/null | xargs -n1 jq -r '.status? // empty' 2>/dev/null | grep -c "^ACTIVE$" || true)"
active_versions="$(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM astravector.document_versions WHERE access_zone_id='${ZONE}'::uuid AND document_id='${doc}'::uuid AND status='ACTIVE'")" || active_versions=0
[[ "$active_versions" -eq 1 ]] || die "activation idempotency active version count is $active_versions"

search_dir="$LOG_DIR/search"
mkdir -p "$search_dir"
search_body="$(jq -n --arg zone "$ZONE" '{correlationId:"consistency-search",accessZoneId:$zone,callerAccessLevel:"RESTRICTED",query:"исковая давность",topK:5,candidateLimit:20,parentLimit:5,timeoutMs:10000}')"
pids=""
for i in $(seq 1 "$concurrent_search_requests"); do
  (grpc_json_ok "$search_body" astravector.embedding.v1.AstraVectorV004Control/Search "$search_dir/search-$i.json" "$search_dir/search-$i.err" 20) &
  pids="$pids $!"
done
for pid in $pids; do wait "$pid" || true; done
for i in $(seq 1 "$concurrent_search_requests"); do
  if jq -e '.results|length > 0' "$search_dir/search-$i.json" >/dev/null 2>&1; then
    concurrent_search_success=$((concurrent_search_success + 1))
    leaks="$(jq --arg doc "$CIVIL_DOC" '[.results[]? | select(.accessZoneId!="'"$ZONE"'")] | length' "$search_dir/search-$i.json")"
    empties="$(jq '[.results[]? | select((.parentText // "") == "")] | length' "$search_dir/search-$i.json")"
    cross_zone_leakage_count=$((cross_zone_leakage_count + leaks))
    empty_parent_context_count=$((empty_parent_context_count + empties))
  else
    concurrent_search_transport_errors=$((concurrent_search_transport_errors + 1))
  fi
done
[[ "$concurrent_search_success" -ge 95 && "$concurrent_search_transport_errors" -eq 0 ]] || die "concurrent search did not meet success criteria"

if rg "smoke-failpoints|FAILPOINT|failpoint" Cargo.toml src >/dev/null 2>&1; then
  atomicity_failpoints_status="NOT_READY"
  atomicity_failpoints_reason="failpoint strings present but no Wave 3 test hook implemented"
fi
if psql "$(postgres_url)" -Atqc "SELECT count(*) FROM information_schema.columns WHERE table_schema='astravector' AND table_name='vector_outbox' AND column_name IN ('lock_generation','fencing_token')" | grep -qx "0"; then
  outbox_double_claim_status="BLOCKED"
  outbox_stale_completion_status="BLOCKED"
else
  outbox_double_claim_status="NOT_READY"
  outbox_stale_completion_status="NOT_READY"
  outbox_fencing_reason="fencing column exists but smoke helper is not implemented"
fi

binding_id="$(psql "$(postgres_url)" -Atqc "SELECT id FROM astravector.vector_bindings_v004 WHERE access_zone_id='${ZONE}'::uuid AND document_id='${doc}'::uuid AND chunk_granularity='PARENT' LIMIT 1")"
binding_point_count="$(curl -sS -X POST -H 'content-type: application/json' --data "$(jq -n --arg binding "$binding_id" '{exact:true,filter:{must:[{key:"binding_id",match:{value:$binding}}]}}')" "${QDRANT_HTTP_URL}/collections/${QDRANT_COLLECTION}/points/count" | jq -r '.result.count // 0')"
[[ "$binding_point_count" -eq 1 ]] && qdrant_idempotent_upsert_pass=true

"$SMOKE_ROOT/scripts/45-data-integrity-audit.sh" >/dev/null || die "data integrity audit failed after Wave 3"
data_integrity_violations_after_wave3="$(jq -r '.postgres_integrity_violations' "$REPORTS_DIR/full-power-data-integrity.json")"

if [[ "$register_rows_created" -eq 1 && "$register_conflict_rejected" == true && "$chunking_conflict_rejected" == true && "$duplicate_chunks" -eq 0 && "$duplicate_bindings" -eq 0 && "$duplicate_outbox_logical_events" -eq 0 && "$active_versions" -eq 1 && "$concurrent_search_transport_errors" -eq 0 && "$cross_zone_leakage_count" -eq 0 && "$empty_parent_context_count" -eq 0 && "$qdrant_idempotent_upsert_pass" == true && "$data_integrity_violations_after_wave3" -eq 0 ]]; then
  write_reports "CONSISTENCY_PARTIAL"
  exit "$PASS"
fi
write_reports "CONSISTENCY_FAIL"
exit "$FAIL_STATUS"
