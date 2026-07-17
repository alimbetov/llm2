#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/fix485.sh"
fix485_evidence_dir mixed-tier
cd "$PROJECT_DIR"

fix485_run_logged fairness-contract cargo test --locked --test mixed_tier_fairness -- --nocapture || {
  fix485_write_summary FAIL MIXED_TIER_FAIRNESS_CONTRACT_FAILED
  exit "$FAIL_STATUS"
}

if [[ -z "${ASTRA_VECTOR_SMOKE_ACCESS_ZONE_ID:-}" ]]; then
  fix485_write_summary BLOCKED LIVE_MIXED_TIER_ACCESS_ZONE_NOT_SET
  exit "$BLOCKED_STATUS"
fi
export ASTRA_VECTOR_SMOKE_RETRIEVE_ENDPOINT="${ASTRAVECTOR_QUALITY_ENDPOINT:-http://127.0.0.1:50051}"
export ASTRA_VECTOR_SMOKE_CONCURRENCY="${ASTRA_VECTOR_SMOKE_CONCURRENCY:-50}"
fix485_run_logged concurrent-live cargo test --locked --features integration-tests --test smoke_load_retrieve_context -- --ignored --nocapture || {
  fix485_write_summary FAIL LIVE_CONCURRENT_RETRIEVAL_FAILED
  exit "$FAIL_STATUS"
}
fix485_write_summary BLOCKED MIXED_70_20_10_AND_60_MINUTE_SOAK_NOT_EXECUTED
exit "$BLOCKED_STATUS"
