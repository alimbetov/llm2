#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/grpc.sh"
load_smoke_env

die() {
  fail "$1"
  if type write_reports >/dev/null 2>&1; then
    write_reports "ACCESS_SECURITY_FAIL"
  fi
  exit "$FAIL_STATUS"
}
command -v grpcurl >/dev/null 2>&1 || blocked "grpcurl not found"
command -v psql >/dev/null 2>&1 || blocked "psql not found"
runtime_pid=""

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
    "$PROJECT_DIR/target/debug/astravector-runtime" >"$LOGS_DIR/access-security-runtime.log" 2>&1
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

ensure_runtime || die "runtime gRPC service did not become ready"
grpc_assert_service "astravector.embedding.v1.AstraVectorV004Control" || exit "$BLOCKED_STATUS"

ZONE_A="${SMOKE_ACCESS_ZONE_A:-11111111-1111-4111-8111-111111111111}"
ZONE_B="${SMOKE_ACCESS_ZONE_B:-22222222-2222-4222-8222-222222222222}"
CIVIL_DOC="${CIVIL_CODE_DOCUMENT_ID:-72fd8953-9f11-5eef-a03c-ef47c3d40daa}"
ZONE_B_SECRET="ZONE_B_SECRET_PHRASE_AST_VECTOR_004"
EVIDENCE="$REPORTS_DIR/access-security-evidence.jsonl"
METRICS="$REPORTS_DIR/access-security-metrics.json"
RESULTS="$REPORTS_DIR/access-security-results.json"
REPORT="$REPORTS_DIR/ACCESS_SECURITY_REPORT.md"
ACCESS_LOG_DIR="$LOGS_DIR/access-security"
mkdir -p "$ACCESS_LOG_DIR"
: > "$EVIDENCE"

cross_zone_leakage_count=0
foreign_parent_text_returned=0
foreign_metadata_returned=0
access_level_violation_count=0
permission_denied_count=0
not_found_count=0
unexpected_ok_count=0
transport_error_count=0
foreign_parent_resolution_attempts=0
foreign_chunk_group_attempts=0
zone_a_search_results=0
zone_b_search_for_civil_code_results=0
zone_a_search_for_zone_b_secret_results=0
zone_b_search_for_zone_b_secret_results=0
zone_a_qdrant_points=0
zone_b_qdrant_points=0

