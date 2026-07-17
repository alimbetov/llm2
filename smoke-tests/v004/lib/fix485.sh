#!/usr/bin/env bash

fix485_evidence_dir() {
  local stage="$1"
  local root="${ASTRAVECTOR_EVIDENCE_ROOT:-/Users/ruslanalimbetov/Documents/llm2/astravector-evidence}"
  local run_id="${FIX485_RUN_ID:-fix485-$(date -u +%Y%m%d-%H%M%S)}"
  FIX485_EVIDENCE_DIR="$root/$run_id/$stage"
  mkdir -p "$FIX485_EVIDENCE_DIR"
  export FIX485_EVIDENCE_DIR
}

fix485_run_logged() {
  local name="$1"
  shift
  set +e
  "$@" >"$FIX485_EVIDENCE_DIR/$name.log" 2>&1
  local rc=$?
  set -e
  jq -n --arg name "$name" --argjson exit_code "$rc" \
    '{assertion:$name,exit_code:$exit_code,status:(if $exit_code == 0 then "PASS" else "FAIL" end)}' \
    >"$FIX485_EVIDENCE_DIR/$name.json"
  return "$rc"
}

fix485_write_summary() {
  local status="$1"
  local reason="$2"
  jq -n \
    --arg status "$status" \
    --arg reason "$reason" \
    --arg source_sha "$(git -C "$PROJECT_DIR" rev-parse HEAD)" \
    --arg cargo_lock_sha "$(shasum -a 256 "$PROJECT_DIR/Cargo.lock" | awk '{print $1}')" \
    --arg model_path "${ASTRAVECTOR_MODEL_PATH:-}" \
    --arg tokenizer_path "${ASTRAVECTOR_TOKENIZER_PATH:-}" \
    '{status:$status,reason:$reason,source_sha:$source_sha,cargo_lock_sha256:$cargo_lock_sha,model_path:$model_path,tokenizer_path:$tokenizer_path}' \
    >"$FIX485_EVIDENCE_DIR/summary.json"
}
