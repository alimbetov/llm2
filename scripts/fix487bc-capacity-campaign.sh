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
# shellcheck disable=SC1091
. scripts/local-demo/common.sh

if [[ "${ASTRAVECTOR_PROFILE}" == "local-demo" ]]; then
  export ASTRAVECTOR_PROFILE="fix489-capacity"
fi
export FIX489_CLIENT_DEADLINE_MS="${FIX489_CLIENT_DEADLINE_MS:-45000}"

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

docker compose up -d postgres qdrant
scripts/local-demo/infra-wait.sh
cargo sqlx migrate run
cargo build --release --locked
if ! grpcurl -plaintext 127.0.0.1:50051 list >/dev/null 2>&1; then
  scripts/local-demo/run-runtime.sh
fi
grpcurl -plaintext 127.0.0.1:50051 list >"$EVIDENCE_DIR/grpc-services.txt"

python3 scripts/fix487bc_capacity_campaign.py --output "$EVIDENCE_DIR"
python3 scripts/fix489_live_capacity.py --capacity-output "$EVIDENCE_DIR"
python3 scripts/fix487bc_capacity_evidence.py --root "$EVIDENCE_DIR"
STATUS="PASS"
REASON="FIX489_CAPACITY_CAMPAIGN_PASS"
echo "FIX489_CAPACITY_CAMPAIGN_PASS evidence=${EVIDENCE_DIR}"
