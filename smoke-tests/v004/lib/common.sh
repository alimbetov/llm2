#!/usr/bin/env bash
set -uo pipefail

SMOKE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROJECT_DIR="$(cd "$SMOKE_ROOT/../.." && pwd)"
SMOKE_ARTIFACT_DIR="${SMOKE_ARTIFACT_DIR:-$SMOKE_ROOT}"
RESULTS_DIR="$SMOKE_ARTIFACT_DIR/results"
REPORTS_DIR="$SMOKE_ARTIFACT_DIR/reports"
LOGS_DIR="$SMOKE_ARTIFACT_DIR/logs"
RUNTIME_DIR="$SMOKE_ARTIFACT_DIR/runtime"
mkdir -p "$RESULTS_DIR" "$REPORTS_DIR" "$LOGS_DIR" "$RUNTIME_DIR"

PASS=0
FAIL_STATUS=1
BLOCKED_STATUS=2
SKIPPED_STATUS=3

load_smoke_env() {
  local env_file="${SMOKE_ENV_FILE:-$SMOKE_ROOT/.env.smoke}"
  if [[ ! -f "$env_file" ]]; then
    env_file="$SMOKE_ROOT/.env.smoke.example"
  fi
  export SMOKE_ENV_FILE="$env_file"
  if [[ -f "$env_file" ]]; then
    set -a
    # shellcheck disable=SC1090
    . "$env_file"
    set +a
  fi
  export SMOKE_RUN_ID="${SMOKE_RUN_ID:-$(date +%Y%m%d%H%M%S)}"
  export ASTRAVECTOR_PROJECT_DIR="${ASTRAVECTOR_PROJECT_DIR:-$PROJECT_DIR}"
  export ASTRAVECTOR_CONFIG="${ASTRAVECTOR_CONFIG:-$SMOKE_ROOT/config/application-smoke.yaml}"
  if [[ -n "${POSTGRES_USER:-}" && -n "${POSTGRES_PASSWORD:-}" && -n "${POSTGRES_HOST:-}" && -n "${POSTGRES_PORT:-}" && -n "${POSTGRES_DB:-}" ]]; then
    export ASTRAVECTOR_DB_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@${POSTGRES_HOST}:${POSTGRES_PORT}/${POSTGRES_DB}"
  fi
  [[ -n "${QDRANT_HTTP_URL:-}" ]] && export ASTRAVECTOR_QDRANT_URL="$QDRANT_HTTP_URL"
  [[ -n "${QDRANT_COLLECTION:-}" ]] && export ASTRAVECTOR_QDRANT_COLLECTION="$QDRANT_COLLECTION"
}

now_iso() { date -u +"%Y-%m-%dT%H:%M:%SZ"; }
now_ms() {
  ruby -e 'puts (Time.now.to_f*1000).to_i' 2>/dev/null || awk 'BEGIN{srand(); print srand()*1000}'
}

log_info() { printf '[INFO] %s\n' "$*"; }
log_warn() { printf '[WARN] %s\n' "$*" >&2; }
log_error() { printf '[ERROR] %s\n' "$*" >&2; }
fail() { log_error "$*"; return "$FAIL_STATUS"; }
blocked() { log_warn "BLOCKED: $*"; return "$BLOCKED_STATUS"; }
skip() { log_warn "SKIPPED: $*"; return "$SKIPPED_STATUS"; }

json_escape() { jq -Rs . <<<"${1:-}"; }

write_result() {
  local test="$1" status="$2" started="$3" finished="$4" duration="$5" assertions="$6" errors="$7"
  jq -n \
    --arg test "$test" --arg status "$status" --arg started "$started" --arg finished "$finished" \
    --argjson duration "$duration" --argjson assertions "$assertions" --argjson errors "$errors" \
    '{test:$test,status:$status,started_at:$started,finished_at:$finished,duration_ms:$duration,assertions:$assertions,errors:$errors}' \
    > "$RESULTS_DIR/$test.json"
}

status_name() {
  case "$1" in
    0) printf PASS ;;
    1) printf FAIL ;;
    2) printf BLOCKED ;;
    3) printf SKIPPED ;;
    *) printf FAIL ;;
  esac
}

run_smoke_step() {
  local test="$1"; shift
  local started finished start_ms end_ms rc status
  started="$(now_iso)"
  start_ms="$(now_ms)"
  log_info "==> $test"
  "$@"
  rc=$?
  finished="$(now_iso)"
  end_ms="$(now_ms)"
  status="$(status_name "$rc")"
  write_result "$test" "$status" "$started" "$finished" "$((end_ms-start_ms))" "[]" "[]"
  log_info "<== $test: $status"
  return "$rc"
}

postgres_url() {
  printf 'postgres://%s:%s@%s:%s/%s' "$POSTGRES_USER" "$POSTGRES_PASSWORD" "$POSTGRES_HOST" "$POSTGRES_PORT" "$POSTGRES_DB"
}

compose_cmd() {
  docker compose --env-file "${SMOKE_ENV_FILE:-$SMOKE_ROOT/.env.smoke.example}" -f "$SMOKE_ROOT/docker-compose.smoke.yml" "$@"
}
