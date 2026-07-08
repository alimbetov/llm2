#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
load_smoke_env
blocked "DeleteChunkGroup API is not registered; legacy DeleteDocumentVectors needs persisted bindings"
