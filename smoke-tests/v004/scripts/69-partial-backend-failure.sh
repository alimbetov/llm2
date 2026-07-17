#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/fix485.sh"
fix485_evidence_dir failures
cd "$PROJECT_DIR"

fix485_run_logged status-semantics cargo test --locked --test retrieval_failure_semantics -- --nocapture || {
  fix485_write_summary FAIL RETRIEVAL_FAILURE_SEMANTICS_FAILED
  exit "$FAIL_STATUS"
}

fix485_run_logged failpoint-build cargo build --locked --features smoke-failpoints --bin astravector-runtime || {
  fix485_write_summary FAIL FAILPOINT_RUNTIME_BUILD_FAILED
  exit "$FAIL_STATUS"
}

endpoint="${ASTRAVECTOR_QUALITY_ENDPOINT:-http://127.0.0.1:50051}"
grpc_target="${endpoint#http://}"
grpc_target="${grpc_target#https://}"
runtime_log="$FIX485_EVIDENCE_DIR/runtime.log"
runtime_pid=""

stop_runtime() {
  local pid
  pid="$(lsof -nP -iTCP:50051 -sTCP:LISTEN -t 2>/dev/null | head -1 || true)"
  if [[ -n "$pid" ]]; then
    kill "$pid" 2>/dev/null || true
    for _ in $(seq 1 30); do
      kill -0 "$pid" 2>/dev/null || break
      sleep 0.2
    done
  fi
}

start_runtime() {
  local failpoint="$1"
  : >"$runtime_log"
  nohup env \
    RUST_LOG="${RUST_LOG:-astravector_runtime=info}" \
    ASTRAVECTOR_CONFIG="${FIX485_RUNTIME_CONFIG:-$PROJECT_DIR/config/application.yaml}" \
    ASTRAVECTOR_DB_URL="${ASTRAVECTOR_DB_URL:-postgres://astravector:astravector@127.0.0.1:55432/astravector}" \
    DATABASE_URL="${DATABASE_URL:-postgres://astravector:astravector@127.0.0.1:55432/astravector}" \
    ASTRAVECTOR_QDRANT_URL="${ASTRAVECTOR_QDRANT_URL:-http://127.0.0.1:6333}" \
    ASTRAVECTOR_QDRANT_COLLECTION="${ASTRAVECTOR_QDRANT_COLLECTION:-astravector_v004}" \
    ASTRAVECTOR_MODEL_PATH="${ASTRAVECTOR_MODEL_PATH:-/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/onnx/model.onnx}" \
    ASTRAVECTOR_TOKENIZER_PATH="${ASTRAVECTOR_TOKENIZER_PATH:-/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/tokenizer.json}" \
    ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true \
    ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_SEARCH=false \
    ASTRAVECTOR_SMOKE_FAILPOINTS_ENABLED=true \
    ASTRAVECTOR_SMOKE_FAILPOINT="$failpoint" \
    "$PROJECT_DIR/target/debug/astravector-runtime" >>"$runtime_log" 2>&1 &
  runtime_pid=$!
  for _ in $(seq 1 90); do
    grpcurl -plaintext "$grpc_target" list >/dev/null 2>&1 && return 0
    kill -0 "$runtime_pid" 2>/dev/null || return 1
    sleep 0.5
  done
  return 1
}

restore_runtime() {
  stop_runtime
  if ! start_runtime ""; then
    printf 'normal runtime restore failed\n' >>"$runtime_log"
  fi
}
trap restore_runtime EXIT INT TERM

request='{"correlationId":"fix485-partial-failure","accessZoneCode":"1700","callerAccessLevel":"PUBLIC","query":"How are missing Qdrant points repaired from PostgreSQL?","topK":5,"candidateLimit":50,"parentLimit":5,"timeoutMs":10000,"searchMode":"SEARCH_MODE_V005_HYBRID","embeddingMode":"EMBEDDING_MODE_V005_DENSE_SPARSE_IF_AVAILABLE","includeDebug":true}'

assert_partial_failure() {
  local failpoint="$1" warning="$2" surviving_branch="$3" output="$4"
  stop_runtime
  start_runtime "$failpoint" || return 1
  grpcurl -plaintext -d "$request" "$grpc_target" astravector.embedding.v1.AstraVectorV004Control/Search >"$output" 2>"$output.err" || return 1
  jq -e --arg warning "$warning" --arg branch "$surviving_branch" '
    (.results | length) > 0 and
    any(.warnings[]?; .code == $warning) and
    (if $branch == "dense" then .diagnostics.denseBranchExecuted == true and ((.diagnostics.sparseBranchExecuted // false) == false)
     else ((.diagnostics.denseBranchExecuted // false) == false) and .diagnostics.sparseBranchExecuted == true end)
  ' "$output" >/dev/null
}

assert_partial_failure qdrant_dense_search DENSE_SEARCH_FAILED sparse "$FIX485_EVIDENCE_DIR/dense-failed.json" || {
  fix485_write_summary FAIL DENSE_FAILURE_DID_NOT_DEGRADE_TO_SPARSE
  exit "$FAIL_STATUS"
}
assert_partial_failure qdrant_sparse_search SPARSE_SEARCH_FAILED dense "$FIX485_EVIDENCE_DIR/sparse-failed.json" || {
  fix485_write_summary FAIL SPARSE_FAILURE_DID_NOT_DEGRADE_TO_DENSE
  exit "$FAIL_STATUS"
}

stop_runtime
start_runtime "qdrant_dense_search,qdrant_sparse_search" || {
  fix485_write_summary FAIL DUAL_FAILURE_RUNTIME_DID_NOT_START
  exit "$FAIL_STATUS"
}
if grpcurl -plaintext -d "$request" "$grpc_target" astravector.embedding.v1.AstraVectorV004Control/Search >"$FIX485_EVIDENCE_DIR/all-failed.json" 2>"$FIX485_EVIDENCE_DIR/all-failed.err"; then
  fix485_write_summary FAIL ALL_BACKEND_FAILURE_RETURNED_SUCCESS
  exit "$FAIL_STATUS"
fi
if ! rg -q 'Unavailable|RETRIEVAL_BACKENDS_UNAVAILABLE|Internal' "$FIX485_EVIDENCE_DIR/all-failed.err"; then
  fix485_write_summary FAIL ALL_BACKEND_FAILURE_STATUS_NOT_EXPLICIT
  exit "$FAIL_STATUS"
fi

fix485_write_summary PASS LIVE_PARTIAL_BACKEND_FAILURE_SEMANTICS_PASSED
exit "$PASS"
