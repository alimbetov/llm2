#!/usr/bin/env bash
set -euo pipefail

log() {
  printf '[astravector-bootstrap] %s\n' "$*" >&2
}

fail() {
  log "FAIL: $*"
  exit 1
}

require_env() {
  local name="$1"
  if [ -z "${!name:-}" ]; then
    fail "required environment variable is missing: ${name}"
  fi
}

sha256_matches() {
  local path="$1"
  local expected="$2"
  [ -f "$path" ] || return 1
  printf '%s  %s\n' "$expected" "$path" | sha256sum -c --status
}

download_verified() {
  local repo_url="$1"
  local file_name="$2"
  local target="$3"
  local expected_sha="$4"
  local tmp
  tmp="$(mktemp "${target}.part.XXXXXX")"
  rm -f "$tmp"

  local netrc
  netrc="$(mktemp "${ASTRAVECTOR_MODEL_DIR}/.netrc.XXXXXX")"
  chmod 0600 "$netrc"
  trap 'rm -f "$netrc" "$tmp"' RETURN
  {
    printf 'machine %s\n' "$(printf '%s' "$repo_url" | awk -F/ '{print $3}')"
    printf 'login %s\n' "$ASTRAVECTOR_NEXUS_USERNAME"
    printf 'password %s\n' "$ASTRAVECTOR_NEXUS_PASSWORD"
  } >"$netrc"

  log "downloading ${file_name}"
  curl --fail --show-error --silent --location --http1.1 --retry-all-errors --continue-at - \
    --retry "${ASTRAVECTOR_BOOTSTRAP_CURL_RETRIES:-5}" \
    --retry-delay "${ASTRAVECTOR_BOOTSTRAP_CURL_RETRY_DELAY_SECONDS:-2}" \
    --connect-timeout "${ASTRAVECTOR_BOOTSTRAP_CONNECT_TIMEOUT_SECONDS:-10}" \
    --max-time "${ASTRAVECTOR_BOOTSTRAP_DOWNLOAD_MAX_SECONDS:-900}" \
    --netrc-file "$netrc" \
    --output "$tmp" \
    "${repo_url%/}/${file_name}" || {
      rm -f "$netrc" "$tmp"
      fail "download failed for ${file_name}"
    }

  sha256_matches "$tmp" "$expected_sha" || {
    rm -f "$netrc" "$tmp"
    fail "checksum mismatch for downloaded ${file_name}"
  }
  mv -f "$tmp" "$target"
  rm -f "$netrc"
  trap - RETURN
}

ensure_artifact() {
  local repo_url="$1"
  local file_name="$2"
  local target="$3"
  local expected_sha="$4"

  if sha256_matches "$target" "$expected_sha"; then
    log "cache valid: ${file_name}"
    return 0
  fi

  require_env ASTRAVECTOR_NEXUS_USERNAME
  require_env ASTRAVECTOR_NEXUS_PASSWORD
  rm -f "$target"
  download_verified "$repo_url" "$file_name" "$target" "$expected_sha"
  sha256_matches "$target" "$expected_sha" || fail "checksum mismatch after promotion: ${file_name}"
}

wait_tcp() {
  local label="$1"
  local host="$2"
  local port="$3"
  local deadline="${4:-60}"
  local start
  start="$(date +%s)"
  while true; do
    if nc -z "$host" "$port" >/dev/null 2>&1; then
      log "${label} reachable at ${host}:${port}"
      return 0
    fi
    if [ $(( $(date +%s) - start )) -ge "$deadline" ]; then
      fail "${label} not reachable within ${deadline}s at ${host}:${port}"
    fi
    sleep 2
  done
}

parse_url_host_port() {
  local url="$1"
  local default_port="$2"
  local rest="${url#*://}"
  rest="${rest%%/*}"
  rest="${rest##*@}"
  local host="$rest"
  local port="$default_port"
  if [[ "$rest" == \[*\]:* ]]; then
    host="${rest%%]:*}"
    host="${host#[}"
    port="${rest##*:}"
  elif [[ "$rest" == *:* ]]; then
    host="${rest%%:*}"
    port="${rest##*:}"
  fi
  [ -n "$host" ] || fail "cannot parse host from URL"
  printf '%s %s\n' "$host" "$port"
}

require_env ASTRAVECTOR_MODEL_REPOSITORY_URL
require_env ASTRAVECTOR_MODEL_DIR
require_env ASTRAVECTOR_MODEL_PATH
require_env ASTRAVECTOR_TOKENIZER_PATH
require_env ASTRAVECTOR_MODEL_SHA256
require_env ASTRAVECTOR_MODEL_DATA_SHA256
require_env ASTRAVECTOR_TOKENIZER_SHA256
require_env ASTRAVECTOR_DB_URL
require_env ASTRAVECTOR_QDRANT_URL
require_env ASTRAVECTOR_QDRANT_COLLECTION

mkdir -p "$ASTRAVECTOR_MODEL_DIR"
[ -w "$ASTRAVECTOR_MODEL_DIR" ] || fail "model directory is not writable: ${ASTRAVECTOR_MODEL_DIR}"

lock_dir="${ASTRAVECTOR_MODEL_DIR}/.bootstrap.lock"
lock_deadline="${ASTRAVECTOR_MODEL_LOCK_TIMEOUT_SECONDS:-900}"
lock_start="$(date +%s)"
while ! mkdir "$lock_dir" 2>/dev/null; do
  if [ $(( $(date +%s) - lock_start )) -ge "$lock_deadline" ]; then
    fail "model cache lock timeout after ${lock_deadline}s"
  fi
  sleep 2
done
trap 'rmdir "$lock_dir" 2>/dev/null || true' EXIT

model_data_path="${ASTRAVECTOR_MODEL_DATA_PATH:-${ASTRAVECTOR_MODEL_DIR}/model.onnx_data}"
ensure_artifact "$ASTRAVECTOR_MODEL_REPOSITORY_URL" model.onnx "$ASTRAVECTOR_MODEL_PATH" "$ASTRAVECTOR_MODEL_SHA256"
ensure_artifact "$ASTRAVECTOR_MODEL_REPOSITORY_URL" model.onnx_data "$model_data_path" "$ASTRAVECTOR_MODEL_DATA_SHA256"
ensure_artifact "$ASTRAVECTOR_MODEL_REPOSITORY_URL" tokenizer.json "$ASTRAVECTOR_TOKENIZER_PATH" "$ASTRAVECTOR_TOKENIZER_SHA256"

{
  printf '%s  model.onnx\n' "$ASTRAVECTOR_MODEL_SHA256"
  printf '%s  model.onnx_data\n' "$ASTRAVECTOR_MODEL_DATA_SHA256"
  printf '%s  tokenizer.json\n' "$ASTRAVECTOR_TOKENIZER_SHA256"
} >"${ASTRAVECTOR_MODEL_DIR}/manifest.sha256"

trap - EXIT
rmdir "$lock_dir" 2>/dev/null || true

read -r pg_host pg_port < <(parse_url_host_port "$ASTRAVECTOR_DB_URL" 5432)
read -r qdrant_host qdrant_port < <(parse_url_host_port "$ASTRAVECTOR_QDRANT_URL" 6333)
wait_tcp PostgreSQL "$pg_host" "$pg_port" "${ASTRAVECTOR_BOOTSTRAP_POSTGRES_TIMEOUT_SECONDS:-60}"
wait_tcp Qdrant "$qdrant_host" "$qdrant_port" "${ASTRAVECTOR_BOOTSTRAP_QDRANT_TIMEOUT_SECONDS:-60}"

log "model and dependency bootstrap complete"
