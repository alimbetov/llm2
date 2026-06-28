#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/assertions.sh"
source "$SMOKE_ROOT/lib/grpc.sh"
source "$SMOKE_ROOT/lib/processes.sh"
load_smoke_env

command -v cargo >/dev/null 2>&1 || blocked "cargo not found"
[[ -f "$ASTRAVECTOR_MODEL_PATH" && -f "$ASTRAVECTOR_TOKENIZER_PATH" ]] || blocked "model/tokenizer files are not available"
[[ -x "$PROJECT_DIR/target/debug/astravector-runtime" ]] || blocked "target/debug/astravector-runtime is missing; run build smoke first"

start_process runtime "$PROJECT_DIR/target/debug/astravector-runtime"
wait_for_grpc "$ASTRAVECTOR_GRPC_HOST" "$ASTRAVECTOR_GRPC_PORT" 60
sleep 3
assert_process_running "$(cat "$RUNTIME_DIR/runtime.pid")"
grpc_plain list >"$LOGS_DIR/services-grpc-list.txt" 2>"$LOGS_DIR/services-grpc-list.err" || fail "runtime gRPC service listing failed after startup"
grep -Fx "astravector.embedding.v1.AstraVectorRuntime" "$LOGS_DIR/services-grpc-list.txt" >/dev/null || fail "runtime service not listed after startup"
if grep -E "panic|thread '.*' panicked|ERROR|FATAL" "$LOGS_DIR/runtime.log" >/dev/null; then
  fail "runtime log contains panic/error"
fi
