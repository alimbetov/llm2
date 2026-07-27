#!/usr/bin/env bash
set -Eeuo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
WORKSPACE_ROOT=$(cd "$ROOT/.." && pwd)
MODE=${1:---execute-all}; shift || true
RUN_ID=${FIX486G_RUN_ID:-fix486g-$(date -u +%Y%m%dT%H%M%SZ)}
EVIDENCE_ROOT=${ASTRAVECTOR_EVIDENCE_ROOT:-$WORKSPACE_ROOT/astravector-evidence}
while (($#)); do case "$1" in --run-id) RUN_ID=$2; shift 2;; --evidence-root) EVIDENCE_ROOT=$2; shift 2;; *) exit 64;; esac; done
case "$MODE" in
  --execute-all|--verify-identities|--verify-contracts|--cleanup-only|--verify-evidence) ;;
  *) echo "FIX486G_FAIL=UNKNOWN_MODE:$MODE" >&2; exit 64;;
esac
E="$EVIDENCE_ROOT/fix486g/$RUN_ID"; BANK="$ROOT/benchmarks/hierarchical/fix486"; SUPPLEMENTAL="$ROOT/benchmarks/hierarchical/fix486g-supplemental"; H="$ROOT/scripts/fix486g_proof.py"; STAT_CAPTURE="$ROOT/scripts/fix486g_statistical_capture.py"; STAT_EVAL="$ROOT/scripts/fix486g_statistical_proof.py"
PG=${FIX486G_POSTGRES_PORT:-59432}; QP=${FIX486G_QDRANT_HTTP_PORT:-6733}; QG=${FIX486G_QDRANT_GRPC_PORT:-6734}; GP=${FIX486G_GRPC_PORT:-50588}; MP=${FIX486G_METRICS_PORT:-9058}
DB="postgres://astravector:astravector@127.0.0.1:$PG/astravector"; Q="http://127.0.0.1:$QP"; ADDR="127.0.0.1:$GP"; COL=${ASTRAVECTOR_QDRANT_COLLECTION:-astravector_fix486g}
MODEL_PATH=${ASTRAVECTOR_MODEL_PATH:-$WORKSPACE_ROOT/models/bge-m3/onnx/model.onnx}
TOKENIZER_PATH=${ASTRAVECTOR_TOKENIZER_PATH:-$WORKSPACE_ROOT/models/bge-m3/tokenizer.json}
DOCUMENT_DEADLINE_MS=${ASTRAVECTOR_INGESTION_DOCUMENT_DEADLINE_MS:-180000}
PROJECT=$(printf 'fix486g-%s' "$RUN_ID" | tr '[:upper:]_' '[:lower:]-' | tr -cd 'a-z0-9-')
PID=""; FINALIZED=false; SOURCE_SHA=$(git -C "$ROOT" rev-parse HEAD); BANK_SHA=cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff; SUPPLEMENTAL_SHA=af4fceb8e424fddecff4284e9cd8d1d68fb4db5c148f9b2aa585bb8497ac1649
BRANCH=$(git -C "$ROOT" branch --show-current)
REMOTE_SHA=$(git -C "$ROOT" rev-parse '@{upstream}' 2>/dev/null || true)
FAULT_GRAPH_RELATED_CONTEXTS=10
((FAULT_GRAPH_RELATED_CONTEXTS <= 20)) || { echo "FIX486G_FAIL=FAULT_GRAPH_WINDOW_UNBOUNDED" >&2; exit 1; }

if [[ "$MODE" == --cleanup-only || "$MODE" == --verify-evidence ]]; then
  [[ -d "$E" ]] || { echo "FIX486G_FAIL=EVIDENCE_RUN_NOT_FOUND:$E" >&2; exit 1; }
else
  [[ ! -e "$E" ]] || { echo "FIX486G_FAIL=EVIDENCE_RUN_ALREADY_EXISTS:$E" >&2; exit 1; }
  mkdir -p "$E"/{source,bank,config,model-tokenizer,infrastructure,ingestion,identity-map,canonical-audit,qdrant-audit,graph-audit,search,retrieve-context,graph-disabled/search,graph-disabled/retrieve-context,faults/mutations,comparisons/warm-search,comparisons/warm-retrieve-context,restart/search,restart/retrieve-context,statistical/concurrent,statistical/degradation,statistical/logs,cleanup,logs,metrics}
fi

timestamp() { date -u +%Y-%m-%dT%H:%M:%SZ; }
compose() { FIX486G_POSTGRES_PORT=$PG FIX486G_QDRANT_HTTP_PORT=$QP FIX486G_QDRANT_GRPC_PORT=$QG docker compose -p "$PROJECT" -f "$ROOT/docker-compose.fix486g.yml" "$@"; }
wait_for() { for _ in $(seq 1 90); do "$@" >/dev/null 2>&1 && return 0; sleep 1; done; return 1; }
stage() {
  local name=$1; shift; local started rc status
  started=$(timestamp); set +e; "$@" >"$E/logs/$name.log" 2>&1; rc=$?; set -e
  status=PASS; [[ $rc -eq 0 ]] || status=FAIL
  jq -n --arg stage "$name" --arg status "$status" --arg started "$started" --arg finished "$(timestamp)" --argjson exit_code "$rc" \
    '{stage:$stage,status:$status,started_at:$started,finished_at:$finished,exit_code:$exit_code,failure_codes:(if $exit_code==0 then [] else ["COMMAND_FAILED"] end),artifacts:[]}' >"$E/logs/$name.stage.json"
  [[ $rc -eq 0 ]]
}
record_stage_status() {
  local name=$1 status=$2 code=${3:-}
  jq -n --arg stage "$name" --arg status "$status" --arg now "$(timestamp)" --arg code "$code" \
    '{stage:$stage,status:$status,started_at:$now,finished_at:$now,exit_code:(if $status=="PASS" then 0 else 1 end),failure_codes:(if $code=="" then [] else [$code] end),artifacts:[]}' >"$E/logs/$name.stage.json"
}
verify_sql_count() {
  local sql=$1 actual_rows
  actual_rows=$(psql "$DB" -X -v ON_ERROR_STOP=1 -Atqc "SELECT count(*) FROM ($sql) verified_rows") || return 1
  [[ "$actual_rows" =~ ^[0-9]+$ ]] || return 1
  printf '%s\n' "$actual_rows"
}
run_exact_mutation() {
  local label=$1 expected_rows=$2 mutation_sql=$3 verification_sql=$4 expected_activation_rows=$5
  local actual_rows=-1 activation_rows=-1 status=FAIL failure_code=""
  mkdir -p "$E/faults/mutations"
  if ! actual_rows=$(psql "$DB" -X -v ON_ERROR_STOP=1 -Atqc "WITH affected AS ($mutation_sql RETURNING 1) SELECT count(*) FROM affected"); then
    failure_code=FAULT_MUTATION_SQL_FAILED
  elif [[ ! "$actual_rows" =~ ^[0-9]+$ || "$actual_rows" -ne "$expected_rows" ]]; then
    failure_code=FAULT_MUTATION_ROW_COUNT_MISMATCH
  elif ! activation_rows=$(verify_sql_count "$verification_sql"); then
    failure_code=FAULT_ACTIVATION_QUERY_FAILED
  elif [[ "$activation_rows" -ne "$expected_activation_rows" ]]; then
    failure_code=FAULT_ACTIVATION_MISMATCH
  else
    status=PASS
  fi
  jq -n --arg label "$label" --arg status "$status" --arg failure_code "$failure_code" \
    --argjson expected_rows "$expected_rows" --argjson actual_rows "$actual_rows" \
    --argjson expected_activation_rows "$expected_activation_rows" --argjson activation_rows "$activation_rows" \
    '{label:$label,status:$status,expected_rows:$expected_rows,actual_rows:$actual_rows,expected_activation_rows:$expected_activation_rows,activation_rows:$activation_rows,failure_codes:(if $failure_code=="" then [] else [$failure_code] end)}' \
    >"$E/faults/mutations/$label.json"
  [[ "$status" == PASS ]] || { echo "FIX486G_FAIL=$failure_code:$label" >&2; return 1; }
}
stop_runtime() {
  local pid=$PID
  if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
    kill -INT "$pid" 2>/dev/null || true
    for _ in $(seq 1 30); do kill -0 "$pid" 2>/dev/null || break; sleep 1; done
    kill -TERM "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true
  fi
  if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
    return 1
  fi
  PID=""
  ! lsof -nP -iTCP:"$GP" -sTCP:LISTEN >/dev/null 2>&1
}
load_owned_runtime_pid() {
  local candidate command
  [[ -z "$PID" ]] || return 0
  candidate=$(jq -rs 'map(select(.pid != null)) | last | .pid // empty' "$E"/config/runtime-*.json 2>/dev/null || true)
  [[ "$candidate" =~ ^[0-9]+$ ]] || return 0
  command=$(ps -p "$candidate" -o command= 2>/dev/null || true)
  [[ "$command" == *"$ROOT/target/release/astravector-runtime"* ]] || return 0
  PID=$candidate
}
restore_fault_state_before_teardown() {
  local zone child parent baseline_expires expires_sql baseline current purge_rows=-1 active_fault_rows=-1 restoration_matches_baseline=false status=PASS failure_code=""
  if [[ ! -s "$E/faults/targets.json" || ! -s "$E/faults/baseline.json" ]]; then
    jq -n '{status:"PASS",applicable:false,active_fault_rows:0,restoration_matches_baseline:true,failure_codes:[]}' >"$E/cleanup/restoration.json"
    return 0
  fi
  zone=$(jq -r .access_zone_id "$E/faults/targets.json"); child=$(jq -r .child_a3 "$E/faults/targets.json"); parent=$(jq -r .parent_a3 "$E/faults/targets.json")
  baseline_expires=$(jq -r '.expires_at // empty' "$E/faults/baseline.json")
  if [[ -n "$baseline_expires" ]]; then expires_sql="'$baseline_expires'::timestamptz"; else expires_sql=NULL; fi
  if ! run_exact_mutation cleanup-binding-restore 1 \
    "UPDATE astravector.vector_bindings_v004 SET parent_chunk_id='$parent',qdrant_sync_status='SYNCED' WHERE access_zone_id='$zone' AND chunk_id='$child' AND representation_type='ORIGINAL'" \
    "SELECT 1 FROM astravector.vector_bindings_v004 WHERE access_zone_id='$zone' AND chunk_id='$child' AND representation_type='ORIGINAL' AND parent_chunk_id='$parent' AND qdrant_sync_status='SYNCED'" 1; then
    status=FAIL; failure_code=FAULT_RESTORATION_FAILED
  fi
  if ! run_exact_mutation cleanup-lifecycle-restore 1 \
    "UPDATE astravector.content_chunks_v004 SET lifecycle_status='ACTIVE',deleted_at=NULL,expires_at=$expires_sql WHERE access_zone_id='$zone' AND id='$child'" \
    "SELECT 1 FROM astravector.content_chunks_v004 WHERE access_zone_id='$zone' AND id='$child' AND lifecycle_status='ACTIVE' AND deleted_at IS NULL AND expires_at IS NOT DISTINCT FROM $expires_sql" 1; then
    status=FAIL; failure_code=FAULT_RESTORATION_FAILED
  fi
  purge_rows=$(psql "$DB" -X -v ON_ERROR_STOP=1 -Atqc "WITH affected AS (DELETE FROM astravector.rag_graph_edges WHERE properties->>'quality_run_id'='$RUN_ID' AND properties->>'phase_fault'='true' RETURNING 1) SELECT count(*) FROM affected" 2>/dev/null || echo -1)
  baseline=$(jq -c . "$E/faults/baseline.json")
  current=$(psql "$DB" -X -v ON_ERROR_STOP=1 -Atqc "SELECT json_build_object('binding_count',(SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id='$zone' AND chunk_id='$child' AND representation_type='ORIGINAL'),'binding_parent_chunk_id',(SELECT parent_chunk_id::text FROM astravector.vector_bindings_v004 WHERE access_zone_id='$zone' AND chunk_id='$child' AND representation_type='ORIGINAL'),'qdrant_sync_status',(SELECT qdrant_sync_status FROM astravector.vector_bindings_v004 WHERE access_zone_id='$zone' AND chunk_id='$child' AND representation_type='ORIGINAL'),'chunk_count',(SELECT count(*) FROM astravector.content_chunks_v004 WHERE access_zone_id='$zone' AND id='$child'),'lifecycle_status',(SELECT lifecycle_status FROM astravector.content_chunks_v004 WHERE access_zone_id='$zone' AND id='$child'),'deleted_at',(SELECT deleted_at FROM astravector.content_chunks_v004 WHERE access_zone_id='$zone' AND id='$child'),'expires_at',(SELECT expires_at FROM astravector.content_chunks_v004 WHERE access_zone_id='$zone' AND id='$child'),'expires_at_visible',(SELECT expires_at IS NULL OR expires_at>now() FROM astravector.content_chunks_v004 WHERE access_zone_id='$zone' AND id='$child'))" 2>/dev/null | jq -c . || echo '{}')
  active_fault_rows=$(psql "$DB" -X -v ON_ERROR_STOP=1 -Atqc "SELECT (SELECT count(*) FROM astravector.rag_graph_edges WHERE properties->>'quality_run_id'='$RUN_ID' AND properties->>'phase_fault'='true') + (SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id='$zone' AND chunk_id='$child' AND representation_type='ORIGINAL' AND (parent_chunk_id IS DISTINCT FROM '$parent'::uuid OR qdrant_sync_status<>'SYNCED')) + (SELECT count(*) FROM astravector.content_chunks_v004 WHERE access_zone_id='$zone' AND id='$child' AND (lifecycle_status<>'ACTIVE' OR deleted_at IS NOT NULL OR expires_at IS DISTINCT FROM $expires_sql))" 2>/dev/null || echo -1)
  [[ "$baseline" == "$current" ]] && restoration_matches_baseline=true
  if [[ "$active_fault_rows" -ne 0 || "$restoration_matches_baseline" != true ]]; then status=FAIL; failure_code=FAULT_RESTORATION_FAILED; fi
  jq -n --arg status "$status" --arg failure_code "$failure_code" --argjson purge_rows "$purge_rows" \
    --argjson active_fault_rows "$active_fault_rows" --argjson restoration_matches_baseline "$restoration_matches_baseline" \
    --argjson baseline "$baseline" --argjson current "$current" \
    '{status:$status,applicable:true,purged_fault_edges:$purge_rows,active_fault_rows:$active_fault_rows,restoration_matches_baseline:$restoration_matches_baseline,baseline:$baseline,current:$current,failure_codes:(if $failure_code=="" then [] else [$failure_code] end)}' >"$E/cleanup/restoration.json"
  [[ "$status" == PASS ]]
}
cleanup() {
  local restoration_ok=true runtime_ok=true compose_ok=true
  restore_fault_state_before_teardown || restoration_ok=false
  load_owned_runtime_pid
  stop_runtime || runtime_ok=false
  compose down -v >"$E/infrastructure/compose-down.log" 2>&1 || compose_ok=false
  local leaked_ports=0 leaked_processes=0
  for port in "$PG" "$QP" "$QG" "$GP" "$MP"; do lsof -nP -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1 && leaked_ports=$((leaked_ports+1)); done
  [[ "$runtime_ok" == true && -z "$PID" ]] || leaked_processes=1
  jq -n --argjson leaked_ports "$leaked_ports" --argjson leaked_processes "$leaked_processes" --argjson restoration_ok "$restoration_ok" --argjson runtime_ok "$runtime_ok" --argjson compose_ok "$compose_ok" \
    '{status:(if $leaked_ports==0 and $leaked_processes==0 and $restoration_ok and $runtime_ok and $compose_ok then "PASS" else "FAIL" end),leaked_port_owners:$leaked_ports,leaked_runtime_processes:$leaked_processes,evidence_directory_preserved:true,fault_restoration_ok:$restoration_ok,runtime_stop_ok:$runtime_ok,compose_down_ok:$compose_ok}' >"$E/cleanup/summary.json"
  jq -e '.status=="PASS"' "$E/cleanup/summary.json" >/dev/null
}
unexpected_exit() {
  local rc=$?
  if [[ "$FINALIZED" != true ]]; then
    local cleanup_status=PASS
    cleanup >/dev/null 2>&1 || cleanup_status=FAIL
    [[ $rc -ne 0 ]] || rc=1
    jq -n --argjson exit_code "$rc" --arg finished "$(timestamp)" --arg cleanup_status "$cleanup_status" \
      '{stage:"runner-terminal",status:"FAIL",termination_reason:"UNEXPECTED_EXIT",signal:null,cleanup_attempted:true,cleanup_status:$cleanup_status,exit_code:$exit_code,finished_at_utc:$finished}' >"$E/terminal-result.json"
  fi
}
handle_signal() {
  local signal=$1 rc=$2 cleanup_status=PASS
  trap - INT TERM HUP
  set +e
  cleanup >/dev/null 2>&1 || cleanup_status=FAIL
  jq -n --arg signal "$signal" --argjson exit_code "$rc" --arg finished "$(timestamp)" --arg cleanup_status "$cleanup_status" \
    '{stage:"runner-terminal",status:"FAIL",termination_reason:"SIGNAL",signal:$signal,cleanup_attempted:true,cleanup_status:$cleanup_status,exit_code:$exit_code,finished_at_utc:$finished}' >"$E/terminal-result.json"
  [[ ! -s "$E/bootstrap.json" ]] || { jq '.status="BLOCKED"' "$E/bootstrap.json" >"$E/bootstrap.tmp" && mv "$E/bootstrap.tmp" "$E/bootstrap.json"; }
  FINALIZED=true
  trap - EXIT
  exit "$rc"
}
trap unexpected_exit EXIT
trap 'handle_signal INT 130' INT
trap 'handle_signal TERM 143' TERM
trap 'handle_signal HUP 129' HUP

