#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
load_smoke_env
blocked "fault-injection hooks for PostgreSQL/Qdrant outage and outbox crash recovery are not implemented"
