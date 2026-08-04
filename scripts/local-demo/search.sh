#!/usr/bin/env bash
set -Eeuo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/common.sh"
run_local_demo_py search "$@"

