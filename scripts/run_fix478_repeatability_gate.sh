#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
[[ -z "$(git status --porcelain)" ]] || { echo 'BLOCKED: source tree is dirty' >&2; exit 2; }

sha7="$(git rev-parse --short=7 HEAD)"
evidence_root="${ASTRAVECTOR_EVIDENCE_ROOT:-$ROOT_DIR/../astravector-evidence}"
runs=()
for run in 1 2 3; do
  run_id="fix478-${sha7}-run-${run}"
  LOAD_RUN_ID="$run_id" "$ROOT_DIR/scripts/macbook-model-backed-load.sh"
  runs+=("$evidence_root/$run_id/astravector-macbook-load-report.json")
done
python3 "$ROOT_DIR/scripts/finalize_fix478_repeatability_report.py" \
  --output "$evidence_root/fix478-${sha7}-repeatability-report.json" "${runs[@]}"