json_event() {
  local type="$1" status="$2" details="$3"
  jq -nc --arg type "$type" --arg status "$status" --argjson details "$details" \
    '{type:$type,status:$status,details:$details}' >> "$EVIDENCE"
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

qdrant_count_zone() {
  local zone="$1"
  curl -sS -X POST -H 'content-type: application/json' \
    --data "$(jq -n --arg zone "$zone" '{exact:true,filter:{must:[{key:"access_zone_id",match:{value:$zone}}]}}')" \
    "${QDRANT_HTTP_URL}/collections/${QDRANT_COLLECTION}/points/count" | jq -r '.result.count // 0'
}

qdrant_count_doc() {
  local zone="$1" doc="$2"
  curl -sS -X POST -H 'content-type: application/json' \
    --data "$(jq -n --arg zone "$zone" --arg doc "$doc" '{exact:true,filter:{must:[{key:"access_zone_id",match:{value:$zone}},{key:"document_id",match:{value:$doc}},{key:"chunk_granularity",match:{any:["PARENT","SUB_180","SUB_260"]}}]}}')" \
    "${QDRANT_HTTP_URL}/collections/${QDRANT_COLLECTION}/points/count" | jq -r '.result.count // 0'
}

clear_doc() {
  local zone="$1" doc="$2"
  psql "$(postgres_url)" -v ON_ERROR_STOP=1 \
    -c "DELETE FROM astravector.vector_outbox o USING astravector.vector_bindings_v004 b WHERE o.binding_access_zone_id=b.access_zone_id AND o.binding_id=b.id AND b.access_zone_id='${zone}'::uuid AND b.document_id='${doc}'::uuid" \
    -c "DELETE FROM astravector.vector_bindings_v004 WHERE access_zone_id='${zone}'::uuid AND document_id='${doc}'::uuid" \
    -c "DELETE FROM astravector.content_chunks_v004 WHERE access_zone_id='${zone}'::uuid AND document_id='${doc}'::uuid" \
    -c "DELETE FROM astravector.document_versions WHERE access_zone_id='${zone}'::uuid AND document_id='${doc}'::uuid" >/dev/null || die "failed to clear document $doc"
  curl -sS -X POST -H 'content-type: application/json' \
    --data "$(jq -n --arg zone "$zone" --arg doc "$doc" '{filter:{must:[{key:"access_zone_id",match:{value:$zone}},{key:"document_id",match:{value:$doc}}]}}')" \
    "${QDRANT_HTTP_URL}/collections/${QDRANT_COLLECTION}/points/delete?wait=true" >/dev/null || die "failed to clear qdrant document $doc"
}

create_document() {
  local zone="$1" doc="$2" text="$3" level="$4" label="$5"
  local hash register_body chunk_body activate_body
  hash="$(sha256_text "$text")"
  clear_doc "$zone" "$doc"
  register_body="$(jq -n --arg zone "$zone" --arg doc "$doc" --arg hash "$hash" '{accessZoneId:$zone,documentId:$doc,documentVersion:1,contentHash:$hash,activationPolicy:"ACTIVE_LATEST_ONLY"}')"
  grpc_plain -d "$register_body" astravector.embedding.v1.AstraVectorV004Control/RegisterDocumentVersion >"$ACCESS_LOG_DIR/${label}-register.json" 2>"$ACCESS_LOG_DIR/${label}-register.err" || die "RegisterDocumentVersion failed for $label"
  chunk_body="$(jq -n --arg zone "$zone" --arg doc "$doc" --arg text "$text" --arg level "$level" --arg label "$label" '{
    accessZoneId:$zone,
    documentId:$doc,
    documentVersion:1,
    sourceText:$text,
    accessLevel:$level,
    profile:{
      preserveHeadings:true,
      preserveParagraphs:true,
      preserveSentences:true,
      profileVersion:"access-security-v1",
      parent:{granularity:"PARENT_V004",targetTokens:60,minTokens:1,maxTokens:120,overlapTokens:0},
      granularities:[
        {granularity:"SUB_180_V004",targetTokens:30,minTokens:1,maxTokens:80,overlapTokens:0},
        {granularity:"SUB_260_V004",targetTokens:45,minTokens:1,maxTokens:100,overlapTokens:0}
      ]
    },
    metadata:{smoke:"access-security", label:$label},
    idempotencyKey:("access-security-" + $label),
    correlationId:"access-security-smoke"
  }')"
  grpc_plain -d "$chunk_body" astravector.embedding.v1.AstraVectorV004Control/CreateMultiGranularityChunks >"$ACCESS_LOG_DIR/${label}-chunks.json" 2>"$ACCESS_LOG_DIR/${label}-chunks.err" || die "CreateMultiGranularityChunks failed for $label"
  wait_document_indexed "$zone" "$doc" "$label"
  activate_body="$(jq -n --arg zone "$zone" --arg doc "$doc" '{accessZoneId:$zone,documentId:$doc,documentVersion:1}')"
  grpc_plain -d "$activate_body" astravector.embedding.v1.AstraVectorV004Control/ActivateDocumentVersion >"$ACCESS_LOG_DIR/${label}-activate.json" 2>"$ACCESS_LOG_DIR/${label}-activate.err" || die "ActivateDocumentVersion failed for $label"
  wait_document_active "$zone" "$doc" "$label"
}

wait_document_indexed() {
  local zone="$1" doc="$2" label="$3" deadline
  deadline=$((SECONDS + 120))
  while (( SECONDS < deadline )); do
    local searchable synced completed qdrant
    searchable="$(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id='${zone}'::uuid AND document_id='${doc}'::uuid AND chunk_granularity IN('PARENT','SUB_180','SUB_260')")" || searchable=0
    synced="$(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id='${zone}'::uuid AND document_id='${doc}'::uuid AND chunk_granularity IN('PARENT','SUB_180','SUB_260') AND qdrant_sync_status='SYNCED'")" || synced=0
    completed="$(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM astravector.vector_outbox o JOIN astravector.vector_bindings_v004 b ON b.access_zone_id=o.binding_access_zone_id AND b.id=o.binding_id WHERE b.access_zone_id='${zone}'::uuid AND b.document_id='${doc}'::uuid AND o.operation='UPSERT_POINT' AND o.status='COMPLETED'")" || completed=0
    qdrant="$(qdrant_count_doc "$zone" "$doc")" || qdrant=0
    if [[ "$searchable" -gt 0 && "$synced" -eq "$searchable" && "$completed" -eq "$searchable" && "$qdrant" -eq "$searchable" ]]; then
      json_event "document_indexed" "PASS" "$(jq -n --arg label "$label" --arg doc "$doc" --argjson bindings "$searchable" '{label:$label,document_id:$doc,searchable_bindings:$bindings}')"
      return 0
    fi
    sleep 2
  done
  die "timeout waiting indexed document $label"
}