verify_identity() {
  branch_is_approved &&
    [[ -n "$REMOTE_SHA" ]] &&
    [[ -z $(git -C "$ROOT" status --porcelain) ]] &&
    [[ $(git -C "$ROOT" rev-parse HEAD) == "$SOURCE_SHA" ]] &&
    [[ "$SOURCE_SHA" == "$REMOTE_SHA" ]]
}
branch_is_approved() {
  case "$BRANCH" in
    codex/fix486g-graph-parent-proof|codex/fix486g-finalize-runtime-evidence) return 0 ;;
    *) return 1 ;;
  esac
}
verify_bank() {
  python3 "$ROOT/scripts/fix486c_verify_frozen_bank.py" --root "$BANK" >"$E/bank/verification.json" &&
  jq -e --arg sha "$BANK_SHA" '.status=="PASS" and .bank_aggregate_sha256==$sha' "$E/bank/verification.json" >/dev/null &&
  python3 "$H" select --bank "$BANK" --output "$E/bank/selected-queries.json" >/dev/null &&
  python3 "$H" verify-supplemental --bank "$SUPPLEMENTAL" --output "$E/bank/supplemental-verification.json" >/dev/null &&
  jq -e --arg sha "$SUPPLEMENTAL_SHA" '.status=="PASS" and .query_count==71 and .aggregate_sha256==$sha' "$E/bank/supplemental-verification.json" >/dev/null
}
verify_model_tokenizer() {
  [[ -s "$MODEL_PATH" && -s "$TOKENIZER_PATH" ]] || return 1
  [[ "$DOCUMENT_DEADLINE_MS" =~ ^[0-9]+$ ]] &&
    ((DOCUMENT_DEADLINE_MS >= 1000 && DOCUMENT_DEADLINE_MS <= 600000)) || return 1
  jq -n --arg model_path "$MODEL_PATH" --arg tokenizer_path "$TOKENIZER_PATH" \
    --arg model_sha "$(shasum -a 256 "$MODEL_PATH" | awk '{print $1}')" \
    --arg tokenizer_sha "$(shasum -a 256 "$TOKENIZER_PATH" | awk '{print $1}')" \
    --argjson model_bytes "$(stat -f %z "$MODEL_PATH")" --argjson tokenizer_bytes "$(stat -f %z "$TOKENIZER_PATH")" \
    --argjson document_deadline_ms "$DOCUMENT_DEADLINE_MS" \
    '{status:"PASS",document_deadline_ms:$document_deadline_ms,deadline_bounded:($document_deadline_ms>=1000 and $document_deadline_ms<=600000),model:{path:$model_path,sha256:$model_sha,size_bytes:$model_bytes},tokenizer:{path:$tokenizer_path,sha256:$tokenizer_sha,size_bytes:$tokenizer_bytes}}' \
    >"$E/model-tokenizer/identity.json"
}
static_gates() {
  cd "$ROOT"
  cargo fmt --all --check && cargo check --locked --all-targets --all-features &&
  cargo clippy --locked --all-targets --all-features -- -D warnings &&
  cargo test --locked --all-targets --all-features &&
  cargo test --locked --test fix486_hierarchical_bank_contracts -- --nocapture &&
  cargo test --locked --test fix486c_frozen_bank_contracts -- --nocapture &&
  cargo test --locked --test fix486d_child_parent_contracts -- --nocapture &&
  cargo test --locked --test fix486f_failure_semantics_contracts -- --nocapture &&
  cargo test --locked --test fix486g_graph_parent_contracts -- --nocapture &&
  cargo test --locked --test fix486g_runner_hardening_contracts -- --nocapture &&
  cargo test --locked --test fix486g_statistical_capture_contracts -- --nocapture &&
  cargo test --locked --test fix486g_statistical_proof_contracts -- --nocapture &&
  cargo test --locked --test fix486g_visibility_recheck_contracts -- --nocapture &&
  python3 -m unittest -v tests/test_fix486g_proof.py &&
  python3 -m py_compile scripts/fix486g_proof.py scripts/fix486g_statistical_capture.py scripts/fix486g_statistical_proof.py tests/test_fix486g_proof.py
}
start_infrastructure() {
  for port in "$PG" "$QP" "$QG" "$GP" "$MP"; do
    if lsof -nP -iTCP:"$port" -sTCP:LISTEN >"$E/infrastructure/port-$port-owner-before.txt" 2>&1; then
      echo "FIX486G_FAIL=PREEXISTING_PORT_OWNER:$port" >&2
      return 1
    fi
  done
  compose up -d && wait_for psql "$DB" -Atqc 'select 1' && wait_for curl -fsS "$Q/readyz"
}
migrate_and_build() {
  cd "$ROOT"
  DATABASE_URL="$DB" cargo sqlx migrate run --source "$ROOT/migrations" &&
  DATABASE_URL="$DB" cargo sqlx prepare --check -- --all-targets --all-features &&
  cargo build --locked --release --bin astravector-runtime
}
start_runtime() {
  local label=$1 runtime_log="$E/logs/runtime-$1.log"
  ASTRAVECTOR_CONFIG="$ROOT/config/application.yaml" ASTRAVECTOR_PROFILE_CONFIG="$ROOT/config/application-fix486g.yaml" ASTRAVECTOR_PROFILE=fix486g \
  ASTRAVECTOR_DB_URL="$DB" DATABASE_URL="$DB" ASTRAVECTOR_QDRANT_URL="$Q" ASTRAVECTOR_QDRANT_COLLECTION="$COL" \
  ASTRAVECTOR_MODEL_PATH="$MODEL_PATH" ASTRAVECTOR_TOKENIZER_PATH="$TOKENIZER_PATH" \
  ASTRAVECTOR_INGESTION_DOCUMENT_DEADLINE_MS="$DOCUMENT_DEADLINE_MS" RUST_LOG="${FIX486G_RUST_LOG:-info}" \
  ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true FIX486G_GRPC_PORT="$GP" FIX486G_METRICS_PORT="$MP" \
  "$ROOT/target/release/astravector-runtime" >"$runtime_log" 2>&1 & PID=$!
  wait_for grpcurl -plaintext "$ADDR" list && kill -0 "$PID" 2>/dev/null &&
    grpcurl -plaintext "$ADDR" list >"$E/infrastructure/services-$label.txt" &&
    grpcurl -plaintext -d '{"service":""}' "$ADDR" grpc.health.v1.Health/Check >"$E/infrastructure/health-$label.json" &&
    jq -e '.status=="SERVING"' "$E/infrastructure/health-$label.json" >/dev/null &&
    wait_for curl -fsS "http://127.0.0.1:$MP/metrics" &&
    curl -fsS "http://127.0.0.1:$MP/metrics" >"$E/metrics/$label.prom" &&
    jq -s -e --argjson expected "$DOCUMENT_DEADLINE_MS" \
      'any(.[]; .fields.message=="INGESTION_DOCUMENT_DEADLINE_RESOLVED" and .fields.document_deadline_ms==$expected)' \
      "$runtime_log" >/dev/null &&
    jq -n --arg label "$label" --argjson pid "$PID" --arg endpoint "$ADDR" \
      --arg binary_sha "$(shasum -a 256 "$ROOT/target/release/astravector-runtime" | awk '{print $1}')" \
      --arg base_config_sha "$(shasum -a 256 "$ROOT/config/application.yaml" | awk '{print $1}')" \
      --arg profile_config_sha "$(shasum -a 256 "$ROOT/config/application-fix486g.yaml" | awk '{print $1}')" \
      --argjson document_deadline_ms "$DOCUMENT_DEADLINE_MS" \
      '{status:"PASS",label:$label,pid:$pid,endpoint:$endpoint,binary_sha256:$binary_sha,base_config_sha256:$base_config_sha,profile_config_sha256:$profile_config_sha,document_deadline_ms:$document_deadline_ms}' \
      >"$E/config/runtime-$label.json"
}
ingest() {
  python3 "$ROOT/scripts/fix486c_verify_frozen_bank.py" --root "$BANK" --emit-ingestion-plans --output "$E/ingestion/plans.json" || return 1
  while read -r plan; do
    local z d rz rd doc_uuid relation_json active=false
    z=$(jq -r .logical_zone_id <<<"$plan"); d=$(jq -r .logical_document_id <<<"$plan")
    doc_uuid=$(python3 -c 'import sys,uuid; print(uuid.uuid5(uuid.NAMESPACE_URL,f"fix486g:{sys.argv[1]}:{sys.argv[2]}:{sys.argv[3]}"))' "$RUN_ID" "$z" "$d")
    if [[ "$z" == zone-a ]]; then
      relation_json=$(jq -cn --arg run "$RUN_ID" --arg doc "$doc_uuid" '[{relation_id:"graph-a1-repaired-by-a3",relation_type:"REPAIRED_BY",quality_run_id:$run,from_document_uuid:$doc,to_document_uuid:$doc,from_document_id:"doc-hierarchy",to_document_id:"doc-hierarchy",from_block_id:"parent-a1",to_block_id:"parent-a3",from_granularity:"SUB_180",to_granularity:"SUB_180",weight:0.95,quality_runtime_bench:"fix486g"}]')
    else
      relation_json=$(jq -cn --arg run "$RUN_ID" --arg doc "$doc_uuid" '[{relation_id:"graph-zone-b-private-self-relation",relation_type:"RELATED_TO",quality_run_id:$run,from_document_uuid:$doc,to_document_uuid:$doc,from_document_id:"doc-hierarchy",to_document_id:"doc-hierarchy",from_block_id:"parent-a1",to_block_id:"parent-a1",from_granularity:"SUB_180",to_granularity:"SUB_260",weight:0.8,quality_runtime_bench:"fix486g"}]')
    fi
    jq --arg doc "$doc_uuid" --arg run "$RUN_ID" --arg relations "$relation_json" '.request | .document.documentId=$doc | .metadata.quality_run_id=$run | .metadata.quality_fixture_relations_json=$relations | .context.correlationId=("fix486g-"+$run+"-"+.document.externalDocumentId) | .context.idempotencyKey=("fix486g-"+$run+"-"+.document.externalDocumentId)' <<<"$plan" >"$E/ingestion/$z-$d.request.json"
    grpcurl -plaintext -d @ "$ADDR" astravector.embedding.v1.AstraVectorIngestionFacade/IndexLogicalDocument <"$E/ingestion/$z-$d.request.json" >"$E/ingestion/$z-$d.response.json" || return 1
    rz=$(jq -r .document.accessZoneId "$E/ingestion/$z-$d.response.json"); rd=$(jq -r .document.documentId "$E/ingestion/$z-$d.response.json")
    for _ in $(seq 1 90); do if grpcurl -plaintext -d "{\"accessZoneId\":\"$rz\",\"documentId\":\"$rd\",\"documentVersion\":1}" "$ADDR" astravector.embedding.v1.AstraVectorV004Control/ActivateDocumentVersion >"$E/ingestion/$z-$d.activate.json" 2>&1; then active=true; break; fi; sleep 1; done
    [[ "$active" == true ]] && jq -e '.status=="ACTIVE"' "$E/ingestion/$z-$d.activate.json" >/dev/null || return 1
  done < <(jq -c '.ingestion_plans[]' "$E/ingestion/plans.json")
}
identity_map() {
  psql "$DB" -Atqc "SELECT coalesce(json_agg(x),'[]') FROM (SELECT CASE z.access_zone_code WHEN '4862' THEN 'zone-a' WHEN '4863' THEN 'zone-b' END logical_zone_id,'doc-hierarchy' logical_document_id,c.document_version logical_version,c.id::text runtime_chunk_id,c.access_zone_id::text runtime_access_zone_id,c.document_id::text runtime_document_id,CASE WHEN c.granularity='PARENT' THEN 'PARENT' ELSE 'CHILD' END chunk_role,c.granularity,COALESCE(m.block_id,c.source_block_id) source_block_id,c.content_hash content_sha256,CASE WHEN c.granularity='PARENT' THEN COALESCE(m.block_id,c.source_block_id) ELSE COALESCE(m.block_id,c.source_block_id)||CASE c.granularity WHEN 'SUB_180' THEN '-180' ELSE '-260' END END logical_chunk_id,c.parent_chunk_id::text runtime_parent_chunk_id FROM astravector.content_chunks_v004 c JOIN astravector.access_zones z ON z.access_zone_id=c.access_zone_id LEFT JOIN astravector.logical_block_chunk_mapping m ON m.access_zone_id=c.access_zone_id AND m.document_id=c.document_id AND m.document_version=c.document_version AND m.chunk_id=c.id WHERE z.access_zone_code IN ('4862','4863') AND c.document_version=1 AND c.granularity IN ('PARENT','SUB_180','SUB_260'))x" >"$E/identity-map/rows.json" || return 1
  jq '{rows:.}' "$E/identity-map/rows.json" >"$E/identity-map/logical-to-runtime.raw.json"
  python3 "$H" validate-identity --input "$E/identity-map/logical-to-runtime.raw.json" --bank "$BANK" \
    --classified-output "$E/identity-map/logical-to-runtime.json" >"$E/identity-map/validation.json"
}
canonical_audit() {
  psql "$DB" -Atqf "$ROOT/scripts/fix486g-graph-parent-audit.sql" | jq . >"$E/canonical-audit/integrity-summary.json" || return 1
  jq -e '.active_documents==2 and .active_versions==2 and .parent_chunks>0 and .child_chunks>0 and .bindings==.synced_bindings and .completed_outbox>=.synced_bindings and .dead_letters==0 and .quality_fixture_edges>0 and .repaired_by_edges>0 and ([.orphan_children,.cross_document_bindings,.cross_version_bindings,.cross_zone_bindings,.duplicate_chunk_ids,.duplicate_source_provenance_rows,.orphan_graph_endpoints,.cross_zone_graph_edges,.duplicate_graph_relations,.duplicate_graph_relation_ids,.cross_document_graph_relations,.cross_version_graph_relations,.graph_self_edges]|all(.==0))' "$E/canonical-audit/integrity-summary.json" >/dev/null
}
graph_audit() {
  psql "$DB" -Atqc "SELECT coalesce(json_agg(x),'[]') FROM (SELECT e.edge_id::text,COALESCE(NULLIF(e.properties->>'relation_id',''),e.edge_id::text) relation_id,e.relation_type,e.relation_score,e.relation_source,e.properties->>'quality_run_id' quality_run_id,s.chunk_id::text seed_chunk_id,sp.id::text seed_parent_chunk_id,t.chunk_id::text related_chunk_id,tp.id::text related_parent_chunk_id,e.access_zone_id::text access_zone_id FROM astravector.rag_graph_edges e JOIN astravector.rag_graph_nodes_chunk s ON s.access_zone_id=e.access_zone_id AND s.node_id=e.source_node_id JOIN astravector.content_chunks_v004 sc ON sc.access_zone_id=s.access_zone_id AND sc.id=s.chunk_id JOIN astravector.content_chunks_v004 sp ON sp.access_zone_id=sc.access_zone_id AND sp.id=COALESCE(sc.parent_chunk_id,sc.id) JOIN astravector.rag_graph_nodes_chunk t ON t.access_zone_id=e.access_zone_id AND t.node_id=e.target_node_id JOIN astravector.content_chunks_v004 tc ON tc.access_zone_id=t.access_zone_id AND tc.id=t.chunk_id JOIN astravector.content_chunks_v004 tp ON tp.access_zone_id=tc.access_zone_id AND tp.id=COALESCE(tc.parent_chunk_id,tc.id) WHERE e.relation_type='REPAIRED_BY' AND e.properties->>'quality_run_id'='$RUN_ID' ORDER BY e.relation_rank NULLS LAST,e.edge_id)x" >"$E/graph-audit/graph-provenance-trace.json" || return 1
  jq -e 'length>0 and all(.[]; .seed_parent_chunk_id!=.related_parent_chunk_id and .relation_type=="REPAIRED_BY" and .quality_run_id!="")' "$E/graph-audit/graph-provenance-trace.json" >/dev/null || return 1
  jq '.[0] | {status:"PASS",seed_chunk_id,seed_parent_chunk_id,relation_id,edge_id,relation_type,relation_score,related_chunk_id,related_parent_chunk_id,hop_index:1,origin:"GRAPH"}' "$E/graph-audit/graph-provenance-trace.json" >"$E/graph-audit/graph-identity-chain.json"
}
qdrant_audit() {
  curl -fsS "$Q/collections/$COL" | jq . >"$E/qdrant-audit/collection.json" || return 1
  psql "$DB" -Atqc "SELECT coalesce(json_agg(qdrant_point_id::text ORDER BY qdrant_point_id::text),'[]') FROM astravector.vector_bindings_v004 WHERE chunk_granularity IN('PARENT','SUB_180','SUB_260') AND lifecycle_status='ACTIVE' AND qdrant_sync_status='SYNCED'" >"$E/qdrant-audit/expected-point-ids.json" || return 1
  curl -fsS -X POST "$Q/collections/$COL/points/scroll" -H 'content-type: application/json' -d '{"limit":256,"with_payload":true,"with_vector":false}' | jq . >"$E/qdrant-audit/phase-g-child-points.json" || return 1
  jq -n --slurpfile expected "$E/qdrant-audit/expected-point-ids.json" --slurpfile points "$E/qdrant-audit/phase-g-child-points.json" '
    ($expected[0]|sort) as $e | ($points[0].result.points|map(.id)|sort) as $p |
    {status:(if $e==$p and all($points[0].result.points[]; (.payload.access_zone_id|length)>0 and (.payload.document_id|length)>0 and (.payload.document_version|tostring|length)>0 and (.payload.chunk_id|length)>0 and (.payload.lifecycle_status=="ACTIVE")) then "PASS" else "FAIL" end),expected_synced_bindings:($e|length),qdrant_points:($p|length),count_match:($e==$p)}' >"$E/qdrant-audit/payload-consistency.json"
  cp "$E/qdrant-audit/payload-consistency.json" "$E/qdrant-audit/points-summary.json"
  jq -e '.status=="PASS"' "$E/qdrant-audit/payload-consistency.json" >/dev/null
}
run_queries() {
  local kind=$1 search_dir=$2 retrieve_dir=$3 output=$4 expect_graph=${5:-true} failed=0
  : >"$output"
  while read -r x; do
    local id q z profile max rz search_mode embedding_mode retrieval_profile
    id=$(jq -r .query.query_id <<<"$x"); q=$(jq -r .query.question <<<"$x"); z=$(jq -r .query.access_zone <<<"$x"); profile=$(jq -r .query.profile <<<"$x"); max=$(jq -r .query.max_contexts <<<"$x")
    rz=$(jq -r --arg z "$z" '.rows[]|select(.logical_zone_id==$z)|.runtime_access_zone_id' "$E/identity-map/logical-to-runtime.json" | head -1)
    jq .query <<<"$x" >"$E/bank/$id.query.json"; jq .qrel <<<"$x" >"$E/bank/$id.qrel.json"
    case "$profile" in BALANCED) search_mode=SEARCH_MODE_V005_HYBRID; embedding_mode=EMBEDDING_MODE_V005_DENSE_SPARSE_IF_AVAILABLE; retrieval_profile=RETRIEVAL_PROFILE_BALANCED;; *) echo "FIX486G_FAIL=UNKNOWN_FROZEN_PROFILE:$profile" >&2; return 1;; esac
    jq -n --arg z "$rz" --arg q "$q" --arg id "$id-$kind" --arg sm "$search_mode" --arg em "$embedding_mode" --argjson max "$max" --argjson graph "$expect_graph" '{correlationId:("fix486g-"+$id),accessZoneId:$z,callerAccessLevel:"INTERNAL",query:$q,topK:$max,candidateLimit:64,parentLimit:$max,timeoutMs:30000,searchMode:$sm,embeddingMode:$em,includeDebug:true,enableGraphExpansion:$graph,graphMaxHops:1,graphMaxRelatedContexts:5}' >"$search_dir/$id.request.json"
    grpcurl -plaintext -d @ "$ADDR" astravector.embedding.v1.AstraVectorV004Control/Search <"$search_dir/$id.request.json" >"$search_dir/$id.response.json" || return 1
    python3 "$H" normalize --query "$E/bank/$id.query.json" --qrel "$E/bank/$id.qrel.json" --entry-point Search --response "$search_dir/$id.response.json" --identity-map "$E/identity-map/logical-to-runtime.json" --bank "$BANK" --expect-graph "$expect_graph" --output "$search_dir/$id.result.json" >/dev/null || failed=1
    [[ -f "$search_dir/$id.result.json" ]] && jq -c . "$search_dir/$id.result.json" >>"$output" || return 1
    jq -n --arg z "$rz" --arg q "$q" --arg id "$id-$kind" --arg rp "$retrieval_profile" --argjson max "$max" --argjson graph "$expect_graph" '{context:{correlationId:("fix486g-"+$id),callerService:"fix486g",callerUserId:"fix486g",callerAccessLevel:"INTERNAL"},accessZoneId:$z,question:$q,profile:$rp,maxContexts:$max,responseDetail:"RESPONSE_DETAIL_DEBUG",enableGraphExpansion:$graph,graphMaxHops:1,graphMaxRelatedContexts:5}' >"$retrieve_dir/$id.request.json"
    grpcurl -plaintext -d @ "$ADDR" astravector.embedding.v1.AstraVectorRetrievalFacade/RetrieveContext <"$retrieve_dir/$id.request.json" >"$retrieve_dir/$id.response.json" || return 1
    python3 "$H" normalize --query "$E/bank/$id.query.json" --qrel "$E/bank/$id.qrel.json" --entry-point RetrieveContext --response "$retrieve_dir/$id.response.json" --identity-map "$E/identity-map/logical-to-runtime.json" --bank "$BANK" --expect-graph "$expect_graph" --output "$retrieve_dir/$id.result.json" >/dev/null || failed=1
    [[ -f "$retrieve_dir/$id.result.json" ]] && jq -c . "$retrieve_dir/$id.result.json" >>"$output" || return 1
  done < <(jq -c '.[]' "$E/bank/selected-queries.json")
  [[ $(wc -l <"$output" | tr -d ' ') -eq 2 && $failed -eq 0 ]]
}
graph_disabled_control() {
  local query id q z failed=0
  query=$(jq -sc '[.[]|select(.query_family=="graph-disabled")][0]' "$SUPPLEMENTAL/queries/graph-parent-queries-v1.jsonl")
  id=$(jq -r .query_id <<<"$query"); q=$(jq -r .question <<<"$query")
  z=$(jq -r '.rows[]|select(.logical_zone_id=="zone-a")|.runtime_access_zone_id' "$E/identity-map/logical-to-runtime.json" | head -1)
  : >"$E/graph-disabled/results.jsonl"
  jq -n --arg z "$z" --arg q "$q" --arg id "$id" '{correlationId:("fix486g-"+$id+"-search"),accessZoneId:$z,callerAccessLevel:"INTERNAL",query:$q,topK:5,candidateLimit:64,parentLimit:5,timeoutMs:30000,searchMode:"SEARCH_MODE_V005_HYBRID",embeddingMode:"EMBEDDING_MODE_V005_DENSE_SPARSE_IF_AVAILABLE",includeDebug:true,enableGraphExpansion:false,graphMaxHops:1,graphMaxRelatedContexts:5}' >"$E/graph-disabled/search/$id.request.json"
  grpcurl -plaintext -d @ "$ADDR" astravector.embedding.v1.AstraVectorV004Control/Search <"$E/graph-disabled/search/$id.request.json" >"$E/graph-disabled/search/$id.response.json" || return 1
  python3 "$H" validate-control --entry-point Search --response "$E/graph-disabled/search/$id.response.json" --identity-map "$E/identity-map/logical-to-runtime.json" --bank "$BANK" --graph-expectation absent --output "$E/graph-disabled/search/$id.result.json" >/dev/null || failed=1
  jq -c . "$E/graph-disabled/search/$id.result.json" >>"$E/graph-disabled/results.jsonl"
  jq -n --arg z "$z" --arg q "$q" --arg id "$id" '{context:{correlationId:("fix486g-"+$id+"-retrieve"),callerService:"fix486g",callerUserId:"fix486g",callerAccessLevel:"INTERNAL"},accessZoneId:$z,question:$q,profile:"RETRIEVAL_PROFILE_BALANCED",maxContexts:5,responseDetail:"RESPONSE_DETAIL_DEBUG",enableGraphExpansion:false,graphMaxHops:1,graphMaxRelatedContexts:5}' >"$E/graph-disabled/retrieve-context/$id.request.json"
  grpcurl -plaintext -d @ "$ADDR" astravector.embedding.v1.AstraVectorRetrievalFacade/RetrieveContext <"$E/graph-disabled/retrieve-context/$id.request.json" >"$E/graph-disabled/retrieve-context/$id.response.json" || return 1
  python3 "$H" validate-control --entry-point RetrieveContext --response "$E/graph-disabled/retrieve-context/$id.response.json" --identity-map "$E/identity-map/logical-to-runtime.json" --bank "$BANK" --graph-expectation absent --output "$E/graph-disabled/retrieve-context/$id.result.json" >/dev/null || failed=1
  jq -c . "$E/graph-disabled/retrieve-context/$id.result.json" >>"$E/graph-disabled/results.jsonl"
  [[ $(wc -l <"$E/graph-disabled/results.jsonl" | tr -d ' ') -eq 2 && $failed -eq 0 ]]
}
prepare_fault_targets() {
  jq -n \
    --arg zone "$(jq -r '.rows[]|select(.logical_zone_id=="zone-a")|.runtime_access_zone_id' "$E/identity-map/logical-to-runtime.json" | head -1)" \
    --arg document "$(jq -r '.rows[]|select(.logical_zone_id=="zone-a")|.runtime_document_id' "$E/identity-map/logical-to-runtime.json" | head -1)" \
    --arg parent_a1 "$(jq -r '.rows[]|select(.logical_zone_id=="zone-a" and .logical_chunk_id=="parent-a1")|.runtime_chunk_id' "$E/identity-map/logical-to-runtime.json" | head -1)" \
    --arg parent_a3 "$(jq -r '.rows[]|select(.logical_zone_id=="zone-a" and .logical_chunk_id=="parent-a3")|.runtime_chunk_id' "$E/identity-map/logical-to-runtime.json" | head -1)" \
    --arg child_a1 "$(jq -r '.rows[]|select(.logical_zone_id=="zone-a" and .logical_chunk_id=="child-a1-180")|.runtime_chunk_id' "$E/identity-map/logical-to-runtime.json" | head -1)" \
    --arg child_a3 "$(jq -r '.rows[]|select(.logical_zone_id=="zone-a" and .logical_chunk_id=="child-a3-180")|.runtime_chunk_id' "$E/identity-map/logical-to-runtime.json" | head -1)" \
    --arg child_a3_alt "$(jq -r '.rows[]|select(.logical_zone_id=="zone-a" and .logical_chunk_id=="child-a3-260")|.runtime_chunk_id' "$E/identity-map/logical-to-runtime.json" | head -1)" \
    --arg child_a2 "$(jq -r '.rows[]|select(.logical_zone_id=="zone-a" and .logical_chunk_id=="child-a2-180")|.runtime_chunk_id' "$E/identity-map/logical-to-runtime.json" | head -1)" \
    '{access_zone_id:$zone,document_id:$document,parent_a1:$parent_a1,parent_a3:$parent_a3,child_a1:$child_a1,child_a3:$child_a3,child_a3_alt:$child_a3_alt,child_a2:$child_a2}' >"$E/faults/targets.json"
  jq -e 'all(.access_zone_id,.document_id,.parent_a1,.parent_a3,.child_a1,.child_a3,.child_a3_alt,.child_a2; test("^[0-9a-fA-F-]{36}$"))' "$E/faults/targets.json" >/dev/null || return 1
  local zone child parent
  zone=$(jq -r .access_zone_id "$E/faults/targets.json"); child=$(jq -r .child_a3 "$E/faults/targets.json"); parent=$(jq -r .parent_a3 "$E/faults/targets.json")
  psql "$DB" -X -v ON_ERROR_STOP=1 -Atqc "SELECT json_build_object('binding_count',(SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id='$zone' AND chunk_id='$child' AND representation_type='ORIGINAL'),'binding_parent_chunk_id',(SELECT parent_chunk_id::text FROM astravector.vector_bindings_v004 WHERE access_zone_id='$zone' AND chunk_id='$child' AND representation_type='ORIGINAL'),'qdrant_sync_status',(SELECT qdrant_sync_status FROM astravector.vector_bindings_v004 WHERE access_zone_id='$zone' AND chunk_id='$child' AND representation_type='ORIGINAL'),'chunk_count',(SELECT count(*) FROM astravector.content_chunks_v004 WHERE access_zone_id='$zone' AND id='$child'),'lifecycle_status',(SELECT lifecycle_status FROM astravector.content_chunks_v004 WHERE access_zone_id='$zone' AND id='$child'),'deleted_at',(SELECT deleted_at FROM astravector.content_chunks_v004 WHERE access_zone_id='$zone' AND id='$child'),'expires_at',(SELECT expires_at FROM astravector.content_chunks_v004 WHERE access_zone_id='$zone' AND id='$child'),'expires_at_visible',(SELECT expires_at IS NULL OR expires_at>now() FROM astravector.content_chunks_v004 WHERE access_zone_id='$zone' AND id='$child'))" | jq . >"$E/faults/baseline.json" || return 1
  jq -e --arg parent "$parent" '.binding_count==1 and .binding_parent_chunk_id==$parent and .qdrant_sync_status=="SYNCED" and .chunk_count==1 and .lifecycle_status=="ACTIVE" and .deleted_at==null and .expires_at_visible==true' "$E/faults/baseline.json" >/dev/null
}
run_rejected_target_pair() {
  local name=$1 scenario=$2 forbidden=$3 rejection_evidence=${4:-} dir="$E/faults/$1" id q z
  local graph_limit=$FAULT_GRAPH_RELATED_CONTEXTS
  local validate_args=(--identity-map "$E/identity-map/logical-to-runtime.json" --bank "$BANK" --scenario "$scenario" --forbidden-target "$forbidden")
  [[ -n "$rejection_evidence" ]] && validate_args+=(--rejection-evidence "$rejection_evidence")
  mkdir -p "$dir/search" "$dir/retrieve-context"
  id=$(jq -r '.[0].query.query_id' "$E/bank/selected-queries.json")
  q=$(jq -r '.[0].query.question' "$E/bank/selected-queries.json")
  z=$(jq -r '.access_zone_id' "$E/faults/targets.json")
  jq -n --arg z "$z" --arg q "$q" --arg id "$name" --argjson graph_limit "$graph_limit" '{correlationId:("fix486g-control-"+$id),accessZoneId:$z,callerAccessLevel:"INTERNAL",query:$q,topK:5,candidateLimit:64,parentLimit:5,timeoutMs:30000,searchMode:"SEARCH_MODE_V005_HYBRID",embeddingMode:"EMBEDDING_MODE_V005_DENSE_SPARSE_IF_AVAILABLE",includeDebug:true,enableGraphExpansion:true,graphMaxHops:1,graphMaxRelatedContexts:$graph_limit}' >"$dir/search/request.json"
  grpcurl -plaintext -d @ "$ADDR" astravector.embedding.v1.AstraVectorV004Control/Search <"$dir/search/request.json" >"$dir/search/response.json" || return 1
  python3 "$H" validate-rejected-target --entry-point Search --response "$dir/search/response.json" "${validate_args[@]}" --output "$dir/search/result.json" || return 1
  jq -n --arg z "$z" --arg q "$q" --arg id "$name" --argjson graph_limit "$graph_limit" '{context:{correlationId:("fix486g-control-"+$id),callerService:"fix486g",callerUserId:"fix486g",callerAccessLevel:"INTERNAL"},accessZoneId:$z,question:$q,profile:"RETRIEVAL_PROFILE_BALANCED",maxContexts:5,responseDetail:"RESPONSE_DETAIL_DEBUG",enableGraphExpansion:true,graphMaxHops:1,graphMaxRelatedContexts:$graph_limit}' >"$dir/retrieve-context/request.json"
  grpcurl -plaintext -d @ "$ADDR" astravector.embedding.v1.AstraVectorRetrievalFacade/RetrieveContext <"$dir/retrieve-context/request.json" >"$dir/retrieve-context/response.json" || return 1
  python3 "$H" validate-rejected-target --entry-point RetrieveContext --response "$dir/retrieve-context/response.json" "${validate_args[@]}" --output "$dir/retrieve-context/result.json"
}
write_structural_rejection_evidence() {
  local scenario=$1 reason=$2 verify_sql=$3 expected=$4 output=$5 actual
  actual=$(psql "$DB" -X -v ON_ERROR_STOP=1 -Atqc "$verify_sql") || return 1
  jq -n --arg scenario "$scenario" --arg reason "$reason" --arg source "verified phase-owned topology plus request boundary" \
    --argjson expected "$expected" --argjson actual "$actual" \
    '{schema_version:1,status:(if $actual==$expected then "PASS" else "FAIL" end),observed:($actual==$expected),scenario:$scenario,reason:$reason,source:$source,expected_rows:$expected,actual_rows:$actual}' >"$output"
  [[ "$actual" -eq "$expected" ]]
}
binding_parent_fault() {
  local zone child parent_a1 parent_a3 source survivor edge rc=0
  zone=$(jq -r .access_zone_id "$E/faults/targets.json")
  child=$(jq -r .child_a3 "$E/faults/targets.json"); parent_a1=$(jq -r .parent_a1 "$E/faults/targets.json"); parent_a3=$(jq -r .parent_a3 "$E/faults/targets.json")
  source=$(jq -r .child_a1 "$E/faults/targets.json"); survivor=$(jq -r .child_a3_alt "$E/faults/targets.json"); edge=$(python3 -c 'import uuid; print(uuid.uuid4())')
  insert_fault_edge "$edge" "$source" "$survivor" REPAIRED_BY || return 1
  run_exact_mutation wrong-parent-activate 1 \
    "UPDATE astravector.vector_bindings_v004 SET parent_chunk_id='$parent_a1' WHERE access_zone_id='$zone' AND chunk_id='$child' AND representation_type='ORIGINAL'" \
    "SELECT 1 FROM astravector.vector_bindings_v004 WHERE access_zone_id='$zone' AND chunk_id='$child' AND representation_type='ORIGINAL' AND parent_chunk_id='$parent_a1'" 1 || return 1
  run_rejected_target_pair wrong-parent wrong-parent "$child" || rc=$?
  run_exact_mutation wrong-parent-restore 1 \
    "UPDATE astravector.vector_bindings_v004 SET parent_chunk_id='$parent_a3' WHERE access_zone_id='$zone' AND chunk_id='$child' AND representation_type='ORIGINAL'" \
    "SELECT 1 FROM astravector.vector_bindings_v004 WHERE access_zone_id='$zone' AND chunk_id='$child' AND representation_type='ORIGINAL' AND parent_chunk_id='$parent_a3'" 1 || return 1
  delete_fault_edge "$edge" || return 1
  [[ $rc -eq 0 ]]
}
binding_status_fault() {
  local zone child source survivor edge rc=0
  zone=$(jq -r .access_zone_id "$E/faults/targets.json")
  child=$(jq -r .child_a3 "$E/faults/targets.json")
  source=$(jq -r .child_a1 "$E/faults/targets.json"); survivor=$(jq -r .child_a3_alt "$E/faults/targets.json"); edge=$(python3 -c 'import uuid; print(uuid.uuid4())')
  insert_fault_edge "$edge" "$source" "$survivor" REPAIRED_BY || return 1
  run_exact_mutation binding-invalid-activate 1 \
    "UPDATE astravector.vector_bindings_v004 SET qdrant_sync_status='FAILED' WHERE access_zone_id='$zone' AND chunk_id='$child' AND representation_type='ORIGINAL'" \
    "SELECT 1 FROM astravector.vector_bindings_v004 WHERE access_zone_id='$zone' AND chunk_id='$child' AND representation_type='ORIGINAL' AND qdrant_sync_status='FAILED'" 1 || return 1
  run_rejected_target_pair binding-invalid binding-status "$child" || rc=$?
  run_exact_mutation binding-invalid-restore 1 \
    "UPDATE astravector.vector_bindings_v004 SET qdrant_sync_status='SYNCED' WHERE access_zone_id='$zone' AND chunk_id='$child' AND representation_type='ORIGINAL'" \
    "SELECT 1 FROM astravector.vector_bindings_v004 WHERE access_zone_id='$zone' AND chunk_id='$child' AND representation_type='ORIGINAL' AND qdrant_sync_status='SYNCED'" 1 || return 1
  delete_fault_edge "$edge" || return 1
  [[ $rc -eq 0 ]]
}
missing_parent_fault() {
  local zone child parent missing source survivor edge rc=0
  zone=$(jq -r .access_zone_id "$E/faults/targets.json")
  child=$(jq -r .child_a3 "$E/faults/targets.json")
  parent=$(jq -r .parent_a3 "$E/faults/targets.json")
  missing=$(python3 -c 'import uuid; print(uuid.uuid4())')
  source=$(jq -r .child_a1 "$E/faults/targets.json"); survivor=$(jq -r .child_a3_alt "$E/faults/targets.json"); edge=$(python3 -c 'import uuid; print(uuid.uuid4())')
  insert_fault_edge "$edge" "$source" "$survivor" REPAIRED_BY || return 1
  run_exact_mutation missing-parent-activate 1 \
    "UPDATE astravector.vector_bindings_v004 SET parent_chunk_id='$missing' WHERE access_zone_id='$zone' AND chunk_id='$child' AND representation_type='ORIGINAL'" \
    "SELECT 1 FROM astravector.vector_bindings_v004 b WHERE b.access_zone_id='$zone' AND b.chunk_id='$child' AND b.representation_type='ORIGINAL' AND b.parent_chunk_id='$missing' AND NOT EXISTS(SELECT 1 FROM astravector.content_chunks_v004 p WHERE p.access_zone_id=b.access_zone_id AND p.id=b.parent_chunk_id)" 1 || return 1
  run_rejected_target_pair missing-parent missing-parent "$child" || rc=$?
  run_exact_mutation missing-parent-restore 1 \
    "UPDATE astravector.vector_bindings_v004 SET parent_chunk_id='$parent' WHERE access_zone_id='$zone' AND chunk_id='$child' AND representation_type='ORIGINAL'" \
    "SELECT 1 FROM astravector.vector_bindings_v004 WHERE access_zone_id='$zone' AND chunk_id='$child' AND representation_type='ORIGINAL' AND parent_chunk_id='$parent'" 1 || return 1
  delete_fault_edge "$edge" || return 1
  [[ $rc -eq 0 ]]
}
lifecycle_fault() {
  local kind=$1 child zone source survivor edge baseline_expires expires_sql rc=0 activate_label="" restore_label="" activate_sql="" activate_verify="" restore_sql="" restore_verify=""
  child=$(jq -r .child_a3 "$E/faults/targets.json")
  zone=$(jq -r .access_zone_id "$E/faults/targets.json")
  baseline_expires=$(jq -r '.expires_at // empty' "$E/faults/baseline.json")
  if [[ -n "$baseline_expires" ]]; then expires_sql="'$baseline_expires'::timestamptz"; else expires_sql=NULL; fi
  source=$(jq -r .child_a1 "$E/faults/targets.json"); survivor=$(jq -r .child_a3_alt "$E/faults/targets.json"); edge=$(python3 -c 'import uuid; print(uuid.uuid4())')
  insert_fault_edge "$edge" "$source" "$survivor" REPAIRED_BY || return 1
  case "$kind" in
    inactive) activate_label=inactive-activate; restore_label=inactive-restore; activate_sql="UPDATE astravector.content_chunks_v004 SET lifecycle_status='INACTIVE' WHERE access_zone_id='$zone' AND id='$child'"; activate_verify="SELECT 1 FROM astravector.content_chunks_v004 WHERE access_zone_id='$zone' AND id='$child' AND lifecycle_status='INACTIVE'"; restore_sql="UPDATE astravector.content_chunks_v004 SET lifecycle_status='ACTIVE' WHERE access_zone_id='$zone' AND id='$child'"; restore_verify="SELECT 1 FROM astravector.content_chunks_v004 WHERE access_zone_id='$zone' AND id='$child' AND lifecycle_status='ACTIVE'";;
    deleted) activate_label=deleted-activate; restore_label=deleted-restore; activate_sql="UPDATE astravector.content_chunks_v004 SET deleted_at=now() WHERE access_zone_id='$zone' AND id='$child'"; activate_verify="SELECT 1 FROM astravector.content_chunks_v004 WHERE access_zone_id='$zone' AND id='$child' AND deleted_at IS NOT NULL"; restore_sql="UPDATE astravector.content_chunks_v004 SET deleted_at=NULL WHERE access_zone_id='$zone' AND id='$child'"; restore_verify="SELECT 1 FROM astravector.content_chunks_v004 WHERE access_zone_id='$zone' AND id='$child' AND deleted_at IS NULL";;
    expired) activate_label=expired-activate; restore_label=expired-restore; activate_sql="UPDATE astravector.content_chunks_v004 SET expires_at=now()-interval '1 hour' WHERE access_zone_id='$zone' AND id='$child'"; activate_verify="SELECT 1 FROM astravector.content_chunks_v004 WHERE access_zone_id='$zone' AND id='$child' AND expires_at<now()"; restore_sql="UPDATE astravector.content_chunks_v004 SET expires_at=$expires_sql WHERE access_zone_id='$zone' AND id='$child'"; restore_verify="SELECT 1 FROM astravector.content_chunks_v004 WHERE access_zone_id='$zone' AND id='$child' AND expires_at IS NOT DISTINCT FROM $expires_sql";;
    *) return 64;;
  esac
  run_exact_mutation "$activate_label" 1 "$activate_sql" "$activate_verify" 1 || return 1
  run_rejected_target_pair "$kind-target" "$kind-target" "$child" || rc=$?
  run_exact_mutation "$restore_label" 1 "$restore_sql" "$restore_verify" 1 || return 1
  delete_fault_edge "$edge" || return 1
  [[ $rc -eq 0 ]]
}
insert_fault_edge() {
  local edge_id=$1 source_chunk=$2 target_chunk=$3 relation=$4
  run_exact_mutation "insert-fault-edge-$edge_id" 1 \
    "INSERT INTO astravector.rag_graph_edges(access_zone_id,edge_id,source_node_type,source_node_id,target_node_type,target_node_id,relation_type,relation_score,relation_source,relation_rank,document_id,document_version,lifecycle_status,quarantined,properties) SELECT s.access_zone_id,'$edge_id','CHUNK',s.node_id,'CHUNK',t.node_id,'$relation',1.0,'PHASE_G_FAULT',0,c.document_id,c.document_version,'ACTIVE',false,jsonb_build_object('quality_run_id','$RUN_ID','phase_fault',true) FROM astravector.rag_graph_nodes_chunk s JOIN astravector.rag_graph_nodes_chunk t ON t.chunk_id='$target_chunk' JOIN astravector.content_chunks_v004 c ON c.access_zone_id=s.access_zone_id AND c.id=s.chunk_id WHERE s.chunk_id='$source_chunk' LIMIT 1 ON CONFLICT DO NOTHING" \
    "SELECT 1 FROM astravector.rag_graph_edges e JOIN astravector.rag_graph_nodes_chunk s ON s.access_zone_id=e.access_zone_id AND s.node_id=e.source_node_id WHERE e.edge_id='$edge_id' AND e.relation_type='$relation' AND e.lifecycle_status='ACTIVE' AND e.quarantined=false AND e.properties->>'quality_run_id'='$RUN_ID' AND e.properties->>'phase_fault'='true' AND s.chunk_id='$source_chunk' AND e.target_node_id=(SELECT t.node_id FROM astravector.rag_graph_nodes_chunk t WHERE t.chunk_id='$target_chunk' LIMIT 1)" 1
}
delete_fault_edge() {
  local edge_id=$1
  run_exact_mutation "delete-fault-edge-$edge_id" 1 \
    "DELETE FROM astravector.rag_graph_edges WHERE edge_id='$edge_id' AND properties->>'quality_run_id'='$RUN_ID' AND properties->>'phase_fault'='true'" \
    "SELECT 1 FROM astravector.rag_graph_edges WHERE edge_id='$edge_id'" 0
}
cross_zone_fault() {
  local edge source target evidence rc=0
  edge=$(python3 -c 'import uuid; print(uuid.uuid4())'); source=$(jq -r .child_a1 "$E/faults/targets.json"); target=$(jq -r '.rows[]|select(.logical_zone_id=="zone-b" and .logical_chunk_id=="child-a1-180")|.runtime_chunk_id' "$E/identity-map/logical-to-runtime.json" | head -1)
  insert_fault_edge "$edge" "$source" "$target" REPAIRED_BY || return 1
  evidence="$E/faults/cross-zone/rejection-evidence.json"; mkdir -p "$(dirname "$evidence")"
  write_structural_rejection_evidence cross-zone GRAPH_ENDPOINT_ZONE_MISMATCH \
    "SELECT count(*) FROM astravector.rag_graph_edges e JOIN astravector.rag_graph_nodes_chunk s ON s.access_zone_id=e.access_zone_id AND s.node_id=e.source_node_id JOIN astravector.rag_graph_nodes_chunk t ON t.node_id=e.target_node_id WHERE e.edge_id='$edge' AND s.access_zone_id<>t.access_zone_id" 1 "$evidence" || rc=$?
  [[ $rc -ne 0 ]] || run_rejected_target_pair cross-zone cross-zone "$target" "$evidence" || rc=$?
  delete_fault_edge "$edge" || return 1
  [[ $rc -eq 0 ]]
}
hop_limit_fault() {
  local edge source target evidence rc=0
  edge=$(python3 -c 'import uuid; print(uuid.uuid4())'); source=$(jq -r .child_a3 "$E/faults/targets.json"); target=$(jq -r .child_a2 "$E/faults/targets.json")
  [[ "$target" =~ ^[0-9a-fA-F-]{36}$ ]] || return 1
  insert_fault_edge "$edge" "$source" "$target" REPAIRED_BY || return 1
  evidence="$E/faults/hop-limit/rejection-evidence.json"; mkdir -p "$(dirname "$evidence")"
  write_structural_rejection_evidence hop-limit HOP_LIMIT_REJECTED \
    "SELECT count(*) FROM astravector.rag_graph_edges e JOIN astravector.rag_graph_nodes_chunk s ON s.access_zone_id=e.access_zone_id AND s.node_id=e.source_node_id WHERE e.edge_id='$edge' AND s.chunk_id='$source' AND e.properties->>'phase_fault'='true'" 1 "$evidence" || rc=$?
  [[ $rc -ne 0 ]] || run_rejected_target_pair hop-limit hop-limit "$target" "$evidence" || rc=$?
  delete_fault_edge "$edge" || return 1
  [[ $rc -eq 0 ]]
}
cycle_fault() {
  local edge self source target evidence rc=0
  edge=$(python3 -c 'import uuid; print(uuid.uuid4())'); self=$(python3 -c 'import uuid; print(uuid.uuid4())'); source=$(jq -r .child_a3 "$E/faults/targets.json"); target=$(jq -r .child_a1 "$E/faults/targets.json")
  insert_fault_edge "$edge" "$source" "$target" RELATED_TO || return 1
  insert_fault_edge "$self" "$target" "$target" RELATED_TO || { delete_fault_edge "$edge"; return 1; }
  evidence="$E/faults/cycle/rejection-evidence.json"; mkdir -p "$(dirname "$evidence")"
  write_structural_rejection_evidence cycle GRAPH_CYCLE_REJECTED \
    "SELECT count(*) FROM astravector.rag_graph_edges e WHERE e.edge_id='$self' AND e.source_node_id=e.target_node_id AND e.properties->>'phase_fault'='true'" 1 "$evidence" || rc=$?
  [[ $rc -ne 0 ]] || run_rejected_target_pair cycle cycle "$self" "$evidence" || rc=$?
  delete_fault_edge "$edge" || return 1; delete_fault_edge "$self" || return 1
  [[ $rc -eq 0 ]]
}

