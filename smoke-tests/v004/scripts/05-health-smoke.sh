#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/assertions.sh"
source "$SMOKE_ROOT/lib/grpc.sh"
load_smoke_env

command -v grpcurl >/dev/null 2>&1 || blocked "grpcurl not found"
rc=0
services="$(grpc_plain list 2>"$LOGS_DIR/grpc-list.err")" || fail "grpc service listing failed"
echo "$services" > "$LOGS_DIR/grpc-services.txt"
grep -Fx "astravector.embedding.v1.AstraVectorRuntime" "$LOGS_DIR/grpc-services.txt" >/dev/null || { fail "runtime service not registered"; rc=1; }
grep -Fx "grpc.health.v1.Health" "$LOGS_DIR/grpc-services.txt" >/dev/null || { fail "standard grpc health service not registered"; rc=1; }
if ! grep -Fx "astravector.embedding.v1.AstraVectorV004Control" "$LOGS_DIR/grpc-services.txt" >/dev/null; then
  blocked "V004_CONTROL_NOT_REGISTERED"
fi

std_health="$(grpc_plain grpc.health.v1.Health/Check 2>"$LOGS_DIR/std-health.err")" || { fail "standard health call failed"; rc=1; std_health='{}'; }
echo "$std_health" > "$LOGS_DIR/std-health.json"
jq -e '.status == "SERVING"' "$LOGS_DIR/std-health.json" >/dev/null || { fail "standard health is not SERVING"; rc=1; }

health="$(grpc_plain astravector.embedding.v1.AstraVectorRuntime/Health 2>"$LOGS_DIR/health.err")" || { fail "runtime health call failed"; rc=1; health='{}'; }
echo "$health" > "$LOGS_DIR/runtime-health.json"
jq -e '.status == "SERVING" and .ready == true' "$LOGS_DIR/runtime-health.json" >/dev/null || { fail "runtime health is not SERVING"; rc=1; }
exit "$rc"
