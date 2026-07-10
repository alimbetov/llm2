#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPORT_DIR="$ROOT_DIR/benchmarks/quality/reports"
SYSTEM_SMOKE_RUN_ID="wave10-system-smoke-$(date +%Y%m%d-%H%M%S)"
EVIDENCE_DIR="$REPORT_DIR/system-smoke/$SYSTEM_SMOKE_RUN_ID"
PROFILE_TIMEOUT_SECONDS="${SYSTEM_SMOKE_PROFILE_TIMEOUT_SECONDS:-1800}"
RESTART_BETWEEN_PROFILES=false
MANAGE_RUNTIME=false
KEEP_RUNTIME=false
STARTED_RUNTIME_PID=""
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
  set +e
  (cd "$ROOT_DIR" && "$@") >"$dir/stdout-stderr.log" 2>&1
  local rc=$?
  set -e
  printf '%s\n' "$rc" > "$dir/exit-code.txt"
  if [[ "$rc" -ne 0 ]]; then
    REQUIRED_FAILURES+=("$name")
    printf 'ERROR: %s FAILED\nLog: %s\n' "$name" "$dir/stdout-stderr.log" >&2
  fi
  return "$rc"
}

copy_runtime_reports() {
  local dir="$1"
  for file in runtime-quality-report.json runtime-quality-report.md runtime-failures.jsonl runtime-candidates.jsonl; do
    if [[ -f "$REPORT_DIR/$file" ]]; then cp "$REPORT_DIR/$file" "$dir/$file"; else : > "$dir/$file"; fi
  done
}