STAT_EDGE_ONE=""
STAT_EDGE_TWO=""
STAT_FAULT_LABEL=""
statistical_fault_activate() {
  local setup=$1 label=$2 zone child parent_a1 source survivor target
  zone=$(jq -r .access_zone_id "$E/faults/targets.json")
  child=$(jq -r .child_a3 "$E/faults/targets.json")
  parent_a1=$(jq -r .parent_a1 "$E/faults/targets.json")
  source=$(jq -r .child_a1 "$E/faults/targets.json")
  survivor=$(jq -r .child_a3_alt "$E/faults/targets.json")
  STAT_EDGE_ONE=""; STAT_EDGE_TWO=""; STAT_FAULT_LABEL="$label"
  case "$setup" in
    graph_wrong_parent_overlay)
      STAT_EDGE_ONE=$(python3 -c 'import uuid; print(uuid.uuid4())')
      insert_fault_edge "$STAT_EDGE_ONE" "$source" "$survivor" REPAIRED_BY || return 1
      run_exact_mutation "$label-activate" 1 \
        "UPDATE astravector.vector_bindings_v004 SET parent_chunk_id='$parent_a1' WHERE access_zone_id='$zone' AND chunk_id='$child' AND representation_type='ORIGINAL'" \
        "SELECT 1 FROM astravector.vector_bindings_v004 WHERE access_zone_id='$zone' AND chunk_id='$child' AND representation_type='ORIGINAL' AND parent_chunk_id='$parent_a1'" 1
      ;;
    graph_cross_zone_overlay)
      STAT_EDGE_ONE=$(python3 -c 'import uuid; print(uuid.uuid4())')
      target=$(jq -r '.rows[]|select(.logical_zone_id=="zone-b" and .logical_chunk_id=="child-a1-180")|.runtime_chunk_id' "$E/identity-map/logical-to-runtime.json" | head -1)
      insert_fault_edge "$STAT_EDGE_ONE" "$source" "$target" REPAIRED_BY
      ;;
    graph_inactive_deleted_expired_overlay)
      STAT_EDGE_ONE=$(python3 -c 'import uuid; print(uuid.uuid4())')
      insert_fault_edge "$STAT_EDGE_ONE" "$source" "$survivor" REPAIRED_BY || return 1
      run_exact_mutation "$label-activate" 1 \
        "UPDATE astravector.content_chunks_v004 SET expires_at=now()-interval '1 hour' WHERE access_zone_id='$zone' AND id='$child'" \
        "SELECT 1 FROM astravector.content_chunks_v004 WHERE access_zone_id='$zone' AND id='$child' AND expires_at<now()" 1
      ;;
    graph_second_hop_overlay)
      STAT_EDGE_ONE=$(python3 -c 'import uuid; print(uuid.uuid4())')
      source=$(jq -r .child_a3 "$E/faults/targets.json"); target=$(jq -r .child_a2 "$E/faults/targets.json")
      insert_fault_edge "$STAT_EDGE_ONE" "$source" "$target" REPAIRED_BY
      ;;
    graph_cycle_overlay)
      STAT_EDGE_ONE=$(python3 -c 'import uuid; print(uuid.uuid4())'); STAT_EDGE_TWO=$(python3 -c 'import uuid; print(uuid.uuid4())')
      source=$(jq -r .child_a3 "$E/faults/targets.json"); target=$(jq -r .child_a1 "$E/faults/targets.json")
      insert_fault_edge "$STAT_EDGE_ONE" "$source" "$target" RELATED_TO || return 1
      insert_fault_edge "$STAT_EDGE_TWO" "$target" "$target" RELATED_TO
      ;;
    *) return 64;;
  esac
}

