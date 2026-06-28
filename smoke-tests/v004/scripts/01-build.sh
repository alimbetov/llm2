#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
load_smoke_env

report="$REPORTS_DIR/build-report.md"
{
  echo "# Build Report"
  echo
  echo "- Started: $(now_iso)"
  if command -v rustc >/dev/null 2>&1; then
    echo "- Rust: $(rustc --version)"
  else
    echo "- Rust: not found"
  fi
  if command -v cargo >/dev/null 2>&1; then
    echo "- Cargo: $(cargo --version)"
  else
    echo "- Cargo: not found"
  fi
  echo
} > "$report"

command -v cargo >/dev/null 2>&1 || { echo "- Status: FAIL, cargo not found" >> "$report"; exit 1; }
[[ -f Cargo.lock ]] || cargo generate-lockfile >>"$LOGS_DIR/build.log" 2>&1 || exit 1

steps=(
  "cargo fmt --check"
  "cargo check --all-targets --all-features"
  "cargo test --all-targets --all-features"
  "cargo clippy --all-targets --all-features -- -D warnings"
  "cargo build --release"
  "cargo build --release --locked"
)

for step in "${steps[@]}"; do
  echo "## $step" >> "$report"
  if $step >>"$LOGS_DIR/build.log" 2>&1; then
    echo "PASS" >> "$report"
  else
    echo "FAIL" >> "$report"
    echo "- Failed command: \`$step\`" >> "$report"
    exit 1
  fi
done
echo "- Finished: $(now_iso)" >> "$report"
