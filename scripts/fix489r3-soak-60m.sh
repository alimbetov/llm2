#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_ID="${FIX489R3_SOAK_RUN_ID:-fix489-r3-soak-$(date -u +%Y%m%dT%H%M%SZ)}"
EVIDENCE_ROOT="${ASTRAVECTOR_EVIDENCE_ROOT:-${ROOT_DIR}/../astravector-evidence}"
EVIDENCE_DIR="${EVIDENCE_ROOT}/fix489-r3-soak/${RUN_ID}"
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
export FIX489_CLIENT_DEADLINE_MS="${FIX489_CLIENT_DEADLINE_MS:-67500}"
export FIX489_SOAK_MODE="LOCAL_STABLE_FLOOR"

if [[ "${ASTRAVECTOR_FIX489R3_EXECUTE_SOAK:-false}" != "true" ]]; then
  STATUS="BLOCKED"
  REASON="EXPLICIT_SOAK_OPT_IN_REQUIRED"
  printf '{"run_id":"%s","status":"BLOCKED","reason":"%s"}\n' "$RUN_ID" "$REASON" >"$EVIDENCE_DIR/bootstrap.json"
  echo "FIX489_R3_SOAK_BLOCKED=EXPLICIT_SOAK_OPT_IN_REQUIRED"
  exit 2
fi

if [[ -n "$(git status --short)" ]]; then
  STATUS="BLOCKED"
  REASON="DIRTY_WORKTREE"
  echo "FIX489_R3_SOAK_BLOCKED=DIRTY_WORKTREE"
  exit 2
fi

REASON="RETRIEVAL_FREEZE_FAILED"
make verify-fix487a-retrieval-freeze
REASON="R3_SOAK_CONTRACTS_FAILED"
env -u FIX489_CAMPAIGN_MODE -u FIX489_CAPACITY_LEVELS make verify-fix489r3-soak-contracts

if [[ -z "${FIX489_R3_CAPACITY_EVIDENCE_DIR:-}" || ! -f "${FIX489_R3_CAPACITY_EVIDENCE_DIR}/capacity-curve.json" ]]; then
  STATUS="BLOCKED"
  REASON="CAPACITY_EVIDENCE_NOT_AVAILABLE"
  echo "FIX489_R3_SOAK_60M_BLOCKED reason=CAPACITY_EVIDENCE_NOT_AVAILABLE"
  exit 2
fi

docker compose up -d postgres qdrant
scripts/local-demo/infra-wait.sh
REASON="MIGRATION_FAILED"
cargo sqlx migrate run
REASON="SQLX_PREPARE_CHECK_FAILED"
cargo sqlx prepare --check -- --all-targets --all-features
REASON="RELEASE_BUILD_FAILED"
cargo build --release --locked
if ! grpcurl -plaintext 127.0.0.1:50051 list >/dev/null 2>&1; then
  REASON="RUNTIME_START_FAILED"
  scripts/local-demo/run-runtime.sh
fi
REASON="GRPC_REFLECTION_FAILED"
grpcurl -plaintext 127.0.0.1:50051 list >"$EVIDENCE_DIR/grpc-services.txt"

python3 scripts/fix487c_soak.py --capacity-curve "${FIX489_R3_CAPACITY_EVIDENCE_DIR}/capacity-curve.json" >"$EVIDENCE_DIR/soak-plan.json"
python3 scripts/fix489_live_capacity.py --soak-output "$EVIDENCE_DIR" --capacity-root "$FIX489_R3_CAPACITY_EVIDENCE_DIR"
python3 scripts/fix487c_soak.py --verify-evidence-root "$EVIDENCE_DIR"
STATUS="PASS"
REASON="FIX489_R3_SOAK_60M_PASS"
echo "FIX489_R3_SOAK_60M_PASS evidence=${EVIDENCE_DIR}"
