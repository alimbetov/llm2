#!/usr/bin/env bash
set -uo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
LOAD_RUN_ID="${LOAD_RUN_ID:-macbook-m2-load-$(date +%Y%m%d-%H%M%S)}"
EVIDENCE_DIR="$ROOT_DIR/benchmarks/load/reports/runs/$LOAD_RUN_ID"
REPORTS_DIR="$ROOT_DIR/benchmarks/load/reports"
MODEL_PATH="${ASTRAVECTOR_MODEL_PATH:-/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/onnx/model.onnx}"
TOKENIZER_PATH="${ASTRAVECTOR_TOKENIZER_PATH:-/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/tokenizer.json}"
DB_URL="${ASTRAVECTOR_DB_URL:-postgres://astravector:astravector@127.0.0.1:55432/astravector}"
PROTO_PATH="$ROOT_DIR/proto/astravector_embedding.proto"
GRPC_METHOD="astravector.embedding.v1.AstraVectorRetrievalFacade/RetrieveContext"
RUNTIME_PID=""
RESOURCE_PID=""
METRICS_PID=""
OVERALL_VERDICT="INCOMPLETE"
FAILURE_REASONS=()

mkdir -p "$EVIDENCE_DIR"/{environment,static,infrastructure,runtime,corpus,contract,warmup,baseline,step,soak,spike,recovery,post-load-quality,system,metrics}

finish_owned_processes() {
  for pid in "$RESOURCE_PID" "$METRICS_PID" "$RUNTIME_PID"; do
    [[ -n "$pid" ]] && kill "$pid" 2>/dev/null || true
  done
}
trap finish_owned_processes EXIT INT TERM

record() {
  local dir="$1"; shift
  mkdir -p "$dir"
  printf '%q ' "$@" > "$dir/command.txt"; printf '\n' >> "$dir/command.txt"
  date -Iseconds > "$dir/started-at.txt"
  "$@" > "$dir/stdout-stderr.log" 2>&1
  local rc=$?
  date -Iseconds > "$dir/finished-at.txt"
  printf '%s\n' "$rc" > "$dir/exit-code.txt"
  return "$rc"
}

block() {
  OVERALL_VERDICT="BLOCKED"
  FAILURE_REASONS+=("$1")
  write_report
  exit 2
}

json_valid() { jq -e . "$1" >/dev/null 2>&1; }

copy_quality_reports() {
  local dest="$1"
  mkdir -p "$dest"
  for f in runtime-quality-report.json runtime-quality-report.md runtime-candidates.jsonl runtime-failures.jsonl; do
    [[ -f "benchmarks/quality/reports/$f" ]] && cp "benchmarks/quality/reports/$f" "$dest/$f"
  done
}

quality_passes() {
  jq -e '
    .runtime_execution == "MODEL_BACKED_E2E_CONFIRMED" and .verdict == "PASS" and
    .retrieval.queries_total == 97 and .retrieval.queries_passed == 97 and
    .retrieval.queries_failed == 0 and .retrieval.queries_blocked == 0 and
    .retrieval.queries_skipped == 0 and .graph.graph_expected_related_hits == 13 and
    .graph.graph_expected_related_total == 13 and .graph.graph_timeout_count == 0 and
    .graph.graph_db_error_count == 0 and .retrieval.cross_zone_leakage_count == 0 and
    .retrieval.access_level_violation_count == 0 and .qdrant.qdrant_missing_points == 0 and
    .outbox.outbox_dead_letter_count == 0
  ' "$1" >/dev/null
}

latency_ms() {
  local file="$1" percentile="$2"
  jq -r --argjson p "$percentile" '[.latencyDistribution[] | select(.percentage >= $p)][0].latency / 1000000' "$file"
}

error_rate() {
  jq -r '((.errorDistribution // {} | to_entries | map(.value) | add) // 0) / (.count | if . == 0 then 1 else . end)' "$1"
}

success_rate() {
  jq -r '1 - (((.errorDistribution // {} | to_entries | map(.value) | add) // 0) / (.count | if . == 0 then 1 else . end))' "$1"
}

