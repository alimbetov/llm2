#!/usr/bin/env bash

grpc_addr() {
  printf '%s:%s' "${ASTRAVECTOR_GRPC_HOST:-127.0.0.1}" "${ASTRAVECTOR_GRPC_PORT:-55051}"
}

grpc_plain() {
  if [[ "${1:-}" == "-d" ]]; then
    local body="$2"
    shift 2
    grpcurl -plaintext -d "$body" "$(grpc_addr)" "$@"
    return
  fi
  grpcurl -plaintext "$(grpc_addr)" "$@"
}

grpc_assert_service() {
  local service="$1"
  grpc_plain list | grep -Fx "$service" >/dev/null || blocked "gRPC service not registered: $service"
}
