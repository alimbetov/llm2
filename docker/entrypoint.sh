#!/usr/bin/env bash
set -euo pipefail

astravector-model-bootstrap

if [ "$#" -eq 0 ]; then
  set -- astravector-runtime
fi

exec "$@"
