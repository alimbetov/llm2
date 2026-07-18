#!/usr/bin/env bash
# Phase D uses only the public ingestion/Search/RetrieveContext APIs for positives.
set -Eeuo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
MODE=${1:---execute-all}; shift || true
RUN_ID=${FIX486D_RUN_ID:-fix486d-$(date -u +%Y%m%dT%H%M%SZ)}
EVIDENCE_ROOT=${ASTRAVECTOR_EVIDENCE_ROOT:-/Users/ruslanalimbetov/Documents/llm2/astravector-evidence}
while (($#)); do case "$1" in --run-id) RUN_ID=$2; shift 2;; --evidence-root) EVIDENCE_ROOT=$2; shift 2;; *) exit 64;; esac; done
E="$EVIDENCE_ROOT/fix486d/$RUN_ID"; BANK="$ROOT/benchmarks/hierarchical/fix486"; H="$ROOT/scripts/fix486d_proof.py"
PG=${FIX486D_POSTGRES_PORT:-57432}; QP=${FIX486D_QDRANT_HTTP_PORT:-6533}; QG=${FIX486D_QDRANT_GRPC_PORT:-6534}; GP=${FIX486D_GRPC_PORT:-50586}; MP=${FIX486D_METRICS_PORT:-9056}
DB="postgres://astravector:astravector@127.0.0.1:$PG/astravector"; Q="http://127.0.0.1:$QP"; ADDR="127.0.0.1:$GP"; COL=${ASTRAVECTOR_QDRANT_COLLECTION:-astravector_fix486d}; PID=""; PROJECT="fix486d-$RUN_ID"; PROJECT=$(printf %s "$PROJECT"|tr '[:upper:]_' '[:lower:]-'|tr -cd 'a-z0-9-')
mkdir -p "$E"/{source,bank,config,model-tokenizer,infrastructure,ingestion,identity-map,canonical-audit,qdrant-audit,search,retrieve-context,comparisons,restart,logs,metrics}
STARTED_AT=$(date -u +%Y-%m-%dT%H:%M:%SZ)
jq -n --arg run_id "$RUN_ID" --arg mode "$MODE" --arg started_at "$STARTED_AT" --arg branch "$(git branch --show-current)" --arg source_sha "$(git rev-parse HEAD)" '{run_id:$run_id,mode:$mode,started_at_utc:$started_at,branch:$branch,source_sha:$source_sha,status:"RUNNING"}' >"$E/bootstrap.json"
fail() { echo "FIX486D_FAIL=$1" >&2; return 1; }
compose() { FIX486D_POSTGRES_PORT=$PG FIX486D_QDRANT_HTTP_PORT=$QP FIX486D_QDRANT_GRPC_PORT=$QG docker compose -p "$PROJECT" -f "$ROOT/docker-compose.fix486d.yml" "$@"; }
wait_for() { for _ in $(seq 1 90); do "$@" >/dev/null 2>&1 && return; sleep 1; done; return 1; }
cleanup() { [[ -n "$PID" ]] && kill -INT "$PID" 2>/dev/null || true; [[ -n "$PID" ]] && wait "$PID" 2>/dev/null || true; compose down -v >"$E/infrastructure/compose-down.log" 2>&1 || true; }
terminal_result() { rc=$?; jq -n --argjson exit_code "$rc" --arg finished_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" '{stage:"runner-terminal",status:(if $exit_code==0 then "PASS" else "FAIL" end),exit_code:$exit_code,signal:null,finished_at_utc:$finished_at}' >"$E/terminal-result.json"; cleanup; exit "$rc"; }
on_signal() { signal=$1; jq -n --arg signal "$signal" --arg finished_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" '{stage:"runner-terminal",status:"BLOCKED",exit_code:null,signal:$signal,finished_at_utc:$finished_at,failure_code:"RUNNER_TERMINATED_BY_SIGNAL"}' >"$E/terminal-result.json"; cleanup; exit 1; }
trap terminal_result EXIT
trap 'on_signal INT' INT
trap 'on_signal TERM' TERM
trap 'on_signal HUP' HUP

verify() {
  [[ -z $(git status --porcelain) ]] || fail DIRTY_WORKTREE
  python3 "$ROOT/scripts/fix486c_verify_frozen_bank.py" --root "$BANK" >"$E/bank/verification.json"
  jq -e '.aggregate_sha256=="cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff"' "$E/bank/verification.json" >/dev/null
  python3 "$H" select --bank "$BANK" --output "$E/bank/selected-queries.json" >/dev/null
  jq -n --arg branch "$(git branch --show-current)" --arg source "$(git rev-parse HEAD)" '{branch:$branch,source_sha:$source}' >"$E/source/git-identity.json"
}
start() {
  for port in "$GP" "$MP"; do
    if lsof -nP -iTCP:"$port" -sTCP:LISTEN >"$E/infrastructure/port-$port-owner.txt" 2>&1; then
      echo "FIX486D_FAIL=PREEXISTING_PORT_OWNER port=$port" >&2
      return 1
    fi
  done
  compose up -d >"$E/infrastructure/compose-up.log" 2>&1
  wait_for psql "$DB" -Atqc 'select 1'; wait_for curl -fsS "$Q/readyz"
  DATABASE_URL="$DB" cargo sqlx migrate run --source "$ROOT/migrations" >"$E/logs/migrations.log" 2>&1
  cargo build --locked --release --bin astravector-runtime >"$E/logs/release-build.log" 2>&1
  ASTRAVECTOR_CONFIG="$ROOT/config/application.yaml" ASTRAVECTOR_PROFILE_CONFIG="$ROOT/config/application-fix486d.yaml" ASTRAVECTOR_PROFILE=fix486d ASTRAVECTOR_DB_URL="$DB" DATABASE_URL="$DB" ASTRAVECTOR_QDRANT_URL="$Q" ASTRAVECTOR_QDRANT_COLLECTION="$COL" ASTRAVECTOR_MODEL_PATH="${ASTRAVECTOR_MODEL_PATH:-/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/onnx/model.onnx}" ASTRAVECTOR_TOKENIZER_PATH="${ASTRAVECTOR_TOKENIZER_PATH:-/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/tokenizer.json}" ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true FIX486D_GRPC_PORT="$GP" FIX486D_METRICS_PORT="$MP" "$ROOT/target/release/astravector-runtime" >"$E/logs/runtime.log" 2>&1 & PID=$!
  wait_for grpcurl -plaintext "$ADDR" list || return 1
  kill -0 "$PID" 2>/dev/null || return 1
  grpcurl -plaintext "$ADDR" list >"$E/infrastructure/services.txt"
}
ingest() {
  python3 "$ROOT/scripts/fix486c_verify_frozen_bank.py" --root "$BANK" --emit-ingestion-plans --output "$E/ingestion/plans.json"
  while read -r plan; do
    z=$(jq -r .logical_zone_id <<<"$plan"); d=$(jq -r .logical_document_id <<<"$plan"); jq .request <<<"$plan" >"$E/ingestion/$z-$d.request.json"
    grpcurl -plaintext -d @ "$ADDR" astravector.embedding.v1.AstraVectorIngestionFacade/IndexLogicalDocument <"$E/ingestion/$z-$d.request.json" >"$E/ingestion/$z-$d.response.json"
    rz=$(jq -r .document.accessZoneId "$E/ingestion/$z-$d.response.json"); rd=$(jq -r .document.documentId "$E/ingestion/$z-$d.response.json")
    for _ in $(seq 1 90); do grpcurl -plaintext -d "{\"accessZoneId\":\"$rz\",\"documentId\":\"$rd\",\"documentVersion\":1}" "$ADDR" astravector.embedding.v1.AstraVectorV004Control/ActivateDocumentVersion >"$E/ingestion/$z-$d.activate.json" 2>&1 && break; sleep 1; done
    jq -e '.status=="ACTIVE"' "$E/ingestion/$z-$d.activate.json" >/dev/null
  done < <(jq -c '.ingestion_plans[]' "$E/ingestion/plans.json")
}
audit() {
  psql "$DB" -Atqc "SELECT coalesce(json_agg(x),'[]') FROM (SELECT CASE z.access_zone_code WHEN '4862' THEN 'zone-a' WHEN '4863' THEN 'zone-b' END logical_zone_id,'doc-hierarchy' logical_document_id,c.document_version logical_version,c.id::text runtime_chunk_id,c.access_zone_id::text runtime_access_zone_id,c.document_id::text runtime_document_id,CASE WHEN c.granularity='PARENT' THEN 'PARENT' ELSE 'CHILD' END chunk_role,c.granularity,COALESCE(m.block_id,c.source_block_id) source_block_id,c.content_hash content_sha256,CASE WHEN c.granularity='PARENT' THEN COALESCE(m.block_id,c.source_block_id) ELSE COALESCE(m.block_id,c.source_block_id)||CASE c.granularity WHEN 'SUB_180' THEN '-180' ELSE '-260' END END logical_chunk_id,c.parent_chunk_id::text runtime_parent_chunk_id FROM astravector.content_chunks_v004 c JOIN astravector.access_zones z ON z.access_zone_id=c.access_zone_id LEFT JOIN astravector.logical_block_chunk_mapping m ON m.access_zone_id=c.access_zone_id AND m.document_id=c.document_id AND m.document_version=c.document_version AND m.chunk_id=c.id WHERE z.access_zone_code IN ('4862','4863') AND c.document_version=1 AND c.granularity IN ('PARENT','SUB_180','SUB_260'))x" >"$E/identity-map/rows.json"
  jq '{rows:.}' "$E/identity-map/rows.json" >"$E/identity-map/logical-to-runtime.json"; python3 "$H" validate-identity --input "$E/identity-map/logical-to-runtime.json" --bank "$BANK" >"$E/identity-map/validation.json"
  psql "$DB" -Atqf "$ROOT/scripts/fix486d-child-parent-audit.sql" | jq . >"$E/canonical-audit/integrity-summary.json"
  jq -e '[.orphan_children,.cross_document_bindings,.cross_version_bindings]|all(.==0)' "$E/canonical-audit/integrity-summary.json" >/dev/null
  pts=$(curl -fsS "$Q/collections/$COL"|jq '.result.points_count'); jq -n --argjson points "$pts" '{qdrant_points:$points,count_match:($points>0)}' >"$E/qdrant-audit/summary.json"
}
run_queries() {
  : >"$E/query-results.jsonl"
  while read -r x; do id=$(jq -r .query.query_id <<<"$x"); q=$(jq -r .query.question <<<"$x"); z=$(jq -r .query.access_zone <<<"$x"); rz=$(jq -r --arg z "$z" '.rows[]|select(.logical_zone_id==$z)|.runtime_access_zone_id' "$E/identity-map/logical-to-runtime.json"|head -1); jq .query <<<"$x" >"$E/bank/$id.query.json"; jq .qrel <<<"$x" >"$E/bank/$id.qrel.json"
    jq -n --arg z "$rz" --arg q "$q" --arg id "$id" '{correlationId:("fix486d-"+$id),accessZoneId:$z,callerAccessLevel:"INTERNAL",query:$q,topK:5,candidateLimit:20,parentLimit:5,timeoutMs:30000,searchMode:"SEARCH_MODE_V005_HYBRID",embeddingMode:"EMBEDDING_MODE_V005_DENSE_SPARSE_IF_AVAILABLE",includeDebug:true}' >"$E/search/$id.request.json"; grpcurl -plaintext -d @ "$ADDR" astravector.embedding.v1.AstraVectorV004Control/Search <"$E/search/$id.request.json" >"$E/search/$id.response.json"; python3 "$H" normalize --query "$E/bank/$id.query.json" --qrel "$E/bank/$id.qrel.json" --entry-point Search --response "$E/search/$id.response.json" --identity-map "$E/identity-map/logical-to-runtime.json" --bank "$BANK" --output "$E/search/$id.result.json" >/dev/null; cat "$E/search/$id.result.json" >>"$E/query-results.jsonl"
    jq -n --arg z "$rz" --arg q "$q" --arg id "$id" '{context:{correlationId:("fix486d-"+$id),callerService:"fix486d",callerUserId:"fix486d",callerAccessLevel:"INTERNAL"},accessZoneId:$z,question:$q,profile:"RETRIEVAL_PROFILE_TECHNICAL",maxContexts:5,responseDetail:"RESPONSE_DETAIL_DEBUG",enableGraphExpansion:false}' >"$E/retrieve-context/$id.request.json"; grpcurl -plaintext -d @ "$ADDR" astravector.embedding.v1.AstraVectorRetrievalFacade/RetrieveContext <"$E/retrieve-context/$id.request.json" >"$E/retrieve-context/$id.response.json"; python3 "$H" normalize --query "$E/bank/$id.query.json" --qrel "$E/bank/$id.qrel.json" --entry-point RetrieveContext --response "$E/retrieve-context/$id.response.json" --identity-map "$E/identity-map/logical-to-runtime.json" --bank "$BANK" --output "$E/retrieve-context/$id.result.json" >/dev/null; cat "$E/retrieve-context/$id.result.json" >>"$E/query-results.jsonl"
  done < <(jq -c '.[]' "$E/bank/selected-queries.json")
}
finalize() { [[ -f "$E/aggregate.json" ]] || { python3 "$H" aggregate --run "$E" --output "$E/aggregate.json" || true; }; v=$(jq -r .verdict "$E/aggregate.json" 2>/dev/null||echo FIX486_CHILD_PARENT_RUNTIME_PROOF_BLOCKED); find "$E" -type f ! -name manifest.json -print0|sort -z|xargs -0 shasum -a 256|jq -Rsc 'split("\n")|map(select(length>0))' >"$E/manifest.json"; echo "$v"; [[ "$v" == FIX486_CHILD_PARENT_RUNTIME_PROOF_PASS ]]; }
ok=true
case "$MODE" in
  --verify-identities) verify || ok=false ;;
  --prepare) verify && start || ok=false ;;
  --ingest) verify && start && ingest && audit || ok=false ;;
  --execute-search|--execute-retrieve-context|--repeat|--restart-proof|--execute-all) verify && start && ingest && audit && run_queries || ok=false ;;
  *) exit 64 ;;
esac
if [[ "$ok" != true ]]; then
  jq -n '{verdict:"FIX486_CHILD_PARENT_RUNTIME_PROOF_BLOCKED",failure_codes:["MANDATORY_STAGE_FAILED"],primary_result_count:0}' >"$E/aggregate.json"
fi
finalize