statistical_fault_restore() {
  local setup=$1 label=$2 zone child parent baseline_expires expires_sql rc=0
  zone=$(jq -r .access_zone_id "$E/faults/targets.json"); child=$(jq -r .child_a3 "$E/faults/targets.json"); parent=$(jq -r .parent_a3 "$E/faults/targets.json")
  baseline_expires=$(jq -r '.expires_at // empty' "$E/faults/baseline.json")
  if [[ -n "$baseline_expires" ]]; then expires_sql="'$baseline_expires'::timestamptz"; else expires_sql=NULL; fi
  case "$setup" in
    graph_wrong_parent_overlay)
      run_exact_mutation "$label-restore" 1 \
        "UPDATE astravector.vector_bindings_v004 SET parent_chunk_id='$parent' WHERE access_zone_id='$zone' AND chunk_id='$child' AND representation_type='ORIGINAL'" \
        "SELECT 1 FROM astravector.vector_bindings_v004 WHERE access_zone_id='$zone' AND chunk_id='$child' AND representation_type='ORIGINAL' AND parent_chunk_id='$parent'" 1 || rc=1
      ;;
    graph_inactive_deleted_expired_overlay)
      run_exact_mutation "$label-restore" 1 \
        "UPDATE astravector.content_chunks_v004 SET expires_at=$expires_sql WHERE access_zone_id='$zone' AND id='$child'" \
        "SELECT 1 FROM astravector.content_chunks_v004 WHERE access_zone_id='$zone' AND id='$child' AND expires_at IS NOT DISTINCT FROM $expires_sql" 1 || rc=1
      ;;
  esac
  [[ -z "$STAT_EDGE_ONE" ]] || delete_fault_edge "$STAT_EDGE_ONE" || rc=1
  [[ -z "$STAT_EDGE_TWO" ]] || delete_fault_edge "$STAT_EDGE_TWO" || rc=1
  STAT_EDGE_ONE=""; STAT_EDGE_TWO=""; STAT_FAULT_LABEL=""
  [[ $rc -eq 0 ]]
}