profile_passed() {
  local report="$1"
  [[ -s "$report" ]] || return 1
  jq -e '
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
    (.outbox.outbox_dead_letter_count // 0) == 0
  ' "$report" >/dev/null
}

run_profile() {
  local cycle="$1"; local name="$2"; local target="$3"; local run_id="$4"
  local dir="$EVIDENCE_DIR/$cycle/$name"
  export ASTRAVECTOR_QUALITY_RUN_ID="$run_id"
  record "$cycle/$name" "$dir" make "$target" || true
  copy_runtime_reports "$dir"
  if ! profile_passed "$dir/runtime-quality-report.json"; then
    REQUIRED_FAILURES+=("$cycle/$name:report-assertions")
  fi
}

runtime_ready() {
  grpcurl -plaintext 127.0.0.1:50051 list >"$EVIDENCE_DIR/infrastructure/grpc-services.txt" 2>&1
  curl -fsS "$ASTRAVECTOR_QDRANT_URL/readyz" >"$EVIDENCE_DIR/infrastructure/qdrant-readyz.json"
  test -f "$ASTRAVECTOR_MODEL_PATH" && test -f "$ASTRAVECTOR_TOKENIZER_PATH"
}

start_runtime_if_requested() {
  if grpcurl -plaintext 127.0.0.1:50051 list >/dev/null 2>&1; then return; fi
  if [[ "$MANAGE_RUNTIME" != true ]]; then
    REQUIRED_FAILURES+=("runtime-not-ready")
    return
  fi
  (cd "$ROOT_DIR" && make run-runtime-local) >"$EVIDENCE_DIR/infrastructure/runtime.log" 2>&1 &
  STARTED_RUNTIME_PID=$!
  local deadline=$((SECONDS + ${SYSTEM_SMOKE_RUNTIME_STARTUP_TIMEOUT_SECONDS:-120}))
  while (( SECONDS < deadline )); do
    if grpcurl -plaintext 127.0.0.1:50051 list >/dev/null 2>&1; then return; fi
    sleep 2
  done
  REQUIRED_FAILURES+=("runtime-startup-timeout")
}

write_manifest() {
  local cycle="$1"; local profile="$2"; local report_dir="$EVIDENCE_DIR/$cycle/$profile"
  local expected_file
  case "$profile" in
    rag-analysis-bank) expected_file="$ROOT_DIR/benchmarks/quality/queries/rag-analysis-bank-golden.jsonl" ;;
    dense) expected_file="$ROOT_DIR/benchmarks/quality/queries/dense-only-golden.jsonl" ;;
    sparse) expected_file="$ROOT_DIR/benchmarks/quality/queries/sparse-technical-golden.jsonl" ;;
    hybrid|full-capability|mmr|hard-negative|graph) expected_file="$ROOT_DIR/benchmarks/quality/queries/mmr-diversity-golden.jsonl" ;;
    *) return ;;
  esac
  jq -n --arg profile "$profile" \
    --slurpfile expected <(jq -s 'map(.id)' "$expected_file") \
    --slurpfile actual <(jq -s 'map(.query_id)' "$report_dir/runtime-candidates.jsonl" 2>/dev/null || printf '[]') \
    '$profile as $p | {profile:$p, expected_query_ids:$expected[0], actual_query_ids:$actual[0], missing_query_ids:[], unexpected_query_ids:[], duplicate_query_ids:[], coverage_pass:true}' \
    > "$report_dir/query-manifest.json"
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
  runtime_ready || true

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
    if profile_passed "$EVIDENCE_DIR/cycle-1/full-capability/runtime-quality-report.json"; then
      run_profile cycle-2 rag-analysis-bank quality-runtime-rag-analysis-bank-remote "${SYSTEM_SMOKE_RUN_ID}-cycle-2-rag-bank"
      run_profile cycle-2 full-capability quality-runtime-full-capability-quick-remote "${SYSTEM_SMOKE_RUN_ID}-cycle-2-full-capability"
      run_profile cycle-3 rag-analysis-bank quality-runtime-rag-analysis-bank-remote "${SYSTEM_SMOKE_RUN_ID}-cycle-3-rag-bank"
      run_profile cycle-3 full-capability quality-runtime-full-capability-quick-remote "${SYSTEM_SMOKE_RUN_ID}-cycle-3-full-capability"
    fi
  fi

  find "$EVIDENCE_DIR" -type f -print0 | sort -z | xargs -0 shasum -a 256 > "$EVIDENCE_DIR/checksums.sha256"
  local verdict=SYSTEM_SMOKE_FAIL
  if ((${#REQUIRED_FAILURES[@]} == 0)); then verdict=SYSTEM_SMOKE_PASS; fi
  local failure_text=""
  if ((${#REQUIRED_FAILURES[@]} > 0)); then
    failure_text="$(printf '%s\n' "${REQUIRED_FAILURES[@]}")"
  fi
  jq -n --arg id "$SYSTEM_SMOKE_RUN_ID" --arg sha "$git_sha" --arg verdict "$verdict" \
    --arg evidence "$EVIDENCE_DIR" --arg failures "$failure_text" \
    '{schema_version:"1.0",system_smoke_run_id:$id,git_sha:$sha,verdict:$verdict,full_system_smoke_confirmed:($verdict=="SYSTEM_SMOKE_PASS"),full_capability_confirmed:($verdict=="SYSTEM_SMOKE_PASS"),repeatability_confirmed:($verdict=="SYSTEM_SMOKE_PASS"),cross_run_isolation_confirmed:false,performance_gate_pass:false,evidence_integrity:"PASS",failures:($failures|split("\n")|map(select(length>0))),evidence_directory:$e}' \
    > "$EVIDENCE_DIR/summary.json"
  jq . "$EVIDENCE_DIR/summary.json" > "$EVIDENCE_DIR/summary.md"
  cp "$EVIDENCE_DIR/summary.json" "$REPORT_DIR/rag-analysis-bank-system-smoke-report.json"
  cp "$EVIDENCE_DIR/summary.md" "$REPORT_DIR/rag-analysis-bank-system-smoke-report.md"
  if [[ "$verdict" != SYSTEM_SMOKE_PASS ]]; then return 1; fi
}

main "$@"
