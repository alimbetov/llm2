#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_ID="${FIX487BC_RUN_ID:-fix487bc-$(date -u +%Y%m%dT%H%M%SZ)}"
EVIDENCE_ROOT="${ASTRAVECTOR_EVIDENCE_ROOT:-${ROOT_DIR}/../astravector-evidence}"
EVIDENCE_DIR="${EVIDENCE_ROOT}/fix487bc/${RUN_ID}"
STATUS="BLOCKED"
REASON="UNKNOWN"

finish() {
  local exit_code=$?
  mkdir -p "$EVIDENCE_DIR"
  printf '{"status":"%s","reason":"%s","exit_code":%s}\n' "$STATUS" "$REASON" "$exit_code" >"$EVIDENCE_DIR/terminal-status.json"
  exit "$exit_code"
}
trap finish EXIT INT TERM

cd "$ROOT_DIR"
mkdir -p "$EVIDENCE_DIR"

if [[ "${ASTRAVECTOR_FIX487BC_EXECUTE_CAPACITY:-false}" != "true" ]]; then
  STATUS="BLOCKED"
  REASON="EXPLICIT_CAPACITY_OPT_IN_REQUIRED"
  printf '{"run_id":"%s","status":"BLOCKED","reason":"%s"}\n' "$RUN_ID" "$REASON" >"$EVIDENCE_DIR/bootstrap.json"
  echo "FIX487BC_BLOCKED=EXPLICIT_CAPACITY_OPT_IN_REQUIRED"
  exit 2
fi

if [[ -n "$(git status --short)" ]]; then
  STATUS="BLOCKED"
  REASON="DIRTY_WORKTREE"
  echo "FIX487BC_BLOCKED=DIRTY_WORKTREE"
  exit 2
fi

make verify-fix487a-retrieval-freeze
make verify-fix487b-contracts
make verify-fix487bc-capacity-contracts

if [[ -z "${ASTRAVECTOR_MODEL_PATH:-}" || ! -f "${ASTRAVECTOR_MODEL_PATH:-}" ]]; then
  STATUS="BLOCKED"
  REASON="MODEL_NOT_AVAILABLE"
  echo "FIX487BC_CAPACITY_CAMPAIGN_BLOCKED reason=MODEL_NOT_AVAILABLE"
  exit 2
fi
if [[ -z "${ASTRAVECTOR_TOKENIZER_PATH:-}" || ! -f "${ASTRAVECTOR_TOKENIZER_PATH:-}" ]]; then
  STATUS="BLOCKED"
  REASON="TOKENIZER_NOT_AVAILABLE"
  echo "FIX487BC_CAPACITY_CAMPAIGN_BLOCKED reason=TOKENIZER_NOT_AVAILABLE"
  exit 2
fi

python3 scripts/fix487bc_capacity_campaign.py --output "$EVIDENCE_DIR"
STATUS="BLOCKED"
REASON="LIVE_CAPACITY_EXECUTION_NOT_IMPLEMENTED_IN_THIS_RUN"
echo "FIX487BC_CAPACITY_CAMPAIGN_BLOCKED reason=${REASON}"
exit 2