wait_document_active() {
  local zone="$1" doc="$2" label="$3" deadline status
  deadline=$((SECONDS + 120))
  while (( SECONDS < deadline )); do
    status="$(psql "$(postgres_url)" -Atqc "SELECT status FROM astravector.document_versions WHERE access_zone_id='${zone}'::uuid AND document_id='${doc}'::uuid AND document_version=1")" || status=""
    [[ "$status" == "ACTIVE" ]] && return 0
    sleep 2
  done
  die "timeout waiting ACTIVE document $label"
}

grpc_status_from_err() {
  local err="$1"
  if grep -q "Code: NotFound" "$err"; then
    not_found_count=$((not_found_count + 1))
    printf "NOT_FOUND"
  elif grep -q "Code: PermissionDenied" "$err"; then
    permission_denied_count=$((permission_denied_count + 1))
    printf "PERMISSION_DENIED"
  else
    transport_error_count=$((transport_error_count + 1))
    printf "TRANSPORT_ERROR"
  fi
}

search_call() {
  local label="$1" zone="$2" level="$3" query="$4" out
  out="$ACCESS_LOG_DIR/${label}.json"
  local body
  body="$(jq -n --arg zone "$zone" --arg level "$level" --arg query "$query" '{correlationId:"access-security",accessZoneId:$zone,callerAccessLevel:$level,query:$query,topK:10,candidateLimit:50,parentLimit:10,timeoutMs:10000}')"
  grpc_plain -d "$body" astravector.embedding.v1.AstraVectorV004Control/Search >"$out" 2>"$ACCESS_LOG_DIR/${label}.err" || die "Search failed for $label"
  printf "%s" "$out"
}

assert_no_zone_b_leak() {
  local file="$1" label="$2"
  local leaks
  leaks="$(jq --arg doc "$zone_b_doc" --arg secret "$ZONE_B_SECRET" '[.results[]? | select(.documentId==$doc or (.parentText|contains($secret)) or .accessZoneId=="'$ZONE_B'")] | length' "$file")"
  if [[ "$leaks" -ne 0 ]]; then
    cross_zone_leakage_count=$((cross_zone_leakage_count + leaks))
    json_event "$label" "FAIL" "$(jq -n --argjson leaks "$leaks" '{zone_b_leaks:$leaks}')"
  else
    json_event "$label" "PASS" "$(jq -n '{zone_b_leaks:0}')"
  fi
}

assert_no_civil_leak() {
  local file="$1" label="$2"
  local leaks
  leaks="$(jq --arg doc "$CIVIL_DOC" '[.results[]? | select(.documentId==$doc or .accessZoneId=="'$ZONE_A'")] | length' "$file")"
  if [[ "$leaks" -ne 0 ]]; then
    cross_zone_leakage_count=$((cross_zone_leakage_count + leaks))
    json_event "$label" "FAIL" "$(jq -n --argjson leaks "$leaks" '{zone_a_leaks:$leaks}')"
  else
    json_event "$label" "PASS" "$(jq -n '{zone_a_leaks:0}')"
  fi
}

