#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
load_smoke_env
blocked "enrichment worker uses DisabledEnrichmentProvider; provider-backed enrichment is not implemented"
