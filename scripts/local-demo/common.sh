#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LOCAL_DEMO_DIR="${ROOT_DIR}/.local-demo"
LOCAL_DEMO_ENV="${LOCAL_DEMO_DIR}/demo.env"
LOCAL_DEMO_PY="${ROOT_DIR}/scripts/local-demo/local_demo.py"

mkdir -p "${LOCAL_DEMO_DIR}"

if [[ -f "${ROOT_DIR}/.env.local-demo" ]]; then
  set -a
  # shellcheck disable=SC1091
  . "${ROOT_DIR}/.env.local-demo"
  set +a
fi

if [[ -f "${LOCAL_DEMO_ENV}" ]]; then
  set -a
  # shellcheck disable=SC1090
  . "${LOCAL_DEMO_ENV}"
  set +a
fi

export DATABASE_URL="${DATABASE_URL:-postgres://astravector:astravector@127.0.0.1:55432/astravector}"
export ASTRAVECTOR_DB_URL="${ASTRAVECTOR_DB_URL:-${DATABASE_URL}}"
export ASTRAVECTOR_QDRANT_URL="${ASTRAVECTOR_QDRANT_URL:-http://127.0.0.1:6333}"
export ASTRAVECTOR_QDRANT_COLLECTION="${ASTRAVECTOR_QDRANT_COLLECTION:-astravector_local_demo}"
export ASTRAVECTOR_CONFIG="${ASTRAVECTOR_CONFIG:-config/application.yaml}"
export ASTRAVECTOR_PROFILE="${ASTRAVECTOR_PROFILE:-local-demo}"
export ASTRAVECTOR_LOCAL_DEMO_GRPC_ADDR="${ASTRAVECTOR_LOCAL_DEMO_GRPC_ADDR:-127.0.0.1:50051}"
export ASTRAVECTOR_LOCAL_DEMO_METRICS_URL="${ASTRAVECTOR_LOCAL_DEMO_METRICS_URL:-http://127.0.0.1:9090}"
export ASTRAVECTOR_LOCAL_DEMO_ACCESS_ZONE_CODE="${ASTRAVECTOR_LOCAL_DEMO_ACCESS_ZONE_CODE:-0488}"
export ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION="${ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION:-true}"
export ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_SEARCH="${ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_SEARCH:-false}"
export ASTRAVECTOR_EVIDENCE_ROOT="${ASTRAVECTOR_EVIDENCE_ROOT:-${ROOT_DIR}/../astravector-evidence}"

run_local_demo_py() {
  python3 "${LOCAL_DEMO_PY}" "$@"
}

