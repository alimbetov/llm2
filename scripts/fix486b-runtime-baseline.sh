#!/usr/bin/env bash
set -Eeuo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
RUN_ID=${FIX486B_RUN_ID:-fix486b-$(date -u +%Y%m%dT%H%M%SZ)}
EVIDENCE_ROOT=${FIX486B_EVIDENCE_ROOT:-/Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix486b}
EVIDENCE="$EVIDENCE_ROOT/$RUN_ID"
COMPOSE_FILE="$ROOT/docker-compose.fix486b.yml"
FIXTURE="$ROOT/benchmarks/hierarchical/fix486/runtime-baseline-control-v1.json"
OVERLAY="$ROOT/config/application-fix486b.yaml"
TARGET_DIR=${CARGO_TARGET_DIR:-$ROOT/target}
RUNTIME="$TARGET_DIR/release/astravector-runtime"
MODEL=${ASTRAVECTOR_MODEL_PATH:-/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/onnx/model.onnx}
TOKENIZER=${ASTRAVECTOR_TOKENIZER_PATH:-/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/tokenizer.json}
PG_PORT=${FIX486B_POSTGRES_PORT:-56432}
QDRANT_PORT=${FIX486B_QDRANT_HTTP_PORT:-6433}
QDRANT_GRPC_PORT=${FIX486B_QDRANT_GRPC_PORT:-6434}
GRPC_PORT=${FIX486B_GRPC_PORT:-50486}
METRICS_PORT=${FIX486B_METRICS_PORT:-9046}
COLLECTION=${ASTRAVECTOR_QDRANT_COLLECTION:-astravector_fix486b}
DB_URL="postgres://astravector:astravector@127.0.0.1:$PG_PORT/astravector"
GRPC_ADDR="127.0.0.1:$GRPC_PORT"
QDRANT_URL="http://127.0.0.1:$QDRANT_PORT"
RUNTIME_PID=""
ACTIVE_PROJECT=""
FAILURES=()

mkdir -p "$EVIDENCE"/{environment,source,static,infrastructure,migrations,model-tokenizer,build,runtime,fixture,ingestion,retrieval,restart,dependency-recovery,comparisons,logs,metrics}

timestamp() { date -u +%Y-%m-%dT%H:%M:%SZ; }
sha256() { shasum -a 256 "$1" | awk '{print $1}'; }
fail() { FAILURES+=("$1"); printf 'FIX486B_FAIL=%s\n' "$1" >&2; }

record() {
  local name=$1
  shift
  local log="$EVIDENCE/logs/$name.log"
  local started finished rc
  started=$(timestamp)
  set +e
  (set -o pipefail; "$@") >"$log" 2>&1
  rc=$?
  set -e
  finished=$(timestamp)
  jq -n --arg stage "$name" --arg started "$started" --arg finished "$finished" \
    --arg command "$(printf '%q ' "$@")" --argjson exit_code "$rc" \
    '{stage:$stage,started_at:$started,finished_at:$finished,command:$command,exit_code:$exit_code}' \
    >"$EVIDENCE/logs/$name.json"
  cat "$log"
  return "$rc"
}

compose() {
  FIX486B_POSTGRES_PORT=$PG_PORT FIX486B_QDRANT_HTTP_PORT=$QDRANT_PORT \
    FIX486B_QDRANT_GRPC_PORT=$QDRANT_GRPC_PORT \
    docker compose -p "$ACTIVE_PROJECT" -f "$COMPOSE_FILE" "$@"
}

cleanup() {
  if [[ -n "$RUNTIME_PID" ]] && kill -0 "$RUNTIME_PID" 2>/dev/null; then
    kill -INT "$RUNTIME_PID" 2>/dev/null || true
    for _ in $(seq 1 20); do
      kill -0 "$RUNTIME_PID" 2>/dev/null || break
      sleep 1
    done
    kill -TERM "$RUNTIME_PID" 2>/dev/null || true
    wait "$RUNTIME_PID" 2>/dev/null || true
  fi
  RUNTIME_PID=""
}
trap cleanup EXIT

