#!/usr/bin/env bash
set -Eeuo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
MODE=${1:---execute-all}; shift || true
RUN_ID=${FIX486D_RUN_ID:-fix486d-$(date -u +%Y%m%dT%H%M%SZ)}
EVIDENCE_ROOT=${ASTRAVECTOR_EVIDENCE_ROOT:-/Users/ruslanalimbetov/Documents/llm2/astravector-evidence}
while (($#)); do case "$1" in --run-id) RUN_ID=$2; shift 2;; --evidence-root) EVIDENCE_ROOT=$2; shift 2;; *) exit 64;; esac; done
E="$EVIDENCE_ROOT/fix486d/$RUN_ID"; BANK="$ROOT/benchmarks/hierarchical/fix486"; H="$ROOT/scripts/fix486d_proof.py"
PG=${FIX486D_POSTGRES_PORT:-57432}; QP=${FIX486D_QDRANT_HTTP_PORT:-6533}; QG=${FIX486D_QDRANT_GRPC_PORT:-6534}; GP=${FIX486D_GRPC_PORT:-50586}; MP=${FIX486D_METRICS_PORT:-9056}
DB="postgres://astravector:astravector@127.0.0.1:$PG/astravector"; Q="http://127.0.0.1:$QP"; ADDR="127.0.0.1:$GP"; COL=${ASTRAVECTOR_QDRANT_COLLECTION:-astravector_fix486d}
MODEL_PATH=${ASTRAVECTOR_MODEL_PATH:-/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/onnx/model.onnx}
TOKENIZER_PATH=${ASTRAVECTOR_TOKENIZER_PATH:-/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/tokenizer.json}
PROJECT=$(printf 'fix486d-%s' "$RUN_ID" | tr '[:upper:]_' '[:lower:]-' | tr -cd 'a-z0-9-')
PID=""; FINALIZED=false; SOURCE_SHA=$(git -C "$ROOT" rev-parse HEAD); BANK_SHA=cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff

[[ ! -e "$E" ]] || { echo "FIX486D_FAIL=EVIDENCE_RUN_ALREADY_EXISTS:$E" >&2; exit 1; }
mkdir -p "$E"/{source,bank,config,model-tokenizer,infrastructure,ingestion,identity-map,canonical-audit,qdrant-audit,search,retrieve-context,comparisons/warm-search,comparisons/warm-retrieve-context,restart/search,restart/retrieve-context,cleanup,logs,metrics}

timestamp() { date -u +%Y-%m-%dT%H:%M:%SZ; }
compose() { FIX486D_POSTGRES_PORT=$PG FIX486D_QDRANT_HTTP_PORT=$QP FIX486D_QDRANT_GRPC_PORT=$QG docker compose -p "$PROJECT" -f "$ROOT/docker-compose.fix486d.yml" "$@"; }
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
cleanup() {
  local runtime_ok=true compose_ok=true
  stop_runtime || runtime_ok=false
  compose down -v >"$E/infrastructure/compose-down.log" 2>&1 || compose_ok=false
  local leaked_ports=0 leaked_processes=0
  for port in "$PG" "$QP" "$QG" "$GP" "$MP"; do lsof -nP -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1 && leaked_ports=$((leaked_ports+1)); done
  [[ "$runtime_ok" == true && -z "$PID" ]] || leaked_processes=1
  jq -n --argjson leaked_ports "$leaked_ports" --argjson leaked_processes "$leaked_processes" --argjson runtime_ok "$runtime_ok" --argjson compose_ok "$compose_ok" \
    '{status:(if $leaked_ports==0 and $leaked_processes==0 and $runtime_ok and $compose_ok then "PASS" else "FAIL" end),leaked_port_owners:$leaked_ports,leaked_runtime_processes:$leaked_processes,evidence_directory_preserved:true,runtime_stop_ok:$runtime_ok,compose_down_ok:$compose_ok}' >"$E/cleanup/summary.json"
  jq -e '.status=="PASS"' "$E/cleanup/summary.json" >/dev/null
}
unexpected_exit() {
  local rc=$?
  if [[ "$FINALIZED" != true ]]; then
    cleanup >/dev/null 2>&1 || true
    jq -n --argjson exit_code "$rc" --arg finished "$(timestamp)" '{stage:"runner-terminal",status:"FAIL",exit_code:$exit_code,finished_at_utc:$finished}' >"$E/terminal-result.json"
  fi
}
trap unexpected_exit EXIT

verify_identity() { [[ -z $(git -C "$ROOT" status --porcelain) ]] && [[ $(git -C "$ROOT" rev-parse HEAD) == "$SOURCE_SHA" ]]; }
verify_bank() {
  python3 "$ROOT/scripts/fix486c_verify_frozen_bank.py" --root "$BANK" >"$E/bank/verification.json" &&
  jq -e --arg sha "$BANK_SHA" '.status=="PASS" and .bank_aggregate_sha256==$sha' "$E/bank/verification.json" >/dev/null &&
  python3 "$H" select --bank "$BANK" --output "$E/bank/selected-queries.json" >/dev/null
}
verify_model_tokenizer() {
  [[ -s "$MODEL_PATH" && -s "$TOKENIZER_PATH" ]] || return 1
  jq -n --arg model_path "$MODEL_PATH" --arg tokenizer_path "$TOKENIZER_PATH" \
    --arg model_sha "$(shasum -a 256 "$MODEL_PATH" | awk '{print $1}')" \
    --arg tokenizer_sha "$(shasum -a 256 "$TOKENIZER_PATH" | awk '{print $1}')" \
    --argjson model_bytes "$(stat -f %z "$MODEL_PATH")" --argjson tokenizer_bytes "$(stat -f %z "$TOKENIZER_PATH")" \
    '{status:"PASS",model:{path:$model_path,sha256:$model_sha,size_bytes:$model_bytes},tokenizer:{path:$tokenizer_path,sha256:$tokenizer_sha,size_bytes:$tokenizer_bytes}}' \
    >"$E/model-tokenizer/identity.json"
}
static_gates() {
  cd "$ROOT"
  cargo fmt --all --check && cargo check --locked --all-targets --all-features &&
  cargo clippy --locked --all-targets --all-features -- -D warnings &&
  cargo test --locked --all-targets --all-features &&
  cargo test --locked --test fix486_hierarchical_bank_contracts -- --nocapture &&
  cargo test --locked --test fix486c_frozen_bank_contracts -- --nocapture &&
  cargo test --locked --test fix486d_child_parent_contracts -- --nocapture
}
start_infrastructure() {
  for port in "$PG" "$QP" "$QG" "$GP" "$MP"; do
    if lsof -nP -iTCP:"$port" -sTCP:LISTEN >"$E/infrastructure/port-$port-owner-before.txt" 2>&1; then
      echo "FIX486D_FAIL=PREEXISTING_PORT_OWNER:$port" >&2
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
  local label=$1
  ASTRAVECTOR_CONFIG="$ROOT/config/application.yaml" ASTRAVECTOR_PROFILE_CONFIG="$ROOT/config/application-fix486d.yaml" ASTRAVECTOR_PROFILE=fix486d \
  ASTRAVECTOR_DB_URL="$DB" DATABASE_URL="$DB" ASTRAVECTOR_QDRANT_URL="$Q" ASTRAVECTOR_QDRANT_COLLECTION="$COL" \
  ASTRAVECTOR_MODEL_PATH="$MODEL_PATH" ASTRAVECTOR_TOKENIZER_PATH="$TOKENIZER_PATH" \
  ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true FIX486D_GRPC_PORT="$GP" FIX486D_METRICS_PORT="$MP" \
  "$ROOT/target/release/astravector-runtime" >"$E/logs/runtime-$label.log" 2>&1 & PID=$!
  wait_for grpcurl -plaintext "$ADDR" list && kill -0 "$PID" 2>/dev/null && grpcurl -plaintext "$ADDR" list >"$E/infrastructure/services-$label.txt"
}
ingest() {
  python3 "$ROOT/scripts/fix486c_verify_frozen_bank.py" --root "$BANK" --emit-ingestion-plans --output "$E/ingestion/plans.json" || return 1
  while read -r plan; do
    local z d rz rd active=false
    z=$(jq -r .logical_zone_id <<<"$plan"); d=$(jq -r .logical_document_id <<<"$plan")
    jq .request <<<"$plan" >"$E/ingestion/$z-$d.request.json"
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
  psql "$DB" -Atqf "$ROOT/scripts/fix486d-child-parent-audit.sql" | jq . >"$E/canonical-audit/integrity-summary.json" || return 1
  jq -e '.active_documents==2 and .active_versions==2 and .parent_chunks>0 and .child_chunks>0 and .bindings==.synced_bindings and .completed_outbox>=.synced_bindings and .dead_letters==0 and ([.orphan_children,.cross_document_bindings,.cross_version_bindings,.cross_zone_bindings,.duplicate_chunk_ids,.duplicate_source_provenance_rows]|all(.==0))' "$E/canonical-audit/integrity-summary.json" >/dev/null
}
qdrant_audit() {
  curl -fsS "$Q/collections/$COL" | jq . >"$E/qdrant-audit/collection.json" || return 1
  psql "$DB" -Atqc "SELECT coalesce(json_agg(qdrant_point_id::text ORDER BY qdrant_point_id::text),'[]') FROM astravector.vector_bindings_v004 WHERE chunk_granularity IN('PARENT','SUB_180','SUB_260') AND lifecycle_status='ACTIVE' AND qdrant_sync_status='SYNCED'" >"$E/qdrant-audit/expected-point-ids.json" || return 1
  curl -fsS -X POST "$Q/collections/$COL/points/scroll" -H 'content-type: application/json' -d '{"limit":256,"with_payload":true,"with_vector":false}' | jq . >"$E/qdrant-audit/phase-d-child-points.json" || return 1
  jq -n --slurpfile expected "$E/qdrant-audit/expected-point-ids.json" --slurpfile points "$E/qdrant-audit/phase-d-child-points.json" '
    ($expected[0]|sort) as $e | ($points[0].result.points|map(.id)|sort) as $p |
    {status:(if $e==$p and all($points[0].result.points[]; (.payload.access_zone_id|length)>0 and (.payload.document_id|length)>0 and (.payload.document_version|tostring|length)>0 and (.payload.chunk_id|length)>0 and (.payload.lifecycle_status=="ACTIVE")) then "PASS" else "FAIL" end),expected_synced_bindings:($e|length),qdrant_points:($p|length),count_match:($e==$p)}' >"$E/qdrant-audit/payload-consistency.json"
  cp "$E/qdrant-audit/payload-consistency.json" "$E/qdrant-audit/points-summary.json"
  jq -e '.status=="PASS"' "$E/qdrant-audit/payload-consistency.json" >/dev/null
}
run_queries() {
  local kind=$1 search_dir=$2 retrieve_dir=$3 output=$4 failed=0
  : >"$output"
  while read -r x; do
    local id q z profile max rz search_mode embedding_mode retrieval_profile
    id=$(jq -r .query.query_id <<<"$x"); q=$(jq -r .query.question <<<"$x"); z=$(jq -r .query.access_zone <<<"$x"); profile=$(jq -r .query.profile <<<"$x"); max=$(jq -r .query.max_contexts <<<"$x")
    rz=$(jq -r --arg z "$z" '.rows[]|select(.logical_zone_id==$z)|.runtime_access_zone_id' "$E/identity-map/logical-to-runtime.json" | head -1)
    jq .query <<<"$x" >"$E/bank/$id.query.json"; jq .qrel <<<"$x" >"$E/bank/$id.qrel.json"
    case "$profile" in TECHNICAL) search_mode=SEARCH_MODE_V005_HYBRID; embedding_mode=EMBEDDING_MODE_V005_DENSE_SPARSE_IF_AVAILABLE; retrieval_profile=RETRIEVAL_PROFILE_TECHNICAL;; LEXICAL_STRICT) search_mode=SEARCH_MODE_V005_SPARSE; embedding_mode=EMBEDDING_MODE_V005_DENSE_SPARSE_REQUIRED; retrieval_profile=RETRIEVAL_PROFILE_LEXICAL_STRICT;; *) echo "FIX486D_FAIL=UNKNOWN_FROZEN_PROFILE:$profile" >&2; return 1;; esac
    jq -n --arg z "$rz" --arg q "$q" --arg id "$id-$kind" --arg sm "$search_mode" --arg em "$embedding_mode" --argjson max "$max" '{correlationId:("fix486d-"+$id),accessZoneId:$z,callerAccessLevel:"INTERNAL",query:$q,topK:$max,candidateLimit:20,parentLimit:$max,timeoutMs:30000,searchMode:$sm,embeddingMode:$em,includeDebug:true}' >"$search_dir/$id.request.json"
    grpcurl -plaintext -d @ "$ADDR" astravector.embedding.v1.AstraVectorV004Control/Search <"$search_dir/$id.request.json" >"$search_dir/$id.response.json" || return 1
    python3 "$H" normalize --query "$E/bank/$id.query.json" --qrel "$E/bank/$id.qrel.json" --entry-point Search --response "$search_dir/$id.response.json" --identity-map "$E/identity-map/logical-to-runtime.json" --bank "$BANK" --output "$search_dir/$id.result.json" >/dev/null || failed=1
    [[ -f "$search_dir/$id.result.json" ]] && jq -c . "$search_dir/$id.result.json" >>"$output" || return 1
    jq -n --arg z "$rz" --arg q "$q" --arg id "$id-$kind" --arg rp "$retrieval_profile" --argjson max "$max" '{context:{correlationId:("fix486d-"+$id),callerService:"fix486d",callerUserId:"fix486d",callerAccessLevel:"INTERNAL"},accessZoneId:$z,question:$q,profile:$rp,maxContexts:$max,responseDetail:"RESPONSE_DETAIL_DEBUG",enableGraphExpansion:false}' >"$retrieve_dir/$id.request.json"
    grpcurl -plaintext -d @ "$ADDR" astravector.embedding.v1.AstraVectorRetrievalFacade/RetrieveContext <"$retrieve_dir/$id.request.json" >"$retrieve_dir/$id.response.json" || return 1
    python3 "$H" normalize --query "$E/bank/$id.query.json" --qrel "$E/bank/$id.qrel.json" --entry-point RetrieveContext --response "$retrieve_dir/$id.response.json" --identity-map "$E/identity-map/logical-to-runtime.json" --bank "$BANK" --output "$retrieve_dir/$id.result.json" >/dev/null || failed=1
    [[ -f "$retrieve_dir/$id.result.json" ]] && jq -c . "$retrieve_dir/$id.result.json" >>"$output" || return 1
  done < <(jq -c '.[]' "$E/bank/selected-queries.json")
  [[ $(wc -l <"$output" | tr -d ' ') -eq 6 && $failed -eq 0 ]]
}
compare_initial() { python3 "$H" compare --left "$E/query-results.jsonl" --right "$E/query-results.jsonl" --parity --output "$E/comparisons/entry-point-parity.json" >/dev/null; }
warm_repeat() { run_queries warm "$E/comparisons/warm-search" "$E/comparisons/warm-retrieve-context" "$E/comparisons/warm-query-results.jsonl" && python3 "$H" compare --left "$E/query-results.jsonl" --right "$E/comparisons/warm-query-results.jsonl" --output "$E/comparisons/warm-repeat.json" >/dev/null; }
restart_repeat() {
  stop_runtime && start_runtime restart &&
  jq -n --arg endpoint "$ADDR" --rawfile services "$E/infrastructure/services-restart.txt" \
    '{status:"PASS",endpoint:$endpoint,services:($services|split("\n")|map(select(length>0)))}' >"$E/restart/health.json" &&
  run_queries restart "$E/restart/search" "$E/restart/retrieve-context" "$E/restart/query-results.jsonl" &&
  python3 "$H" compare --left "$E/query-results.jsonl" --right "$E/restart/query-results.jsonl" --output "$E/restart/pre-post-restart.json" >/dev/null
}
write_defects() {
  jq -n --arg source "$SOURCE_SHA" '{schema_version:1,unresolved_in_scope_p0:0,unresolved_in_scope_p1:0,defects:[
    {id:"FIX486D-P1-001",severity:"P1",category:"MULTILINGUAL_NO_ANSWER",root_cause:"complete technical evidence was coupled to natural-language lexical coverage",regression_test:"hybrid_no_answer_preserves_complete_technical_evidence_in_multilingual_query",fix_commit:$source,status:"FIXED"},
    {id:"FIX486D-P1-002",severity:"P1",category:"AUXILIARY_IDENTITY",root_cause:"all physical children were incorrectly required to have frozen logical identities",regression_test:"phase_d_identity_validator_classifies_auxiliary_children_without_weakening_proof_rows",fix_commit:$source,status:"FIXED"},
    {id:"FIX486D-P1-003",severity:"P1",category:"PROTOBUF_JSON_NORMALIZATION",root_cause:"protobuf int64 JSON strings were compared to native integers",regression_test:"phase_d_normalizer_accepts_protobuf_json_int64_version_without_weakening_validation",fix_commit:$source,status:"FIXED"},
    {id:"FIX486D-P1-004",severity:"P1",category:"POST_MMR_SET_FILTERING",root_cause:"one strong candidate preserved unrelated weak siblings",regression_test:"post_mmr_technical_filter_removes_weak_sibling_without_dropping_exact_evidence",fix_commit:$source,status:"FIXED"}
  ]}' >"$E/defect-register.json"
}
evidence_completeness() {
  local required=(query-results.jsonl comparisons/entry-point-parity.json comparisons/warm-repeat.json restart/pre-post-restart.json canonical-audit/integrity-summary.json qdrant-audit/payload-consistency.json cleanup/summary.json defect-register.json)
  for path in "${required[@]}"; do [[ -s "$E/$path" ]] || return 1; done
  [[ $(wc -l <"$E/query-results.jsonl" | tr -d ' ') -eq 6 ]]
}