write_reports() {
  local verdict="$1"
  jq -n \
    --arg verdict "$verdict" \
    --argjson zone_a_qdrant_points "${zone_a_qdrant_points:-0}" \
    --argjson zone_b_qdrant_points "${zone_b_qdrant_points:-0}" \
    --argjson zone_a_search_results "${zone_a_search_results:-0}" \
    --argjson zone_b_search_for_civil_code_results "${zone_b_search_for_civil_code_results:-0}" \
    --argjson zone_a_search_for_zone_b_secret_results "${zone_a_search_for_zone_b_secret_results:-0}" \
    --argjson zone_b_search_for_zone_b_secret_results "${zone_b_search_for_zone_b_secret_results:-0}" \
    --argjson foreign_parent_resolution_attempts "${foreign_parent_resolution_attempts:-0}" \
    --argjson foreign_chunk_group_attempts "${foreign_chunk_group_attempts:-0}" \
    --argjson cross_zone_leakage_count "${cross_zone_leakage_count:-0}" \
    --argjson foreign_parent_text_returned "${foreign_parent_text_returned:-0}" \
    --argjson foreign_metadata_returned "${foreign_metadata_returned:-0}" \
    --argjson access_level_violation_count "${access_level_violation_count:-0}" \
    --argjson permission_denied_count "${permission_denied_count:-0}" \
    --argjson not_found_count "${not_found_count:-0}" \
    --argjson unexpected_ok_count "${unexpected_ok_count:-0}" \
    --argjson transport_error_count "${transport_error_count:-0}" \
    '{verdict:$verdict,zone_a_qdrant_points:$zone_a_qdrant_points,zone_b_qdrant_points:$zone_b_qdrant_points,zone_a_search_results:$zone_a_search_results,zone_b_search_for_civil_code_results:$zone_b_search_for_civil_code_results,zone_a_search_for_zone_b_secret_results:$zone_a_search_for_zone_b_secret_results,zone_b_search_for_zone_b_secret_results:$zone_b_search_for_zone_b_secret_results,foreign_parent_resolution_attempts:$foreign_parent_resolution_attempts,foreign_chunk_group_attempts:$foreign_chunk_group_attempts,cross_zone_leakage_count:$cross_zone_leakage_count,foreign_parent_text_returned:$foreign_parent_text_returned,foreign_metadata_returned:$foreign_metadata_returned,access_level_violation_count:$access_level_violation_count,permission_denied_count:$permission_denied_count,not_found_count:$not_found_count,unexpected_ok_count:$unexpected_ok_count,transport_error_count:$transport_error_count}' > "$METRICS"
  jq -n --slurpfile metrics "$METRICS" --rawfile evidence "$EVIDENCE" '{verdict:$metrics[0].verdict,metrics:$metrics[0],evidence_jsonl:$evidence}' > "$RESULTS"
  {
    echo "# AstraVector_v004 Access Security Report"
    echo
    echo "## 1. Verdict"
    echo "$verdict"
    echo
    echo "## 2. Environment"
    echo "- Date: $(now_iso)"
    echo "- ZONE_A: $ZONE_A"
    echo "- ZONE_B: $ZONE_B"
    echo "- Civil Code document_id: $CIVIL_DOC"
    echo "- Zone B document_id: ${zone_b_doc:-unknown}"
    echo "- Qdrant collection: $QDRANT_COLLECTION"
    echo "- gRPC endpoint: $(grpc_addr)"
    echo
    echo "## 3. Summary"
    echo "| Check | Status | Evidence |"
    echo "|---|---|---|"
    echo "| Search isolation | $([[ ${cross_zone_leakage_count:-0} -eq 0 ]] && echo PASS || echo FAIL) | $EVIDENCE |"
    echo "| Foreign ResolveParentContext | $([[ ${foreign_parent_text_returned:-0} -eq 0 && ${foreign_metadata_returned:-0} -eq 0 && ${transport_error_count:-0} -eq 0 ]] && echo PASS || echo FAIL) | $ACCESS_LOG_DIR |"
    echo "| Foreign GetChunkGroup | $([[ ${unexpected_ok_count:-0} -eq 0 && ${transport_error_count:-0} -eq 0 ]] && echo PASS || echo FAIL) | $ACCESS_LOG_DIR |"
    echo "| Access level matrix | $([[ ${access_level_violation_count:-0} -eq 0 ]] && echo PASS || echo FAIL) | $ACCESS_LOG_DIR |"
    echo
    echo "## 4. Search Isolation"
    echo "| Query | Request Zone | Expected | Actual | Status |"
    echo "|---|---|---|---|---|"
    echo "| Civil Code | ZONE_A | Civil Code only | results=${zone_a_search_results:-0} | PASS |"
    echo "| Civil Code | ZONE_B | no Civil Code | results=${zone_b_search_for_civil_code_results:-0} | $([[ ${cross_zone_leakage_count:-0} -eq 0 ]] && echo PASS || echo FAIL) |"
    echo "| Zone B secret | ZONE_A | no Zone B secret | results=${zone_a_search_for_zone_b_secret_results:-0} | $([[ ${cross_zone_leakage_count:-0} -eq 0 ]] && echo PASS || echo FAIL) |"
    echo "| Zone B secret | ZONE_B | Zone B secret | results=${zone_b_search_for_zone_b_secret_results:-0} | PASS |"
    echo
    echo "## 5. Foreign ResolveParentContext"
    echo "| Attack | Expected | Actual gRPC status | Leaked text | Leaked metadata | Status |"
    echo "|---|---|---|---|---|---|"
    echo "| ZONE_B resolves ZONE_A parent | NOT_FOUND/PERMISSION_DENIED | ${foreign_resolve_status:-unknown} | ${foreign_parent_text_returned:-0} | ${foreign_metadata_returned:-0} | $([[ ${foreign_parent_text_returned:-0} -eq 0 && ${foreign_metadata_returned:-0} -eq 0 ]] && echo PASS || echo FAIL) |"
    echo
    echo "## 6. Foreign GetChunkGroup"
    echo "| Attack | Expected | Actual gRPC status | Returned chunks | Leaked metadata | Status |"
    echo "|---|---|---|---:|---|---|"
    echo "| ZONE_B gets ZONE_A root | NOT_FOUND/PERMISSION_DENIED | ${foreign_group_status:-unknown} | ${foreign_group_chunks:-0} | ${foreign_group_metadata:-0} | $([[ ${foreign_group_chunks:-0} -eq 0 && ${foreign_group_metadata:-0} -eq 0 ]] && echo PASS || echo FAIL) |"
    echo
    echo "## 7. Access Level Matrix"
    echo "| Caller Level | Expected Visible Levels | Forbidden Secret Found | Status |"
    echo "|---:|---|---|---|"
    for level in 1 2 3 4; do
      local key="matrix_forbidden_${level}"
      local value="${!key:-0}"
      echo "| $level | <= $level | $value | $([[ "$value" -eq 0 ]] && echo PASS || echo FAIL) |"
    done
    echo
    echo "## 8. Qdrant Evidence"
    echo "| Metric | Value |"
    echo "|---|---:|"
    echo "| zone_a_qdrant_points | ${zone_a_qdrant_points:-0} |"
    echo "| zone_b_qdrant_points | ${zone_b_qdrant_points:-0} |"
    echo
    echo "## 9. PostgreSQL Double Check Evidence"
    echo "| Check | Value |"
    echo "|---|---:|"
    echo "| zone_a_parent_in_zone_a | ${zone_a_parent_count:-0} |"
    echo "| zone_a_parent_in_zone_b | ${zone_a_parent_in_zone_b_count:-0} |"
    echo
    echo "## 10. Metrics"
    echo '```json'
    jq . "$METRICS"
    echo '```'
    echo
    echo "## 11. Remaining Risks"
    echo "- timing side-channel not measured deeply"
    echo "- auth/mTLS not covered unless implemented"
    echo "- admin endpoint security not covered unless implemented"
  } > "$REPORT"
}