wait_http() {
  local url=$1
  for _ in $(seq 1 90); do
    curl -fsS "$url" >/dev/null 2>&1 && return 0
    sleep 1
  done
  return 1
}

wait_postgres() {
  for _ in $(seq 1 90); do
    psql "$DB_URL" -Atqc 'SELECT 1' >/dev/null 2>&1 && return 0
    sleep 1
  done
  return 1
}

health_status() {
  grpcurl -plaintext -d '{"service":"astravector.embedding.v1.AstraVectorV004Control"}' \
    "$GRPC_ADDR" grpc.health.v1.Health/Check 2>/dev/null | jq -r '.status // "UNKNOWN"'
}

wait_health() {
  local expected=$1
  for _ in $(seq 1 40); do
    [[ "$(health_status || true)" == "$expected" ]] && return 0
    sleep 1
  done
  return 1
}

runtime_env() {
  env \
    ASTRAVECTOR_CONFIG="$ROOT/config/application.yaml" \
    ASTRAVECTOR_PROFILE_CONFIG="$OVERLAY" \
    ASTRAVECTOR_PROFILE=fix486b \
    ASTRAVECTOR_DB_URL="$DB_URL" DATABASE_URL="$DB_URL" \
    ASTRAVECTOR_QDRANT_URL="$QDRANT_URL" ASTRAVECTOR_QDRANT_COLLECTION="$COLLECTION" \
    ASTRAVECTOR_MODEL_PATH="$MODEL" ASTRAVECTOR_TOKENIZER_PATH="$TOKENIZER" \
    ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true \
    ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_SEARCH=false \
    FIX486B_GRPC_PORT="$GRPC_PORT" FIX486B_METRICS_PORT="$METRICS_PORT" \
    RUST_LOG=${RUST_LOG:-info} "$@"
}

start_runtime() {
  local run=$1
  local log="$EVIDENCE/runtime/${run,,}-runtime.log"
  runtime_env "$RUNTIME" >"$log" 2>&1 &
  RUNTIME_PID=$!
  echo "$RUNTIME_PID" >"$EVIDENCE/runtime/${run,,}-runtime.pid"
  for _ in $(seq 1 90); do
    grpcurl -plaintext "$GRPC_ADDR" list >/dev/null 2>&1 && return 0
    kill -0 "$RUNTIME_PID" 2>/dev/null || return 1
    sleep 1
  done
  return 1
}

stop_runtime() {
  cleanup
  if lsof -nP -iTCP:"$GRPC_PORT" -sTCP:LISTEN >"$EVIDENCE/runtime/ports-after-stop.txt" 2>&1; then
    return 1
  fi
}