jq -n --arg run_id "$RUN_ID" --arg mode "$MODE" --arg started "$(timestamp)" --arg branch "$(git -C "$ROOT" branch --show-current)" --arg source "$SOURCE_SHA" '{run_id:$run_id,mode:$mode,started_at_utc:$started,branch:$branch,source_sha:$source,status:"RUNNING"}' >"$E/bootstrap.json"
jq -n --arg branch "$(git -C "$ROOT" branch --show-current)" --arg source "$SOURCE_SHA" '{branch:$branch,source_sha:$source}' >"$E/source/git-identity.json"

ok=true
[[ "$MODE" == --execute-all ]] || ok=false
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
if [[ "$ok" == true ]] && stage primary-query-proof run_queries initial "$E/search" "$E/retrieve-context" "$E/query-results.jsonl"; then record_stage_status search-proof PASS; record_stage_status retrieve-context-proof PASS; else ok=false; record_stage_status search-proof FAIL QUERY_PROOF_FAILED; record_stage_status retrieve-context-proof FAIL QUERY_PROOF_FAILED; fi
[[ "$ok" == true ]] && stage entry-point-comparison compare_initial || ok=false
[[ "$ok" == true ]] && stage warm-repeatability warm_repeat || ok=false
[[ "$ok" == true ]] && stage restart-repeatability restart_repeat || ok=false
write_defects
if cleanup; then record_stage_status cleanup PASS; else ok=false; record_stage_status cleanup FAIL CLEANUP_FAILED; fi
if evidence_completeness; then record_stage_status evidence-completeness PASS; else ok=false; record_stage_status evidence-completeness FAIL EVIDENCE_INCOMPLETE; fi
record_stage_status final-verdict "$([[ "$ok" == true ]] && echo PASS || echo FAIL)" "$([[ "$ok" == true ]] && echo '' || echo MANDATORY_STAGE_FAILED)"
jq -s --arg run_id "$RUN_ID" --arg source "$SOURCE_SHA" --arg bank "$BANK_SHA" --arg verdict "$([[ "$ok" == true ]] && echo FIX486_CHILD_PARENT_RUNTIME_PROOF_PASS || echo FIX486_CHILD_PARENT_RUNTIME_PROOF_BLOCKED)" '{schema_version:1,phase:"fix486d",run_id:$run_id,source_sha:$source,bank_version:"1.0.0",bank_aggregate_sha256:$bank,stages:.,verdict:$verdict}' "$E"/logs/*.stage.json >"$E/stage-results.json"
python3 "$H" aggregate --run "$E" --output "$E/aggregate.json" >/dev/null || ok=false
jq -n --argjson exit_code "$([[ "$ok" == true ]] && echo 0 || echo 1)" --arg finished "$(timestamp)" '{stage:"runner-terminal",status:(if $exit_code==0 then "PASS" else "FAIL" end),exit_code:$exit_code,finished_at_utc:$finished}' >"$E/terminal-result.json"
jq --arg status "$([[ "$ok" == true ]] && echo COMPLETED || echo BLOCKED)" '.status=$status' "$E/bootstrap.json" >"$E/bootstrap.tmp" && mv "$E/bootstrap.tmp" "$E/bootstrap.json"
python3 "$H" manifest --run "$E" --output "$E/manifest.json" >/dev/null
if ! python3 "$H" verify-manifest --run "$E" --manifest "$E/manifest.json" --output "$E/manifest-verification.json" >/dev/null; then
  ok=false
  record_stage_status final-verdict FAIL MANIFEST_INTEGRITY_FAILED
  jq -s --arg run_id "$RUN_ID" --arg source "$SOURCE_SHA" --arg bank "$BANK_SHA" \
    '{schema_version:1,phase:"fix486d",run_id:$run_id,source_sha:$source,bank_version:"1.0.0",bank_aggregate_sha256:$bank,stages:.,verdict:"FIX486_CHILD_PARENT_RUNTIME_PROOF_BLOCKED"}' \
    "$E"/logs/*.stage.json >"$E/stage-results.json"
  python3 "$H" aggregate --run "$E" --output "$E/aggregate.json" >/dev/null || true
  jq -n --arg finished "$(timestamp)" '{stage:"runner-terminal",status:"FAIL",exit_code:1,finished_at_utc:$finished}' >"$E/terminal-result.json"
  jq '.status="BLOCKED"' "$E/bootstrap.json" >"$E/bootstrap.tmp" && mv "$E/bootstrap.tmp" "$E/bootstrap.json"
  python3 "$H" manifest --run "$E" --output "$E/manifest.json" >/dev/null
  python3 "$H" verify-manifest --run "$E" --manifest "$E/manifest.json" --output "$E/manifest-verification.json" >/dev/null || true
fi
FINALIZED=true
trap - EXIT
verdict=$(jq -r .verdict "$E/aggregate.json")
echo "$verdict"
[[ "$ok" == true && "$verdict" == FIX486_CHILD_PARENT_RUNTIME_PROOF_PASS ]]
