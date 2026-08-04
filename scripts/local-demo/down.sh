#!/usr/bin/env bash
set -Eeuo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/common.sh"
"${ROOT_DIR}/scripts/local-demo/stop-runtime.sh"
docker compose stop postgres qdrant