snapshot() {
  local output=$1
  psql "$DB_URL" -Atqc "
    WITH z AS (SELECT access_zone_id FROM astravector.access_zones WHERE access_zone_code='4861')
    SELECT json_build_object(
      'access_zone_id',(SELECT access_zone_id::text FROM z LIMIT 1),
      'documents',(SELECT count(*) FROM astravector.document_versions WHERE access_zone_id IN(SELECT access_zone_id FROM z)),
      'active_documents',(SELECT count(*) FROM astravector.document_versions WHERE access_zone_id IN(SELECT access_zone_id FROM z) AND status='ACTIVE'),
      'chunks',(SELECT count(*) FROM astravector.content_chunks_v004 WHERE access_zone_id IN(SELECT access_zone_id FROM z)),
      'source_chunks',(SELECT count(*) FROM astravector.content_chunks_v004 WHERE access_zone_id IN(SELECT access_zone_id FROM z) AND granularity='SOURCE'),
      'parent_chunks',(SELECT count(*) FROM astravector.content_chunks_v004 WHERE access_zone_id IN(SELECT access_zone_id FROM z) AND granularity='PARENT'),
      'child_chunks',(SELECT count(*) FROM astravector.content_chunks_v004 WHERE access_zone_id IN(SELECT access_zone_id FROM z) AND granularity IN('SUB_180','SUB_260')),
      'bindings',(SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id IN(SELECT access_zone_id FROM z)),
      'synced_bindings',(SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id IN(SELECT access_zone_id FROM z) AND qdrant_sync_status='SYNCED'),
      'completed_outbox',(SELECT count(*) FROM astravector.vector_outbox WHERE binding_access_zone_id IN(SELECT access_zone_id FROM z) AND status='COMPLETED'),
      'dead_letters',(SELECT count(*) FROM astravector.vector_outbox WHERE binding_access_zone_id IN(SELECT access_zone_id FROM z) AND status='DEAD_LETTER'),
      'orphan_children',(SELECT count(*) FROM astravector.content_chunks_v004 c WHERE c.access_zone_id IN(SELECT access_zone_id FROM z) AND c.granularity IN('SUB_180','SUB_260') AND NOT EXISTS(SELECT 1 FROM astravector.content_chunks_v004 p WHERE p.access_zone_id=c.access_zone_id AND p.id=c.parent_chunk_id AND p.granularity='PARENT')),
      'duplicate_chunks',(SELECT count(*) FROM (SELECT granularity,representation_type,sequence_no,count(*) FROM astravector.content_chunks_v004 WHERE access_zone_id IN(SELECT access_zone_id FROM z) GROUP BY granularity,representation_type,sequence_no HAVING count(*)>1)d),
      'duplicate_bindings',(SELECT count(*) FROM (SELECT chunk_id,representation_type,count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id IN(SELECT access_zone_id FROM z) GROUP BY chunk_id,representation_type HAVING count(*)>1)d),
      'duplicate_outbox_effects',(SELECT count(*) FROM (SELECT binding_access_zone_id,binding_id,operation,operation_version,count(*) FROM astravector.vector_outbox WHERE binding_access_zone_id IN(SELECT access_zone_id FROM z) GROUP BY binding_access_zone_id,binding_id,operation,operation_version HAVING count(*)>1)d)
    )" | jq . >"$output"
}

wait_indexed() {
  local zone=$1 doc=$2
  for _ in $(seq 1 120); do
    local ready
    ready=$(psql "$DB_URL" -Atqc "SELECT count(*) FROM astravector.vector_bindings_v004 WHERE access_zone_id='$zone' AND document_id='$doc' AND document_version=1 AND qdrant_sync_status='SYNCED'" 2>/dev/null || echo 0)
    [[ "$ready" -gt 0 ]] && return 0
    sleep 1
  done
  return 1
}