update_full_power_report() {
  local system_verdict="RAG_CORE_E2E_CANDIDATE"
  if [[ -f "$METRICS" ]] && jq -e '.verdict=="ACCESS_SECURITY_PASS" and .cross_zone_leakage_count==0 and .foreign_parent_text_returned==0 and .foreign_metadata_returned==0 and .access_level_violation_count==0' "$METRICS" >/dev/null; then
    system_verdict="SECURE_RAG_CORE_CANDIDATE"
  fi
  local metrics_tmp results_tmp
  metrics_tmp="$REPORTS_DIR/full-power-smoke-metrics.tmp.json"
  results_tmp="$REPORTS_DIR/full-power-smoke-results.tmp.json"
  jq -n \
    --arg verdict "$system_verdict" \
    --slurpfile access "$METRICS" \
    --slurpfile wave1 "$REPORTS_DIR/full-power-smoke-metrics.json" \
    '{verdict:$verdict,wave1:($wave1[0] // null),access_security:$access[0]}' \
    > "$metrics_tmp"
  mv "$metrics_tmp" "$REPORTS_DIR/full-power-smoke-metrics.json"
  jq -n \
    --arg verdict "$system_verdict" \
    --slurpfile access "$RESULTS" \
    --slurpfile metrics "$REPORTS_DIR/full-power-smoke-metrics.json" \
    '{verdict:$verdict,access_security:$access[0],metrics:$metrics[0]}' \
    > "$results_tmp"
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
    echo "| Wave 2 Access Security | $(jq -r '.verdict' "$METRICS") | $REPORT |"
    echo
    echo "## 3. Access Security Metrics"
    echo '```json'
    jq . "$METRICS"
    echo '```'
    echo
    echo "## 4. Remaining Blockers"
    echo "- TTL/legal-hold/delete not yet full-power tested"
    echo "- reconciliation/rebuild not yet full-power tested"
    echo "- failpoints/atomicity not yet full-power tested"
    echo "- outbox fencing not yet full-power tested"
    echo "- overload/backpressure not yet full-power tested"
    echo "- observability not yet full-power tested"
    echo "- sparse/hybrid legal quality not yet proven, if sparse disabled"
  } > "$REPORTS_DIR/FULL_POWER_SMOKE_REPORT.md"
}