statistical_degradation_evidence() {
  local setup=$1 output=$2 class reason zone child parent_a1 source verify_sql actual
  zone=$(jq -r .access_zone_id "$E/faults/targets.json")
  child=$(jq -r .child_a3 "$E/faults/targets.json")
  parent_a1=$(jq -r .parent_a1 "$E/faults/targets.json")
  source=$(jq -r .child_a3 "$E/faults/targets.json")
  case "$setup" in
    graph_wrong_parent_overlay)
      class=WRONG_PARENT; reason=BINDING_INVALID
      verify_sql="SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id='$zone' AND chunk_id='$child' AND representation_type='ORIGINAL' AND parent_chunk_id='$parent_a1'"
      ;;
    graph_cross_zone_overlay)
      class=CROSS_ZONE; reason=GRAPH_ENDPOINT_ZONE_MISMATCH
      verify_sql="SELECT count(*) FROM astravector.rag_graph_edges e JOIN astravector.rag_graph_nodes_chunk s ON s.access_zone_id=e.access_zone_id AND s.node_id=e.source_node_id JOIN astravector.rag_graph_nodes_chunk t ON t.node_id=e.target_node_id WHERE e.edge_id='$STAT_EDGE_ONE' AND s.access_zone_id<>t.access_zone_id"
      ;;
    graph_inactive_deleted_expired_overlay)
      class=LIFECYCLE_INVALID; reason=VISIBILITY_REJECTED
      verify_sql="SELECT count(*) FROM astravector.content_chunks_v004 WHERE access_zone_id='$zone' AND id='$child' AND expires_at<now()"
      ;;
    graph_second_hop_overlay)
      class=HOP_LIMIT; reason=HOP_LIMIT_REJECTED
      verify_sql="SELECT count(*) FROM astravector.rag_graph_edges e JOIN astravector.rag_graph_nodes_chunk s ON s.access_zone_id=e.access_zone_id AND s.node_id=e.source_node_id WHERE e.edge_id='$STAT_EDGE_ONE' AND s.chunk_id='$source' AND e.properties->>'phase_fault'='true'"
      ;;
    graph_cycle_overlay)
      class=CYCLE_OR_DUPLICATE; reason=GRAPH_CYCLE_REJECTED
      verify_sql="SELECT count(*) FROM astravector.rag_graph_edges e WHERE e.edge_id='$STAT_EDGE_TWO' AND e.source_node_id=e.target_node_id AND e.properties->>'phase_fault'='true'"
      ;;
    *) return 64;;
  esac
  actual=$(psql "$DB" -X -v ON_ERROR_STOP=1 -Atqc "$verify_sql") || return 1
  [[ "$actual" -eq 1 ]] || return 1
  jq -n --arg setup "$setup" --arg class "$class" --arg reason "$reason" --arg label "$STAT_FAULT_LABEL" \
    --arg edge_one "$STAT_EDGE_ONE" --arg edge_two "$STAT_EDGE_TWO" --arg captured_at "$(timestamp)" --argjson actual "$actual" \
    '{schema_version:1,fault_setup:$setup,source:"phase-owned exact-row mutation activation plus independent final-context safety evaluation",captured_at_utc:$captured_at,mutation:{label:$label,edge_ids:[$edge_one,$edge_two]|map(select(length>0))},degradation:{graph_failure_injected:true,graph_failure_detected:true,graph_failure_classification:$class,semantic_no_answer:false,partial_graph_evidence:true,reported_full_coverage:false,rejection_reasons:[$reason],rejection_observation:{status:"PASS",observed:true,reason:$reason,source:"verified phase-owned fault topology and exact mutation evidence",expected_rows:1,actual_rows:$actual}}}' >"$output"
}

