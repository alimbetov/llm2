#!/usr/bin/env bash
set -Eeuo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
WORKSPACE_ROOT=$(cd "$ROOT/.." && pwd)
MODE=${1:---execute-all}; shift || true
RUN_ID=${FIX486G_RUN_ID:-fix486g-$(date -u +%Y%m%dT%H%M%SZ)}
EVIDENCE_ROOT=${ASTRAVECTOR_EVIDENCE_ROOT:-$WORKSPACE_ROOT/astravector-evidence}
while (($#)); do case "$1" in --run-id) RUN_ID=$2; shift 2;; --evidence-root) EVIDENCE_ROOT=$2; shift 2;; *) exit 64;; esac; done
E="$EVIDENCE_ROOT/fix486g/$RUN_ID"; BANK="$ROOT/benchmarks/hierarchical/fix486"; SUPPLEMENTAL="$ROOT/benchmarks/hierarchical/fix486g-supplemental"; H="$ROOT/scripts/fix486g_proof.py"
PG=${FIX486G_POSTGRES_PORT:-59432}; QP=${FIX486G_QDRANT_HTTP_PORT:-6733}; QG=${FIX486G_QDRANT_GRPC_PORT:-6734}; GP=${FIX486G_GRPC_PORT:-50588}; MP=${FIX486G_METRICS_PORT:-9058}
DB="postgres://astravector:astravector@127.0.0.1:$PG/astravector"; Q="http://127.0.0.1:$QP"; ADDR="127.0.0.1:$GP"; COL=${ASTRAVECTOR_QDRANT_COLLECTION:-astravector_fix486g}
MODEL_PATH=${ASTRAVECTOR_MODEL_PATH:-$WORKSPACE_ROOT/models/bge-m3/onnx/model.onnx}
TOKENIZER_PATH=${ASTRAVECTOR_TOKENIZER_PATH:-$WORKSPACE_ROOT/models/bge-m3/tokenizer.json}
DOCUMENT_DEADLINE_MS=${ASTRAVECTOR_INGESTION_DOCUMENT_DEADLINE_MS:-180000}
PROJECT=$(printf 'fix486g-%s' "$RUN_ID" | tr '[:upper:]_' '[:lower:]-' | tr -cd 'a-z0-9-')
PID=""; FINALIZED=false; SOURCE_SHA=$(git -C "$ROOT" rev-parse HEAD); BANK_SHA=cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff; SUPPLEMENTAL_SHA=af4fceb8e424fddecff4284e9cd8d1d68fb4db5c148f9b2aa585bb8497ac1649
BRANCH=$(git -C "$ROOT" branch --show-current)
REMOTE_SHA=$(git -C "$ROOT" rev-parse '@{upstream}' 2>/dev/null || true)

[[ ! -e "$E" ]] || { echo "FIX486G_FAIL=EVIDENCE_RUN_ALREADY_EXISTS:$E" >&2; exit 1; }
mkdir -p "$E"/{source,bank,config,model-tokenizer,infrastructure,ingestion,identity-map,canonical-audit,qdrant-audit,graph-audit,search,retrieve-context,graph-disabled/search,graph-disabled/retrieve-context,faults,comparisons/warm-search,comparisons/warm-retrieve-context,restart/search,restart/retrieve-context,statistical,cleanup,logs,metrics}

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

verify_identity() {
  [[ -n "$BRANCH" && -n "$REMOTE_SHA" ]] &&
    [[ -z $(git -C "$ROOT" status --porcelain) ]] &&
    [[ $(git -C "$ROOT" rev-parse HEAD) == "$SOURCE_SHA" ]] &&
    [[ "$SOURCE_SHA" == "$REMOTE_SHA" ]]
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
  cargo test --locked --test fix486g_graph_parent_contracts -- --nocapture
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
  jq -e '.active_documents==2 and .active_versions==2 and .parent_chunks>0 and .child_chunks>0 and .bindings==.synced_bindings and .completed_outbox>=.synced_bindings and .dead_letters==0 and .quality_fixture_edges>0 and .repaired_by_edges>0 and ([.orphan_children,.cross_document_bindings,.cross_version_bindings,.cross_zone_bindings,.duplicate_chunk_ids,.duplicate_source_provenance_rows,.orphan_graph_endpoints,.cross_zone_graph_edges,.graph_self_edges]|all(.==0))' "$E/canonical-audit/integrity-summary.json" >/dev/null
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
  jq -e 'all(.access_zone_id,.document_id,.parent_a1,.parent_a3,.child_a1,.child_a3,.child_a3_alt,.child_a2; test("^[0-9a-fA-F-]{36}$"))' "$E/faults/targets.json" >/dev/null
}
run_control_pair() {
  local name=$1 expectation=$2 forbidden=${3:-} dir="$E/faults/$1" id q z
  local extra=()
  [[ -n "$forbidden" ]] && extra=(--forbidden-chunk-id "$forbidden")
  mkdir -p "$dir/search" "$dir/retrieve-context"
  id=$(jq -r '.[0].query.query_id' "$E/bank/selected-queries.json")
  q=$(jq -r '.[0].query.question' "$E/bank/selected-queries.json")
  z=$(jq -r '.access_zone_id' "$E/faults/targets.json")
  jq -n --arg z "$z" --arg q "$q" --arg id "$name" '{correlationId:("fix486g-control-"+$id),accessZoneId:$z,callerAccessLevel:"INTERNAL",query:$q,topK:5,candidateLimit:64,parentLimit:5,timeoutMs:30000,searchMode:"SEARCH_MODE_V005_HYBRID",embeddingMode:"EMBEDDING_MODE_V005_DENSE_SPARSE_IF_AVAILABLE",includeDebug:true,enableGraphExpansion:true,graphMaxHops:1,graphMaxRelatedContexts:5}' >"$dir/search/request.json"
  grpcurl -plaintext -d @ "$ADDR" astravector.embedding.v1.AstraVectorV004Control/Search <"$dir/search/request.json" >"$dir/search/response.json" || return 1
  python3 "$H" validate-control --entry-point Search --response "$dir/search/response.json" --identity-map "$E/identity-map/logical-to-runtime.json" --bank "$BANK" --graph-expectation "$expectation" "${extra[@]}" --output "$dir/search/result.json" >/dev/null || return 1
  jq -n --arg z "$z" --arg q "$q" --arg id "$name" '{context:{correlationId:("fix486g-control-"+$id),callerService:"fix486g",callerUserId:"fix486g",callerAccessLevel:"INTERNAL"},accessZoneId:$z,question:$q,profile:"RETRIEVAL_PROFILE_BALANCED",maxContexts:5,responseDetail:"RESPONSE_DETAIL_DEBUG",enableGraphExpansion:true,graphMaxHops:1,graphMaxRelatedContexts:5}' >"$dir/retrieve-context/request.json"
  grpcurl -plaintext -d @ "$ADDR" astravector.embedding.v1.AstraVectorRetrievalFacade/RetrieveContext <"$dir/retrieve-context/request.json" >"$dir/retrieve-context/response.json" || return 1
  python3 "$H" validate-control --entry-point RetrieveContext --response "$dir/retrieve-context/response.json" --identity-map "$E/identity-map/logical-to-runtime.json" --bank "$BANK" --graph-expectation "$expectation" "${extra[@]}" --output "$dir/retrieve-context/result.json" >/dev/null
}
binding_parent_fault() {
  local child parent_a1 parent_a3 source survivor edge rc=0
  child=$(jq -r .child_a3 "$E/faults/targets.json"); parent_a1=$(jq -r .parent_a1 "$E/faults/targets.json"); parent_a3=$(jq -r .parent_a3 "$E/faults/targets.json")
  source=$(jq -r .child_a1 "$E/faults/targets.json"); survivor=$(jq -r .child_a3_alt "$E/faults/targets.json"); edge=$(python3 -c 'import uuid; print(uuid.uuid4())')
  insert_fault_edge "$edge" "$source" "$survivor" REPAIRED_BY || return 1
  psql "$DB" -Atqc "UPDATE astravector.vector_bindings_v004 SET parent_chunk_id='$parent_a1' WHERE chunk_id='$child' AND representation_type='ORIGINAL' RETURNING id,parent_chunk_id" >"$E/faults/wrong-parent-activation.txt" || return 1
  run_control_pair wrong-parent present "$child" || rc=$?
  psql "$DB" -Atqc "UPDATE astravector.vector_bindings_v004 SET parent_chunk_id='$parent_a3' WHERE chunk_id='$child' AND representation_type='ORIGINAL'" || return 1
  delete_fault_edge "$edge" || return 1
  [[ $rc -eq 0 ]]
}
binding_status_fault() {
  local child source survivor edge rc=0
  child=$(jq -r .child_a3 "$E/faults/targets.json")
  source=$(jq -r .child_a1 "$E/faults/targets.json"); survivor=$(jq -r .child_a3_alt "$E/faults/targets.json"); edge=$(python3 -c 'import uuid; print(uuid.uuid4())')
  insert_fault_edge "$edge" "$source" "$survivor" REPAIRED_BY || return 1
  psql "$DB" -Atqc "UPDATE astravector.vector_bindings_v004 SET qdrant_sync_status='FAILED' WHERE chunk_id='$child' AND representation_type='ORIGINAL' RETURNING id" >"$E/faults/binding-invalid-activation.txt" || return 1
  run_control_pair binding-invalid present "$child" || rc=$?
  psql "$DB" -Atqc "UPDATE astravector.vector_bindings_v004 SET qdrant_sync_status='SYNCED' WHERE chunk_id='$child' AND representation_type='ORIGINAL'" || return 1
  delete_fault_edge "$edge" || return 1
  [[ $rc -eq 0 ]]
}
lifecycle_fault() {
  local kind=$1 child zone source survivor edge rc=0 restore=""
  child=$(jq -r .child_a3 "$E/faults/targets.json")
  zone=$(jq -r .access_zone_id "$E/faults/targets.json")
  source=$(jq -r .child_a1 "$E/faults/targets.json"); survivor=$(jq -r .child_a3_alt "$E/faults/targets.json"); edge=$(python3 -c 'import uuid; print(uuid.uuid4())')
  insert_fault_edge "$edge" "$source" "$survivor" REPAIRED_BY || return 1
  case "$kind" in
    inactive) psql "$DB" -Atqc "UPDATE astravector.content_chunks_v004 SET lifecycle_status='INACTIVE' WHERE access_zone_id='$zone' AND id='$child' RETURNING id" >"$E/faults/$kind-activation.txt"; restore="lifecycle_status='ACTIVE'";;
    deleted) psql "$DB" -Atqc "UPDATE astravector.content_chunks_v004 SET deleted_at=now() WHERE access_zone_id='$zone' AND id='$child' RETURNING id" >"$E/faults/$kind-activation.txt"; restore="deleted_at=NULL";;
    expired) psql "$DB" -Atqc "UPDATE astravector.content_chunks_v004 SET expires_at=now()-interval '1 hour' WHERE access_zone_id='$zone' AND id='$child' RETURNING id" >"$E/faults/$kind-activation.txt"; restore="expires_at=NULL";;
    *) return 64;;
  esac
  run_control_pair "$kind-target" present "$child" || rc=$?
  psql "$DB" -Atqc "UPDATE astravector.content_chunks_v004 SET $restore WHERE access_zone_id='$zone' AND id='$child'" || return 1
  delete_fault_edge "$edge" || return 1
  [[ $rc -eq 0 ]]
}
insert_fault_edge() {
  local edge_id=$1 source_chunk=$2 target_chunk=$3 relation=$4
  psql "$DB" -Atqc "INSERT INTO astravector.rag_graph_edges(access_zone_id,edge_id,source_node_type,source_node_id,target_node_type,target_node_id,relation_type,relation_score,relation_source,relation_rank,document_id,document_version,lifecycle_status,quarantined,properties) SELECT s.access_zone_id,'$edge_id','CHUNK',s.node_id,'CHUNK',t.node_id,'$relation',1.0,'PHASE_G_FAULT',0,c.document_id,c.document_version,'ACTIVE',false,jsonb_build_object('quality_run_id','$RUN_ID','phase_fault',true) FROM astravector.rag_graph_nodes_chunk s JOIN astravector.rag_graph_nodes_chunk t ON t.chunk_id='$target_chunk' JOIN astravector.content_chunks_v004 c ON c.access_zone_id=s.access_zone_id AND c.id=s.chunk_id WHERE s.chunk_id='$source_chunk' LIMIT 1 ON CONFLICT DO NOTHING" >/dev/null
}
delete_fault_edge() { psql "$DB" -Atqc "DELETE FROM astravector.rag_graph_edges WHERE edge_id='$1'" >/dev/null; }
cross_zone_fault() {
  local edge source target rc=0
  edge=$(python3 -c 'import uuid; print(uuid.uuid4())'); source=$(jq -r .child_a1 "$E/faults/targets.json"); target=$(jq -r '.rows[]|select(.logical_zone_id=="zone-b" and .logical_chunk_id=="child-a1-180")|.runtime_chunk_id' "$E/identity-map/logical-to-runtime.json" | head -1)
  insert_fault_edge "$edge" "$source" "$target" REPAIRED_BY || return 1
  run_control_pair cross-zone present "$target" || rc=$?
  delete_fault_edge "$edge" || return 1
  [[ $rc -eq 0 ]]
}
hop_limit_fault() {
  local edge source target rc=0
  edge=$(python3 -c 'import uuid; print(uuid.uuid4())'); source=$(jq -r .child_a3 "$E/faults/targets.json"); target=$(jq -r .child_a2 "$E/faults/targets.json")
  [[ "$target" =~ ^[0-9a-fA-F-]{36}$ ]] || return 1
  insert_fault_edge "$edge" "$source" "$target" REPAIRED_BY || return 1
  run_control_pair hop-limit present "$target" || rc=$?
  delete_fault_edge "$edge" || return 1
  [[ $rc -eq 0 ]]
}
cycle_fault() {
  local edge reverse self source target rc=0
  edge=$(python3 -c 'import uuid; print(uuid.uuid4())'); self=$(python3 -c 'import uuid; print(uuid.uuid4())'); source=$(jq -r .child_a3 "$E/faults/targets.json"); target=$(jq -r .child_a1 "$E/faults/targets.json")
  insert_fault_edge "$edge" "$source" "$target" RELATED_TO || return 1
  insert_fault_edge "$self" "$target" "$target" RELATED_TO || { delete_fault_edge "$edge"; return 1; }
  run_control_pair cycle present || rc=$?
  delete_fault_edge "$edge" || return 1; delete_fault_edge "$self" || return 1
  [[ $rc -eq 0 ]]
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
  ]}' >"$E/defect-register.json"
}
evidence_completeness() {
  local required=(query-results.jsonl graph-disabled/results.jsonl graph-audit/graph-identity-chain.json graph-audit/graph-provenance-trace.json comparisons/entry-point-parity.json comparisons/warm-repeat.json restart/pre-post-restart.json canonical-audit/integrity-summary.json qdrant-audit/payload-consistency.json cleanup/summary.json defect-register.json)
  for path in "${required[@]}"; do [[ -s "$E/$path" ]] || return 1; done
  [[ $(wc -l <"$E/query-results.jsonl" | tr -d ' ') -eq 2 ]] &&
  [[ $(wc -l <"$E/graph-disabled/results.jsonl" | tr -d ' ') -eq 2 ]]
}

