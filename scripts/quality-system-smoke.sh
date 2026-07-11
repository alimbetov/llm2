#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPORT_DIR="$ROOT_DIR/benchmarks/quality/reports"
SYSTEM_SMOKE_RUN_ID="wave10-system-smoke-$(date +%Y%m%d-%H%M%S)"
EVIDENCE_DIR="$REPORT_DIR/system-smoke/$SYSTEM_SMOKE_RUN_ID"
PROFILE_TIMEOUT_SECONDS="${SYSTEM_SMOKE_PROFILE_TIMEOUT_SECONDS:-1800}"
SYSTEM_TIMEOUT_SECONDS="${SYSTEM_SMOKE_TIMEOUT_SECONDS:-7200}"
RUNTIME_STARTUP_TIMEOUT_SECONDS="${SYSTEM_SMOKE_RUNTIME_STARTUP_TIMEOUT_SECONDS:-120}"
RETRIEVE_P95_THRESHOLD_MS="${SYSTEM_SMOKE_RETRIEVE_P95_THRESHOLD_MS:-5000}"
GRAPH_QUERY_P95_THRESHOLD_MS="${SYSTEM_SMOKE_GRAPH_QUERY_P95_THRESHOLD_MS:-5000}"
RESTART_BETWEEN_PROFILES=false
MANAGE_RUNTIME=false
KEEP_RUNTIME=false
STARTED_RUNTIME_PID=""
CYCLE_3_RESTART_CONFIRMED=false
CROSS_RUN_ISOLATION_CONFIRMED=false
QUERY_COVERAGE_CONFIRMED=true
PERFORMANCE_GATE_PASS=false
EVIDENCE_INTEGRITY="FAIL"
START_EPOCH="$(date +%s)"
declare -a REQUIRED_FAILURES=()

for arg in "$@"; do
  case "$arg" in
    --use-local-defaults) ;;
    --external-runtime) MANAGE_RUNTIME=false ;;
    --manage-runtime) MANAGE_RUNTIME=true ;;
    --keep-runtime) KEEP_RUNTIME=true ;;
    --restart-between-profiles) RESTART_BETWEEN_PROFILES=true ;;
    *) printf 'ERROR: unknown argument: %s\n' "$arg" >&2; exit 2 ;;
  esac
done

mkdir -p "$EVIDENCE_DIR/environment" "$EVIDENCE_DIR/static" \
  "$EVIDENCE_DIR/infrastructure" "$EVIDENCE_DIR/migrations" \
  "$EVIDENCE_DIR/cycle-1" "$EVIDENCE_DIR/cycle-2" "$EVIDENCE_DIR/cycle-3" \
  "$EVIDENCE_DIR/isolation" "$EVIDENCE_DIR/performance" target/smoke-logs

export ASTRAVECTOR_DB_URL="${ASTRAVECTOR_DB_URL:-postgres://astravector:astravector@127.0.0.1:55432/astravector}"
export DATABASE_URL="${DATABASE_URL:-$ASTRAVECTOR_DB_URL}"
export ASTRAVECTOR_QDRANT_URL="${ASTRAVECTOR_QDRANT_URL:-http://127.0.0.1:6333}"
export ASTRAVECTOR_QDRANT_COLLECTION="${ASTRAVECTOR_QDRANT_COLLECTION:-astravector_v004}"
export ASTRAVECTOR_MODEL_PATH="${ASTRAVECTOR_MODEL_PATH:-/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/onnx/model.onnx}"
export ASTRAVECTOR_TOKENIZER_PATH="${ASTRAVECTOR_TOKENIZER_PATH:-/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/tokenizer.json}"
export ASTRAVECTOR_QUALITY_ENDPOINT="${ASTRAVECTOR_QUALITY_ENDPOINT:-http://127.0.0.1:50051}"
export ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true
export ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_SEARCH=false
unset QUERY_FILTER QUERY_ID_FILTER ASTRAVECTOR_QUALITY_PROFILE