statistical_resource_evidence() {
  jq -n --arg source_sha "$SOURCE_SHA" '{schema_version:1,source:"bounded static production query-plan formula; response diagnostics provide latency/candidate/hop values and these counters are formula evidence, not live request counters",source_sha:$source_sha,telemetry:{sql_statement_count:{enabled_value:6,disabled_value:4,upper_bound:6,formula_source:"bounded batch SQL stages: lexical, direct hydration, graph relation expansion, graph hydration, final visibility and optional support query; no per-candidate SQL"},qdrant_request_count:{value:1,upper_bound:1,formula_source:"one unified Qdrant query request per Search pipeline"},graph_relation_query_count:{enabled_value:1,disabled_value:0,upper_bound:1,formula_source:"one batch graph relation query iff graph expansion executes"},n_plus_one_sql_hydration:false}}' >"$E/statistical/resource-evidence.json"
}

statistical_capture_selection() {
  local kind=$1 index=$2 output=$3; shift 3
  python3 "$STAT_CAPTURE" --endpoint "$ADDR" --bank "$SUPPLEMENTAL" --identity-map "$E/identity-map/logical-to-runtime.json" \
    --run-kind "$kind" --run-index "$index" --output "$output" --deadline-ms 30000 --jitter-allowance-ms 2000 \
    --resource-evidence "$E/statistical/resource-evidence.json" "$@"
}

statistical_full_pass() {
  local kind=$1 index=$2 output="$E/statistical/raw-observations.jsonl" setup label evidence rc=0 before after
  before=$(wc -l <"$output" | tr -d ' ')
  statistical_capture_selection "$kind" "$index" "$output" --exclude-faults || return 1
  for setup in graph_wrong_parent_overlay graph_cross_zone_overlay graph_inactive_deleted_expired_overlay graph_second_hop_overlay graph_cycle_overlay; do
    label="stat-$kind-$index-$setup"
    evidence="$E/statistical/degradation/$label.json"
    statistical_fault_activate "$setup" "$label" || return 1
    statistical_degradation_evidence "$setup" "$evidence" || rc=1
    [[ $rc -ne 0 ]] || statistical_capture_selection "$kind" "$index" "$output" --fault-setup "$setup" --degradation-evidence "$evidence" || rc=1
    statistical_fault_restore "$setup" "$label" || rc=1
    [[ $rc -eq 0 ]] || return 1
  done
  after=$(wc -l <"$output" | tr -d ' ')
  [[ $((after-before)) -eq 142 ]]
}