civil_active="$(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM astravector.document_versions WHERE access_zone_id='${ZONE_A}'::uuid AND document_id='${CIVIL_DOC}'::uuid AND status='ACTIVE'")" || civil_active=0
[[ "$civil_active" -eq 1 ]] || die "Civil Code document is not ACTIVE in ZONE_A; run full-power-wave1/corpus first"

zone_b_doc="$(uuid_for "access-security-zone-b-document-v1")"
zone_b_text="Zone B internal access security document.
This document must never appear in Zone A search results.
${ZONE_B_SECRET}"
create_document "$ZONE_B" "$zone_b_doc" "$zone_b_text" "PUBLIC" "zone-b-secret"

for level in 1 2 3 4; do
  case "$level" in
    1) enum="PUBLIC" ;;
    2) enum="INTERNAL" ;;
    3) enum="CONFIDENTIAL" ;;
    4) enum="RESTRICTED" ;;
  esac
  doc="$(uuid_for "access-security-zone-a-level-${level}-document-v1")"
  text="Access level ${level} security fixture.
ACCESS_LEVEL_${level}_SECRET_AST_VECTOR_004
This document validates caller access level ${level}."
  create_document "$ZONE_A" "$doc" "$text" "$enum" "zone-a-level-${level}"
  parent="$(psql "$(postgres_url)" -Atqc "SELECT id FROM astravector.content_chunks_v004 WHERE access_zone_id='${ZONE_A}'::uuid AND document_id='${doc}'::uuid AND granularity='PARENT' ORDER BY sequence_no LIMIT 1")"
  root="$(psql "$(postgres_url)" -Atqc "SELECT root_chunk_id FROM astravector.content_chunks_v004 WHERE access_zone_id='${ZONE_A}'::uuid AND document_id='${doc}'::uuid LIMIT 1")"
  eval "level_${level}_doc='$doc'"
  eval "level_${level}_parent='$parent'"
  eval "level_${level}_root='$root'"
done

zone_a_qdrant_points="$(qdrant_count_zone "$ZONE_A")" || die "Qdrant zone A count failed"
zone_b_qdrant_points="$(qdrant_count_zone "$ZONE_B")" || die "Qdrant zone B count failed"
[[ "$zone_a_qdrant_points" -gt 0 ]] || die "zone A qdrant count is zero"
[[ "$zone_b_qdrant_points" -gt 0 ]] || die "zone B qdrant count is zero"

zone_a_civil_file="$(search_call "search-zone-a-civil" "$ZONE_A" "RESTRICTED" "Каков общий срок исковой давности?")"
zone_a_search_results="$(jq '.results|length' "$zone_a_civil_file")"
[[ "$zone_a_search_results" -gt 0 ]] || die "ZONE_A Civil Code search returned no results"
assert_no_zone_b_leak "$zone_a_civil_file" "zone_a_civil_search"

zone_b_civil_file="$(search_call "search-zone-b-civil" "$ZONE_B" "RESTRICTED" "Каков общий срок исковой давности?")"
zone_b_search_for_civil_code_results="$(jq '.results|length' "$zone_b_civil_file")"
assert_no_civil_leak "$zone_b_civil_file" "zone_b_civil_search"

zone_a_secret_file="$(search_call "search-zone-a-zone-b-secret" "$ZONE_A" "RESTRICTED" "$ZONE_B_SECRET")"
zone_a_search_for_zone_b_secret_results="$(jq '.results|length' "$zone_a_secret_file")"
assert_no_zone_b_leak "$zone_a_secret_file" "zone_a_zone_b_secret_search"

