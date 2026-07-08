#!/usr/bin/env bash

start_process() {
  local name="$1"; shift
  local log="$LOGS_DIR/$name.log"
  nohup env RUST_LOG="${RUST_LOG:-info}" RUST_BACKTRACE=1 "$@" >"$log" 2>&1 </dev/null &
  local pid=$!
  disown "$pid" >/dev/null 2>&1 || true
  echo "$pid" > "$RUNTIME_DIR/$name.pid"
  sleep 1
  assert_process_running "$pid"
}

stop_process() {
  local name="$1"
  local pid_file="$RUNTIME_DIR/$name.pid"
  [[ -f "$pid_file" ]] || return 0
  local pid
  pid="$(cat "$pid_file")"
  if kill -0 "$pid" >/dev/null 2>&1; then
    kill "$pid" >/dev/null 2>&1 || true
    for _ in 1 2 3 4 5 6 7 8 9 10; do
      kill -0 "$pid" >/dev/null 2>&1 || break
      sleep 1
    done
    kill -0 "$pid" >/dev/null 2>&1 && kill -9 "$pid" >/dev/null 2>&1 || true
  fi
  rm -f "$pid_file"
}

stop_all_processes() {
  for f in "$RUNTIME_DIR"/*.pid; do
    [[ -e "$f" ]] || continue
    stop_process "$(basename "$f" .pid)"
  done
}
