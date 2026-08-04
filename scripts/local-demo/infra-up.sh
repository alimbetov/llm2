#!/usr/bin/env bash
set -Eeuo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/common.sh"
docker compose up -d postgres qdrant