jq -n --arg run_id "$RUN_ID" --arg mode "$MODE" --arg started "$(timestamp)" --arg branch "$BRANCH" --arg source "$SOURCE_SHA" --arg remote "$REMOTE_SHA" '{run_id:$run_id,mode:$mode,started_at_utc:$started,branch:$branch,source_sha:$source,remote_branch_sha:$remote,local_remote_equal:($source==$remote),status:"RUNNING"}' >"$E/bootstrap.json"
jq -n --arg branch "$BRANCH" --arg source "$SOURCE_SHA" --arg remote "$REMOTE_SHA" '{branch:$branch,source_sha:$source,remote_branch_sha:$remote,local_remote_equal:($source==$remote)}' >"$E/source/git-identity.json"

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
[[ "$ok" == true ]] && stage graph-audit graph_audit || ok=false
[[ "$ok" == true ]] && stage graph-disabled-control graph_disabled_control || ok=false
if [[ "$ok" == true ]] && stage primary-query-proof run_queries initial "$E/search" "$E/retrieve-context" "$E/query-results.jsonl" true; then record_stage_status search-proof PASS; record_stage_status retrieve-context-proof PASS; else ok=false; record_stage_status search-proof FAIL QUERY_PROOF_FAILED; record_stage_status retrieve-context-proof FAIL QUERY_PROOF_FAILED; fi
[[ "$ok" == true ]] && stage entry-point-comparison compare_initial || ok=false
[[ "$ok" == true ]] && stage fault-target-preparation prepare_fault_targets || ok=false
[[ "$ok" == true ]] && stage wrong-parent-fault binding_parent_fault || ok=false
[[ "$ok" == true ]] && stage binding-invalid-fault binding_status_fault || ok=false
[[ "$ok" == true ]] && stage inactive-target-fault lifecycle_fault inactive || ok=false
[[ "$ok" == true ]] && stage deleted-target-fault lifecycle_fault deleted || ok=false
[[ "$ok" == true ]] && stage expired-target-fault lifecycle_fault expired || ok=false
[[ "$ok" == true ]] && stage cross-zone-fault cross_zone_fault || ok=false
[[ "$ok" == true ]] && stage hop-limit-control hop_limit_fault || ok=false
[[ "$ok" == true ]] && stage cycle-control cycle_fault || ok=false
[[ "$ok" == true ]] && stage post-fault-canonical-audit canonical_audit || ok=false
[[ "$ok" == true ]] && stage warm-repeatability warm_repeat || ok=false
[[ "$ok" == true ]] && stage restart-repeatability restart_repeat || ok=false
write_defects
if cleanup; then record_stage_status cleanup PASS; else ok=false; record_stage_status cleanup FAIL CLEANUP_FAILED; fi
if evidence_completeness; then record_stage_status evidence-completeness PASS; else ok=false; record_stage_status evidence-completeness FAIL EVIDENCE_INCOMPLETE; fi
record_stage_status final-verdict "$([[ "$ok" == true ]] && echo PASS || echo FAIL)" "$([[ "$ok" == true ]] && echo '' || echo MANDATORY_STAGE_FAILED)"
jq -s --arg run_id "$RUN_ID" --arg source "$SOURCE_SHA" --arg bank "$BANK_SHA" --arg verdict "$([[ "$ok" == true ]] && echo FIX486_GRAPH_PARENT_RUNTIME_PROOF_PASS || echo FIX486_GRAPH_PARENT_RUNTIME_PROOF_BLOCKED)" '{schema_version:1,phase:"fix486g",run_id:$run_id,source_sha:$source,bank_version:"1.0.0",bank_aggregate_sha256:$bank,stages:.,verdict:$verdict}' "$E"/logs/*.stage.json >"$E/stage-results.json"
python3 "$H" aggregate --run "$E" --output "$E/aggregate.json" >/dev/null || ok=false
jq -n --argjson exit_code "$([[ "$ok" == true ]] && echo 0 || echo 1)" --arg finished "$(timestamp)" '{stage:"runner-terminal",status:(if $exit_code==0 then "PASS" else "FAIL" end),exit_code:$exit_code,finished_at_utc:$finished}' >"$E/terminal-result.json"
jq --arg status "$([[ "$ok" == true ]] && echo COMPLETED || echo BLOCKED)" '.status=$status' "$E/bootstrap.json" >"$E/bootstrap.tmp" && mv "$E/bootstrap.tmp" "$E/bootstrap.json"
python3 "$H" manifest --run "$E" --output "$E/manifest.json" >/dev/null
if ! python3 "$H" verify-manifest --run "$E" --manifest "$E/manifest.json" --output "$E/manifest-verification.json" >/dev/null; then
  ok=false
  record_stage_status final-verdict FAIL MANIFEST_INTEGRITY_FAILED
  jq -s --arg run_id "$RUN_ID" --arg source "$SOURCE_SHA" --arg bank "$BANK_SHA" \
    '{schema_version:1,phase:"fix486g",run_id:$run_id,source_sha:$source,bank_version:"1.0.0",bank_aggregate_sha256:$bank,stages:.,verdict:"FIX486_GRAPH_PARENT_RUNTIME_PROOF_BLOCKED"}' \
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
[[ "$ok" == true && "$verdict" == FIX486_GRAPH_PARENT_RUNTIME_PROOF_PASS ]]