ingest_and_probe() {
  local run=$1 dir="$EVIDENCE/ingestion/${1,,}"
  mkdir -p "$dir" "$EVIDENCE/retrieval/${run,,}"
  jq '.request' "$FIXTURE" >"$dir/request.json"
  grpcurl -plaintext -d @ "$GRPC_ADDR" \
    astravector.embedding.v1.AstraVectorIngestionFacade/IndexLogicalDocument \
    <"$dir/request.json" >"$dir/first-response.json"
  local zone doc
  zone=$(jq -r '.document.accessZoneId' "$dir/first-response.json")
  doc=$(jq -r '.document.documentId' "$dir/first-response.json")
  [[ -n "$zone" && "$zone" != null && -n "$doc" && "$doc" != null ]]
  wait_indexed "$zone" "$doc"
  grpcurl -plaintext -d "{\"accessZoneId\":\"$zone\",\"documentId\":\"$doc\",\"documentVersion\":1}" \
    "$GRPC_ADDR" astravector.embedding.v1.AstraVectorV004Control/ActivateDocumentVersion \
    >"$dir/activate-response.json"
  snapshot "$dir/before-repeat.json"
  grpcurl -plaintext -d @ "$GRPC_ADDR" \
    astravector.embedding.v1.AstraVectorIngestionFacade/IndexLogicalDocument \
    <"$dir/request.json" >"$dir/repeat-response.json"
  sleep 2
  snapshot "$dir/after-repeat.json"
  jq -e --slurp '.[0] == .[1]' "$dir/before-repeat.json" "$dir/after-repeat.json" >/dev/null

  probe_existing "$run" "$zone" "$doc"
  jq -n --arg zone "$zone" --arg doc "$doc" '{access_zone_id:$zone,document_id:$doc,document_version:1}' >"$EVIDENCE/fixture/${run,,}-physical-identity.json"
  psql "$DB_URL" -Atqc "SELECT coalesce(json_agg(x ORDER BY x.granularity,x.sequence_no),'[]') FROM (SELECT id::text chunk_id,root_chunk_id::text,source_chunk_id::text,parent_chunk_id::text,granularity,sequence_no,content_hash::text FROM astravector.content_chunks_v004 WHERE access_zone_id='$zone' AND document_id='$doc')x" | jq . >"$EVIDENCE/fixture/${run,,}-hierarchy.json"
  curl -fsS "$QDRANT_URL/collections/$COLLECTION" | jq . >"$EVIDENCE/retrieval/${run,,}/qdrant-collection.json"
}

probe_existing() {
  local run=$1 zone=$2 doc=$3
  mkdir -p "$EVIDENCE/retrieval/${run,,}"

  jq -n --arg zone "$zone" '{correlationId:"fix486b-search",accessZoneId:$zone,callerAccessLevel:"INTERNAL",query:"ASTRA_FIX486B_RUNTIME_CONTROL canonical state",topK:5,candidateLimit:20,parentLimit:5,timeoutMs:5000,searchMode:"SEARCH_MODE_V005_HYBRID",embeddingMode:"EMBEDDING_MODE_V005_DENSE_SPARSE_IF_AVAILABLE",includeDebug:true}' >"$EVIDENCE/retrieval/${run,,}/search-request.json"
  grpcurl -plaintext -d @ "$GRPC_ADDR" astravector.embedding.v1.AstraVectorV004Control/Search \
    <"$EVIDENCE/retrieval/${run,,}/search-request.json" >"$EVIDENCE/retrieval/${run,,}/search-response.json"
  jq -e --arg doc "$doc" --arg zone "$zone" '.results|length>0 and any(.[]; .documentId==$doc and .accessZoneId==$zone and (.matchedChunkId|length)>0 and (.parentChunkId|length)>0 and (.parentText|length)>0 and (.matchedText|length)>0)' "$EVIDENCE/retrieval/${run,,}/search-response.json" >/dev/null

  jq -n --arg zone "$zone" '{context:{correlationId:"fix486b-retrieve",callerService:"fix486b-runtime-baseline",callerUserId:"fix486b-runtime-baseline",callerAccessLevel:"INTERNAL"},accessZoneId:$zone,question:"ASTRA_FIX486B_RUNTIME_CONTROL canonical state",profile:"RETRIEVAL_PROFILE_BALANCED",maxContexts:5,responseDetail:"RESPONSE_DETAIL_DEBUG",enableGraphExpansion:false}' >"$EVIDENCE/retrieval/${run,,}/retrieve-request.json"
  grpcurl -plaintext -d @ "$GRPC_ADDR" astravector.embedding.v1.AstraVectorRetrievalFacade/RetrieveContext \
    <"$EVIDENCE/retrieval/${run,,}/retrieve-request.json" >"$EVIDENCE/retrieval/${run,,}/retrieve-response.json"
  jq -e --arg doc "$doc" --arg zone "$zone" '.contexts|length>0 and any(.[]; .documentId==$doc and .accessZoneId==$zone and (.matchedChunkId|length)>0 and (.parentChunkId|length)>0 and (.parentText|length)>0 and (.matchedText|length)>0)' "$EVIDENCE/retrieval/${run,,}/retrieve-response.json" >/dev/null
}

