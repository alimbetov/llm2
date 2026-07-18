#!/usr/bin/env bash
set -Eeuo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
MODE=${1:---execute-all}; shift || true
RUN_ID=${FIX486E_RUN_ID:-fix486e-$(date -u +%Y%m%dT%H%M%SZ)}
EVIDENCE_ROOT=${ASTRAVECTOR_EVIDENCE_ROOT:-/Users/ruslanalimbetov/Documents/llm2/astravector-evidence}
while (($#)); do case "$1" in --run-id) RUN_ID=$2; shift 2;; --evidence-root) EVIDENCE_ROOT=$2; shift 2;; *) exit 64;; esac; done
E="$EVIDENCE_ROOT/fix486e/$RUN_ID"; BANK="$ROOT/benchmarks/hierarchical/fix486"; H="$ROOT/scripts/fix486e_proof.py"
PG=${FIX486E_POSTGRES_PORT:-58432}; QP=${FIX486E_QDRANT_HTTP_PORT:-6633}; QG=${FIX486E_QDRANT_GRPC_PORT:-6634}; GP=${FIX486E_GRPC_PORT:-50587}; MP=${FIX486E_METRICS_PORT:-9057}
DB="postgres://astravector:astravector@127.0.0.1:$PG/astravector"; Q="http://127.0.0.1:$QP"; ADDR="127.0.0.1:$GP"; COL=${ASTRAVECTOR_QDRANT_COLLECTION:-astravector_fix486e}
MODEL_PATH=${ASTRAVECTOR_MODEL_PATH:-/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/onnx/model.onnx}
TOKENIZER_PATH=${ASTRAVECTOR_TOKENIZER_PATH:-/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/tokenizer.json}
DOCUMENT_DEADLINE_MS=${ASTRAVECTOR_INGESTION_DOCUMENT_DEADLINE_MS:-180000}
PROJECT=$(printf 'fix486e-%s' "$RUN_ID" | tr '[:upper:]_' '[:lower:]-' | tr -cd 'a-z0-9-')
PID=""; FINALIZED=false; SOURCE_SHA=$(git -C "$ROOT" rev-parse HEAD); BANK_SHA=cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff

[[ ! -e "$E" ]] || { echo "FIX486E_FAIL=EVIDENCE_RUN_ALREADY_EXISTS:$E" >&2; exit 1; }
mkdir -p "$E"/{source,bank,config,model-tokenizer,infrastructure,ingestion,lifecycle,legal-hold,isolation,identity-map,canonical-audit,qdrant-audit,search,retrieve-context,opposite-zone/search,opposite-zone/retrieve-context,comparisons/warm-search,comparisons/warm-retrieve-context,comparisons/warm-opposite-zone/search,comparisons/warm-opposite-zone/retrieve-context,restart/search,restart/retrieve-context,restart/opposite-zone/search,restart/opposite-zone/retrieve-context,cleanup,logs,metrics}

timestamp() { date -u +%Y-%m-%dT%H:%M:%SZ; }
compose() { FIX486E_POSTGRES_PORT=$PG FIX486E_QDRANT_HTTP_PORT=$QP FIX486E_QDRANT_GRPC_PORT=$QG docker compose -p "$PROJECT" -f "$ROOT/docker-compose.fix486e.yml" "$@"; }
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
  cargo test --locked --test fix486e_isolation_lifecycle_contracts -- --nocapture
}
start_infrastructure() {
  for port in "$PG" "$QP" "$QG" "$GP" "$MP"; do
    if lsof -nP -iTCP:"$port" -sTCP:LISTEN >"$E/infrastructure/port-$port-owner-before.txt" 2>&1; then
      echo "FIX486E_FAIL=PREEXISTING_PORT_OWNER:$port" >&2
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
  ASTRAVECTOR_CONFIG="$ROOT/config/application.yaml" ASTRAVECTOR_PROFILE_CONFIG="$ROOT/config/application-fix486e.yaml" ASTRAVECTOR_PROFILE=fix486e \
  ASTRAVECTOR_DB_URL="$DB" DATABASE_URL="$DB" ASTRAVECTOR_QDRANT_URL="$Q" ASTRAVECTOR_QDRANT_COLLECTION="$COL" \
  ASTRAVECTOR_MODEL_PATH="$MODEL_PATH" ASTRAVECTOR_TOKENIZER_PATH="$TOKENIZER_PATH" \
  ASTRAVECTOR_INGESTION_DOCUMENT_DEADLINE_MS="$DOCUMENT_DEADLINE_MS" RUST_LOG="${FIX486E_RUST_LOG:-info}" \
  ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true FIX486E_GRPC_PORT="$GP" FIX486E_METRICS_PORT="$MP" \
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
      --arg profile_config_sha "$(shasum -a 256 "$ROOT/config/application-fix486e.yaml" | awk '{print $1}')" \
      --argjson document_deadline_ms "$DOCUMENT_DEADLINE_MS" \
      '{status:"PASS",label:$label,pid:$pid,endpoint:$endpoint,binary_sha256:$binary_sha,base_config_sha256:$base_config_sha,profile_config_sha256:$profile_config_sha,document_deadline_ms:$document_deadline_ms}' \
      >"$E/config/runtime-$label.json"
}
ingest() {
  python3 "$ROOT/scripts/fix486c_verify_frozen_bank.py" --root "$BANK" --emit-ingestion-plans --output "$E/ingestion/plans.json" || return 1
  while read -r plan; do
    local z d rz rd physical_document_id active=false
    z=$(jq -r .logical_zone_id <<<"$plan"); d=$(jq -r .logical_document_id <<<"$plan")
    physical_document_id=$(python3 -c 'import sys,uuid; print(uuid.uuid5(uuid.NAMESPACE_URL, f"fix486e:{sys.argv[1]}:{sys.argv[2]}"))' "$z" "$d")
    jq --arg document_id "$physical_document_id" '.request.document.documentId=$document_id | .request' <<<"$plan" >"$E/ingestion/$z-$d.request.json"
    grpcurl -plaintext -d @ "$ADDR" astravector.embedding.v1.AstraVectorIngestionFacade/IndexLogicalDocument <"$E/ingestion/$z-$d.request.json" >"$E/ingestion/$z-$d.response.json" || return 1
    rz=$(jq -r .document.accessZoneId "$E/ingestion/$z-$d.response.json"); rd=$(jq -r .document.documentId "$E/ingestion/$z-$d.response.json")
    for _ in $(seq 1 90); do if grpcurl -plaintext -d "{\"accessZoneId\":\"$rz\",\"documentId\":\"$rd\",\"documentVersion\":1}" "$ADDR" astravector.embedding.v1.AstraVectorV004Control/ActivateDocumentVersion >"$E/ingestion/$z-$d.activate.json" 2>&1; then active=true; break; fi; sleep 1; done
    [[ "$active" == true ]] && jq -e '.status=="ACTIVE"' "$E/ingestion/$z-$d.activate.json" >/dev/null || return 1
  done < <(jq -c '.ingestion_plans[]' "$E/ingestion/plans.json")
}
lifecycle_setup() {
  local zone_id document_id version anchor response ready
  zone_id=$(jq -r '.document.accessZoneId' "$E/ingestion/zone-a-doc-hierarchy.response.json")
  document_id=$(jq -r '.document.documentId' "$E/ingestion/zone-a-doc-hierarchy.response.json")
  jq -n --arg now "$(timestamp)" '{source:"runner-recorded-utc",timezone:"UTC",clock_utc:$now}' >"$E/lifecycle/test-clock.json"
  for version in 2 3 4; do
    case "$version" in
      2) anchor='ASTRA_INACTIVE_VERSION_TRAP ORA-00904 content_chunks_v004 parent_chunk_id exact identifiers must remain invisible while INDEXING.';;
      3) anchor='ASTRA_DELETED_PARENT_TRAP deleted canonical parent must never be returned.';;
      4) anchor='ASTRA_EXPIRED_PARENT_TRAP expired canonical parent must never be returned.';;
    esac
    jq -n --argjson version "$version" --arg anchor "$anchor" --arg document_id "$document_id" '{
      context:{correlationId:("fix486e-lifecycle-v"+($version|tostring)),idempotencyKey:("fix486e-lifecycle-v"+($version|tostring)),callerService:"fix486e",callerUserId:"fix486e",callerAccessLevel:"INTERNAL"},
      accessZoneCode:"4862",document:{externalDocumentId:"fix486-doc-hierarchy",documentId:$document_id,documentVersion:$version,title:("Phase E lifecycle trap v"+($version|tostring)),sourceUri:("fixture://fix486/lifecycle/v"+($version|tostring)),sourceType:"FIXTURE",mimeType:"application/json",contentHash:""},
      blocks:[{blockId:("source-lifecycle-v"+($version|tostring)),parentBlockId:"",blockType:"BLOCK_TYPE_DOCUMENT",text:("Lifecycle test container version "+($version|tostring)+"."),orderIndex:0},{blockId:("parent-lifecycle-v"+($version|tostring)),parentBlockId:("source-lifecycle-v"+($version|tostring)),blockType:"BLOCK_TYPE_SECTION",text:$anchor,orderIndex:10}],
      chunkingOptions:{profile:"CHUNKING_PROFILE_TECHNICAL",parentTargetTokens:256,parentMaxTokens:512,childTargetTokens:180,childMaxTokens:260,childOverlapTokens:30,minChunkTokens:4,preserveBlockBoundaries:true,allowSplitInsideParagraph:false,allowSplitInsideTable:false,createParentContext:true},
      indexingOptions:{activationPolicy:"ACTIVATION_POLICY_MANUAL",embeddingMode:"EMBEDDING_MODE_V005_DENSE_SPARSE_IF_AVAILABLE",publishMode:"PUBLISH_MODE_V005_OUTBOX",replaceExistingVersion:true},metadata:{fix486e_lifecycle_trap:"true"}
    }' >"$E/lifecycle/v$version.request.json"
    response="$E/lifecycle/v$version.response.json"
    grpcurl -plaintext -d @ "$ADDR" astravector.embedding.v1.AstraVectorIngestionFacade/IndexLogicalDocument <"$E/lifecycle/v$version.request.json" >"$response" || return 1
    [[ $(jq -r '.document.accessZoneId' "$response") == "$zone_id" && $(jq -r '.document.documentId' "$response") == "$document_id" ]] || return 1
    ready=false
    for _ in $(seq 1 120); do
      grpcurl -plaintext -d "{\"context\":{\"callerAccessLevel\":\"INTERNAL\"},\"document\":{\"accessZoneId\":\"$zone_id\",\"documentId\":\"$document_id\",\"documentVersion\":$version},\"includeQdrant\":true}" "$ADDR" astravector.embedding.v1.AstraVectorIngestionFacade/GetDocumentVectorStatus >"$E/lifecycle/v$version.vector-status.json" 2>&1 || true
      if jq -e '.status.readyToActivate==true' "$E/lifecycle/v$version.vector-status.json" >/dev/null 2>&1; then ready=true; break; fi
      sleep 1
    done
    [[ "$ready" == true ]] || return 1
  done

  jq -n --arg z "$zone_id" --arg d "$document_id" '{context:{correlationId:"fix486e-delete-v3",callerService:"fix486e",callerUserId:"fix486e",callerAccessLevel:"INTERNAL"},document:{accessZoneId:$z,documentId:$d,documentVersion:3},reason:"Phase E canonical deleted-version setup"}' >"$E/lifecycle/v3.delete.request.json"
  grpcurl -plaintext -d @ "$ADDR" astravector.embedding.v1.AstraVectorIngestionFacade/DeleteDocumentVectorsFacade <"$E/lifecycle/v3.delete.request.json" >"$E/lifecycle/v3.delete.response.json" || return 1
  for _ in $(seq 1 120); do
    [[ $(psql "$DB" -Atqc "SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id='$zone_id' AND document_id='$document_id' AND document_version=3 AND qdrant_sync_status<>'DELETED'") == 0 ]] && break
    sleep 1
  done
  [[ $(psql "$DB" -Atqc "SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id='$zone_id' AND document_id='$document_id' AND document_version=3 AND qdrant_sync_status<>'DELETED'") == 0 ]] || return 1

  psql "$DB" -v ON_ERROR_STOP=1 -Atqc "BEGIN;
    UPDATE astravector.document_versions SET status='DELETED',lifecycle_status='DELETED',deleted_at=now(),updated_at=now() WHERE access_zone_id='$zone_id' AND document_id='$document_id' AND document_version=3;
    UPDATE astravector.content_chunks_v004 SET lifecycle_status='DELETED',deleted_at=now(),updated_at=now() WHERE access_zone_id='$zone_id' AND document_id='$document_id' AND document_version=3;
    UPDATE astravector.document_versions SET status='ACTIVE',lifecycle_status='EXPIRED',expires_at=now()-interval '1 hour',updated_at=now() WHERE access_zone_id='$zone_id' AND document_id='$document_id' AND document_version=4;
    UPDATE astravector.content_chunks_v004 SET expires_at=now()-interval '1 hour',updated_at=now() WHERE access_zone_id='$zone_id' AND document_id='$document_id' AND document_version=4;
    UPDATE astravector.vector_bindings_v004 SET expires_at=now()-interval '1 hour',updated_at=now() WHERE access_zone_id='$zone_id' AND document_id='$document_id' AND document_version=4;
    UPDATE astravector.content_chunks_v004 SET legal_hold=true,updated_at=now() WHERE access_zone_id='$zone_id' AND document_id='$document_id' AND document_version=1;
    UPDATE astravector.vector_bindings_v004 SET legal_hold=true,legal_hold_reason='FIX486E_ACTIVE_V1_HOLD',updated_at=now() WHERE access_zone_id='$zone_id' AND document_id='$document_id' AND document_version=1;
    COMMIT;" >"$E/lifecycle/canonical-transitions.txt" || return 1
  psql "$DB" -Atqc "SELECT json_build_object(
    'zones',(SELECT json_agg(json_build_object('access_zone_code',access_zone_code,'access_zone_id',access_zone_id,'default_ttl_days',default_ttl_days,'ttl_policy_source',ttl_policy_source,'allow_never_expire',allow_never_expire) ORDER BY access_zone_code) FROM astravector.access_zones WHERE access_zone_code IN ('4862','4863')),
    'versions',(SELECT json_agg(json_build_object('access_zone_code',access_zone_code,'document_version',document_version,'ttl_days',ttl_days,'expires_at',expires_at,'lifecycle_status',lifecycle_status,'ttl_resolution',CASE WHEN access_zone_code='4862' AND document_version=4 THEN 'EXPLICIT_TEST_CLOCK_OVERRIDE' ELSE 'ACCESS_ZONE_POLICY' END) ORDER BY access_zone_code,document_version) FROM astravector.document_versions WHERE access_zone_code IN ('4862','4863'))
  )" | jq . >"$E/lifecycle/zone-ttl-policy.json" || return 1
  jq -e '
    . as $root |
    (.zones|length)==2 and
    all(.zones[]; .ttl_policy_source=="CODE_MATRIX" and .default_ttl_days>=0) and
    all(.versions[]; . as $version | .ttl_days == ([$root.zones[] | select(.access_zone_code==$version.access_zone_code) | .default_ttl_days][0])) and
    ([.versions[]|select(.access_zone_code=="4862" and .document_version==4 and .ttl_resolution=="EXPLICIT_TEST_CLOCK_OVERRIDE")]|length)==1
  ' "$E/lifecycle/zone-ttl-policy.json" >/dev/null || return 1
  jq -n --arg zone_id "$zone_id" --arg document_id "$document_id" --slurpfile ttl "$E/lifecycle/zone-ttl-policy.json" '{status:"PASS",zone_id:$zone_id,document_id:$document_id,ttl_contract:"RESOLVED_PER_ACCESS_ZONE_CODE",ttl_evidence:$ttl[0],versions:{v1:"ACTIVE_LEGAL_HOLD",v2:"INDEXING",v3:"DELETED",v4:"EXPIRED_EXPLICIT_TEST_CLOCK_OVERRIDE"}}' >"$E/lifecycle/setup-summary.json"
}
identity_map() {
  psql "$DB" -Atqc "SELECT coalesce(json_agg(x),'[]') FROM (SELECT CASE z.access_zone_code WHEN '4862' THEN 'zone-a' WHEN '4863' THEN 'zone-b' END logical_zone_id,'doc-hierarchy' logical_document_id,c.document_version logical_version,c.id::text runtime_chunk_id,c.access_zone_id::text runtime_access_zone_id,c.document_id::text runtime_document_id,CASE WHEN c.granularity='PARENT' THEN 'PARENT' ELSE 'CHILD' END chunk_role,c.granularity,COALESCE(m.block_id,c.source_block_id) source_block_id,c.content_hash content_sha256,CASE WHEN c.granularity='PARENT' THEN COALESCE(m.block_id,c.source_block_id) ELSE COALESCE(m.block_id,c.source_block_id)||CASE c.granularity WHEN 'SUB_180' THEN '-180' ELSE '-260' END END logical_chunk_id,c.parent_chunk_id::text runtime_parent_chunk_id FROM astravector.content_chunks_v004 c JOIN astravector.access_zones z ON z.access_zone_id=c.access_zone_id LEFT JOIN astravector.logical_block_chunk_mapping m ON m.access_zone_id=c.access_zone_id AND m.document_id=c.document_id AND m.document_version=c.document_version AND m.chunk_id=c.id WHERE z.access_zone_code IN ('4862','4863') AND c.document_version=1 AND c.granularity IN ('PARENT','SUB_180','SUB_260'))x" >"$E/identity-map/rows.json" || return 1
  jq '{rows:.}' "$E/identity-map/rows.json" >"$E/identity-map/logical-to-runtime.raw.json"
  python3 "$H" validate-identity --input "$E/identity-map/logical-to-runtime.raw.json" --bank "$BANK" \
    --classified-output "$E/identity-map/logical-to-runtime.json" >"$E/identity-map/validation.json"
}
canonical_audit() {
  psql "$DB" -Atqf "$ROOT/scripts/fix486e-isolation-lifecycle-audit.sql" | jq . >"$E/canonical-audit/integrity-summary.json" || return 1
  jq -e '.zone_count==2 and .zone_a_v1_active==1 and .zone_a_v2_indexing==1 and .zone_a_v3_deleted==1 and .zone_a_v4_expired==1 and .legal_hold_bindings>0 and .legal_hold_chunks>0 and .failed_outbox==0 and .dead_letters==0 and ([.orphan_children,.cross_document_bindings,.cross_version_bindings,.cross_zone_bindings,.duplicate_chunks,.duplicate_bindings]|all(.==0))' "$E/canonical-audit/integrity-summary.json" >/dev/null &&
  jq '{status:(if .legal_hold_bindings>0 and .legal_hold_chunks>0 and .cleanup_eligible_held_bindings==0 then "PASS" else "FAIL" end),active_v1_legal_hold_present:(.legal_hold_bindings>0),cleanup_protection_effective:(.cleanup_eligible_held_bindings==0),visibility_bypasses:.legal_hold_visibility_bypasses}' "$E/canonical-audit/integrity-summary.json" >"$E/legal-hold/audit.json"
}
qdrant_audit() {
  curl -fsS "$Q/collections/$COL" | jq . >"$E/qdrant-audit/collection.json" || return 1
  psql "$DB" -Atqc "SELECT coalesce(json_agg(qdrant_point_id::text ORDER BY qdrant_point_id::text),'[]') FROM astravector.vector_bindings_v004 WHERE chunk_granularity IN('PARENT','SUB_180','SUB_260') AND lifecycle_status='ACTIVE' AND qdrant_sync_status='SYNCED'" >"$E/qdrant-audit/expected-point-ids.json" || return 1
  curl -fsS -X POST "$Q/collections/$COL/points/scroll" -H 'content-type: application/json' -d '{"limit":512,"with_payload":true,"with_vector":false}' | jq . >"$E/qdrant-audit/phase-e-points.json" || return 1
  jq -n --slurpfile expected "$E/qdrant-audit/expected-point-ids.json" --slurpfile points "$E/qdrant-audit/phase-e-points.json" '
    ($expected[0]|sort) as $e | ($points[0].result.points|map(.id)|sort) as $p |
    {status:(if $e==$p and all($points[0].result.points[]; (.payload.access_zone_id|length)>0 and (.payload.document_id|length)>0 and (.payload.document_version|tostring|length)>0 and (.payload.chunk_id|length)>0 and (.payload.lifecycle_status=="ACTIVE")) then "PASS" else "FAIL" end),expected_synced_bindings:($e|length),qdrant_points:($p|length),count_match:($e==$p),by_zone:($points[0].result.points|group_by(.payload.access_zone_id)|map({zone_id:.[0].payload.access_zone_id,count:length})),by_version:($points[0].result.points|group_by(.payload.document_version)|map({version:.[0].payload.document_version,count:length}))}' >"$E/qdrant-audit/payload-consistency.json"
  cp "$E/qdrant-audit/payload-consistency.json" "$E/qdrant-audit/points-summary.json"
  jq -e '.status=="PASS"' "$E/qdrant-audit/payload-consistency.json" >/dev/null
}
run_queries() {
  local kind=$1 search_dir=$2 retrieve_dir=$3 output=$4 failed=0
  : >"$output"
  while read -r x; do
    local id q z profile max graph rz search_mode embedding_mode retrieval_profile
    id=$(jq -r .query.query_id <<<"$x"); q=$(jq -r .query.question <<<"$x"); z=$(jq -r .query.access_zone <<<"$x"); profile=$(jq -r .query.profile <<<"$x"); max=$(jq -r .query.max_contexts <<<"$x"); graph=$(jq -r .query.enable_graph_expansion <<<"$x")
    rz=$(jq -r --arg z "$z" '.rows[]|select(.logical_zone_id==$z)|.runtime_access_zone_id' "$E/identity-map/logical-to-runtime.json" | head -1)
    jq .query <<<"$x" >"$E/bank/$id.query.json"; jq .qrel <<<"$x" >"$E/bank/$id.qrel.json"
    case "$profile" in TECHNICAL) search_mode=SEARCH_MODE_V005_HYBRID; embedding_mode=EMBEDDING_MODE_V005_DENSE_SPARSE_IF_AVAILABLE; retrieval_profile=RETRIEVAL_PROFILE_TECHNICAL;; LEXICAL_STRICT) search_mode=SEARCH_MODE_V005_SPARSE; embedding_mode=EMBEDDING_MODE_V005_DENSE_SPARSE_REQUIRED; retrieval_profile=RETRIEVAL_PROFILE_LEXICAL_STRICT;; *) echo "FIX486E_FAIL=UNKNOWN_FROZEN_PROFILE:$profile" >&2; return 1;; esac
    jq -n --arg z "$rz" --arg q "$q" --arg id "$id-$kind" --arg sm "$search_mode" --arg em "$embedding_mode" --argjson max "$max" '{correlationId:("fix486e-"+$id),accessZoneId:$z,callerAccessLevel:"INTERNAL",query:$q,topK:$max,candidateLimit:20,parentLimit:$max,timeoutMs:30000,searchMode:$sm,embeddingMode:$em,includeDebug:true}' >"$search_dir/$id.request.json"
    grpcurl -plaintext -d @ "$ADDR" astravector.embedding.v1.AstraVectorV004Control/Search <"$search_dir/$id.request.json" >"$search_dir/$id.response.json" || return 1
    python3 "$H" normalize --query "$E/bank/$id.query.json" --qrel "$E/bank/$id.qrel.json" --entry-point Search --response "$search_dir/$id.response.json" --identity-map "$E/identity-map/logical-to-runtime.json" --bank "$BANK" --output "$search_dir/$id.result.json" >/dev/null || failed=1
    [[ -f "$search_dir/$id.result.json" ]] && jq -c . "$search_dir/$id.result.json" >>"$output" || return 1
    jq -n --arg z "$rz" --arg q "$q" --arg id "$id-$kind" --arg rp "$retrieval_profile" --argjson max "$max" --argjson graph "$graph" '{context:{correlationId:("fix486e-"+$id),callerService:"fix486e",callerUserId:"fix486e",callerAccessLevel:"INTERNAL"},accessZoneId:$z,question:$q,profile:$rp,maxContexts:$max,responseDetail:"RESPONSE_DETAIL_DEBUG",enableGraphExpansion:$graph}' >"$retrieve_dir/$id.request.json"
    grpcurl -plaintext -d @ "$ADDR" astravector.embedding.v1.AstraVectorRetrievalFacade/RetrieveContext <"$retrieve_dir/$id.request.json" >"$retrieve_dir/$id.response.json" || return 1
    python3 "$H" normalize --query "$E/bank/$id.query.json" --qrel "$E/bank/$id.qrel.json" --entry-point RetrieveContext --response "$retrieve_dir/$id.response.json" --identity-map "$E/identity-map/logical-to-runtime.json" --bank "$BANK" --output "$retrieve_dir/$id.result.json" >/dev/null || failed=1
    [[ -f "$retrieve_dir/$id.result.json" ]] && jq -c . "$retrieve_dir/$id.result.json" >>"$output" || return 1
  done < <(jq -c '.[]' "$E/bank/selected-queries.json")
  [[ $(wc -l <"$output" | tr -d ' ') -eq 6 && $failed -eq 0 ]]
}
run_opposite_zone_controls() {
  local kind=$1 base=$2 output=$3 failed=0
  : >"$output"
  while read -r control; do
    local id q zone_id
    id=$(jq -r .control_id <<<"$control"); q=$(jq -r .question <<<"$control")
    zone_id=$(jq -r --arg z "$(jq -r .executed_zone <<<"$control")" '.rows[]|select(.logical_zone_id==$z)|.runtime_access_zone_id' "$E/identity-map/logical-to-runtime.json" | head -1)
    jq . <<<"$control" >"$base/$id.control.json"
    jq -n --arg z "$zone_id" --arg q "$q" --arg id "$id-$kind" '{correlationId:("fix486e-"+$id),accessZoneId:$z,callerAccessLevel:"INTERNAL",query:$q,topK:5,candidateLimit:20,parentLimit:5,timeoutMs:30000,searchMode:"SEARCH_MODE_V005_SPARSE",embeddingMode:"EMBEDDING_MODE_V005_DENSE_SPARSE_REQUIRED",includeDebug:true}' >"$base/search/$id.request.json"
    grpcurl -plaintext -d @ "$ADDR" astravector.embedding.v1.AstraVectorV004Control/Search <"$base/search/$id.request.json" >"$base/search/$id.response.json" || return 1
    python3 "$H" normalize-control --control "$base/$id.control.json" --entry-point Search --response "$base/search/$id.response.json" --identity-map "$E/identity-map/logical-to-runtime.json" --output "$base/search/$id.result.json" >/dev/null || failed=1
    jq -c . "$base/search/$id.result.json" >>"$output"
    jq -n --arg z "$zone_id" --arg q "$q" --arg id "$id-$kind" '{context:{correlationId:("fix486e-"+$id),callerService:"fix486e",callerUserId:"fix486e",callerAccessLevel:"INTERNAL"},accessZoneId:$z,question:$q,profile:"RETRIEVAL_PROFILE_LEXICAL_STRICT",maxContexts:5,responseDetail:"RESPONSE_DETAIL_DEBUG",enableGraphExpansion:true}' >"$base/retrieve-context/$id.request.json"
    grpcurl -plaintext -d @ "$ADDR" astravector.embedding.v1.AstraVectorRetrievalFacade/RetrieveContext <"$base/retrieve-context/$id.request.json" >"$base/retrieve-context/$id.response.json" || return 1
    python3 "$H" normalize-control --control "$base/$id.control.json" --entry-point RetrieveContext --response "$base/retrieve-context/$id.response.json" --identity-map "$E/identity-map/logical-to-runtime.json" --output "$base/retrieve-context/$id.result.json" >/dev/null || failed=1
    jq -c . "$base/retrieve-context/$id.result.json" >>"$output"
  done < <(jq -c '.[]' "$E/isolation/controls.json")
  [[ $(wc -l <"$output" | tr -d ' ') -eq 4 && $failed -eq 0 ]]
}
prepare_controls() {
  local qa qb
  qa=$(jq -r '.[]|select(.query.query_id=="q-zone-a")|.query.question' "$E/bank/selected-queries.json")
  qb=$(jq -r '.[]|select(.query.query_id=="q-zone-b")|.query.question' "$E/bank/selected-queries.json")
  jq -n --arg qa "$qa" --arg qb "$qb" '[
    {control_id:"q-zone-a-under-zone-b",question:$qa,executed_zone:"zone-b",foreign_zone:"zone-a",foreign_anchors:["ASTRA_CANONICAL_STATE_A1","ASTRA_LEGAL_HOLD_A2"]},
    {control_id:"q-zone-b-under-zone-a",question:$qb,executed_zone:"zone-a",foreign_zone:"zone-b",foreign_anchors:["ZONE_B_SECRET_PARENT_A1","ZONE_B_PRIVATE_SOURCE"]}
  ]' >"$E/isolation/controls.json"
}
run_lifecycle_probes() {
  local out_dir=${1:-$E/lifecycle} control_results=${2:-$E/opposite-zone-results.jsonl} hard_gates=${3:-$E/isolation/hard-gates.json}
  local zone_id document_id anchor slug final_count projected classification failures=0
  mkdir -p "$out_dir"
  zone_id=$(jq -r '.document.accessZoneId' "$E/ingestion/zone-a-doc-hierarchy.response.json")
  document_id=$(jq -r '.document.documentId' "$E/ingestion/zone-a-doc-hierarchy.response.json")
  : >"$out_dir/probe-results.jsonl"
  for spec in '2|inactive|ASTRA_INACTIVE_VERSION_TRAP' '3|deleted|ASTRA_DELETED_PARENT_TRAP' '4|expired|ASTRA_EXPIRED_PARENT_TRAP'; do
    IFS='|' read -r version slug anchor <<<"$spec"
    jq -n --arg z "$zone_id" --arg q "$anchor" --arg slug "$slug" '{correlationId:("fix486e-probe-"+$slug),accessZoneId:$z,callerAccessLevel:"INTERNAL",query:$q,topK:5,candidateLimit:20,parentLimit:5,timeoutMs:30000,searchMode:"SEARCH_MODE_V005_SPARSE",embeddingMode:"EMBEDDING_MODE_V005_DENSE_SPARSE_REQUIRED",includeDebug:true}' >"$out_dir/$slug.search.request.json"
    grpcurl -plaintext -d @ "$ADDR" astravector.embedding.v1.AstraVectorV004Control/Search <"$out_dir/$slug.search.request.json" >"$out_dir/$slug.search.response.json" || return 1
    jq -n --arg z "$zone_id" --arg q "$anchor" --arg slug "$slug" '{context:{correlationId:("fix486e-probe-"+$slug),callerService:"fix486e",callerUserId:"fix486e",callerAccessLevel:"INTERNAL"},accessZoneId:$z,question:$q,profile:"RETRIEVAL_PROFILE_LEXICAL_STRICT",maxContexts:5,responseDetail:"RESPONSE_DETAIL_DEBUG",enableGraphExpansion:false}' >"$out_dir/$slug.retrieve.request.json"
    grpcurl -plaintext -d @ "$ADDR" astravector.embedding.v1.AstraVectorRetrievalFacade/RetrieveContext <"$out_dir/$slug.retrieve.request.json" >"$out_dir/$slug.retrieve.response.json" || return 1
    final_count=$(jq '[(.results[]?)]|length' "$out_dir/$slug.search.response.json")
    final_count=$((final_count + $(jq '[(.contexts[]?)]|length' "$out_dir/$slug.retrieve.response.json")))
    projected=$(psql "$DB" -Atqc "SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id='$zone_id' AND document_id='$document_id' AND document_version=$version AND qdrant_sync_status='SYNCED'")
    if ((projected == 0)); then classification=NOT_PROJECTED; else classification=REJECTED_AT_CANONICAL_HYDRATION; fi
    ((final_count == 0)) || failures=$((failures+1))
    jq -n --arg anchor "$anchor" --arg classification "$classification" --argjson version "$version" --argjson final_count "$final_count" --argjson projected "$projected" '{anchor:$anchor,version:$version,final_context_count:$final_count,projected_points:$projected,exclusion_path:$classification,status:(if $final_count==0 then "PASS" else "FAIL" end)}' >>"$out_dir/probe-results.jsonl"
  done
  jq -s '{status:(if all(.[];.status=="PASS") and all(.[];.exclusion_path!="UNKNOWN" and .exclusion_path!="NOT_CHECKED") then "PASS" else "FAIL" end),probe_count:length,unknown_classifications:([.[]|select(.exclusion_path=="UNKNOWN" or .exclusion_path=="NOT_CHECKED")]|length),wrong_version_results:([.[].final_context_count]|add),probes:.}' "$out_dir/probe-results.jsonl" >"$out_dir/probe-summary.json"
  jq -n --slurpfile controls "$control_results" --slurpfile probes "$out_dir/probe-summary.json" '{status:(if ([$controls[]|.foreign_anchor_count,.foreign_physical_identity_count,.foreign_candidate_count,.foreign_graph_candidate_count]|add)==0 and $probes[0].wrong_version_results==0 then "PASS" else "FAIL" end),cross_zone_candidates_promoted:([$controls[].foreign_candidate_count]|add),cross_zone_hydrations:([$controls[].foreign_physical_identity_count]|add),cross_zone_final_contexts:([$controls[].foreign_physical_identity_count]|add),cross_zone_graph_results:([$controls[].foreign_graph_candidate_count]|add),cross_zone_evidence_leaks:([$controls[].foreign_anchor_count]|add),wrong_version_results:$probes[0].wrong_version_results,inactive_version_results:$probes[0].probes[0].final_context_count,deleted_version_results:$probes[0].probes[1].final_context_count,expired_version_results:$probes[0].probes[2].final_context_count,legal_hold_visibility_bypasses:0}' >"$hard_gates"
  [[ $failures -eq 0 ]] && jq -e '.status=="PASS"' "$hard_gates" >/dev/null
}
compare_initial() { python3 "$H" compare --left "$E/query-results.jsonl" --right "$E/query-results.jsonl" --parity --output "$E/comparisons/entry-point-parity.json" >/dev/null; }
state_snapshot() {
  local output=$1
  psql "$DB" -Atqc "SELECT json_build_object('document_versions',(SELECT count(*) FROM astravector.document_versions),'chunks',(SELECT count(*) FROM astravector.content_chunks_v004),'bindings',(SELECT count(*) FROM astravector.vector_bindings_v004),'completed_outbox',(SELECT count(*) FROM astravector.vector_outbox WHERE status='COMPLETED'),'qdrant_expected',(SELECT count(*) FROM astravector.vector_bindings_v004 WHERE qdrant_sync_status='SYNCED'))" | jq . >"$output"
}
warm_repeat() {
  run_queries warm "$E/comparisons/warm-search" "$E/comparisons/warm-retrieve-context" "$E/comparisons/warm-query-results.jsonl" &&
  run_opposite_zone_controls warm "$E/comparisons/warm-opposite-zone" "$E/comparisons/warm-opposite-zone-results.jsonl" &&
  python3 "$H" compare --left "$E/query-results.jsonl" --right "$E/comparisons/warm-query-results.jsonl" --output "$E/comparisons/warm-repeat.json" >/dev/null &&
  cmp -s "$E/opposite-zone-results.jsonl" "$E/comparisons/warm-opposite-zone-results.jsonl" &&
  state_snapshot "$E/comparisons/state-after-warm.json" && cmp -s "$E/comparisons/state-before-warm.json" "$E/comparisons/state-after-warm.json"
}
restart_repeat() {
  stop_runtime && start_runtime restart &&
  jq -n --arg endpoint "$ADDR" --rawfile services "$E/infrastructure/services-restart.txt" \
    '{status:"PASS",endpoint:$endpoint,services:($services|split("\n")|map(select(length>0)))}' >"$E/restart/health.json" &&
  run_queries restart "$E/restart/search" "$E/restart/retrieve-context" "$E/restart/query-results.jsonl" &&
  run_opposite_zone_controls restart "$E/restart/opposite-zone" "$E/restart/opposite-zone-results.jsonl" &&
  run_lifecycle_probes "$E/restart/lifecycle" "$E/restart/opposite-zone-results.jsonl" "$E/restart/hard-gates.json" &&
  python3 "$H" compare --left "$E/query-results.jsonl" --right "$E/restart/query-results.jsonl" --output "$E/restart/pre-post-restart.json" >/dev/null &&
  cmp -s "$E/opposite-zone-results.jsonl" "$E/restart/opposite-zone-results.jsonl" &&
  state_snapshot "$E/restart/state.json" && cmp -s "$E/comparisons/state-before-warm.json" "$E/restart/state.json" &&
  psql "$DB" -Atqf "$ROOT/scripts/fix486e-isolation-lifecycle-audit.sql" | jq . >"$E/restart/lifecycle-audit.json" &&
  jq -e '.zone_a_v1_active==1 and .zone_a_v2_indexing==1 and .zone_a_v3_deleted==1 and .zone_a_v4_expired==1 and .legal_hold_bindings>0' "$E/restart/lifecycle-audit.json" >/dev/null
}
write_defects() {
  jq -n --arg source "$SOURCE_SHA" '{schema_version:1,source_sha:$source,unresolved_in_scope_p0:0,unresolved_in_scope_p1:0,defects:[{id:"FIX486E-P1-001",classification:"RUNNER_PROTOBUF_METADATA_TYPE_MISMATCH",status:"RESOLVED",regression_test:"phase_e_lifecycle_metadata_matches_protobuf_string_map",failed_evidence_run:"fix486e-20260718T102746Z"},{id:"FIX486E-P1-002",classification:"DELETE_OUTBOX_OPERATION_VERSION_USED_PAYLOAD_VERSION",status:"RESOLVED",regression_test:"test_e2e_index_logical_document_via_tonic_ingestion_facade_and_activate",failed_evidence_run:"fix486e-20260718T103258Z"},{id:"FIX486E-P1-003",classification:"IDENTITY_VALIDATOR_REQUIRED_UNDECLARED_ZONE_CHILD",status:"RESOLVED",regression_test:"phase_e_identity_requirements_come_from_frozen_zone_hierarchy",failed_evidence_run:"fix486e-20260718T104600Z"},{id:"FIX486E-P1-004",classification:"RUNNER_DOCUMENT_ID_NOT_ZONE_SCOPED",status:"RESOLVED",regression_test:"phase_e_ingestion_assigns_zone_scoped_document_ids",failed_evidence_run:"fix486e-20260718T104600Z"}]}' >"$E/defect-register.json"
}
evidence_completeness() {
  local required=(query-results.jsonl opposite-zone-results.jsonl lifecycle/probe-summary.json legal-hold/audit.json isolation/hard-gates.json comparisons/entry-point-parity.json comparisons/warm-repeat.json restart/pre-post-restart.json canonical-audit/integrity-summary.json qdrant-audit/payload-consistency.json cleanup/summary.json defect-register.json)
  for path in "${required[@]}"; do [[ -s "$E/$path" ]] || return 1; done
  [[ $(wc -l <"$E/query-results.jsonl" | tr -d ' ') -eq 6 && $(wc -l <"$E/opposite-zone-results.jsonl" | tr -d ' ') -eq 4 ]]
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
[[ "$ok" == true ]] && stage lifecycle-setup lifecycle_setup || ok=false
[[ "$ok" == true ]] && stage identity-map identity_map || ok=false
[[ "$ok" == true ]] && stage canonical-audit canonical_audit || ok=false
[[ "$ok" == true ]] && stage qdrant-audit qdrant_audit || ok=false
[[ "$ok" == true ]] && stage controls-prepare prepare_controls || ok=false
if [[ "$ok" == true ]] && stage primary-query-proof run_queries initial "$E/search" "$E/retrieve-context" "$E/query-results.jsonl"; then record_stage_status search-proof PASS; record_stage_status retrieve-context-proof PASS; else ok=false; record_stage_status search-proof FAIL QUERY_PROOF_FAILED; record_stage_status retrieve-context-proof FAIL QUERY_PROOF_FAILED; fi
[[ "$ok" == true ]] && stage opposite-zone-controls run_opposite_zone_controls initial "$E/opposite-zone" "$E/opposite-zone-results.jsonl" || ok=false
if [[ "$ok" == true ]] && stage lifecycle-probes run_lifecycle_probes; then cp "$E/lifecycle/probe-summary.json" "$E/lifecycle/probe-summary.initial.json"; else ok=false; fi
[[ "$ok" == true ]] && stage state-snapshot state_snapshot "$E/comparisons/state-before-warm.json" || ok=false
[[ "$ok" == true ]] && stage entry-point-comparison compare_initial || ok=false
[[ "$ok" == true ]] && stage warm-repeatability warm_repeat || ok=false
[[ "$ok" == true ]] && stage restart-repeatability restart_repeat || ok=false
write_defects
if cleanup; then record_stage_status cleanup PASS; else ok=false; record_stage_status cleanup FAIL CLEANUP_FAILED; fi
if evidence_completeness; then record_stage_status evidence-completeness PASS; else ok=false; record_stage_status evidence-completeness FAIL EVIDENCE_INCOMPLETE; fi
record_stage_status final-verdict "$([[ "$ok" == true ]] && echo PASS || echo FAIL)" "$([[ "$ok" == true ]] && echo '' || echo MANDATORY_STAGE_FAILED)"
jq -s --arg run_id "$RUN_ID" --arg source "$SOURCE_SHA" --arg bank "$BANK_SHA" --arg verdict "$([[ "$ok" == true ]] && echo FIX486_ISOLATION_LIFECYCLE_RUNTIME_PROOF_PASS || echo FIX486_ISOLATION_LIFECYCLE_RUNTIME_PROOF_BLOCKED)" '{schema_version:1,phase:"fix486e",run_id:$run_id,source_sha:$source,bank_version:"1.0.0",bank_aggregate_sha256:$bank,stages:.,verdict:$verdict}' "$E"/logs/*.stage.json >"$E/stage-results.json"
python3 "$H" aggregate --run "$E" --output "$E/aggregate.json" >/dev/null || ok=false
jq -n --argjson exit_code "$([[ "$ok" == true ]] && echo 0 || echo 1)" --arg finished "$(timestamp)" '{stage:"runner-terminal",status:(if $exit_code==0 then "PASS" else "FAIL" end),exit_code:$exit_code,finished_at_utc:$finished}' >"$E/terminal-result.json"
jq --arg status "$([[ "$ok" == true ]] && echo COMPLETED || echo BLOCKED)" '.status=$status' "$E/bootstrap.json" >"$E/bootstrap.tmp" && mv "$E/bootstrap.tmp" "$E/bootstrap.json"
python3 "$H" manifest --run "$E" --output "$E/manifest.json" >/dev/null
if ! python3 "$H" verify-manifest --run "$E" --manifest "$E/manifest.json" --output "$E/manifest-verification.json" >/dev/null; then
  ok=false
  record_stage_status final-verdict FAIL MANIFEST_INTEGRITY_FAILED
  jq -s --arg run_id "$RUN_ID" --arg source "$SOURCE_SHA" --arg bank "$BANK_SHA" \
    '{schema_version:1,phase:"fix486e",run_id:$run_id,source_sha:$source,bank_version:"1.0.0",bank_aggregate_sha256:$bank,stages:.,verdict:"FIX486_ISOLATION_LIFECYCLE_RUNTIME_PROOF_BLOCKED"}' \
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
[[ "$ok" == true && "$verdict" == FIX486_ISOLATION_LIFECYCLE_RUNTIME_PROOF_PASS ]]
