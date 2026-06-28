#!/usr/bin/env bash
set -uo pipefail
exec "$(cd "$(dirname "$0")/.." && pwd)/42-bm25-hybrid-retrieval.sh"