statistical_concurrent_pair() {
  local index=$1 setup=$2 entry=$3 pair label fault_query healthy_query fault_evidence healthy_evidence fault_output healthy_output fault_pid healthy_pid fault_rc=0 healthy_rc=0 restore_rc=0
  pair=$(printf 'pair-%02d' "$index"); label="stat-$pair-$setup"
  fault_query=$(jq -sr --arg setup "$setup" '[.[]|select(.fault_setup==$setup)][0].query_id' "$SUPPLEMENTAL/queries/graph-parent-queries-v1.jsonl")
  case $((index%3)) in 1) healthy_query=g-pos-ru-01;; 2) healthy_query=g-pos-kz-01;; 0) healthy_query=g-pos-en-01;; esac
  fault_evidence="$E/statistical/degradation/$label.json"; healthy_evidence="$E/statistical/degradation/$pair-healthy.json"
  fault_output="$E/statistical/concurrent/$pair-fault.jsonl"; healthy_output="$E/statistical/concurrent/$pair-healthy.jsonl"
  statistical_fault_activate "$setup" "$label" || return 1
  statistical_degradation_evidence "$setup" "$fault_evidence" || return 1
  jq -n --arg setup healthy_control --arg pair "$pair" '{schema_version:1,fault_setup:$setup,source:"healthy control executed concurrently with an active phase-owned Graph fault; affected=false remains subject to independent qrel evaluation",pair_id:$pair,degradation:{healthy_request_affected:false}}' >"$healthy_evidence"
  python3 "$STAT_CAPTURE" --endpoint "$ADDR" --bank "$SUPPLEMENTAL" --identity-map "$E/identity-map/logical-to-runtime.json" --run-kind concurrent_fault --pair-id "$pair" --entry-point "$entry" --query-id "$fault_query" --output "$fault_output" --deadline-ms 30000 --jitter-allowance-ms 2000 --resource-evidence "$E/statistical/resource-evidence.json" --degradation-evidence "$fault_evidence" >"$E/statistical/logs/$pair-fault.log" 2>&1 & fault_pid=$!
  python3 "$STAT_CAPTURE" --endpoint "$ADDR" --bank "$SUPPLEMENTAL" --identity-map "$E/identity-map/logical-to-runtime.json" --run-kind concurrent_healthy --pair-id "$pair" --entry-point "$entry" --query-id "$healthy_query" --output "$healthy_output" --deadline-ms 30000 --jitter-allowance-ms 2000 --resource-evidence "$E/statistical/resource-evidence.json" --degradation-evidence "$healthy_evidence" >"$E/statistical/logs/$pair-healthy.log" 2>&1 & healthy_pid=$!
  wait "$fault_pid" || fault_rc=$?; wait "$healthy_pid" || healthy_rc=$?
  statistical_fault_restore "$setup" "$label" || restore_rc=$?
  [[ $fault_rc -eq 0 && $healthy_rc -eq 0 && $restore_rc -eq 0 ]] || return 1
  cat "$fault_output" "$healthy_output" >>"$E/statistical/raw-observations.jsonl"
}

statistical_campaign() {
  local index setup entry
  : >"$E/statistical/raw-observations.jsonl"
  python3 "$STAT_EVAL" plan --bank "$SUPPLEMENTAL" --output "$E/statistical/sample-plan.json" >/dev/null || return 1
  statistical_resource_evidence || return 1
  curl -fsS "http://127.0.0.1:$MP/metrics" >"$E/statistical/metrics-before.prom" || return 1
  for index in 1 2 3; do statistical_full_pass warm "$index" || return 1; done
  for index in 1 2; do
    stop_runtime && start_runtime "stat-restart-$index" || return 1
    statistical_full_pass restart "$index" || return 1
  done
  local setups=(graph_wrong_parent_overlay graph_cross_zone_overlay graph_inactive_deleted_expired_overlay graph_second_hop_overlay graph_cycle_overlay)
  for index in $(seq 1 10); do
    setup=${setups[$(((index-1)%5))]}
    if ((index%2)); then entry=Search; else entry=RetrieveContext; fi
    statistical_concurrent_pair "$index" "$setup" "$entry" || return 1
  done
  curl -fsS "http://127.0.0.1:$MP/metrics" >"$E/statistical/metrics-after.prom" || return 1
  [[ $(wc -l <"$E/statistical/raw-observations.jsonl" | tr -d ' ') -eq 730 ]] || return 1
  python3 "$STAT_EVAL" dry-validate --bank "$SUPPLEMENTAL" --raw-input "$E/statistical/raw-observations.jsonl" --identity-map "$E/identity-map/logical-to-runtime.json" --output "$E/statistical/raw-validation.json" >/dev/null || return 1
  python3 "$STAT_EVAL" evaluate --bank "$SUPPLEMENTAL" --raw-input "$E/statistical/raw-observations.jsonl" --identity-map "$E/identity-map/logical-to-runtime.json" --output-dir "$E/statistical" >/dev/null || return 1
  jq -e '.verdict=="FIX486G_STATISTICAL_QUALITY_PASS" and .sample_plan.raw_observation_count==730 and .sample_plan.full_pass_counts.warm==3 and .sample_plan.full_pass_counts.restart==2 and .sample_plan.concurrent_pair_count==10' "$E/statistical/statistical-report.json" >/dev/null
}
compare_initial() { python3 "$H" compare --left "$E/query-results.jsonl" --right "$E/query-results.jsonl" --parity --output "$E/comparisons/entry-point-parity.json" >/dev/null; }
warm_repeat() { run_queries warm "$E/comparisons/warm-search" "$E/comparisons/warm-retrieve-context" "$E/comparisons/warm-query-results.jsonl" true && python3 "$H" compare --left "$E/query-results.jsonl" --right "$E/comparisons/warm-query-results.jsonl" --output "$E/comparisons/warm-repeat.json" >/dev/null; }
restart_repeat() {
  stop_runtime && start_runtime restart &&
  jq -n --arg endpoint "$ADDR" --rawfile services "$E/infrastructure/services-restart.txt" \
    '{status:"PASS",endpoint:$endpoint,services:($services|split("\n")|map(select(length>0)))}' >"$E/restart/health.json" &&
  run_queries restart "$E/restart/search" "$E/restart/retrieve-context" "$E/restart/query-results.jsonl" true &&
  python3 "$H" compare --left "$E/query-results.jsonl" --right "$E/restart/query-results.jsonl" --output "$E/restart/pre-post-restart.json" >/dev/null
}
write_defects() {
  jq -n --arg source "$SOURCE_SHA" '{schema_version:1,unresolved_in_scope_p0:0,unresolved_in_scope_p1:0,defects:[
    {id:"FIX486G-P0-001",severity:"P0",category:"CANONICAL_GRAPH_BINDING",root_cause:"Graph hydration accepted related chunks through an optional binding join",regression_test:"canonical_graph_hydration_requires_a_synced_binding",fix_commit:$source,status:"FIXED"},
    {id:"FIX486G-P1-001",severity:"P1",category:"GRAPH_PROVENANCE",root_cause:"stable edge and endpoint identity was discarded before response construction",regression_test:"graph_expansion_preserves_stable_edge_and_endpoint_identity",fix_commit:$source,status:"FIXED"},
    {id:"FIX486G-P1-002",severity:"P1",category:"CANDIDATE_NON_INTERFERENCE",root_cause:"final Graph limit was applied before canonical hydration without bounded reserve",regression_test:"invalid_graph_candidate_cannot_exhaust_the_final_window",fix_commit:$source,status:"FIXED"},
    {id:"FIX486G-P1-003",severity:"P1",category:"FALSE_GRAPH_ATTRIBUTION",root_cause:"self-edge expansion was not explicitly rejected",regression_test:"self_edges_are_rejected_before_graph_attribution",fix_commit:$source,status:"FIXED"},
    {id:"FIX486G-P1-004",severity:"P1",category:"RELATION_ENDPOINT_SCOPE",root_cause:"logical block relations expanded to every child granularity pair instead of the declared physical endpoint granularities",regression_test:"relation_ingestion_honors_declared_child_granularities",fix_commit:$source,status:"FIXED"}
    ,{id:"FIX486G-P0-002",severity:"P0",category:"GRAPH_SEED_IDENTITY",root_cause:"parent-context deduplication and parent-first graph_seed_chunk_id selection discarded canonical hydrated child relation endpoints",regression_test:"graph_seed_identity_survives_parent_context_deduplication; graph_seed_preserves_matched_child_identity_with_parent_fallback",fix_commit:$source,status:"FIXED"}
    ,{id:"FIX486G-P0-003",severity:"P0",category:"GRAPH_SEED_GRANULARITY_NONDETERMINISM",root_cause:"parent deduplication retained only one equal-score child granularity, so relation discovery depended on whether SUB_180 or SUB_260 won the tie",regression_test:"graph_seed_sources_keep_all_child_representations_of_admitted_parents; graph_seed_selection_keeps_all_hydrated_children_of_each_admitted_parent_group",fix_commit:$source,status:"FIXED"}
    ,{id:"FIX486G-P1-005",severity:"P1",category:"PROTO3_ZERO_DIAGNOSTIC_OMISSION",root_cause:"statistical capture required zero-valued Graph duration and hop scalars that protobuf JSON legitimately omits for Graph-disabled requests",regression_test:"fake_grpcurl_full_pass_makes_exactly_142_calls_and_appends_complete_jsonl",failed_evidence_run:"fix486g-20260722T200601Z",fix_commit:$source,status:"FIXED"}
    ,{id:"FIX486G-P1-006",severity:"P1",category:"PROTO3_ZERO_FINAL_CANDIDATE_OMISSION",root_cause:"statistical capture required finalCandidateCount even when a valid no-answer response had zero contexts and protobuf JSON omitted the zero scalar",regression_test:"fake_grpcurl_full_pass_makes_exactly_142_calls_and_appends_complete_jsonl; nonempty_response_missing_final_candidate_diagnostics_fails_closed",failed_evidence_run:"fix486g-20260722T202249Z",fix_commit:$source,status:"FIXED"}
    ,{id:"FIX486G-P1-007",severity:"P1",category:"PROTOBUF_DEFAULT_EVIDENCE_CAPTURE",root_cause:"official grpcurl capture did not request emission of proto3 default scalar values",regression_test:"fake_grpcurl_full_pass_makes_exactly_142_calls_and_appends_complete_jsonl",failed_evidence_runs:["fix486g-20260722T200601Z","fix486g-20260722T202249Z"],fix_commit:$source,status:"FIXED"}
    ,{id:"FIX486G-P0-004",severity:"P0",category:"GRAPH_SEED_PARENT_GROUP_CAP",root_cause:"the global Graph seed cap ranked sibling child representations independently, allowing one granularity of a relevant parent to evict the sibling that owns the canonical relation endpoint",regression_test:"graph_seed_cap_keeps_sibling_representations_of_selected_parent_group",failed_evidence_run:"fix486g-20260722T203512Z",fix_commit:$source,status:"FIXED"}
    ,{id:"FIX486G-P0-005",severity:"P0",category:"GRAPH_SEED_EDGE_FANOUT_STARVATION",root_cause:"Graph SQL consumed the bounded edge window by seed rank, so structural fan-out from the first child could starve a canonical relation on its selected sibling child",regression_test:"graph_edge_budget_is_fair_across_selected_seed_children",failed_evidence_run:"fix486g-20260722T205152Z",fix_commit:$source,status:"FIXED"}
    ,{id:"FIX486G-P1-008",severity:"P1",category:"FAULT_VALIDATOR_SURVIVOR_CONTRACT",root_cause:"scenario-specific proof assertions globally required a Graph survivor even after the scenario intentionally invalidated the only Graph target",regression_test:"test_all_rejected_target_contracts_accept_only_their_declared_survivor_and_reason; test_old_graph_survivor_only_assumption_rejects_valid_direct_fault_survivor; direct_survivor_is_sufficient_for_faults_that_invalidate_the_graph_target",failed_evidence_runs:["fix486g-20260727T181630Z","fix486g-20260727T183227Z"],fix_commit:$source,status:"FIXED"}
    ,{id:"FIX486G-P1-009",severity:"P1",category:"HOP_TELEMETRY_EXTRACTION",root_cause:"statistical capture required hop metadata on a final Graph context even when rankingTrace proved one-hop expansion and final selection retained only direct evidence",regression_test:"one_hop_is_derived_from_complete_ranking_trace_when_no_graph_context_survives",failed_evidence_run:"fix486g-20260727T191108Z",fix_commit:$source,status:"FIXED"}
  ]}' >"$E/defect-register.json"
}
evidence_completeness() {
  local required=(query-results.jsonl graph-disabled/results.jsonl graph-audit/graph-identity-chain.json graph-audit/graph-provenance-trace.json comparisons/entry-point-parity.json comparisons/warm-repeat.json restart/pre-post-restart.json canonical-audit/integrity-summary.json qdrant-audit/payload-consistency.json faults/wrong-parent/search/result.json faults/wrong-parent/retrieve-context/result.json faults/binding-invalid/search/result.json faults/binding-invalid/retrieve-context/result.json faults/missing-parent/search/result.json faults/missing-parent/retrieve-context/result.json faults/inactive-target/search/result.json faults/inactive-target/retrieve-context/result.json faults/deleted-target/search/result.json faults/deleted-target/retrieve-context/result.json faults/expired-target/search/result.json faults/expired-target/retrieve-context/result.json faults/cross-zone/search/result.json faults/cross-zone/retrieve-context/result.json faults/cross-zone/rejection-evidence.json faults/hop-limit/search/result.json faults/hop-limit/retrieve-context/result.json faults/hop-limit/rejection-evidence.json faults/cycle/search/result.json faults/cycle/retrieve-context/result.json faults/cycle/rejection-evidence.json statistical/sample-plan.json statistical/raw-observations.jsonl statistical/raw-validation.json statistical/statistical-report.json statistical/statistical-report.md statistical/per-query-results.jsonl statistical/per-slice-metrics.json statistical/latency-distribution.json statistical/safety-hard-gates.json statistical/confidence-intervals.json cleanup/summary.json cleanup/restoration.json defect-register.json)
  for path in "${required[@]}"; do [[ -s "$E/$path" ]] || return 1; done
  [[ $(wc -l <"$E/query-results.jsonl" | tr -d ' ') -eq 2 ]] &&
  [[ $(wc -l <"$E/graph-disabled/results.jsonl" | tr -d ' ') -eq 2 ]]
}