ghz_run() {
  local dir="$1" rps="$2" concurrency="$3" duration="$4"
  mkdir -p "$dir"
  local output="$dir/result.json"
  local cmd=(ghz --insecure --proto "$PROTO_PATH" --call "$GRPC_METHOD" --data-file "$EVIDENCE_DIR/contract/request-template.json" --concurrency "$concurrency" --rps "$rps" --duration "$duration" --timeout 5s --count-errors --format json --output "$output" 127.0.0.1:50051)
  printf '%q ' "${cmd[@]}" > "$dir/command.txt"; printf '\n' >> "$dir/command.txt"
  date -Iseconds > "$dir/started-at.txt"
  "${cmd[@]}" > "$dir/stdout-stderr.log" 2>&1 &
  local ghz_pid=$!
  printf '%s\n' "$ghz_pid" > "$dir/ghz-current.pid"
  wait "$ghz_pid"; local rc=$?
  date -Iseconds > "$dir/finished-at.txt"
  printf '%s\n' "$rc" > "$dir/exit-code.txt"
  [[ "$rc" -eq 0 ]] && json_valid "$output"
}

start_samplers() {
  (
    printf 'timestamp,runtime_pid,runtime_cpu_percent,runtime_memory_percent,runtime_rss_kb,runtime_elapsed,postgres_cpu,postgres_memory,qdrant_cpu,qdrant_memory,swap_used,load_average\n'
    while kill -0 "$RUNTIME_PID" 2>/dev/null; do
      local_ps="$(ps -p "$RUNTIME_PID" -o %cpu=,%mem=,rss=,etime= | xargs)"
      docker_stats="$(docker stats --no-stream --format '{{.Name}},{{.CPUPerc}},{{.MemUsage}}' astravector-postgres-1 astravector-qdrant-1 2>/dev/null | tr '\n' ';')"
      printf '%s,%s,%s,%s\n' "$(date -Iseconds)" "$RUNTIME_PID" "$(printf '%s' "$local_ps" | tr ' ' ',')" "$(printf '%s' "$docker_stats" | tr ',' '|')|$(sysctl -n vm.swapusage 2>/dev/null | tr ',' ';')|$(sysctl -n vm.loadavg 2>/dev/null)"
      sleep 5
    done
  ) > "$EVIDENCE_DIR/system/resources.csv" & RESOURCE_PID=$!
  printf '%s\n' "$RESOURCE_PID" > "$EVIDENCE_DIR/system/resource-sampler.pid"
  (
    while kill -0 "$RUNTIME_PID" 2>/dev/null; do
      curl -fsS http://127.0.0.1:9090/metrics > "$EVIDENCE_DIR/metrics/$(date +%s).prom" 2>/dev/null || true
      sleep 15
    done
  ) & METRICS_PID=$!
  printf '%s\n' "$METRICS_PID" > "$EVIDENCE_DIR/system/metrics-sampler.pid"
}

