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

# Unit semantics are necessary but are not a substitute for deterministic live
# Dense/Sparse/FTS timeout, cancellation and permit-release failpoints.
fix485_write_summary BLOCKED LIVE_RETRIEVAL_BACKEND_FAILPOINTS_NOT_IMPLEMENTED
exit "$BLOCKED_STATUS"
