#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_ID="${FIX487C_RUN_ID:-fix487c-$(date -u +%Y%m%dT%H%M%SZ)}"
EVIDENCE_ROOT="${ASTRAVECTOR_EVIDENCE_ROOT:-${ROOT_DIR}/../astravector-evidence}"
EVIDENCE_DIR="${EVIDENCE_ROOT}/fix487c/${RUN_ID}"
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

if [[ "${ASTRAVECTOR_FIX487C_EXECUTE_SOAK:-false}" != "true" ]]; then
  STATUS="BLOCKED"
  REASON="EXPLICIT_SOAK_OPT_IN_REQUIRED"
  printf '{"run_id":"%s","status":"BLOCKED","reason":"%s"}\n' "$RUN_ID" "$REASON" >"$EVIDENCE_DIR/bootstrap.json"
  echo "FIX487C_BLOCKED=EXPLICIT_SOAK_OPT_IN_REQUIRED"
  exit 2
fi

if [[ -n "$(git status --short)" ]]; then
  STATUS="BLOCKED"
  REASON="DIRTY_WORKTREE"
  echo "FIX487C_BLOCKED=DIRTY_WORKTREE"
  exit 2
fi

make verify-fix487a-retrieval-freeze
make verify-fix487c-soak-contracts

if [[ -z "${FIX487BC_CAPACITY_EVIDENCE_DIR:-}" || ! -f "${FIX487BC_CAPACITY_EVIDENCE_DIR}/capacity-curve.json" ]]; then
  STATUS="BLOCKED"
  REASON="CAPACITY_EVIDENCE_NOT_AVAILABLE"
  echo "FIX487C_SOAK_60M_BLOCKED reason=CAPACITY_EVIDENCE_NOT_AVAILABLE"
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

python3 scripts/fix487c_soak.py --capacity-curve "${FIX487BC_CAPACITY_EVIDENCE_DIR}/capacity-curve.json" >"$EVIDENCE_DIR/soak-plan.json"
python3 scripts/fix489_live_capacity.py --soak-output "$EVIDENCE_DIR" --capacity-root "$FIX487BC_CAPACITY_EVIDENCE_DIR"
python3 scripts/fix487c_soak.py --verify-evidence-root "$EVIDENCE_DIR"
STATUS="PASS"
REASON="FIX489_SOAK_60M_PASS"
echo "FIX489_SOAK_60M_PASS evidence=${EVIDENCE_DIR}"
