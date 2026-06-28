#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/assertions.sh"
load_smoke_env

rc=0
[[ "$(uname -s)" == "Darwin" ]] || { log_error "expected macOS"; rc=1; }
arch="$(uname -m)"
[[ "$arch" == "arm64" || "$arch" == "x86_64" ]] || { log_error "unsupported architecture: $arch"; rc=1; }
for cmd in cargo rustc docker curl jq grpcurl psql lsof shasum; do
  assert_command_exists "$cmd" || rc=1
done
assert_path_exists "$ASTRAVECTOR_PROJECT_DIR" || rc=1
assert_path_exists "$ASTRAVECTOR_CORPUS_DIR" || rc=1

if [[ -d "$ASTRAVECTOR_CORPUS_DIR" ]]; then
  found="$(find "$ASTRAVECTOR_CORPUS_DIR" -type f ! -name '.DS_Store' ! -name '.*' -size +0c | head -n 1)"
else
  found="$ASTRAVECTOR_CORPUS_DIR"
fi
[[ -n "$found" && -s "$found" ]] || { log_error "corpus has no non-empty readable files"; rc=1; }

if [[ ! -f "$ASTRAVECTOR_MODEL_PATH" || ! -f "$ASTRAVECTOR_TOKENIZER_PATH" ]]; then
  log_warn "model/tokenizer files are not available; runtime encode smoke will be BLOCKED"
fi
exit "$rc"
