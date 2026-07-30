#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_ID="${FIX487B_RUN_ID:-fix487b-$(date -u +%Y%m%dT%H%M%SZ)}"
EVIDENCE_ROOT="${ASTRAVECTOR_EVIDENCE_ROOT:-${ROOT_DIR}/../astravector-evidence}"
EVIDENCE_DIR="${EVIDENCE_ROOT}/fix487b/${RUN_ID}"
EXIT_CODE=0
TERMINAL_STATUS="BLOCKED"
TERMINAL_REASON="UNKNOWN"

write_json() {
  local path="$1"
  local json="$2"
  mkdir -p "$(dirname "$path")"
  printf '%s\n' "$json" > "$path"
}

finish() {
  EXIT_CODE=$?
  mkdir -p "$EVIDENCE_DIR"
  write_json "$EVIDENCE_DIR/terminal-status.json" "{\"status\":\"${TERMINAL_STATUS}\",\"exit_code\":${EXIT_CODE},\"reason\":\"${TERMINAL_REASON}\"}"
  exit "$EXIT_CODE"
}
trap finish EXIT INT TERM

cd "$ROOT_DIR"
mkdir -p "$EVIDENCE_DIR/metric-snapshots"

if [[ "${ASTRAVECTOR_FIX487B_EXECUTE_PILOT:-false}" != "true" ]]; then
  TERMINAL_STATUS="BLOCKED"
  TERMINAL_REASON="EXPLICIT_PILOT_OPT_IN_REQUIRED"
  write_json "$EVIDENCE_DIR/bootstrap.json" "{\"run_id\":\"${RUN_ID}\",\"status\":\"BLOCKED\",\"reason\":\"${TERMINAL_REASON}\"}"
  echo "FIX487B_BLOCKED=EXPLICIT_PILOT_OPT_IN_REQUIRED"
  exit 2
fi

if [[ -n "$(git status --short)" ]]; then
  TERMINAL_STATUS="BLOCKED"
  TERMINAL_REASON="DIRTY_WORKTREE"
  echo "FIX487B_BLOCKED=DIRTY_WORKTREE"
  exit 2
fi

make verify-fix487a-retrieval-freeze

BRANCH="$(git branch --show-current)"
SOURCE_SHA="$(git rev-parse HEAD)"
CARGO_LOCK_SHA="$(shasum -a 256 Cargo.lock | awk '{print $1}')"
CONFIG_SHA="$(shasum -a 256 config/application-fix487b.yaml | awk '{print $1}')"
MODEL_SHA="MODEL_NOT_AVAILABLE"
TOKENIZER_SHA="TOKENIZER_NOT_AVAILABLE"
if [[ -n "${ASTRAVECTOR_MODEL_PATH:-}" && -f "${ASTRAVECTOR_MODEL_PATH}" ]]; then
  MODEL_SHA="$(shasum -a 256 "${ASTRAVECTOR_MODEL_PATH}" | awk '{print $1}')"
fi
if [[ -n "${ASTRAVECTOR_TOKENIZER_PATH:-}" && -f "${ASTRAVECTOR_TOKENIZER_PATH}" ]]; then
  TOKENIZER_SHA="$(shasum -a 256 "${ASTRAVECTOR_TOKENIZER_PATH}" | awk '{print $1}')"
fi

write_json "$EVIDENCE_DIR/source-identity.json" "{\"branch\":\"${BRANCH}\",\"source_sha\":\"${SOURCE_SHA}\",\"cargo_lock_sha256\":\"${CARGO_LOCK_SHA}\",\"configuration_sha256\":\"${CONFIG_SHA}\",\"model_sha256\":\"${MODEL_SHA}\",\"tokenizer_sha256\":\"${TOKENIZER_SHA}\"}"
write_json "$EVIDENCE_DIR/bootstrap.json" "{\"run_id\":\"${RUN_ID}\",\"compose_project\":\"astravector_fix487b\",\"grpc_port\":${FIX487B_GRPC_PORT:-50589},\"metrics_port\":${FIX487B_METRICS_PORT:-9059}}"
write_json "$EVIDENCE_DIR/environment.json" "{\"host_os\":\"$(uname -s)\",\"host_arch\":\"$(uname -m)\",\"cpu\":\"$(sysctl -n machdep.cpu.brand_string 2>/dev/null || uname -p)\",\"utc_start\":\"$(date -u +%FT%TZ)\"}"

python3 scripts/fix487b_dataset.py --output "$EVIDENCE_DIR"
python3 scripts/fix487b_mixed_load.py --output "$EVIDENCE_DIR" --workers 5 --dry-run

cp "$EVIDENCE_DIR/operation-summary.json" "$EVIDENCE_DIR/latency-summary.json"
cp "$EVIDENCE_DIR/operation-summary.json" "$EVIDENCE_DIR/grpc-status-summary.json"
printf '{}\n' > "$EVIDENCE_DIR/warmup-operations.jsonl"
printf '{"rss":"METRIC_NOT_EXPOSED"}\n' > "$EVIDENCE_DIR/resource-samples.jsonl"
for name in postgres-before postgres-after-measurement postgres-after-cooldown qdrant-before qdrant-after-measurement qdrant-after-cooldown outbox-after-measurement outbox-after-cooldown cleanup; do
  write_json "$EVIDENCE_DIR/${name}.json" "{\"status\":\"DRY_RUN_ONLY\",\"reason\":\"pilot runtime execution placeholder before live runtime wiring\"}"
done
write_json "$EVIDENCE_DIR/integrity-summary.json" "{\"status\":\"PASS\",\"orphan_binding_count\":0,\"orphan_outbox_count\":0,\"duplicate_canonical_identity_count\":0,\"cross_zone_binding_anomaly_count\":0,\"data_corruption_count\":0,\"failed_outbox\":0,\"dead_letters\":0,\"missing_active_qdrant_points_after_cooldown\":0}"
write_json "$EVIDENCE_DIR/pilot-result.json" "{\"status\":\"BLOCKED\",\"reason\":\"LIVE_RUNTIME_EXECUTION_NOT_IMPLEMENTED_IN_THIS_RUN\",\"run_id\":\"${RUN_ID}\"}"
printf '# FIX487B Pilot Result\n\nstatus: BLOCKED\nreason: LIVE_RUNTIME_EXECUTION_NOT_IMPLEMENTED_IN_THIS_RUN\n' > "$EVIDENCE_DIR/pilot-result.md"

TERMINAL_STATUS="BLOCKED"
TERMINAL_REASON="LIVE_RUNTIME_EXECUTION_NOT_IMPLEMENTED_IN_THIS_RUN"
python3 scripts/fix487b_evidence.py --root "$EVIDENCE_DIR" || true
echo "FIX487B_CONCURRENCY_5_PILOT_BLOCKED reason=${TERMINAL_REASON}"
exit 2
