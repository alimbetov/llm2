#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
load_smoke_env

"$SMOKE_ROOT/scripts/50-access-security-full-power.sh"