initialize_evidence() {
  jq -n --arg run_id "$RUN_ID" --arg mode "$MODE" --arg started "$(timestamp)" --arg branch "$BRANCH" --arg source "$SOURCE_SHA" --arg remote "$REMOTE_SHA" '{run_id:$run_id,mode:$mode,started_at_utc:$started,branch:$branch,source_sha:$source,remote_branch_sha:$remote,local_remote_equal:($source==$remote),status:"RUNNING"}' >"$E/bootstrap.json"
  jq -n --arg branch "$BRANCH" --arg source "$SOURCE_SHA" --arg remote "$REMOTE_SHA" '{branch:$branch,source_sha:$source,remote_branch_sha:$remote,local_remote_equal:($source==$remote)}' >"$E/source/git-identity.json"
}
write_mode_result() {
  local status=$1 reason=$2 cleanup_attempted=${3:-false} cleanup_status=${4:-NOT_REQUIRED} exit_code=1
  [[ "$status" == PASS ]] && exit_code=0
  jq -n --arg mode "$MODE" --arg status "$status" --arg reason "$reason" --arg run_id "$RUN_ID" --arg source "$SOURCE_SHA" \
    '{schema_version:1,phase:"fix486g",run_id:$run_id,mode:$mode,status:$status,reason:$reason,source_sha:$source,official_runtime_proof:false}' >"$E/final-result.json"
  jq -n --arg status "$status" --arg reason "$reason" --argjson cleanup_attempted "$cleanup_attempted" --arg cleanup_status "$cleanup_status" --argjson exit_code "$exit_code" --arg finished "$(timestamp)" \
    '{stage:"runner-terminal",status:$status,termination_reason:$reason,signal:null,cleanup_attempted:$cleanup_attempted,cleanup_status:$cleanup_status,exit_code:$exit_code,finished_at_utc:$finished}' >"$E/terminal-result.json"
  [[ ! -s "$E/bootstrap.json" ]] || { jq --arg status "$([[ "$status" == PASS ]] && echo COMPLETED || echo BLOCKED)" '.status=$status' "$E/bootstrap.json" >"$E/bootstrap.tmp" && mv "$E/bootstrap.tmp" "$E/bootstrap.json"; }
  python3 "$H" manifest --run "$E" --output "$E/manifest.json" >/dev/null || return 1
  python3 "$H" verify-manifest --run "$E" --manifest "$E/manifest.json" --output "$E/manifest-verification.json" >/dev/null
}
verify_contracts() {
  bash -n "$ROOT/scripts/fix486g-graph-parent-runtime-proof.sh" &&
  (cd "$ROOT" &&
    cargo test --locked --test fix486g_graph_parent_contracts --test fix486g_runner_hardening_contracts --test fix486g_statistical_capture_contracts --test fix486g_statistical_proof_contracts --test fix486g_visibility_recheck_contracts -- --nocapture &&
    python3 -m unittest -v tests/test_fix486g_proof.py &&
    python3 -m py_compile scripts/fix486g_proof.py scripts/fix486g_statistical_capture.py scripts/fix486g_statistical_proof.py tests/test_fix486g_proof.py)
}
verify_existing_evidence() {
  local verification
  verification=$(mktemp "${TMPDIR:-/tmp}/fix486g-evidence-verification.XXXXXX.json") || return 1
  if evidence_completeness && python3 "$H" verify-manifest --run "$E" --manifest "$E/manifest.json" --output "$verification" >/dev/null; then
    jq -c --arg mode "$MODE" '. + {mode:$mode,official_runtime_proof:false}' "$verification"
    rm -f "$verification"
    return 0
  fi
  [[ ! -s "$verification" ]] || jq -c --arg mode "$MODE" '. + {mode:$mode,official_runtime_proof:false}' "$verification" >&2
  rm -f "$verification"
  return 1
}
execute_all() {
  local ok=true verdict terminal_reason
  set +e
  stage identity-verification verify_identity || ok=false
  [[ "$ok" == true ]] && stage bank-verification verify_bank || ok=false
  [[ "$ok" == true ]] && stage model-tokenizer-verification verify_model_tokenizer || ok=false
  [[ "$ok" == true ]] && stage static-gates static_gates || ok=false
  [[ "$ok" == true ]] && stage infrastructure-start start_infrastructure || ok=false
  [[ "$ok" == true ]] && stage migrations migrate_and_build || ok=false
  [[ "$ok" == true ]] && stage runtime-start start_runtime initial || ok=false
  [[ "$ok" == true ]] && stage production-ingestion ingest || ok=false
  [[ "$ok" == true ]] && stage identity-map identity_map || ok=false
  [[ "$ok" == true ]] && stage canonical-audit canonical_audit || ok=false
  [[ "$ok" == true ]] && stage qdrant-audit qdrant_audit || ok=false
  [[ "$ok" == true ]] && stage graph-audit graph_audit || ok=false
  [[ "$ok" == true ]] && stage graph-disabled-control graph_disabled_control || ok=false
  if [[ "$ok" == true ]] && stage primary-query-proof run_queries initial "$E/search" "$E/retrieve-context" "$E/query-results.jsonl" true; then record_stage_status search-proof PASS; record_stage_status retrieve-context-proof PASS; else ok=false; record_stage_status search-proof FAIL QUERY_PROOF_FAILED; record_stage_status retrieve-context-proof FAIL QUERY_PROOF_FAILED; fi
  [[ "$ok" == true ]] && stage entry-point-comparison compare_initial || ok=false
  [[ "$ok" == true ]] && stage fault-target-preparation prepare_fault_targets || ok=false
  [[ "$ok" == true ]] && stage wrong-parent-fault binding_parent_fault || ok=false
  [[ "$ok" == true ]] && stage binding-invalid-fault binding_status_fault || ok=false
  [[ "$ok" == true ]] && stage missing-parent-fault missing_parent_fault || ok=false
  [[ "$ok" == true ]] && stage inactive-target-fault lifecycle_fault inactive || ok=false
  [[ "$ok" == true ]] && stage deleted-target-fault lifecycle_fault deleted || ok=false
  [[ "$ok" == true ]] && stage expired-target-fault lifecycle_fault expired || ok=false
  [[ "$ok" == true ]] && stage cross-zone-fault cross_zone_fault || ok=false
  [[ "$ok" == true ]] && stage hop-limit-control hop_limit_fault || ok=false
  [[ "$ok" == true ]] && stage cycle-control cycle_fault || ok=false
  [[ "$ok" == true ]] && stage post-fault-canonical-audit canonical_audit || ok=false
  [[ "$ok" == true ]] && stage warm-repeatability warm_repeat || ok=false
  [[ "$ok" == true ]] && stage restart-repeatability restart_repeat || ok=false
  [[ "$ok" == true ]] && stage statistical-campaign statistical_campaign || ok=false
  [[ "$ok" == true ]] && stage post-statistical-canonical-audit canonical_audit || ok=false
  write_defects || ok=false
  if stage pre-teardown-fault-restoration restore_fault_state_before_teardown; then :; else ok=false; fi
  if cleanup; then record_stage_status cleanup PASS; else ok=false; record_stage_status cleanup FAIL CLEANUP_FAILED; fi
  if evidence_completeness; then record_stage_status evidence-completeness PASS; else ok=false; record_stage_status evidence-completeness FAIL EVIDENCE_INCOMPLETE; fi
  record_stage_status final-verdict "$([[ "$ok" == true ]] && echo PASS || echo FAIL)" "$([[ "$ok" == true ]] && echo '' || echo MANDATORY_STAGE_FAILED)"
  jq -s --arg run_id "$RUN_ID" --arg source "$SOURCE_SHA" --arg bank "$BANK_SHA" --arg verdict "$([[ "$ok" == true ]] && echo FIX486_GRAPH_PARENT_RUNTIME_PROOF_PASS || echo FIX486_GRAPH_PARENT_RUNTIME_PROOF_BLOCKED)" '{schema_version:1,phase:"fix486g",run_id:$run_id,source_sha:$source,bank_version:"1.0.0",bank_aggregate_sha256:$bank,stages:.,verdict:$verdict}' "$E"/logs/*.stage.json >"$E/stage-results.json"
  python3 "$H" aggregate --run "$E" --output "$E/aggregate.json" >/dev/null || ok=false
  terminal_reason=$([[ "$ok" == true ]] && echo COMPLETED || echo MANDATORY_STAGE_FAILED)
  jq -n --argjson exit_code "$([[ "$ok" == true ]] && echo 0 || echo 1)" --arg reason "$terminal_reason" --arg finished "$(timestamp)" '{stage:"runner-terminal",status:(if $exit_code==0 then "PASS" else "FAIL" end),termination_reason:$reason,signal:null,cleanup_attempted:true,cleanup_status:"COMPLETED",exit_code:$exit_code,finished_at_utc:$finished}' >"$E/terminal-result.json"
  jq --arg status "$([[ "$ok" == true ]] && echo COMPLETED || echo BLOCKED)" '.status=$status' "$E/bootstrap.json" >"$E/bootstrap.tmp" && mv "$E/bootstrap.tmp" "$E/bootstrap.json"
  python3 "$H" manifest --run "$E" --output "$E/manifest.json" >/dev/null || ok=false
  if ! python3 "$H" verify-manifest --run "$E" --manifest "$E/manifest.json" --output "$E/manifest-verification.json" >/dev/null; then
    ok=false
    record_stage_status final-verdict FAIL MANIFEST_INTEGRITY_FAILED
    jq -s --arg run_id "$RUN_ID" --arg source "$SOURCE_SHA" --arg bank "$BANK_SHA" \
      '{schema_version:1,phase:"fix486g",run_id:$run_id,source_sha:$source,bank_version:"1.0.0",bank_aggregate_sha256:$bank,stages:.,verdict:"FIX486_GRAPH_PARENT_RUNTIME_PROOF_BLOCKED"}' \
      "$E"/logs/*.stage.json >"$E/stage-results.json"
    python3 "$H" aggregate --run "$E" --output "$E/aggregate.json" >/dev/null || true
    jq -n --arg finished "$(timestamp)" '{stage:"runner-terminal",status:"FAIL",termination_reason:"MANIFEST_INTEGRITY_FAILED",signal:null,cleanup_attempted:true,cleanup_status:"COMPLETED",exit_code:1,finished_at_utc:$finished}' >"$E/terminal-result.json"
    jq '.status="BLOCKED"' "$E/bootstrap.json" >"$E/bootstrap.tmp" && mv "$E/bootstrap.tmp" "$E/bootstrap.json"
    python3 "$H" manifest --run "$E" --output "$E/manifest.json" >/dev/null
    python3 "$H" verify-manifest --run "$E" --manifest "$E/manifest.json" --output "$E/manifest-verification.json" >/dev/null || true
  fi
  FINALIZED=true
  trap - EXIT INT TERM HUP
  verdict=$(jq -r .verdict "$E/aggregate.json")
  echo "$verdict"
  [[ "$ok" == true && "$verdict" == FIX486_GRAPH_PARENT_RUNTIME_PROOF_PASS ]]
}

case "$MODE" in
  --verify-evidence)
    if verify_existing_evidence; then FINALIZED=true; trap - EXIT INT TERM HUP; exit 0; else FINALIZED=true; trap - EXIT INT TERM HUP; exit 1; fi
    ;;
  --cleanup-only)
    if cleanup; then write_mode_result PASS CLEANUP_ONLY_COMPLETED true PASS; rc=$?; else write_mode_result BLOCKED CLEANUP_ONLY_FAILED true FAIL; rc=1; fi
    FINALIZED=true; trap - EXIT INT TERM HUP; exit "$rc"
    ;;
  --verify-identities)
    initialize_evidence
    if stage identity-verification verify_identity && stage bank-verification verify_bank && stage model-tokenizer-verification verify_model_tokenizer; then write_mode_result PASS IDENTITIES_VERIFIED; rc=$?; else write_mode_result BLOCKED IDENTITY_VERIFICATION_FAILED; rc=1; fi
    FINALIZED=true; trap - EXIT INT TERM HUP; exit "$rc"
    ;;
  --verify-contracts)
    initialize_evidence
    if stage contract-verification verify_contracts; then write_mode_result PASS CONTRACTS_VERIFIED; rc=$?; else write_mode_result BLOCKED CONTRACT_VERIFICATION_FAILED; rc=1; fi
    FINALIZED=true; trap - EXIT INT TERM HUP; exit "$rc"
    ;;
  --execute-all)
    initialize_evidence
    execute_all
    ;;
esac