write_report() {
  local report="$EVIDENCE_DIR/astravector-macbook-load-report.json"
  local git_sha git_dirty model_sha tokenizer_sha binary_sha
  git_sha="$(git rev-parse HEAD 2>/dev/null || true)"
  git_dirty=false; [[ -n "$(git status --short 2>/dev/null)" ]] && git_dirty=true
  model_sha="$(shasum -a 256 "$MODEL_PATH" 2>/dev/null | awk '{print $1}')"
  tokenizer_sha="$(shasum -a 256 "$TOKENIZER_PATH" 2>/dev/null | awk '{print $1}')"
  binary_sha="$(shasum -a 256 target/release/astravector-runtime 2>/dev/null | awk '{print $1}')"
  local reasons; reasons="$(printf '%s\n' "${FAILURE_REASONS[@]:-}" | jq -Rsc 'split("\n")|map(select(length>0))')"
  jq -n --arg id "$LOAD_RUN_ID" --arg sha "$git_sha" --argjson dirty "$git_dirty" \
    --arg verdict "$OVERALL_VERDICT" --arg model "$model_sha" --arg tokenizer "$tokenizer_sha" \
    --arg binary "$binary_sha" --argjson reasons "$reasons" \
    '{schema_version:"2.0",load_run_id:$id,git_sha:$sha,git_dirty:$dirty,machine:{chip:"Apple M2",cpu_cores:8,memory_bytes:17179869184},tools:{ghz:"0.121.0"},runtime:{binary_sha256:($binary|select(length>0)//null),model_sha256:($model|select(length>0)//null),tokenizer_sha256:($tokenizer|select(length>0)//null),model_backed:true,release_build:true},overall_verdict:$verdict,failure_reasons:$reasons,evidence:{simulation_detected:false,incomplete_processes:[]}}' > "$report"
  cat > "$EVIDENCE_DIR/astravector-macbook-load-report.md" <<EOF
# AstraVector MacBook M2 Load Report

## Executive verdict

\`$OVERALL_VERDICT\`

## Evidence

Run: \`$LOAD_RUN_ID\`. Machine-readable stage evidence is stored beside this report.

## Limitations

All components and the load generator ran on the same MacBook. The result is a single-host local capacity benchmark and is not equivalent to Kubernetes or production-server capacity.
EOF
}

# Invalidate prior top-level simulated reports without deleting them.
mkdir -p "$REPORTS_DIR/invalid-simulated"
find "$REPORTS_DIR" -maxdepth 1 -type f \( -name 'baseline-*.json' -o -name 'step-load.json' -o -name 'soak-60m.json' -o -name 'spike.json' -o -name 'post-spike-recovery.json' -o -name 'astravector-macbook-load-report.json' -o -name 'astravector-macbook-load-report.md' \) -exec mv {} "$REPORTS_DIR/invalid-simulated/" \;
cat > "$REPORTS_DIR/invalid-simulated/INVALIDATION.md" <<'EOF'
The previous load report is invalid because the soak, spike,
memory-growth and post-load quality results were simulated or
reported without completed process evidence. These values must
not be used as performance or production-readiness proof.
EOF

{
  echo "timestamp=$(date -Iseconds)"; echo "pwd=$PWD"; echo "branch=$(git branch --show-current)"; echo "head=$(git rev-parse HEAD)"; echo "origin_main=$(git rev-parse origin/main)"; git status --short; git log -5 --oneline
} > "$EVIDENCE_DIR/environment/git.txt"
{ ghz --version; grpcurl --version; docker --version; docker compose version; rustc --version; cargo --version; jq --version; } > "$EVIDENCE_DIR/environment/tools.txt" 2>&1
system_profiler SPHardwareDataType > "$EVIDENCE_DIR/environment/mac-hardware.txt"
sw_vers > "$EVIDENCE_DIR/environment/macos.txt"
sysctl -n hw.ncpu > "$EVIDENCE_DIR/environment/cpu-count.txt"
sysctl -n hw.memsize > "$EVIDENCE_DIR/environment/memory-bytes.txt"
sysctl vm.swapusage > "$EVIDENCE_DIR/environment/swap-before.txt"
df -h . > "$EVIDENCE_DIR/environment/disk.txt"
df -Pk . > "$EVIDENCE_DIR/environment/disk-kb.txt"
uname -a > "$EVIDENCE_DIR/environment/uname.txt"
pmset -g batt > "$EVIDENCE_DIR/environment/power.txt"

available_kb="$(df -Pk . | awk 'NR==2 {print $4}')"
(( available_kb >= 20 * 1024 * 1024 )) || block BLOCKED_BY_DISK_SPACE
grep -q 'AC Power' "$EVIDENCE_DIR/environment/power.txt" || block BLOCKED_BY_POWER
[[ "$(ghz --version 2>&1)" == "0.121.0" ]] || block BLOCKED_BY_GHZ_VERSION

record "$EVIDENCE_DIR/static/fmt" cargo fmt --check || block BLOCKED_BY_STATIC_GATE
record "$EVIDENCE_DIR/static/check" cargo check --all-targets --all-features || block BLOCKED_BY_STATIC_GATE
record "$EVIDENCE_DIR/static/clippy" cargo clippy --all-targets --all-features -- -D warnings || block BLOCKED_BY_STATIC_GATE
record "$EVIDENCE_DIR/static/concurrency-smoke" cargo test --features integration-tests --test smoke_load_retrieve_context_testcontainers -- --nocapture || block BLOCKED_BY_CONCURRENCY_SMOKE

docker compose config --services > "$EVIDENCE_DIR/infrastructure/docker-compose-config.txt" 2>&1
docker compose up -d postgres qdrant > "$EVIDENCE_DIR/infrastructure/docker-compose-up.log" 2>&1 || block BLOCKED_BY_INFRASTRUCTURE
docker compose ps > "$EVIDENCE_DIR/infrastructure/docker-compose-ps.txt"
docker compose exec -T postgres pg_isready -U astravector -d astravector > "$EVIDENCE_DIR/infrastructure/postgres-ready.txt" || block BLOCKED_BY_INFRASTRUCTURE
curl -fsS http://127.0.0.1:6333/readyz > "$EVIDENCE_DIR/infrastructure/qdrant-ready.txt" || block BLOCKED_BY_INFRASTRUCTURE
docker inspect astravector-postgres-1 > "$EVIDENCE_DIR/infrastructure/docker-inspect-postgres.json"
docker inspect astravector-qdrant-1 > "$EVIDENCE_DIR/infrastructure/docker-inspect-qdrant.json"
record "$EVIDENCE_DIR/infrastructure/migrate" env ASTRAVECTOR_DB_URL="$DB_URL" make migrate || block BLOCKED_BY_MIGRATION

[[ -f "$MODEL_PATH" && -f "$TOKENIZER_PATH" ]] || block MODEL_FILES_NOT_FOUND
shasum -a 256 "$MODEL_PATH" "$TOKENIZER_PATH" > "$EVIDENCE_DIR/runtime/model-tokenizer.sha256"
record "$EVIDENCE_DIR/runtime/release-build" cargo build --release --bin astravector-runtime || block BLOCKED_BY_RELEASE_BUILD
shasum -a 256 target/release/astravector-runtime > "$EVIDENCE_DIR/runtime/binary.sha256"
if lsof -nP -iTCP:50051 -sTCP:LISTEN > "$EVIDENCE_DIR/runtime/port-before.txt" 2>&1; then block BLOCKED_BY_EXISTING_RUNTIME; fi

ASTRAVECTOR_DB_URL="$DB_URL" ASTRAVECTOR_MODEL_PATH="$MODEL_PATH" ASTRAVECTOR_TOKENIZER_PATH="$TOKENIZER_PATH" \
ASTRAVECTOR_GRAPH_MERGE_STRATEGY=GRAPH_AS_CONTEXT_APPEND ASTRAVECTOR_GRAPH_MAX_SEED_CHUNKS=16 \
ASTRAVECTOR_GRAPH_CONTEXT_APPEND_LIMIT=5 ASTRAVECTOR_GRAPH_EXPANSION_RESULT_LIMIT=12 ASTRAVECTOR_GRAPH_TIMEOUT_MS=500 \
ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true target/release/astravector-runtime > "$EVIDENCE_DIR/runtime/runtime.log" 2>&1 &
RUNTIME_PID=$!; printf '%s\n' "$RUNTIME_PID" > "$EVIDENCE_DIR/runtime/runtime.pid"
for _ in {1..60}; do grpcurl -plaintext 127.0.0.1:50051 list > "$EVIDENCE_DIR/runtime/grpc-services.txt" 2>&1 && break; kill -0 "$RUNTIME_PID" 2>/dev/null || block RUNTIME_DIED; sleep 2; done
grpcurl -plaintext 127.0.0.1:50051 list >/dev/null || block RUNTIME_NOT_READY
curl -fsS http://127.0.0.1:9090/metrics > "$EVIDENCE_DIR/runtime/metrics-before.prom" || block METRICS_NOT_READY

copy_quality_reports "$EVIDENCE_DIR/corpus/global-reports-before"
record "$EVIDENCE_DIR/corpus/pre-load-quality" env ASTRAVECTOR_QUALITY_RUN_ID="${LOAD_RUN_ID}-pre-load-quality" make quality-runtime-full-capability-quick-remote || block BLOCKED_BY_PRE_LOAD_QUALITY
copy_quality_reports "$EVIDENCE_DIR/corpus/pre-load-quality"
quality_passes "$EVIDENCE_DIR/corpus/pre-load-quality/runtime-quality-report.json" || block BLOCKED_BY_PRE_LOAD_QUALITY

grpcurl -plaintext 127.0.0.1:50051 describe > "$EVIDENCE_DIR/contract/grpc-describe.txt"
find . -type f -name '*.proto' -print > "$EVIDENCE_DIR/contract/proto-files.txt"
zone_id="$(psql "$DB_URL" -Atc "select access_zone_id from astravector.access_zones where access_zone_code='1700' and status='ACTIVE' order by created_at desc limit 1")"
[[ -n "$zone_id" ]] || block ACCESS_ZONE_NOT_FOUND
cat > "$EVIDENCE_DIR/contract/request-template.json" <<EOF
{"context":{"correlationId":"load-{{.RequestNumber}}","callerService":"ghz-load","callerUserId":"local-load","callerAccessLevel":"RESTRICTED"},"accessZoneId":"$zone_id","accessZoneCode":"1700","question":"{{if eq (mod .RequestNumber 8) 0}}How does reconciliation repair Qdrant projection drift?{{else if eq (mod .RequestNumber 8) 1}}How does legal hold constrain TTL orphan cleanup?{{else if eq (mod .RequestNumber 8) 2}}Why is PostgreSQL the retrieval source of truth?{{else if eq (mod .RequestNumber 8) 3}}How are missing Qdrant projection points repaired?{{else if eq (mod .RequestNumber 8) 4}}How does TTL cleanup preserve held documents?{{else if eq (mod .RequestNumber 8) 5}}How are dead-letter outbox events recovered?{{else if eq (mod .RequestNumber 8) 6}}How does GraphRAG relation expansion add evidence?{{else}}How does MMR improve context diversity?{{end}}","profile":"RETRIEVAL_PROFILE_BALANCED","maxContexts":10,"responseDetail":"RESPONSE_DETAIL_STANDARD","enableGraphExpansion":true,"graphMaxHops":1,"graphMaxRelatedContexts":5}
EOF
cat > "$EVIDENCE_DIR/contract/single-request.json" <<EOF
{"context":{"correlationId":"load-single","callerService":"ghz-load","callerUserId":"local-load","callerAccessLevel":"RESTRICTED"},"accessZoneId":"$zone_id","accessZoneCode":"1700","question":"How does reconciliation repair Qdrant projection drift and preserve PostgreSQL as source of truth?","profile":"RETRIEVAL_PROFILE_BALANCED","maxContexts":10,"responseDetail":"RESPONSE_DETAIL_STANDARD","enableGraphExpansion":true,"graphMaxHops":1,"graphMaxRelatedContexts":5}
EOF
grpcurl -plaintext -d "$(cat "$EVIDENCE_DIR/contract/single-request.json")" 127.0.0.1:50051 "$GRPC_METHOD" > "$EVIDENCE_DIR/contract/single-response.json" || block RETRIEVE_CONTEXT_FAILED
jq -e '.contexts | length > 0' "$EVIDENCE_DIR/contract/single-response.json" >/dev/null || block RETRIEVE_CONTEXT_EMPTY

start_samplers
ghz_run "$EVIDENCE_DIR/warmup" 1 1 2m || block WARMUP_FAILED
sleep 60
for spec in '2 2' '5 5' '10 10'; do set -- $spec; ghz_run "$EVIDENCE_DIR/baseline/$1-rps" "$1" "$2" 5m || block BASELINE_FAILED; sleep 60; done

stable_rps=0; saturation_rps=null; failure_rps=null; stop_reason=""
for rps in 2 4 6 8 10 12 14 16 18 20 22 24 26 28 30; do
  concurrency="$rps"; (( concurrency < 4 )) && concurrency=4; (( concurrency > 40 )) && concurrency=40
  dir="$EVIDENCE_DIR/step/$(printf '%03d' "$rps")-rps"
  if ! ghz_run "$dir" "$rps" "$concurrency" 3m; then failure_rps="$rps"; stop_reason="GHZ_EXIT_OR_INVALID_OUTPUT"; break; fi
  er="$(error_rate "$dir/result.json")"; sr="$(success_rate "$dir/result.json")"; p95="$(latency_ms "$dir/result.json" 95)"; p99="$(latency_ms "$dir/result.json" 99)"
  jq -n --argjson requested "$rps" --argjson success "$sr" --argjson error "$er" --argjson p95 "$p95" --argjson p99 "$p99" '{requested_rps:$requested,success_rate:$success,error_rate:$error,p95_ms:$p95,p99_ms:$p99}' > "$dir/metrics.json"
  if awk "BEGIN {exit !($er > 0.02 || $p95 > 5000 || $p99 > 5000)}"; then failure_rps="$rps"; stop_reason="ACCEPTANCE_THRESHOLD"; break; fi
  if awk "BEGIN {exit !($sr >= 0.99 && $er < 0.01 && $p95 <= 5000)}"; then stable_rps="$rps"; else [[ "$saturation_rps" == null ]] && saturation_rps="$rps"; fi
  kill -0 "$RUNTIME_PID" 2>/dev/null || { failure_rps="$rps"; stop_reason="RUNTIME_DIED"; break; }
  sleep 60
done
(( stable_rps > 0 )) || block NO_STABLE_RPS
soak_rps=$(( stable_rps * 65 / 100 )); (( soak_rps < 1 )) && soak_rps=1
jq -n --argjson stable "$stable_rps" --argjson saturation "$saturation_rps" --argjson failure "$failure_rps" --arg reason "$stop_reason" --argjson soak "$soak_rps" '{stable_rps:$stable,saturation_rps:$saturation,failure_rps:$failure,stop_reason:$reason,soak_rps:$soak}' > "$EVIDENCE_DIR/soak/selection.json"

ps -p "$RUNTIME_PID" -o pid=,rss=,etime= > "$EVIDENCE_DIR/soak/runtime-before.txt"
sysctl vm.swapusage > "$EVIDENCE_DIR/soak/swap-before.txt"
ghz_run "$EVIDENCE_DIR/soak" "$soak_rps" "$(( soak_rps > 4 ? soak_rps : 4 ))" 60m || { OVERALL_VERDICT=FAIL; FAILURE_REASONS+=(SOAK_FAILED); write_report; exit 1; }
ps -p "$RUNTIME_PID" -o pid=,rss=,etime= > "$EVIDENCE_DIR/soak/runtime-after.txt" || { OVERALL_VERDICT=FAIL; FAILURE_REASONS+=(RUNTIME_DIED_DURING_SOAK); write_report; exit 1; }
sysctl vm.swapusage > "$EVIDENCE_DIR/soak/swap-after.txt"
sleep 120

spike_rps=$(( stable_rps * 2 )); (( spike_rps > 50 )) && spike_rps=50
spike_concurrency="$spike_rps"; (( spike_concurrency < 10 )) && spike_concurrency=10; (( spike_concurrency > 64 )) && spike_concurrency=64
ghz_run "$EVIDENCE_DIR/spike" "$spike_rps" "$spike_concurrency" 30s || FAILURE_REASONS+=(SPIKE_NONZERO)
ghz_run "$EVIDENCE_DIR/recovery" "$soak_rps" "$(( soak_rps > 4 ? soak_rps : 4 ))" 5m || FAILURE_REASONS+=(RECOVERY_FAILED)

record "$EVIDENCE_DIR/post-load-quality/run" env ASTRAVECTOR_QUALITY_RUN_ID="${LOAD_RUN_ID}-post-load-quality" make quality-runtime-full-capability-quick-remote || FAILURE_REASONS+=(POST_LOAD_QUALITY_FAILED)
copy_quality_reports "$EVIDENCE_DIR/post-load-quality"
quality_passes "$EVIDENCE_DIR/post-load-quality/runtime-quality-report.json" || FAILURE_REASONS+=(POST_LOAD_QUALITY_FAILED)

kill "$RESOURCE_PID" "$METRICS_PID" 2>/dev/null || true; wait "$RESOURCE_PID" "$METRICS_PID" 2>/dev/null || true; RESOURCE_PID=""; METRICS_PID=""
python3 "$ROOT_DIR/scripts/finalize_macbook_load_report.py" "$EVIDENCE_DIR"
