#!/usr/bin/env bash
set -Eeuo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
MODE=${1:---verify-only}
RUN_ID=${FIX486C_RUN_ID:-fix486c-$(date -u +%Y%m%dT%H%M%SZ)}
EVIDENCE_ROOT=${ASTRAVECTOR_EVIDENCE_ROOT:-/Users/ruslanalimbetov/Documents/llm2/astravector-evidence}
EVIDENCE="$EVIDENCE_ROOT/fix486c/$RUN_ID"
ENDPOINT=${ASTRAVECTOR_FIX486C_ENDPOINT:-}
VERIFIER="$ROOT/scripts/fix486c_verify_frozen_bank.py"
MANIFEST="$ROOT/benchmarks/hierarchical/fix486/bank-manifest.json"

mkdir -p "$EVIDENCE"/{source,bank,ingestion,identity-map,query-dry-run,execution,logs,telemetry}

timestamp() { date -u +%Y-%m-%dT%H:%M:%SZ; }
failure() { printf 'FIX486C_FAIL=%s\n' "$1" >&2; }

telemetry() {
  local status=$1 stage=$2 processed=${3:-0} total=${4:-0} error_code=${5:-}
  jq -n --arg run_id "$RUN_ID" --arg status "$status" --arg stage "$stage" --arg error_code "$error_code" \
    --arg updated_at_utc "$(timestamp)" --argjson processed_documents "$processed" --argjson total_documents "$total" \
    '{schema_version:1,run_id:$run_id,status:$status,current_stage:$stage,processed_documents:$processed_documents,total_documents:$total_documents,last_error_code:(if $error_code=="" then null else $error_code end),updated_at_utc:$updated_at_utc}' \
    >"$EVIDENCE/telemetry/ingestion-status.json"
}

record() {
  local stage=$1
  shift
  local started finished rc status code
  started=$(timestamp)
  set +e
  "$@" >"$EVIDENCE/logs/$stage.log" 2>&1
  rc=$?
  set -e
  finished=$(timestamp)
  status=PASS
  code=null
  if [[ "$rc" -ne 0 ]]; then status=FAIL; code=COMMAND_FAILED; fi
  jq -n --arg stage_id "$stage" --arg status "$status" --arg started "$started" --arg finished "$finished" --arg failure_code "$code" --arg command "$(printf '%q ' "$@")" --argjson exit_code "$rc" \
    '{stage_id:$stage_id,status:$status,started_at_utc:$started,finished_at_utc:$finished,exit_code:$exit_code,failure_code:(if $failure_code=="null" then null else $failure_code end),command:$command,evidence:[]}' >"$EVIDENCE/logs/$stage.json"
  cat "$EVIDENCE/logs/$stage.log"
  return "$rc"
}

record_blocked() {
  local stage=$1 code=$2
  jq -n --arg stage_id "$stage" --arg code "$code" --arg now "$(timestamp)" \
    '{stage_id:$stage_id,status:"BLOCKED",started_at_utc:$now,finished_at_utc:$now,exit_code:1,failure_code:$code,evidence:[]}' >"$EVIDENCE/logs/$stage.json"
}

