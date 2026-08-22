#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

RUN_ID="${FIX491_RUN_ID:-fix491-$(date +%Y%m%d-%H%M%S)}"
OUT_DIR="${FIX491_EVIDENCE_DIR:-docs/fix491/evidence/${RUN_ID}}"
mkdir -p "$OUT_DIR"

export ASTRAVECTOR_PROFILE="${ASTRAVECTOR_PROFILE:-local-demo}"

run_capture() {
  local name="$1"
  local status=0
  shift
  echo "==> $name"
  set +e
  "$@" >"${OUT_DIR}/${name}.stdout" 2>"${OUT_DIR}/${name}.stderr"
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    printf '0\n' >"${OUT_DIR}/${name}.status"
    echo "PASS $name"
    return 0
  fi
  printf '%s\n' "$status" >"${OUT_DIR}/${name}.status"
  echo "FAIL $name status=$status"
  return 0
}

run_optional_capture() {
  local name="$1"
  local status=0
  shift
  echo "==> $name"
  set +e
  "$@" >"${OUT_DIR}/${name}.stdout" 2>"${OUT_DIR}/${name}.stderr"
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    printf '0\n' >"${OUT_DIR}/${name}.status"
    echo "PASS $name"
  else
    printf '%s\n' "$status" >"${OUT_DIR}/${name}.status"
    echo "BLOCKED $name status=$status"
  fi
}

run_capture cargo-fmt cargo fmt --all --check
run_capture cargo-check cargo check --locked --all-targets --all-features
run_capture fix491-projection-contracts cargo test --locked --test fix491_projection_contracts -- --nocapture
run_capture fix491-postgres-contracts cargo test --locked --test fix491_postgres_recovery_contracts -- --nocapture
run_capture fix491-recovery-testcontainers cargo test --locked --features integration-tests --test fix491_recovery_testcontainers -- --nocapture

run_capture postgres-audit env ASTRAVECTOR_PROFILE="$ASTRAVECTOR_PROFILE" cargo run --locked --bin astravector-runtime -- recovery postgres-audit
run_capture qdrant-compatibility env ASTRAVECTOR_PROFILE="$ASTRAVECTOR_PROFILE" cargo run --locked --bin astravector-runtime -- recovery qdrant-compatibility
run_capture qdrant-audit env ASTRAVECTOR_PROFILE="$ASTRAVECTOR_PROFILE" cargo run --locked --bin astravector-runtime -- recovery qdrant-audit --batch-size "${FIX491_BATCH_SIZE:-500}"

if [[ "${FIX491_RUN_RETRIEVAL_PARITY:-0}" == "1" ]]; then
  run_optional_capture retrieval-before ./scripts/local-demo/search.sh "${FIX491_RETRIEVAL_QUERY:-Где AstraVector хранит каноническое состояние?}"
  run_optional_capture qdrant-rebuild env ASTRAVECTOR_PROFILE="$ASTRAVECTOR_PROFILE" cargo run --locked --bin astravector-runtime -- recovery qdrant-rebuild --batch-size "${FIX491_BATCH_SIZE:-500}"
  run_optional_capture retrieval-after ./scripts/local-demo/search.sh "${FIX491_RETRIEVAL_QUERY:-Где AstraVector хранит каноническое состояние?}"
else
  printf '127\n' >"${OUT_DIR}/retrieval-before.status"
  printf '127\n' >"${OUT_DIR}/qdrant-rebuild.status"
  printf '127\n' >"${OUT_DIR}/retrieval-after.status"
  printf 'retrieval parity not requested; set FIX491_RUN_RETRIEVAL_PARITY=1 with a running local-demo runtime\n' >"${OUT_DIR}/retrieval-before.stderr"
fi

python3 - "$OUT_DIR" "$RUN_ID" <<'PY'
import json
import pathlib
import sys

out = pathlib.Path(sys.argv[1])
run_id = sys.argv[2]

def status(name):
    p = out / f"{name}.status"
    return int(p.read_text().strip()) if p.exists() else 127

def load_stdout_json(name):
    p = out / f"{name}.stdout"
    if not p.exists():
        return None
    text = p.read_text()
    decoder = json.JSONDecoder()
    for index in range(len(text) - 1, -1, -1):
        if text[index] != "{":
            continue
        try:
            value, offset = decoder.raw_decode(text[index:])
        except json.JSONDecodeError:
            continue
        if text[index + offset:].strip():
            continue
        return value
    return None

commands = [
    "cargo-fmt",
    "cargo-check",
    "fix491-projection-contracts",
    "fix491-postgres-contracts",
    "fix491-recovery-testcontainers",
    "postgres-audit",
    "qdrant-compatibility",
    "qdrant-audit",
]
retrieval_commands = ["retrieval-before", "qdrant-rebuild", "retrieval-after"]
command_status = {name: status(name) for name in commands + retrieval_commands}

postgres = load_stdout_json("postgres-audit") or {}
qdrant_compatibility = load_stdout_json("qdrant-compatibility") or {}
qdrant_audit = load_stdout_json("qdrant-audit") or {}

postgres_pass = postgres.get("verdict") == "POSTGRES_CANONICAL_AUDIT_PASS"
qdrant_compatibility_pass = qdrant_compatibility.get("verdict") == "QDRANT_COLLECTION_COMPATIBLE"
qdrant_projection_pass = qdrant_audit.get("verdict") == "QDRANT_PROJECTION_CONSISTENT"
static_and_contracts_pass = all(command_status[name] == 0 for name in commands[:5])
runtime_audits_pass = all(command_status[name] == 0 for name in commands[5:])
retrieval_parity_pass = all(command_status[name] == 0 for name in retrieval_commands)

final_verdict = (
    "FIX491_PERSISTENCE_RECOVERY_PASS"
    if static_and_contracts_pass and runtime_audits_pass and retrieval_parity_pass
    else "FIX491_PERSISTENCE_RECOVERY_BLOCKED"
)

report = {
    "run_id": run_id,
    "final_verdict": final_verdict,
    "static_and_contracts_pass": static_and_contracts_pass,
    "postgres_recovery_pass": postgres_pass,
    "qdrant_collection_compatibility_pass": qdrant_compatibility_pass,
    "qdrant_projection_recovery_pass": qdrant_projection_pass,
    "retrieval_parity_pass": retrieval_parity_pass,
    "command_status": command_status,
    "postgres": postgres,
    "qdrant_compatibility": qdrant_compatibility,
    "qdrant_audit": qdrant_audit,
    "evidence_dir": str(out),
}

(out / "fix491-persistence-recovery-result.json").write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
pathlib.Path("docs/fix491/persistence-recovery-result.json").write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
print(json.dumps(report, indent=2, ensure_ascii=False))
sys.exit(0 if final_verdict == "FIX491_PERSISTENCE_RECOVERY_PASS" else 2)
PY
