#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/../lib/common.sh"
source "$SMOKE_ROOT/lib/fix485.sh"
fix485_evidence_dir long-query

export ASTRAVECTOR_MODEL_PATH="${ASTRAVECTOR_MODEL_PATH:-/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/onnx/model.onnx}"
export ASTRAVECTOR_TOKENIZER_PATH="${ASTRAVECTOR_TOKENIZER_PATH:-/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/tokenizer.json}"
export ASTRAVECTOR_CONFIG="$PROJECT_DIR/config/application.yaml"

if [[ ! -f "$ASTRAVECTOR_MODEL_PATH" || ! -f "$ASTRAVECTOR_TOKENIZER_PATH" ]]; then
  fix485_write_summary BLOCKED MODEL_FILES_NOT_FOUND
  exit "$BLOCKED_STATUS"
fi

cd "$PROJECT_DIR"
fix485_run_logged tokenizer-boundaries cargo test --locked --test query_tokenizer_model_backed -- --nocapture || {
  fix485_write_summary FAIL MODEL_BACKED_BOUNDARY_ASSERTION_FAILED
  exit "$FAIL_STATUS"
}
fix485_run_logged normalization-offsets cargo test --locked --test query_normalization_offsets -- --nocapture || {
  fix485_write_summary FAIL NORMALIZATION_OFFSET_ASSERTION_FAILED
  exit "$FAIL_STATUS"
}
fix485_run_logged intent-evidence cargo test --locked --test multi_intent_evidence -- --nocapture || {
  fix485_write_summary FAIL MULTI_INTENT_EVIDENCE_ASSERTION_FAILED
  exit "$FAIL_STATUS"
}

fix485_write_summary PASS MODEL_BACKED_LONG_QUERY_ASSERTIONS_PASSED
exit "$PASS"
