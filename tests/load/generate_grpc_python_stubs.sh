#!/usr/bin/env bash
set -euo pipefail
mkdir -p tests/load/generated
python -m grpc_tools.protoc \
  -I proto \
  --python_out tests/load/generated \
  --grpc_python_out tests/load/generated \
  proto/astravector_embedding.proto