finalize() {
  local verdict=$1
  telemetry "FINALIZED" "finalize" 0 0 "$verdict"
  jq -s --arg run_id "$RUN_ID" --arg verdict "$verdict" --arg source_sha "$(git -C "$ROOT" rev-parse HEAD)" \
    --arg bank_aggregate "$(jq -r '.hashes.aggregate_sha256' "$MANIFEST")" \
    '{schema_version:1,run_id:$run_id,verdict:$verdict,source_sha:$source_sha,bank_aggregate_sha256:$bank_aggregate,stages:.}' \
    "$EVIDENCE"/logs/*.json >"$EVIDENCE/stage-results.json"
  find "$EVIDENCE" -type f ! -name manifest.json -print0 | sort -z | xargs -0 shasum -a 256 | jq -Rsc 'split("\n")|map(select(length>0)|capture("^(?<sha256>[0-9a-f]+)  (?<path>.*)$"))' >"$EVIDENCE/manifest.json"
  printf '%s\n' "$verdict"
}

verify() {
  record bank-verify python3 "$VERIFIER" --root "$ROOT/benchmarks/hierarchical/fix486" || return 1
  python3 "$VERIFIER" --root "$ROOT/benchmarks/hierarchical/fix486" >"$EVIDENCE/bank/verification.json"
}

dry_run() {
  record query-dry-run python3 "$VERIFIER" --root "$ROOT/benchmarks/hierarchical/fix486" --dry-run --output "$EVIDENCE/query-dry-run/plans.json" || return 1
  jq -e '.status=="PASS" and .scheduled_queries==11 and ([.plans[].status]|all(.=="PASS"))' "$EVIDENCE/query-dry-run/plans.json" >/dev/null
}

require_endpoint() {
  if [[ -z "$ENDPOINT" ]]; then
    record_blocked runtime-preparation ENDPOINT_NOT_CONFIGURED
    return 1
  fi
  if ! grpcurl -plaintext "$ENDPOINT" list >"$EVIDENCE/runtime-services.txt" 2>&1; then
    record_blocked runtime-preparation ENDPOINT_UNAVAILABLE
    return 1
  fi
  jq -n --arg endpoint "$ENDPOINT" --arg source_sha "$(git -C "$ROOT" rev-parse HEAD)" \
    --arg bank_aggregate "$(jq -r '.hashes.aggregate_sha256' "$MANIFEST")" \
    '{endpoint:$endpoint,source_sha:$source_sha,bank_aggregate_sha256:$bank_aggregate}' >"$EVIDENCE/source/runtime-identity.json"
}

ingest() {
  require_endpoint || return 1
  python3 "$VERIFIER" --root "$ROOT/benchmarks/hierarchical/fix486" --emit-ingestion-plans --output "$EVIDENCE/ingestion/plans.json"
  local total processed
  total=$(jq '.ingestion_plans|length' "$EVIDENCE/ingestion/plans.json")
  processed=0
  telemetry "RUNNING" "production-ingestion" "$processed" "$total"
  while IFS= read -r plan; do
    logical_zone=$(jq -r '.logical_zone_id' <<<"$plan")
    logical_document=$(jq -r '.logical_document_id' <<<"$plan")
    request="$EVIDENCE/ingestion/${logical_zone}-${logical_document}.request.json"
    response="$EVIDENCE/ingestion/${logical_zone}-${logical_document}.response.json"
    jq '.request' <<<"$plan" >"$request"
    if ! grpcurl -plaintext -d @ "$ENDPOINT" astravector.embedding.v1.AstraVectorIngestionFacade/IndexLogicalDocument <"$request" >"$response"; then
      telemetry "BLOCKED" "production-ingestion" "$processed" "$total" "INGESTION_FAILED"
      return 1
    fi
    zone=$(jq -r '.document.accessZoneId' "$response")
    document=$(jq -r '.document.documentId' "$response")
    [[ "$zone" != "null" && "$document" != "null" ]] || { telemetry "BLOCKED" "production-ingestion" "$processed" "$total" "INGESTION_RESPONSE_ID_MISSING"; failure INGESTION_RESPONSE_ID_MISSING; return 1; }
    wait_for_activation "$zone" "$document" "$EVIDENCE/ingestion/${logical_zone}-${logical_document}.activate.json" || { telemetry "BLOCKED" "activation" "$processed" "$total" "OUTBOX_NOT_FINALIZED"; failure OUTBOX_NOT_FINALIZED; return 1; }
    jq -n --arg logical_zone "$logical_zone" --arg logical_document "$logical_document" --slurpfile response "$response" '{logical_zone_id:$logical_zone,logical_document_id:$logical_document,response:$response[0]}' >"$EVIDENCE/identity-map/${logical_zone}-${logical_document}.json"
    processed=$((processed + 1))
    telemetry "RUNNING" "production-ingestion" "$processed" "$total"
  done < <(jq -c '.ingestion_plans[]' "$EVIDENCE/ingestion/plans.json")
  jq -s --arg source_sha "$(git -C "$ROOT" rev-parse HEAD)" --arg aggregate "$(jq -r '.hashes.aggregate_sha256' "$MANIFEST")" \
    '{schema_version:1,bank_id:"fix486-hierarchical-bank",bank_version:"1.0.0",bank_aggregate_sha256:$aggregate,source_sha:$source_sha,documents:.,access_zones:(map({key:.logical_zone_id,value:.response.document.accessZoneId})|from_entries)}' \
    "$EVIDENCE"/identity-map/*.json >"$EVIDENCE/identity-map/logical-to-runtime.json"
}

wait_for_activation() {
  local zone=$1 document=$2 output=$3
  local request
  request="{\"accessZoneId\":\"$zone\",\"documentId\":\"$document\",\"documentVersion\":1}"
  for _ in $(seq 1 90); do
    if grpcurl -plaintext -d "$request" "$ENDPOINT" astravector.embedding.v1.AstraVectorV004Control/ActivateDocumentVersion >"$output" 2>&1; then
      return 0
    fi
    sleep 1
  done
  return 1
}

execute_all() {
  ingest || return 1
  jq -c '.plans[]' "$EVIDENCE/query-dry-run/plans.json" | while IFS= read -r plan; do
    query_id=$(jq -r '.query_id' <<<"$plan")
    logical_zone=$(jq -r '.logical_access_zone' <<<"$plan")
    zone=$(jq -r --arg zone "$logical_zone" '.access_zones[$zone]' "$EVIDENCE/identity-map/logical-to-runtime.json")
    jq -n --arg zone "$zone" --arg question "$(jq -r '.question' <<<"$plan")" --arg correlation "fix486c-$query_id" \
      '{correlationId:$correlation,accessZoneId:$zone,callerAccessLevel:"INTERNAL",query:$question,topK:5,candidateLimit:20,parentLimit:5,timeoutMs:5000,includeDebug:true}' >"$EVIDENCE/execution/$query_id.request.json"
    if grpcurl -plaintext -d @ "$ENDPOINT" astravector.embedding.v1.AstraVectorV004Control/Search <"$EVIDENCE/execution/$query_id.request.json" >"$EVIDENCE/execution/$query_id.response.json"; then
      jq -n --argjson plan "$plan" --slurpfile response "$EVIDENCE/execution/$query_id.response.json" \
        '{query_id:$plan.query_id,case_id:$plan.case_id,status:"PASS",runtime_status:"OK",matched_contexts:($response[0].results // []),warnings:($response[0].warnings // []),hard_gate_results:{},bank_aggregate_sha256:$plan.bank_aggregate_sha256}' >"$EVIDENCE/execution/$query_id.result.json"
    else
      jq -n --argjson plan "$plan" '{query_id:$plan.query_id,case_id:$plan.case_id,status:"FAIL",runtime_status:"GRPC_ERROR",matched_contexts:[],warnings:[],hard_gate_results:{},bank_aggregate_sha256:$plan.bank_aggregate_sha256}' >"$EVIDENCE/execution/$query_id.result.json"
      return 1
    fi
  done
}

cd "$ROOT"
telemetry "RUNNING" "source-verification" 0 0
git status --porcelain >"$EVIDENCE/source/worktree-status.txt"
if [[ -s "$EVIDENCE/source/worktree-status.txt" ]]; then failure DIRTY_WORKTREE; finalize FIX486_FROZEN_EXECUTABLE_BANK_BLOCKED; exit 1; fi

case "$MODE" in
  --verify-only) if verify; then finalize FIX486_FROZEN_EXECUTABLE_BANK_PASS; else finalize FIX486_FROZEN_EXECUTABLE_BANK_BLOCKED; exit 1; fi ;;
  --dry-run) if verify && dry_run; then finalize FIX486_FROZEN_EXECUTABLE_BANK_PASS; else finalize FIX486_FROZEN_EXECUTABLE_BANK_BLOCKED; exit 1; fi ;;
  --prepare-runtime) if verify && dry_run && require_endpoint; then finalize FIX486_FROZEN_EXECUTABLE_BANK_PASS; else finalize FIX486_FROZEN_EXECUTABLE_BANK_BLOCKED; exit 1; fi ;;
  --ingest-only) if verify && dry_run && ingest; then finalize FIX486_FROZEN_EXECUTABLE_BANK_PASS; else finalize FIX486_FROZEN_EXECUTABLE_BANK_BLOCKED; exit 1; fi ;;
  --execute-all) if verify && dry_run && execute_all; then finalize FIX486_FROZEN_EXECUTABLE_BANK_PASS; else finalize FIX486_FROZEN_EXECUTABLE_BANK_BLOCKED; exit 1; fi ;;
  *) echo "usage: $0 [--verify-only|--dry-run|--prepare-runtime|--ingest-only|--execute-all]" >&2; exit 64 ;;
esac
