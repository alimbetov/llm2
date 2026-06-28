#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
load_smoke_env
blocked "access-zone isolation requires v004 control/search APIs that are not registered"
