#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
load_smoke_env
blocked "reconciliation binary initializes Reconciler but has no operational run-loop"