start_infrastructure() {
  local run=$1
  ACTIVE_PROJECT=$(printf 'fix486b-%s-%s' "$RUN_ID" "$run" | tr '[:upper:]_' '[:lower:]-' | tr -cd 'a-z0-9-')
  compose up -d >"$EVIDENCE/infrastructure/${run,,}-compose-up.log" 2>&1
  wait_postgres
  wait_http "$QDRANT_URL/readyz"
  compose ps --format json >"$EVIDENCE/infrastructure/${run,,}-compose-ps.json"
  docker inspect "${ACTIVE_PROJECT}-postgres-1" >"$EVIDENCE/infrastructure/${run,,}-postgres-inspect.json"
  docker inspect "${ACTIVE_PROJECT}-qdrant-1" >"$EVIDENCE/infrastructure/${run,,}-qdrant-inspect.json"
}

migrate() {
  local run=$1
  DATABASE_URL="$DB_URL" cargo sqlx migrate run --source migrations >"$EVIDENCE/migrations/${run,,}-clean-apply.log" 2>&1
  DATABASE_URL="$DB_URL" cargo sqlx migrate run --source migrations >"$EVIDENCE/migrations/${run,,}-reapply.log" 2>&1
  psql "$DB_URL" -Atqc "SELECT json_build_object('count',count(*),'failed',count(*) FILTER(WHERE NOT success),'head',max(version)) FROM _sqlx_migrations" | jq . >"$EVIDENCE/migrations/${run,,}-migration-head.json"
  jq -e '.failed==0' "$EVIDENCE/migrations/${run,,}-migration-head.json" >/dev/null
}

run_clean() {
  local run=$1
  start_infrastructure "$run"
  migrate "$run"
  start_runtime "$run"
  grpcurl -plaintext "$GRPC_ADDR" list >"$EVIDENCE/runtime/${run,,}-services.txt"
  curl -fsS "http://127.0.0.1:$METRICS_PORT/metrics" >"$EVIDENCE/metrics/${run,,}-metrics.prom"
  if ! wait_health SERVING; then
    grpcurl -plaintext -d '{"service":"astravector.embedding.v1.AstraVectorV004Control"}' "$GRPC_ADDR" grpc.health.v1.Health/Check >"$EVIDENCE/runtime/${run,,}-health-failed.json" 2>&1 || true
    fail "${run}_HEALTH_NOT_SERVING"
    return 1
  fi
  ingest_and_probe "$run"
  snapshot "$EVIDENCE/fixture/${run,,}-snapshot.json"
  stop_runtime
}

record_identity() {
  git status -sb >"$EVIDENCE/source/worktree-status.txt"
  [[ -z "$(git status --porcelain)" ]] || { fail DIRTY_WORKTREE; return 1; }
  jq -n --arg branch "$(git branch --show-current)" --arg source "$(git rev-parse HEAD)" \
    --arg main "$(git rev-parse origin/main)" --arg epic "$(git rev-parse origin/epic/fix486-hierarchical-retrieval-validation)" \
    '{branch:$branch,source_sha:$source,origin_main_sha:$main,epic_sha:$epic}' >"$EVIDENCE/source/git-identity.json"
  sha256 "$ROOT/Cargo.lock" >"$EVIDENCE/source/cargo-lock.sha256"
  sha256 "$FIXTURE" >"$EVIDENCE/fixture/control-input.sha256"
  cp "$FIXTURE" "$EVIDENCE/fixture/control-input.json"
  jq -n --arg model "$MODEL" --arg model_sha "$(sha256 "$MODEL")" --arg tokenizer "$TOKENIZER" --arg tokenizer_sha "$(sha256 "$TOKENIZER")" \
    '{model_path:$model,model_sha256:$model_sha,tokenizer_path:$tokenizer,tokenizer_sha256:$tokenizer_sha,dense_dimension:1024,sparse_capability_recorded:true}' >"$EVIDENCE/model-tokenizer/identities.json"
  { uname -a; sw_vers; } >"$EVIDENCE/environment/os.txt"
  { rustc --version; cargo --version; cargo sqlx --version; docker --version; docker compose version; grpcurl --version; jq --version; } >"$EVIDENCE/environment/tools.txt" 2>&1
  system_profiler SPHardwareDataType -json >"$EVIDENCE/environment/hardware.json"
  { cat "$ROOT/config/application.yaml"; cat "$OVERLAY"; printf '%s\n' "$DB_URL" "$QDRANT_URL" "$COLLECTION" "$MODEL" "$TOKENIZER"; } | shasum -a 256 | awk '{print $1}' >"$EVIDENCE/build/resolved-config.sha256"
}

