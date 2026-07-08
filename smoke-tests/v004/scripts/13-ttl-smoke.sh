#!/usr/bin/env bash
set -uo pipefail
source "$(dirname "$0")/../lib/common.sh"
load_smoke_env
blocked "group TTL modes are declared in v004 control proto but not registered; legacy ExtendVectorTtl needs existing binding"
