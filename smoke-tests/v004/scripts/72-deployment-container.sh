#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/fix485.sh"
fix485_evidence_dir packaging
cd "$PROJECT_DIR"

tag="astravector:fix485-${FIX485_RUN_ID:-local}"
fix485_run_logged docker-build docker build --pull=false -t "$tag" . || {
  fix485_write_summary FAIL DOCKER_BUILD_FAILED
  exit "$FAIL_STATUS"
}
docker image inspect "$tag" >"$FIX485_EVIDENCE_DIR/image-inspect.json"
if docker history --no-trunc "$tag" | rg -i '(password=|api[_-]?key=|bearer )' >"$FIX485_EVIDENCE_DIR/image-secret-findings.log"; then
  fix485_write_summary FAIL IMAGE_HISTORY_SECRET_FOUND
  exit "$FAIL_STATUS"
fi

if ! command -v kubectl >/dev/null 2>&1; then
  fix485_write_summary BLOCKED KUBECTL_NOT_AVAILABLE_FOR_SERVER_DRY_RUN
  exit "$BLOCKED_STATUS"
fi
fix485_write_summary BLOCKED TEST_CLUSTER_NOT_CONFIGURED_FOR_ROLLOUT_ROLLBACK
exit "$BLOCKED_STATUS"