run_static_gates() {
  record static-fmt cargo fmt --all --check || fail STATIC_FMT_FAILED
  record static-check env CARGO_TARGET_DIR="$TARGET_DIR" cargo check --locked --all-targets --all-features || fail STATIC_CHECK_FAILED
  record static-tests env CARGO_TARGET_DIR="$TARGET_DIR" cargo test --locked --all-targets --all-features || fail STATIC_TESTS_FAILED
  record static-clippy env CARGO_TARGET_DIR="$TARGET_DIR" cargo clippy --locked --all-targets --all-features -- -D warnings || fail STATIC_CLIPPY_FAILED
  record static-sqlx env CARGO_TARGET_DIR="$TARGET_DIR" cargo sqlx prepare --check -- --all-targets --all-features || fail SQLX_PREPARE_FAILED
  record test-e2e env CARGO_TARGET_DIR="$TARGET_DIR" cargo test --locked --features integration-tests --test e2e_testcontainers -- --nocapture || fail E2E_TESTCONTAINERS_FAILED
  record test-concurrency env CARGO_TARGET_DIR="$TARGET_DIR" cargo test --locked --features integration-tests --test smoke_load_retrieve_context_testcontainers -- --nocapture || fail CONCURRENCY_TESTCONTAINERS_FAILED
  record test-bank env CARGO_TARGET_DIR="$TARGET_DIR" cargo test --locked --test fix486_hierarchical_bank_contracts -- --nocapture || fail FIX486_BANK_CONTRACT_FAILED
  ((${#FAILURES[@]} == 0))
}

build_release() {
  record release-build env CARGO_TARGET_DIR="$TARGET_DIR" cargo build --locked --release --bin astravector-runtime || { fail RELEASE_BUILD_FAILED; return 1; }
  sha256 "$RUNTIME" >"$EVIDENCE/build/binary.sha256"
}

run_r3() {
  local zone doc
  zone=$(jq -r '.access_zone_id' "$EVIDENCE/fixture/r2-physical-identity.json")
  doc=$(jq -r '.document_id' "$EVIDENCE/fixture/r2-physical-identity.json")
  start_runtime R3
  wait_health SERVING
  probe_existing R3-restart "$zone" "$doc"
  compose stop qdrant >"$EVIDENCE/dependency-recovery/qdrant-stop.log" 2>&1
  wait_health NOT_SERVING
  health_status >"$EVIDENCE/dependency-recovery/qdrant-down-health.txt"
  compose start qdrant >"$EVIDENCE/dependency-recovery/qdrant-start.log" 2>&1
  wait_http "$QDRANT_URL/readyz"
  wait_health SERVING
  compose stop postgres >"$EVIDENCE/dependency-recovery/postgres-stop.log" 2>&1
  wait_health NOT_SERVING
  health_status >"$EVIDENCE/dependency-recovery/postgres-down-health.txt"
  compose start postgres >"$EVIDENCE/dependency-recovery/postgres-start.log" 2>&1
  wait_postgres
  wait_health SERVING
  probe_existing R3-recovered "$zone" "$doc"
  stop_runtime
}

finalize() {
  local verdict=FIX486_RUNTIME_BASELINE_PASS
  ((${#FAILURES[@]} == 0)) || verdict=FIX486_RUNTIME_BASELINE_BLOCKED
  jq -n --arg run_id "$RUN_ID" --arg verdict "$verdict" --argjson failures "$(printf '%s\n' "${FAILURES[@]:-}" | jq -Rsc 'split("\n")|map(select(length>0))')" \
    '{schema_version:1,run_id:$run_id,phase:"FIX486B_REPRODUCIBLE_RUNTIME_BASELINE",verdict:$verdict,failure_codes:$failures,bank_version:"0.1.0-analysis-seed",bank_frozen:false}' >"$EVIDENCE/stage-results.json"
  find "$EVIDENCE" -type f ! -name manifest.json -print0 | sort -z | xargs -0 shasum -a 256 | \
    jq -Rsc 'split("\n")|map(select(length>0)|capture("^(?<sha256>[0-9a-f]+)  (?<path>.*)$"))' >"$EVIDENCE/manifest.json"
  cat >"$EVIDENCE/FIX486B-RUNTIME-BASELINE-RESULT.md" <<EOF
# FIX486B runtime baseline result

Run: $RUN_ID

Verdict: \`$verdict\`

Bank remains \`0.1.0-analysis-seed\` and is not frozen.
EOF
  printf '%s\n' "$verdict"
  [[ "$verdict" == FIX486_RUNTIME_BASELINE_PASS ]]
}

main() {
  cd "$ROOT"
  record_identity || { finalize; return 1; }
  lsof -nP -iTCP:"$GRPC_PORT" -iTCP:"$METRICS_PORT" -iTCP:"$PG_PORT" -iTCP:"$QDRANT_PORT" -sTCP:LISTEN >"$EVIDENCE/environment/ports-before.txt" 2>&1 && { fail PREEXISTING_PORT_OWNER; finalize; return 1; } || true
  run_static_gates || { finalize; return 1; }
  build_release || { finalize; return 1; }
  run_clean R1 || { finalize; return 1; }
  compose down -v >"$EVIDENCE/infrastructure/r1-compose-down.log" 2>&1
  run_clean R2 || { finalize; return 1; }
  jq -n --slurpfile r1 "$EVIDENCE/fixture/r1-snapshot.json" --slurpfile r2 "$EVIDENCE/fixture/r2-snapshot.json" \
    --slurpfile h1 "$EVIDENCE/fixture/r1-hierarchy.json" --slurpfile h2 "$EVIDENCE/fixture/r2-hierarchy.json" \
    '{snapshot_match:($r1[0]==$r2[0]),hierarchy_match:($h1[0]==$h2[0]),r1:$r1[0],r2:$r2[0]}' >"$EVIDENCE/comparisons/r1-r2-normalized.json"
  jq -e '.snapshot_match and .hierarchy_match' "$EVIDENCE/comparisons/r1-r2-normalized.json" >/dev/null || { fail NONDETERMINISTIC_RUNTIME_RESULT; finalize; return 1; }
  run_r3 || fail R3_RECOVERY_FAILED
  compose down -v >"$EVIDENCE/infrastructure/r3-compose-down.log" 2>&1 || fail INFRASTRUCTURE_CLEANUP_FAILED
  lsof -nP -iTCP:"$GRPC_PORT" -iTCP:"$METRICS_PORT" -iTCP:"$PG_PORT" -iTCP:"$QDRANT_PORT" -sTCP:LISTEN >"$EVIDENCE/environment/ports-after.txt" 2>&1 && fail LEAKED_PORT || true
  finalize
}

main "$@"
