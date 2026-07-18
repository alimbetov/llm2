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

mkdir -p "$EVIDENCE"/{source,bank,ingestion,identity-map,query-dry-run,execution,logs}

timestamp() { date -u +%Y-%m-%dT%H:%M:%SZ; }
failure() { printf 'FIX486C_FAIL=%s\n' "$1" >&2; }

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
  jq -c '.ingestion_plans[]' "$EVIDENCE/ingestion/plans.json" | while IFS= read -r plan; do
    logical_zone=$(jq -r '.logical_zone_id' <<<"$plan")
    logical_document=$(jq -r '.logical_document_id' <<<"$plan")
    request="$EVIDENCE/ingestion/${logical_zone}-${logical_document}.request.json"
    response="$EVIDENCE/ingestion/${logical_zone}-${logical_document}.response.json"
    jq '.request' <<<"$plan" >"$request"
    grpcurl -plaintext -d @ "$ENDPOINT" astravector.embedding.v1.AstraVectorIngestionFacade/IndexLogicalDocument <"$request" >"$response"
    zone=$(jq -r '.document.accessZoneId' "$response")
    document=$(jq -r '.document.documentId' "$response")
    grpcurl -plaintext -d "{\"accessZoneId\":\"$zone\",\"documentId\":\"$document\",\"documentVersion\":1}" "$ENDPOINT" astravector.embedding.v1.AstraVectorV004Control/ActivateDocumentVersion >"$EVIDENCE/ingestion/${logical_zone}-${logical_document}.activate.json"
    jq -n --arg logical_zone "$logical_zone" --arg logical_document "$logical_document" --argfile response "$response" '{logical_zone_id:$logical_zone,logical_document_id:$logical_document,response:$response}' >"$EVIDENCE/identity-map/${logical_zone}-${logical_document}.json"
  done
  jq -s --arg source_sha "$(git -C "$ROOT" rev-parse HEAD)" --arg aggregate "$(jq -r '.hashes.aggregate_sha256' "$MANIFEST")" \
    '{schema_version:1,bank_id:"fix486-hierarchical-bank",bank_version:"1.0.0",bank_aggregate_sha256:$aggregate,source_sha:$source_sha,documents:.,access_zones:(map({key:.logical_zone_id,value:.response.document.accessZoneId})|from_entries)}' \
    "$EVIDENCE"/identity-map/*.json >"$EVIDENCE/identity-map/logical-to-runtime.json"
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
      jq -n --argfile plan <(printf '%s' "$plan") --argfile response "$EVIDENCE/execution/$query_id.response.json" \
        '{query_id:$plan.query_id,case_id:$plan.case_id,status:"PASS",runtime_status:"OK",matched_contexts:($response.results // []),warnings:($response.warnings // []),hard_gate_results:{},bank_aggregate_sha256:$plan.bank_aggregate_sha256}' >"$EVIDENCE/execution/$query_id.result.json"
    else
      jq -n --argfile plan <(printf '%s' "$plan") '{query_id:$plan.query_id,case_id:$plan.case_id,status:"FAIL",runtime_status:"GRPC_ERROR",matched_contexts:[],warnings:[],hard_gate_results:{},bank_aggregate_sha256:$plan.bank_aggregate_sha256}' >"$EVIDENCE/execution/$query_id.result.json"
      return 1
    fi
  done
}

cd "$ROOT"
git status --porcelain >"$EVIDENCE/source/worktree-status.txt"
if [[ -s "$EVIDENCE/source/worktree-status.txt" ]]; then failure DIRTY_WORKTREE; finalize FIX486_FROZEN_EXECUTABLE_BANK_BLOCKED; exit 1; fi

case "$MODE" in
  --verify-only) verify && finalize FIX486_FROZEN_EXECUTABLE_BANK_PASS ;;
  --dry-run) verify && dry_run && finalize FIX486_FROZEN_EXECUTABLE_BANK_PASS ;;
  --prepare-runtime) verify && dry_run && require_endpoint && finalize FIX486_FROZEN_EXECUTABLE_BANK_PASS ;;
  --ingest-only) verify && dry_run && ingest && finalize FIX486_FROZEN_EXECUTABLE_BANK_PASS ;;
  --execute-all) verify && dry_run && execute_all && finalize FIX486_FROZEN_EXECUTABLE_BANK_PASS ;;
  *) echo "usage: $0 [--verify-only|--dry-run|--prepare-runtime|--ingest-only|--execute-all]" >&2; exit 64 ;;
esac