cleanup() {
  if [[ "$KEEP_RUNTIME" != true && -n "$STARTED_RUNTIME_PID" ]]; then
    kill "$STARTED_RUNTIME_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

record() {
  local name="$1"; shift
  local dir="$1"; shift
  mkdir -p "$dir"
  printf '%q ' "$@" > "$dir/command.txt"
  printf '\n' >> "$dir/command.txt"
  local started_at
  started_at="$(date +%s)"
  set +e
  (cd "$ROOT_DIR" && "$@") >"$dir/stdout-stderr.log" 2>&1
  local rc=$?
  set -e
  printf '%s\n' "$rc" > "$dir/exit-code.txt"
  printf '%s\n' "$(( $(date +%s) - started_at ))" > "$dir/duration-seconds.txt"
  if [[ "$rc" -ne 0 ]]; then
    REQUIRED_FAILURES+=("$name")
    printf 'ERROR: %s FAILED\nLog: %s\n' "$name" "$dir/stdout-stderr.log" >&2
  fi
  return "$rc"
}

run_with_timeout() {
  local seconds="$1"; shift
  if command -v gtimeout >/dev/null 2>&1; then
    gtimeout "$seconds" "$@"
  elif command -v timeout >/dev/null 2>&1; then
    timeout "$seconds" "$@"
  else
    python3 -c 'import subprocess,sys; sys.exit(subprocess.run(sys.argv[2:], timeout=int(sys.argv[1])).returncode)' "$seconds" "$@"
  fi
}

copy_runtime_reports() {
  local dir="$1"
  for file in runtime-quality-report.json runtime-quality-report.md runtime-failures.jsonl runtime-candidates.jsonl; do
    if [[ -f "$REPORT_DIR/$file" ]]; then cp "$REPORT_DIR/$file" "$dir/$file"; else : > "$dir/$file"; fi
  done
}

profile_passed() {
  local report="$1"
  local profile="${2:-generic}"
  [[ -s "$report" ]] || return 1
  jq -e --arg profile "$profile" '
    .runtime_execution == "MODEL_BACKED_E2E_CONFIRMED" and
    .verdict == "PASS" and
    (.retrieval.queries_failed // 0) == 0 and
    (.retrieval.queries_blocked // 0) == 0 and
    (.retrieval.queries_skipped // 0) == 0 and
    (.retrieval.cross_zone_leakage_count // 0) == 0 and
    (.retrieval.access_level_violation_count // 0) == 0 and
    (.graph.graph_timeout_count // 0) == 0 and
    (.graph.graph_db_error_count // 0) == 0 and
    (.graph.unsupported_relation_types // []) == [] and
    (.qdrant.qdrant_missing_points // 0) == 0 and
    (.outbox.outbox_dead_letter_count // 0) == 0 and
    ($profile != "full-capability" or (
      .capability_requirements.require_dense == true and
      .capability_requirements.require_sparse == true and
      .capability_requirements.require_hybrid == true and
      .capability_requirements.require_graph == true and
      .capability_requirements.require_mmr == true and
      (.graph.graph_expected_related_total // 0) > 0 and
      .graph.graph_expected_related_hit_rate == 1.0 and
      (.graph.graph_expansion_used_count // 0) > 0 and
      (.graph.graph_expanded_contexts_count // 0) > 0 and
      (.graph.graph_access_violation_count // 0) == 0 and
      (.graph.aliased_relation_edges_count // 0) == 0 and
      (.retrieval.hard_negative_false_positive_rate // 1.0) == 0.0
    ))
  ' "$report" >/dev/null
}

run_profile() {
  local cycle="$1"; local name="$2"; local target="$3"; local run_id="$4"
  local dir="$EVIDENCE_DIR/$cycle/$name"
  if (( $(date +%s) - START_EPOCH >= SYSTEM_TIMEOUT_SECONDS )); then
    REQUIRED_FAILURES+=("system-smoke-timeout")
    return
  fi
  export ASTRAVECTOR_QUALITY_RUN_ID="$run_id"
  record "$cycle/$name" "$dir" run_with_timeout "$PROFILE_TIMEOUT_SECONDS" make "$target" || true
  copy_runtime_reports "$dir"
  write_manifest "$cycle" "$name"
  if ! profile_passed "$dir/runtime-quality-report.json" "$name"; then
    REQUIRED_FAILURES+=("$cycle/$name:report-assertions")
  fi
  if ! jq -e '.coverage_pass == true' "$dir/query-manifest.json" >/dev/null; then
    QUERY_COVERAGE_CONFIRMED=false
    REQUIRED_FAILURES+=("$cycle/$name:query-coverage")
  fi
}

runtime_ready() {
  grpcurl -plaintext 127.0.0.1:50051 list >"$EVIDENCE_DIR/infrastructure/grpc-services.txt" 2>&1
  curl -fsS "$ASTRAVECTOR_QDRANT_URL/readyz" >"$EVIDENCE_DIR/infrastructure/qdrant-readyz.json"
  test -f "$ASTRAVECTOR_MODEL_PATH" && test -f "$ASTRAVECTOR_TOKENIZER_PATH"
}

start_managed_runtime() {
  record runtime-build "$EVIDENCE_DIR/infrastructure/runtime-build" cargo build --bin astravector-runtime || return
  ASTRAVECTOR_GRAPH_MERGE_STRATEGY=GRAPH_AS_CONTEXT_APPEND \
  ASTRAVECTOR_GRAPH_MAX_SEED_CHUNKS=16 \
  ASTRAVECTOR_GRAPH_CONTEXT_APPEND_LIMIT=5 \
  ASTRAVECTOR_GRAPH_EXPANSION_RESULT_LIMIT=12 \
  "$ROOT_DIR/target/debug/astravector-runtime" >"$EVIDENCE_DIR/infrastructure/runtime.log" 2>&1 &
  STARTED_RUNTIME_PID=$!
  local deadline=$((SECONDS + RUNTIME_STARTUP_TIMEOUT_SECONDS))
  while (( SECONDS < deadline )); do
    if grpcurl -plaintext 127.0.0.1:50051 list >/dev/null 2>&1; then
      jq -n --argjson pid "$STARTED_RUNTIME_PID" --arg sha "$(git rev-parse HEAD)" \
        '{managed:true,pid:$pid,git_sha:$sha,graph_merge_strategy:"GRAPH_AS_CONTEXT_APPEND",graph_max_seed_chunks:16,graph_context_append_limit:5,graph_expansion_result_limit:12,graph_timeout_ms:500}' \
        > "$EVIDENCE_DIR/infrastructure/runtime-config.json"
      return
    fi
    sleep 2
  done
  REQUIRED_FAILURES+=("runtime-startup-timeout")
}

stop_managed_runtime() {
  [[ -n "$STARTED_RUNTIME_PID" ]] || return 0
  kill "$STARTED_RUNTIME_PID" 2>/dev/null || true
  local deadline=$((SECONDS + 30))
  while lsof -nP -iTCP:50051 -sTCP:LISTEN >/dev/null 2>&1 && (( SECONDS < deadline )); do sleep 1; done
  STARTED_RUNTIME_PID=""
}

start_runtime_if_requested() {
  if [[ "$MANAGE_RUNTIME" == true ]]; then
    if grpcurl -plaintext 127.0.0.1:50051 list >/dev/null 2>&1; then
      REQUIRED_FAILURES+=("managed-runtime-port-already-in-use")
      return
    fi
    start_managed_runtime
  elif grpcurl -plaintext 127.0.0.1:50051 list >/dev/null 2>&1; then
    REQUIRED_FAILURES+=("external-runtime-config-unverifiable")
  else
    REQUIRED_FAILURES+=("runtime-not-ready")
  fi
}

write_manifest() {
  local cycle="$1"; local profile="$2"; local report_dir="$EVIDENCE_DIR/$cycle/$profile"
  local profile_name query_filter=""
  case "$profile" in
    rag-analysis-bank) profile_name="rag-analysis-bank" ;;
    dense) profile_name="dense-only-quick" ;;
    sparse) profile_name="sparse-quick" ;;
    hybrid) profile_name="hybrid-quick" ;;
    graph) profile_name="graph-quick" ;;
    mmr) profile_name="full-capability-quick"; query_filter='^mmr' ;;
    hard-negative) profile_name="full-capability-quick"; query_filter='^(negative|technical-negative)' ;;
    full-capability) profile_name="full-capability-quick" ;;
    *) return ;;
  esac
  local expected_raw="$report_dir/expected-query-ids.txt"
  local actual_raw="$report_dir/actual-query-ids.txt"
  : > "$expected_raw"
  while IFS= read -r query_set; do
    jq -r '.id' "$ROOT_DIR/benchmarks/quality/queries/$query_set.jsonl" >> "$expected_raw"
  done < <(jq -r '.queries[]' "$ROOT_DIR/benchmarks/quality/profiles/$profile_name.json")
  if [[ -n "$query_filter" ]]; then
    awk -v pattern="$query_filter" '$0 ~ pattern' "$expected_raw" > "$expected_raw.filtered"
    mv "$expected_raw.filtered" "$expected_raw"
  fi
  jq -r '.query_id' "$report_dir/runtime-candidates.jsonl" > "$actual_raw"
  sort -u "$expected_raw" > "$expected_raw.sorted"
  sort -u "$actual_raw" > "$actual_raw.sorted"
  local expected_json actual_json missing_json unexpected_json duplicate_json
  expected_json="$(jq -Rsc 'split("\n")|map(select(length>0))' < "$expected_raw.sorted")"
  actual_json="$(jq -Rsc 'split("\n")|map(select(length>0))' < "$actual_raw.sorted")"
  missing_json="$(comm -23 "$expected_raw.sorted" "$actual_raw.sorted" | jq -Rsc 'split("\n")|map(select(length>0))')"
  unexpected_json="$(comm -13 "$expected_raw.sorted" "$actual_raw.sorted" | jq -Rsc 'split("\n")|map(select(length>0))')"
  duplicate_json="$(sort "$actual_raw" | uniq -d | jq -Rsc 'split("\n")|map(select(length>0))')"
  jq -n --arg profile "$profile" --argjson expected "$expected_json" --argjson actual "$actual_json" \
    --argjson missing "$missing_json" --argjson unexpected "$unexpected_json" --argjson duplicate "$duplicate_json" \
    '{profile:$profile,expected_query_ids:$expected,actual_query_ids:$actual,missing_query_ids:$missing,unexpected_query_ids:$unexpected,duplicate_query_ids:$duplicate,coverage_pass:(($missing|length)==0 and ($unexpected|length)==0 and ($duplicate|length)==0 and ($expected|length)==($actual|length))}' \
    > "$report_dir/query-manifest.json"
}

run_cross_run_isolation() {
  local run_a="${SYSTEM_SMOKE_RUN_ID}-isolation-a"
  local run_b="${SYSTEM_SMOKE_RUN_ID}-isolation-b"
  local dir_a="$EVIDENCE_DIR/isolation/run-a" dir_b="$EVIDENCE_DIR/isolation/run-b"
  export ASTRAVECTOR_QUALITY_RUN_ID="$run_a"
  record isolation-a "$dir_a" run_with_timeout "$PROFILE_TIMEOUT_SECONDS" make quality-runtime-rag-analysis-bank-remote || true
  copy_runtime_reports "$dir_a"
  export ASTRAVECTOR_QUALITY_RUN_ID="$run_b"
  record isolation-b "$dir_b" run_with_timeout "$PROFILE_TIMEOUT_SECONDS" make quality-runtime-rag-analysis-bank-remote || true
  copy_runtime_reports "$dir_b"
  local postgres qdrant graph final
  postgres="$(jq -s --arg run "$run_b" '[.[].contexts[]? | select(.metadata.quality_run_id != $run)] | length' "$dir_b/runtime-candidates.jsonl")"
  qdrant="$(jq -s --arg run "$run_b" '[.[].contexts[]? | select((.metadata.retrieval_sources // "") | contains("VECTOR_DIRECT")) | select(.metadata.quality_run_id != $run)] | length' "$dir_b/runtime-candidates.jsonl")"
  graph="$(jq -s --arg run "$run_b" '[.[].contexts[]? | select((.metadata.retrieval_sources // "") | contains("GRAPH_EXPANDED")) | select(.metadata.quality_run_id != $run)] | length' "$dir_b/runtime-candidates.jsonl")"
  final="$postgres"
  jq -n --arg run_a "$run_a" --arg run_b "$run_b" --argjson postgres "$postgres" \
    --argjson qdrant "$qdrant" --argjson graph "$graph" --argjson final "$final" \
    '{run_a:$run_a,run_b:$run_b,cross_run_postgres_leakage_count:$postgres,cross_run_qdrant_leakage_count:$qdrant,cross_run_graph_leakage_count:$graph,cross_run_final_context_leakage_count:$final,confirmed:($postgres==0 and $qdrant==0 and $graph==0 and $final==0)}' \
    > "$EVIDENCE_DIR/isolation/result.json"
  if jq -e '.confirmed == true' "$EVIDENCE_DIR/isolation/result.json" >/dev/null; then
    CROSS_RUN_ISOLATION_CONFIRMED=true
  else
    REQUIRED_FAILURES+=("cross-run-isolation")
  fi
}

evaluate_performance() {
  local candidates="$EVIDENCE_DIR/cycle-1/full-capability/runtime-candidates.jsonl"
  local report="$EVIDENCE_DIR/cycle-1/full-capability/runtime-quality-report.json"
  local duration_file="$EVIDENCE_DIR/cycle-1/full-capability/duration-seconds.txt"
  local overall_p50 overall_p95 graph_p50 graph_p95 graph_p99 timeouts profile_duration
  overall_p50="$(jq -s '[.[].elapsed_ms]|sort|.[(length*0.50|floor)] // 0' "$candidates")"
  overall_p95="$(jq -s '[.[].elapsed_ms]|sort|.[(length*0.95|floor)] // 0' "$candidates")"
  graph_p50="$(jq -s '[.[]|select(.category=="graph_rag")|.elapsed_ms]|sort|.[(length*0.50|floor)] // 0' "$candidates")"
  graph_p95="$(jq -s '[.[]|select(.category=="graph_rag")|.elapsed_ms]|sort|.[(length*0.95|floor)] // 0' "$candidates")"
  graph_p99="$(jq -s '[.[]|select(.category=="graph_rag")|.elapsed_ms]|sort|.[(length*0.99|floor)] // 0' "$candidates")"
  timeouts="$(jq -r '.graph.graph_timeout_count // 0' "$report")"
  profile_duration="$(cat "$duration_file")"
  jq -n --argjson p50 "$overall_p50" --argjson p95 "$overall_p95" --argjson gp50 "$graph_p50" \
    --argjson gp95 "$graph_p95" --argjson gp99 "$graph_p99" --argjson timeouts "$timeouts" \
    --argjson duration "$profile_duration" --argjson retrieve_threshold "$RETRIEVE_P95_THRESHOLD_MS" \
    --argjson graph_query_threshold "$GRAPH_QUERY_P95_THRESHOLD_MS" \
    '{retrieve_p50_ms:$p50,retrieve_p95_ms:$p95,graph_query_end_to_end_p50_ms:$gp50,graph_query_end_to_end_p95_ms:$gp95,graph_query_end_to_end_p99_ms:$gp99,graph_lookup_latency_available:false,graph_timeout_count:$timeouts,profile_duration_seconds:$duration,retrieve_p95_threshold_ms:$retrieve_threshold,graph_query_p95_threshold_ms:$graph_query_threshold,pass:($p95<=$retrieve_threshold and $gp95<=$graph_query_threshold and $timeouts==0)}' \
    > "$EVIDENCE_DIR/performance/result.json"
  if jq -e '.pass == true' "$EVIDENCE_DIR/performance/result.json" >/dev/null; then
    PERFORMANCE_GATE_PASS=true
  else
    REQUIRED_FAILURES+=("performance-gate")
  fi
}

restart_for_cycle_3() {
  if [[ "$MANAGE_RUNTIME" != true ]]; then
    REQUIRED_FAILURES+=("cycle-3-restart-unavailable")
    return
  fi
  local old_pid="$STARTED_RUNTIME_PID"
  stop_managed_runtime
  start_managed_runtime
  if [[ -n "$STARTED_RUNTIME_PID" && "$STARTED_RUNTIME_PID" != "$old_pid" ]]; then
    CYCLE_3_RESTART_CONFIRMED=true
    jq -n --argjson old_pid "$old_pid" --argjson new_pid "$STARTED_RUNTIME_PID" \
      '{old_pid:$old_pid,new_pid:$new_pid,port_released:true,confirmed:($old_pid!=$new_pid)}' \
      > "$EVIDENCE_DIR/cycle-3/runtime-restart.json"
  else
    REQUIRED_FAILURES+=("cycle-3-restart-not-confirmed")
  fi
}

main() {
  local git_sha
  git_sha="$(git rev-parse HEAD)"
  printf '%s\n' "$git_sha" > "$EVIDENCE_DIR/environment/local_head.txt"
  git rev-parse origin/main > "$EVIDENCE_DIR/environment/remote_main_head.txt"
  git status --short > "$EVIDENCE_DIR/environment/git-status.txt"
  git diff --stat > "$EVIDENCE_DIR/environment/git-diff-stat.txt"
  df -Pk "$ROOT_DIR" > "$EVIDENCE_DIR/environment/disk.txt"
  uname -a > "$EVIDENCE_DIR/environment/uname.txt"
  rustc --version > "$EVIDENCE_DIR/environment/rustc.txt"
  cargo --version > "$EVIDENCE_DIR/environment/cargo.txt"
  du -h "$ASTRAVECTOR_MODEL_PATH" "$ASTRAVECTOR_TOKENIZER_PATH" > "$EVIDENCE_DIR/environment/model-sizes.txt"
  shasum -a 256 "$ASTRAVECTOR_MODEL_PATH" "$ASTRAVECTOR_TOKENIZER_PATH" > "$EVIDENCE_DIR/environment/model-tokenizer.sha256"
  start_runtime_if_requested
  if ! runtime_ready; then REQUIRED_FAILURES+=("runtime-preflight"); fi

  record fmt "$EVIDENCE_DIR/static/fmt" cargo fmt --check || true
  record check "$EVIDENCE_DIR/static/check" cargo check --all-targets --all-features || true
  record clippy "$EVIDENCE_DIR/static/clippy" cargo clippy --all-targets --all-features -- -D warnings || true
  record test "$EVIDENCE_DIR/static/test" env -u ASTRAVECTOR_QUALITY_ENDPOINT -u ASTRAVECTOR_QUALITY_RUNTIME_MODE cargo test --all-targets --all-features || true
  record fixtures "$EVIDENCE_DIR/static/fixtures" make quality-fixtures || true
  record migrate "$EVIDENCE_DIR/migrations" make migrate || true

  if ((${#REQUIRED_FAILURES[@]} > 0)); then
    : > "$EVIDENCE_DIR/partial-failure.txt"
  else
    run_profile cycle-1 rag-analysis-bank quality-runtime-rag-analysis-bank-remote "${SYSTEM_SMOKE_RUN_ID}-cycle-1-rag-bank"
    run_profile cycle-1 dense quality-runtime-dense-quick-remote "${SYSTEM_SMOKE_RUN_ID}-cycle-1-dense"
    run_profile cycle-1 sparse quality-runtime-sparse-quick-remote "${SYSTEM_SMOKE_RUN_ID}-cycle-1-sparse"
    run_profile cycle-1 hybrid quality-runtime-hybrid-quick-remote "${SYSTEM_SMOKE_RUN_ID}-cycle-1-hybrid"
    run_profile cycle-1 graph quality-runtime-graph-quick-remote "${SYSTEM_SMOKE_RUN_ID}-cycle-1-graph"
    run_profile cycle-1 mmr quality-runtime-mmr-quick-remote "${SYSTEM_SMOKE_RUN_ID}-cycle-1-mmr"
    run_profile cycle-1 hard-negative quality-runtime-hard-negative-quick-remote "${SYSTEM_SMOKE_RUN_ID}-cycle-1-hard-negative"
    run_profile cycle-1 full-capability quality-runtime-full-capability-quick-remote "${SYSTEM_SMOKE_RUN_ID}-cycle-1-full-capability"
    if profile_passed "$EVIDENCE_DIR/cycle-1/full-capability/runtime-quality-report.json" "full-capability"; then
      run_profile cycle-2 rag-analysis-bank quality-runtime-rag-analysis-bank-remote "${SYSTEM_SMOKE_RUN_ID}-cycle-2-rag-bank"
      run_profile cycle-2 full-capability quality-runtime-full-capability-quick-remote "${SYSTEM_SMOKE_RUN_ID}-cycle-2-full-capability"
      restart_for_cycle_3
      run_profile cycle-3 rag-analysis-bank quality-runtime-rag-analysis-bank-remote "${SYSTEM_SMOKE_RUN_ID}-cycle-3-rag-bank"
      run_profile cycle-3 full-capability quality-runtime-full-capability-quick-remote "${SYSTEM_SMOKE_RUN_ID}-cycle-3-full-capability"
      run_cross_run_isolation
      evaluate_performance
    fi
  fi

  find "$EVIDENCE_DIR" -type f ! -name checksums.sha256 ! -name summary.json ! -name summary.md -print0 \
    | sort -z | xargs -0 shasum -a 256 > "$EVIDENCE_DIR/checksums.sha256"
  if (cd "$ROOT_DIR" && shasum -a 256 -c "$EVIDENCE_DIR/checksums.sha256" >/dev/null); then
    EVIDENCE_INTEGRITY="PASS"
  else
    REQUIRED_FAILURES+=("evidence-integrity")
  fi
  local full_capability_confirmed=false repeatability_confirmed=false
  if profile_passed "$EVIDENCE_DIR/cycle-1/full-capability/runtime-quality-report.json" "full-capability" 2>/dev/null; then
    full_capability_confirmed=true
  fi
  if [[ "$full_capability_confirmed" == true && "$CYCLE_3_RESTART_CONFIRMED" == true ]] \
    && profile_passed "$EVIDENCE_DIR/cycle-2/full-capability/runtime-quality-report.json" "full-capability" 2>/dev/null \
    && profile_passed "$EVIDENCE_DIR/cycle-3/full-capability/runtime-quality-report.json" "full-capability" 2>/dev/null; then
    repeatability_confirmed=true
  fi
  local verdict=SYSTEM_SMOKE_FAIL
  if ((${#REQUIRED_FAILURES[@]} == 0)) \
    && [[ "$full_capability_confirmed" == true ]] \
    && [[ "$repeatability_confirmed" == true ]] \
    && [[ "$CYCLE_3_RESTART_CONFIRMED" == true ]] \
    && [[ "$CROSS_RUN_ISOLATION_CONFIRMED" == true ]] \
    && [[ "$QUERY_COVERAGE_CONFIRMED" == true ]] \
    && [[ "$PERFORMANCE_GATE_PASS" == true ]] \
    && [[ "$EVIDENCE_INTEGRITY" == PASS ]]; then verdict=SYSTEM_SMOKE_PASS; fi
  local failure_text=""
  if ((${#REQUIRED_FAILURES[@]} > 0)); then
    failure_text="$(printf '%s\n' "${REQUIRED_FAILURES[@]}")"
  fi
  local summary_tmp="$EVIDENCE_DIR/summary.json.tmp"
  jq -n --arg id "$SYSTEM_SMOKE_RUN_ID" --arg sha "$git_sha" --arg verdict "$verdict" \
    --arg evidence "$EVIDENCE_DIR" --arg failures "$failure_text" --arg integrity "$EVIDENCE_INTEGRITY" \
    --argjson full "$full_capability_confirmed" --argjson repeat "$repeatability_confirmed" \
    --argjson restart "$CYCLE_3_RESTART_CONFIRMED" --argjson isolation "$CROSS_RUN_ISOLATION_CONFIRMED" \
    --argjson coverage "$QUERY_COVERAGE_CONFIRMED" --argjson performance "$PERFORMANCE_GATE_PASS" \
    '{schema_version:"1.0",system_smoke_run_id:$id,git_sha:$sha,verdict:$verdict,full_system_smoke_confirmed:($verdict=="SYSTEM_SMOKE_PASS"),full_capability_confirmed:$full,repeatability_confirmed:$repeat,cycle_3_runtime_restart_confirmed:$restart,cross_run_isolation_confirmed:$isolation,query_coverage_confirmed:$coverage,performance_gate_pass:$performance,evidence_integrity:$integrity,failures:($failures|split("\n")|map(select(length>0))),evidence_directory:$evidence}' \
    > "$summary_tmp"
  mv "$summary_tmp" "$EVIDENCE_DIR/summary.json"
  jq . "$EVIDENCE_DIR/summary.json" > "$EVIDENCE_DIR/summary.md"
  cp "$EVIDENCE_DIR/summary.json" "$REPORT_DIR/rag-analysis-bank-system-smoke-report.json"
  cp "$EVIDENCE_DIR/summary.md" "$REPORT_DIR/rag-analysis-bank-system-smoke-report.md"
  if [[ "$verdict" != SYSTEM_SMOKE_PASS ]]; then return 1; fi
}

main "$@"
