#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
[[ -z "$(git status --porcelain)" ]] || { echo 'BLOCKED: source tree is dirty' >&2; exit 2; }
sha7="$(git rev-parse --short=7 HEAD)"
evidence_root="${ASTRAVECTOR_EVIDENCE_ROOT:-$ROOT_DIR/../astravector-evidence}"
runs=()
for run in 1 2 3; do
  docker compose down -v --remove-orphans
  for port in 55432 6333 50051 9090; do
    if lsof -nP -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1; then
      echo "BLOCKED: port $port remains occupied before run $run" >&2
      exit 2
    fi
  done
  run_id="fix480-${sha7}-run-${run}"
  ASTRAVECTOR_PROFILE=search-production-candidate LOAD_RUN_ID="$run_id" \
    "$ROOT_DIR/scripts/macbook-model-backed-load.sh"
  report="$evidence_root/$run_id/astravector-macbook-load-report.json"
  jq -e '.overall_verdict == "PASS"' "$report" >/dev/null
  runs+=("$report")
done
python3 "$ROOT_DIR/scripts/finalize_fix480_repeatability_report.py" \
  --output-dir "$evidence_root/fix480-${sha7}-repeatability" "${runs[@]}"
