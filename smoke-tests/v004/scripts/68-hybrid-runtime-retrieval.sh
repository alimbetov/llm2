#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/fix485.sh"
fix485_evidence_dir hybrid-runtime

endpoint="${ASTRAVECTOR_QUALITY_ENDPOINT:-http://127.0.0.1:50051}"
grpc_target="${endpoint#http://}"
grpc_target="${grpc_target#https://}"
if ! grpcurl -plaintext "$grpc_target" list >"$FIX485_EVIDENCE_DIR/grpc-services.log" 2>&1; then
  fix485_write_summary BLOCKED GRPC_ENDPOINT_UNAVAILABLE
  exit "$BLOCKED_STATUS"
fi

cd "$PROJECT_DIR"
export ASTRAVECTOR_QUALITY_OUTPUT_DIR="$FIX485_EVIDENCE_DIR/quality"
export ASTRAVECTOR_QUALITY_ENDPOINT="$endpoint"
export ASTRAVECTOR_QUALITY_RUN_ID="${FIX485_RUN_ID:-fix485-hybrid-runtime}"
export ASTRAVECTOR_QUALITY_PROFILE=hybrid-quick
export ASTRAVECTOR_QUALITY_REQUIRE_DENSE=true
export ASTRAVECTOR_QUALITY_REQUIRE_SPARSE=true
export ASTRAVECTOR_QUALITY_REQUIRE_HYBRID=true
export ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true
export ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_SEARCH=false
export ASTRAVECTOR_QUALITY_RUNTIME_MODE=ingest-and-retrieve

mkdir -p "$ASTRAVECTOR_QUALITY_OUTPUT_DIR"
if ! fix485_run_logged hybrid-quality cargo test --locked --test quality_bench_runtime_quick -- --nocapture; then
  fix485_write_summary FAIL HYBRID_QUALITY_ASSERTIONS_FAILED
  exit "$FAIL_STATUS"
fi
report="$ASTRAVECTOR_QUALITY_OUTPUT_DIR/runtime-quality-report.json"
if [[ ! -f "$report" ]] || ! jq -e '
  .runtime_execution == "MODEL_BACKED_E2E_CONFIRMED" and
  .verdict == "PASS" and
  .capabilities.dense_available == true and
  .capabilities.sparse_available == true and
  .capabilities.hybrid_available == true and
  ((.hard_negative.false_positive_count // .hard_negative_false_positive_count // 0) == 0)
' "$report" >"$FIX485_EVIDENCE_DIR/hybrid-report-assertion.log"; then
  fix485_write_summary FAIL HYBRID_REPORT_CONTRACT_FAILED
  exit "$FAIL_STATUS"
fi

fix485_run_logged postgres-fts cargo test --locked --features integration-tests --test lexical_retrieval_integration -- --nocapture || {
  fix485_write_summary FAIL POSTGRES_FTS_ASSERTION_FAILED
  exit "$FAIL_STATUS"
}
fix485_write_summary PASS DENSE_SPARSE_FTS_HYBRID_RUNTIME_CONFIRMED
exit "$PASS"
