#!/usr/bin/env bash

assert_equals() {
  local expected="$1" actual="$2" message="${3:-values differ}"
  [[ "$expected" == "$actual" ]] || fail "$message expected=[$expected] actual=[$actual]"
}

assert_not_empty() {
  local value="${1:-}" message="${2:-value is empty}"
  [[ -n "$value" ]] || fail "$message"
}

assert_file_exists() {
  local path="$1"
  [[ -f "$path" ]] || fail "file does not exist: $path"
}

assert_path_exists() {
  local path="$1"
  [[ -e "$path" ]] || fail "path does not exist: $path"
}

assert_command_exists() {
  local cmd="$1"
  command -v "$cmd" >/dev/null 2>&1 || fail "required command not found: $cmd"
}

assert_process_running() {
  local pid="$1"
  kill -0 "$pid" >/dev/null 2>&1 || fail "process is not running: $pid"
}

assert_process_stopped() {
  local pid="$1"
  ! kill -0 "$pid" >/dev/null 2>&1 || fail "process is still running: $pid"
}

assert_http_status() {
  local url="$1" expected="$2"
  local status
  status="$(curl -sS -o /dev/null -w '%{http_code}' "$url" || true)"
  assert_equals "$expected" "$status" "HTTP status mismatch for $url"
}

assert_json_equals() {
  local json="$1" filter="$2" expected="$3"
  local actual
  actual="$(jq -r "$filter" <<<"$json")"
  assert_equals "$expected" "$actual" "JSON assertion failed: $filter"
}

assert_json_not_empty() {
  local json="$1" filter="$2"
  local actual
  actual="$(jq -r "$filter // empty" <<<"$json")"
  assert_not_empty "$actual" "JSON value is empty: $filter"
}

assert_sql_equals() {
  local sql="$1" expected="$2" actual
  actual="$(psql "$(postgres_url)" -Atqc "$sql")" || return "$FAIL_STATUS"
  assert_equals "$expected" "$actual" "SQL assertion failed: $sql"
}

wait_for_port() {
  local host="$1" port="$2" timeout="${3:-30}" start
  start="$(date +%s)"
  while (( $(date +%s) - start < timeout )); do
    if (echo >/dev/tcp/"$host"/"$port") >/dev/null 2>&1; then return 0; fi
    sleep 1
  done
  fail "port did not open: $host:$port"
}

wait_for_http() {
  local url="$1" timeout="${2:-30}" start status
  start="$(date +%s)"
  while (( $(date +%s) - start < timeout )); do
    status="$(curl -sS -o /dev/null -w '%{http_code}' "$url" || true)"
    [[ "$status" =~ ^2|3 ]] && return 0
    sleep 1
  done
  fail "HTTP endpoint not ready: $url last_status=$status"
}

wait_for_postgres() {
  local timeout="${1:-60}" start
  start="$(date +%s)"
  while (( $(date +%s) - start < timeout )); do
    psql "$(postgres_url)" -Atqc 'SELECT 1' >/dev/null 2>&1 && return 0
    sleep 1
  done
  fail "PostgreSQL not ready"
}

wait_for_qdrant() {
  wait_for_http "${QDRANT_HTTP_URL}/readyz" "${1:-60}"
}

wait_for_grpc() {
  local host="$1" port="$2" timeout="${3:-30}"
  wait_for_port "$host" "$port" "$timeout"
}