zone_b_secret_file="$(search_call "search-zone-b-zone-b-secret" "$ZONE_B" "RESTRICTED" "$ZONE_B_SECRET")"
zone_b_search_for_zone_b_secret_results="$(jq '.results|length' "$zone_b_secret_file")"
[[ "$zone_b_search_for_zone_b_secret_results" -gt 0 ]] || die "ZONE_B secret search returned no results"
jq -e --arg secret "$ZONE_B_SECRET" --arg zone "$ZONE_B" 'all(.results[]; .accessZoneId==$zone) and any(.results[]; .parentText|contains($secret))' "$zone_b_secret_file" >/dev/null || die "ZONE_B secret search did not return scoped secret evidence"

zone_a_parent_id="$(jq -r '.results[0].parentChunkId' "$zone_a_civil_file")"
zone_a_root_id="$(jq -r '.results[0].rootChunkId' "$zone_a_civil_file")"
zone_a_parent_count="$(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM astravector.content_chunks_v004 WHERE id='${zone_a_parent_id}'::uuid AND access_zone_id='${ZONE_A}'::uuid")" || zone_a_parent_count=0
zone_a_parent_in_zone_b_count="$(psql "$(postgres_url)" -Atqc "SELECT count(*) FROM astravector.content_chunks_v004 WHERE id='${zone_a_parent_id}'::uuid AND access_zone_id='${ZONE_B}'::uuid")" || zone_a_parent_in_zone_b_count=0
[[ "$zone_a_parent_count" -eq 1 && "$zone_a_parent_in_zone_b_count" -eq 0 ]] || die "PostgreSQL zone scoping assertion failed"

foreign_parent_resolution_attempts=1
resolve_body="$(jq -n --arg zone "$ZONE_B" --arg parent "$zone_a_parent_id" '{accessZoneId:$zone,chunkIds:[$parent],maxContextTokens:1000,callerAccessLevel:"RESTRICTED"}')"
if grpc_plain -d "$resolve_body" astravector.embedding.v1.AstraVectorV004Control/ResolveParentContext >"$ACCESS_LOG_DIR/foreign-resolve.json" 2>"$ACCESS_LOG_DIR/foreign-resolve.err"; then
  unexpected_ok_count=$((unexpected_ok_count + 1))
  foreign_resolve_status="OK"
  foreign_parent_text_returned="$(jq '[.contexts[]? | select((.content // "") != "")] | length' "$ACCESS_LOG_DIR/foreign-resolve.json")"
  foreign_metadata_returned="$(jq '[.contexts[]? | select(.chunk != null)] | length' "$ACCESS_LOG_DIR/foreign-resolve.json")"
else
  foreign_resolve_status="$(grpc_status_from_err "$ACCESS_LOG_DIR/foreign-resolve.err")"
fi

foreign_chunk_group_attempts=1
group_body="$(jq -n --arg zone "$ZONE_B" --arg root "$zone_a_root_id" '{accessZoneId:$zone,rootChunkId:$root,callerAccessLevel:"RESTRICTED"}')"
if grpc_plain -d "$group_body" astravector.embedding.v1.AstraVectorV004Control/GetChunkGroup >"$ACCESS_LOG_DIR/foreign-group.json" 2>"$ACCESS_LOG_DIR/foreign-group.err"; then
  unexpected_ok_count=$((unexpected_ok_count + 1))
  foreign_group_status="OK"
  foreign_group_chunks="$(jq '.chunks|length' "$ACCESS_LOG_DIR/foreign-group.json")"
  foreign_group_metadata="$(jq '[.chunks[]? | select(.chunk != null)] | length' "$ACCESS_LOG_DIR/foreign-group.json")"
else
  foreign_group_status="$(grpc_status_from_err "$ACCESS_LOG_DIR/foreign-group.err")"
  foreign_group_chunks=0
  foreign_group_metadata=0
fi

for caller in 1 2 3 4; do
  case "$caller" in
    1) enum="PUBLIC" ;;
    2) enum="INTERNAL" ;;
    3) enum="CONFIDENTIAL" ;;
    4) enum="RESTRICTED" ;;
  esac
  matrix_file="$(search_call "search-access-level-${caller}" "$ZONE_A" "$enum" "ACCESS_LEVEL")"
  forbidden=0
  for level in 1 2 3 4; do
    if [[ "$level" -gt "$caller" ]]; then
      found="$(jq --arg secret "ACCESS_LEVEL_${level}_SECRET_AST_VECTOR_004" '[.results[]? | select(.parentText|contains($secret))] | length' "$matrix_file")"
      forbidden=$((forbidden + found))
    fi
  done
  eval "matrix_forbidden_${caller}=$forbidden"
  if [[ "$forbidden" -ne 0 ]]; then
    access_level_violation_count=$((access_level_violation_count + forbidden))
  fi
done

level4_parent="$level_4_parent"
low_resolve_body="$(jq -n --arg zone "$ZONE_A" --arg parent "$level4_parent" '{accessZoneId:$zone,chunkIds:[$parent],maxContextTokens:1000,callerAccessLevel:"PUBLIC"}')"
if grpc_plain -d "$low_resolve_body" astravector.embedding.v1.AstraVectorV004Control/ResolveParentContext >"$ACCESS_LOG_DIR/level4-low-resolve.json" 2>"$ACCESS_LOG_DIR/level4-low-resolve.err"; then
  unexpected_ok_count=$((unexpected_ok_count + 1))
  leaked="$(jq --arg secret "ACCESS_LEVEL_4_SECRET_AST_VECTOR_004" '[.contexts[]? | select(.content|contains($secret))] | length' "$ACCESS_LOG_DIR/level4-low-resolve.json")"
  access_level_violation_count=$((access_level_violation_count + leaked))
else
  grpc_status_from_err "$ACCESS_LOG_DIR/level4-low-resolve.err" >/dev/null
fi

level4_root="$level_4_root"
low_group_body="$(jq -n --arg zone "$ZONE_A" --arg root "$level4_root" '{accessZoneId:$zone,rootChunkId:$root,callerAccessLevel:"PUBLIC"}')"
if grpc_plain -d "$low_group_body" astravector.embedding.v1.AstraVectorV004Control/GetChunkGroup >"$ACCESS_LOG_DIR/level4-low-group.json" 2>"$ACCESS_LOG_DIR/level4-low-group.err"; then
  unexpected_ok_count=$((unexpected_ok_count + 1))
  leaked="$(jq --arg secret "ACCESS_LEVEL_4_SECRET_AST_VECTOR_004" '[.chunks[]? | select(.content|contains($secret))] | length' "$ACCESS_LOG_DIR/level4-low-group.json")"
  access_level_violation_count=$((access_level_violation_count + leaked))
else
  grpc_status_from_err "$ACCESS_LOG_DIR/level4-low-group.err" >/dev/null
fi

curl -sS -X POST -H 'content-type: application/json' --data "$(jq -n --arg zone "$ZONE_A" '{limit:1,with_payload:true,with_vector:false,filter:{must:[{key:"access_zone_id",match:{value:$zone}}]}}')" "${QDRANT_HTTP_URL}/collections/${QDRANT_COLLECTION}/points/scroll" >"$ACCESS_LOG_DIR/qdrant-zone-a-sample.json" || die "Qdrant zone A sample failed"
curl -sS -X POST -H 'content-type: application/json' --data "$(jq -n --arg zone "$ZONE_B" '{limit:1,with_payload:true,with_vector:false,filter:{must:[{key:"access_zone_id",match:{value:$zone}}]}}')" "${QDRANT_HTTP_URL}/collections/${QDRANT_COLLECTION}/points/scroll" >"$ACCESS_LOG_DIR/qdrant-zone-b-sample.json" || die "Qdrant zone B sample failed"
jq -e '.result.points[0].payload | has("access_zone_id") and has("document_id") and has("access_level") and has("lifecycle_status")' "$ACCESS_LOG_DIR/qdrant-zone-a-sample.json" >/dev/null || die "Qdrant zone A payload sample missing fields"
jq -e '.result.points[0].payload | has("access_zone_id") and has("document_id") and has("access_level") and has("lifecycle_status")' "$ACCESS_LOG_DIR/qdrant-zone-b-sample.json" >/dev/null || die "Qdrant zone B payload sample missing fields"

if [[ "$cross_zone_leakage_count" -eq 0 && "$foreign_parent_text_returned" -eq 0 && "$foreign_metadata_returned" -eq 0 && "$access_level_violation_count" -eq 0 && "$unexpected_ok_count" -eq 0 && "$transport_error_count" -eq 0 ]]; then
  write_reports "ACCESS_SECURITY_PASS"
  update_full_power_report
  exit "$PASS"
fi
write_reports "ACCESS_SECURITY_FAIL"
update_full_power_report
exit "$FAIL_STATUS"
