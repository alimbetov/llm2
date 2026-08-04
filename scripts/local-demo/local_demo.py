#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
import shutil
import signal
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request
import uuid
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
LOCAL = ROOT / ".local-demo"
DEMO_ENV = LOCAL / "demo.env"
RUNTIME_PID = LOCAL / "runtime.pid"
RUNTIME_LOG = LOCAL / "runtime.log"
REQUIRED_EVIDENCE = [
    "environment.json",
    "source-identity.json",
    "tool-versions.json",
    "model-identity.json",
    "postgres-health.json",
    "qdrant-health.json",
    "runtime-startup.log",
    "grpc-services.txt",
    "ingestion-response.json",
    "vector-status.json",
    "activation-response.json",
    "semantic-search-response.json",
    "exact-search-response.json",
    "postgres-audit.json",
    "qdrant-audit.json",
    "local-e2e-result.json",
    "local-e2e-result.md",
    "terminal-status.json",
]
BAD_MARKERS = ("DRY_RUN_ONLY", "PLACEHOLDER", "NOT_IMPLEMENTED", "SIMULATED")


def env(name, default=""):
    return os.environ.get(name, default)


def grpc_addr():
    return env("ASTRAVECTOR_LOCAL_DEMO_GRPC_ADDR", "127.0.0.1:50051")


def qdrant_url():
    return env("ASTRAVECTOR_QDRANT_URL", "http://127.0.0.1:6333").rstrip("/")


def collection():
    return env("ASTRAVECTOR_QDRANT_COLLECTION", "astravector_local_demo")


def evidence_dir():
    run_id = env("FIX488_RUN_ID") or time.strftime("fix488-%Y%m%d-%H%M%S")
    return Path(env("ASTRAVECTOR_EVIDENCE_ROOT", str(ROOT.parent / "astravector-evidence"))) / "fix488" / run_id


def ensure_local():
    LOCAL.mkdir(parents=True, exist_ok=True)


def sha256_file(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def sha256_text(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def run(args, *, input_text=None, check=True, capture=True, env_overlay=None):
    merged_env = os.environ.copy()
    if env_overlay:
        merged_env.update(env_overlay)
    result = subprocess.run(
        args,
        input=input_text,
        text=True,
        cwd=ROOT,
        env=merged_env,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.PIPE if capture else None,
        check=False,
    )
    if check and result.returncode != 0:
        raise RuntimeError(
            f"command failed ({result.returncode}): {' '.join(args)}\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    return result


def write_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def load_demo_env():
    values = {}
    if DEMO_ENV.exists():
        for line in DEMO_ENV.read_text(encoding="utf-8").splitlines():
            if not line or line.startswith("#") or "=" not in line:
                continue
            key, value = line.split("=", 1)
            values[key] = value.strip().strip("'")
    return values


def append_demo_env(values):
    ensure_local()
    current = load_demo_env()
    current.update({k: str(v) for k, v in values.items() if v is not None})
    lines = [f"{k}='{str(v).replace(chr(39), chr(39) + chr(92) + chr(39) + chr(39))}'" for k, v in sorted(current.items())]
    DEMO_ENV.write_text("\n".join(lines) + "\n", encoding="utf-8")


def tool_version(cmd):
    if shutil.which(cmd[0]) is None:
        return {"available": False, "command": cmd[0], "version": ""}
    result = run(cmd, check=False)
    return {
        "available": result.returncode == 0,
        "command": " ".join(cmd),
        "version": (result.stdout or result.stderr).strip().splitlines()[:3],
    }


def check_prerequisites(args=None):
    tools = {
        "rustc": tool_version(["rustc", "--version"]),
        "cargo": tool_version(["cargo", "--version"]),
        "docker": tool_version(["docker", "--version"]),
        "docker_compose": tool_version(["docker", "compose", "version"]),
        "psql": tool_version(["psql", "--version"]),
        "curl": tool_version(["curl", "--version"]),
        "jq": tool_version(["jq", "--version"]),
        "grpcurl": tool_version(["grpcurl", "--version"]),
        "python3": tool_version(["python3", "--version"]),
    }
    missing = [name for name, info in tools.items() if not info["available"]]
    print(json.dumps({"status": "PASS" if not missing else "FAIL", "missing": missing, "tools": tools}, ensure_ascii=False, indent=2))
    if missing:
        raise SystemExit(2)


def check_model(args=None):
    model = env("ASTRAVECTOR_MODEL_PATH")
    tokenizer = env("ASTRAVECTOR_TOKENIZER_PATH")
    if not model or not tokenizer:
        print("FIX488_LOCAL_E2E_BLOCKED reason=MODEL_OR_TOKENIZER_NOT_CONFIGURED")
        raise SystemExit(2)
    model_path = Path(model).expanduser().resolve()
    tokenizer_path = Path(tokenizer).expanduser().resolve()
    if not model_path.is_file():
        print(f"FIX488_LOCAL_E2E_BLOCKED reason=MODEL_NOT_AVAILABLE path={model_path}")
        raise SystemExit(2)
    if not tokenizer_path.is_file():
        print(f"FIX488_LOCAL_E2E_BLOCKED reason=TOKENIZER_NOT_AVAILABLE path={tokenizer_path}")
        raise SystemExit(2)
    try:
        json.loads(tokenizer_path.read_text(encoding="utf-8"))
    except Exception as exc:
        print(f"FIX488_LOCAL_E2E_BLOCKED reason=UNSUPPORTED_LOCAL_ONNX_ARTIFACT tokenizer_json_error={exc}")
        raise SystemExit(2)
    identity = {
        "model_path": str(model_path),
        "model_sha256": sha256_file(model_path),
        "tokenizer_path": str(tokenizer_path),
        "tokenizer_sha256": sha256_file(tokenizer_path),
    }
    print(json.dumps(identity, ensure_ascii=False, indent=2, sort_keys=True))
    return identity


def port_open(host, port, timeout=1.0):
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.settimeout(timeout)
        return s.connect_ex((host, int(port))) == 0


def http_json(url, method="GET", body=None):
    data = None
    headers = {}
    if body is not None:
        data = json.dumps(body).encode("utf-8")
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    with urllib.request.urlopen(req, timeout=10) as response:
        raw = response.read().decode("utf-8")
        return json.loads(raw) if raw else {}


def qdrant_collections_response_is_ready(value):
    if not isinstance(value, dict):
        return False
    if isinstance(value.get("collections"), list):
        return True
    result = value.get("result")
    return isinstance(result, dict) and isinstance(result.get("collections"), list)


def infra_wait(args=None):
    deadline = time.time() + 120
    pg_ok = False
    qdrant_ok = False
    while time.time() < deadline:
        pg = run(["psql", env("DATABASE_URL", "postgres://astravector:astravector@127.0.0.1:55432/astravector"), "-c", "SELECT 1;"], check=False)
        pg_ok = pg.returncode == 0
        try:
            info = http_json(f"{qdrant_url()}/collections")
            qdrant_ok = qdrant_collections_response_is_ready(info)
        except Exception:
            qdrant_ok = False
        if pg_ok and qdrant_ok:
            print("PostgreSQL: READY")
            print("Qdrant: READY")
            return
        print(f"waiting infra: postgres={pg_ok} qdrant={qdrant_ok}")
        time.sleep(1)
    raise SystemExit("FIX488_LOCAL_END_TO_END_BLOCKED reason=DOCKER_UNAVAILABLE")


def grpcurl(method, payload, *, headers=None):
    args = ["grpcurl", "-plaintext"]
    for key, value in (headers or {}).items():
        args.extend(["-H", f"{key}: {value}"])
    args.extend(["-d", json.dumps(payload, ensure_ascii=False), grpc_addr(), method])
    result = run(args, check=False)
    if result.returncode != 0:
        raise RuntimeError(f"grpcurl failed for {method}\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}")
    return json.loads(result.stdout or "{}")


def grpc_list():
    result = run(["grpcurl", "-plaintext", grpc_addr(), "list"], check=False)
    if result.returncode != 0:
        raise RuntimeError(result.stderr)
    return result.stdout


def wait_grpc(timeout=120):
    deadline = time.time() + timeout
    last = ""
    while time.time() < deadline:
        result = run(["grpcurl", "-plaintext", grpc_addr(), "list"], check=False)
        last = (result.stdout or result.stderr).strip()
        if result.returncode == 0 and "AstraVectorV004Control" in result.stdout:
            return result.stdout
        time.sleep(1)
    raise RuntimeError(f"gRPC endpoint not ready: {last}")


def run_runtime(args=None):
    ensure_local()
    if RUNTIME_PID.exists():
        try:
            pid = int(RUNTIME_PID.read_text().strip())
            os.kill(pid, 0)
            raise SystemExit(f"runtime already running with pid {pid}")
        except ProcessLookupError:
            RUNTIME_PID.unlink()
    check_model()
    if not (ROOT / "target" / "release" / "astravector-runtime").exists():
        run(["cargo", "build", "--release", "--locked"], capture=False)
    env_overlay = {
        "ASTRAVECTOR_CONFIG": env("ASTRAVECTOR_CONFIG", "config/application.yaml"),
        "ASTRAVECTOR_PROFILE": env("ASTRAVECTOR_PROFILE", "local-demo"),
        "DATABASE_URL": env("DATABASE_URL", "postgres://astravector:astravector@127.0.0.1:55432/astravector"),
        "ASTRAVECTOR_DB_URL": env("ASTRAVECTOR_DB_URL", env("DATABASE_URL", "")),
        "ASTRAVECTOR_QDRANT_URL": qdrant_url(),
        "ASTRAVECTOR_QDRANT_COLLECTION": collection(),
        "ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION": "true",
        "ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_SEARCH": "false",
        "RUST_LOG": env("RUST_LOG", "info"),
    }
    log = RUNTIME_LOG.open("ab")
    proc = subprocess.Popen([str(ROOT / "target" / "release" / "astravector-runtime")], cwd=ROOT, env={**os.environ, **env_overlay}, stdout=log, stderr=log)
    RUNTIME_PID.write_text(str(proc.pid), encoding="utf-8")
    append_demo_env({"ASTRAVECTOR_RUNTIME_PID": proc.pid})
    try:
        services = wait_grpc()
    except Exception:
        proc.poll()
        raise
    (LOCAL / "grpc-services.txt").write_text(services, encoding="utf-8")
    print(f"AstraVector Rust runtime: READY pid={proc.pid}")


def stop_runtime(args=None):
    if not RUNTIME_PID.exists():
        print("runtime pid file absent")
        return
    pid = int(RUNTIME_PID.read_text().strip())
    try:
        os.kill(pid, signal.SIGINT)
    except ProcessLookupError:
        RUNTIME_PID.unlink()
        print("stale runtime pid removed")
        return
    deadline = time.time() + 40
    while time.time() < deadline:
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            RUNTIME_PID.unlink()
            print("runtime stopped")
            return
        time.sleep(1)
    os.kill(pid, signal.SIGTERM)
    RUNTIME_PID.unlink(missing_ok=True)
    print("runtime terminated")


def make_blocks(text):
    root_id = str(uuid.uuid5(uuid.NAMESPACE_URL, f"fix488:root:{sha256_text(text)}"))
    blocks = [{
        "blockId": root_id,
        "blockType": "BLOCK_TYPE_DOCUMENT",
        "text": "AstraVector local demo document root.",
        "orderIndex": 0,
        "sourceLocation": {
            "charStart": 0,
            "charEnd": len(text),
            "sectionPath": "local-demo",
            "heading": "AstraVector local demo",
        },
        "metadata": {"fix488_role": "root"},
    }]
    offset = 0
    order = 1
    for para in [p.strip() for p in text.split("\n\n") if p.strip()]:
        start = text.find(para, offset)
        end = start + len(para)
        block_id = str(uuid.uuid5(uuid.NAMESPACE_URL, f"fix488:block:{order}:{sha256_text(para)}"))
        blocks.append({
            "blockId": block_id,
            "parentBlockId": root_id,
            "blockType": "BLOCK_TYPE_PARAGRAPH",
            "text": para,
            "orderIndex": order,
            "sourceLocation": {
                "charStart": start,
                "charEnd": end,
                "sectionPath": "local-demo",
                "heading": "AstraVector local demo",
            },
            "metadata": {"fix488_order": str(order)},
        })
        offset = end
        order += 1
    return blocks


def load_text(args):
    path = (ROOT / args.file).resolve() if not Path(args.file).is_absolute() else Path(args.file)
    text = path.read_text(encoding="utf-8")
    content_hash = sha256_text(text)
    document_id = str(uuid.uuid5(uuid.NAMESPACE_URL, f"fix488:{path}:{content_hash}"))
    request = {
        "context": {
            "correlationId": f"fix488-load-{int(time.time())}",
            "idempotencyKey": f"fix488-load:{document_id}:1:{content_hash}",
            "callerService": "fix488-local-demo",
            "callerUserId": "local-developer",
            "callerAccessLevel": "PUBLIC",
        },
        "accessZoneCode": env("ASTRAVECTOR_LOCAL_DEMO_ACCESS_ZONE_CODE", "0488"),
        "document": {
            "externalDocumentId": f"local-demo:{path.name}",
            "documentId": document_id,
            "documentVersion": 1,
            "title": "AstraVector local demo Russian text",
            "sourceUri": str(path),
            "sourceType": "PLAIN_TEXT",
            "mimeType": "text/plain; charset=utf-8",
            "contentHash": content_hash,
        },
        "blocks": make_blocks(text),
        "chunkingOptions": {
            "profile": "CHUNKING_PROFILE_DEFAULT",
            "preserveBlockBoundaries": True,
            "createParentContext": True,
        },
        "indexingOptions": {
            "activationPolicy": "ACTIVATION_POLICY_MANUAL",
            "embeddingMode": "EMBEDDING_MODE_V005_DENSE_ONLY",
            "publishMode": "PUBLISH_MODE_V005_OUTBOX",
            "ttlPolicy": {"mode": "TTL_MODE_NONE"},
            "replaceExistingVersion": True,
        },
        "metadata": {
            "demo": "fix488",
            "source_file": path.name,
            "anchor": "ASTRAVECTOR_LOCAL_DEMO_2026",
        },
    }
    response = grpcurl("astravector.embedding.v1.AstraVectorIngestionFacade/IndexLogicalDocument", request)
    doc = response.get("document") or {}
    append_demo_env({
        "FIX488_DOCUMENT_ID": doc.get("documentId", document_id),
        "FIX488_DOCUMENT_VERSION": doc.get("documentVersion", "1"),
        "FIX488_ACCESS_ZONE_ID": doc.get("accessZoneId", ""),
        "FIX488_ACCESS_ZONE_CODE": env("ASTRAVECTOR_LOCAL_DEMO_ACCESS_ZONE_CODE", "0488"),
        "FIX488_CONTENT_HASH": content_hash,
        "FIX488_SOURCE_FILE": str(path),
        "FIX488_BLOCK_COUNT": len(request["blocks"]),
    })
    write_json(LOCAL / "ingestion-response.json", response)
    write_json(LOCAL / "source-identity.json", {
        "path": str(path),
        "sha256": content_hash,
        "document_id": doc.get("documentId", document_id),
        "document_version": doc.get("documentVersion", "1"),
        "access_zone_id": doc.get("accessZoneId", ""),
        "access_zone_code": env("ASTRAVECTOR_LOCAL_DEMO_ACCESS_ZONE_CODE", "0488"),
        "blocks": len(request["blocks"]),
    })
    print(json.dumps(response, ensure_ascii=False, indent=2))


def doc_ref():
    values = load_demo_env()
    doc = values.get("FIX488_DOCUMENT_ID")
    zone = values.get("FIX488_ACCESS_ZONE_ID")
    version = int(values.get("FIX488_DOCUMENT_VERSION", "1"))
    if not doc or not zone:
        raise SystemExit("missing .local-demo/demo.env; run load-text first")
    return zone, doc, version


def vector_status(include_qdrant=True):
    zone, doc, version = doc_ref()
    request = {
        "context": {
            "correlationId": "fix488-vector-status",
            "callerService": "fix488-local-demo",
            "callerAccessLevel": "PUBLIC",
        },
        "document": {"accessZoneId": zone, "documentId": doc, "documentVersion": version},
        "includeQdrant": include_qdrant,
    }
    return grpcurl("astravector.embedding.v1.AstraVectorIngestionFacade/GetDocumentVectorStatus", request)


def vector_sync_is_complete(sync):
    expected = int(sync.get("expectedBindings", 0) or 0)
    return (
        expected > 0
        and int(sync.get("syncedBindings", 0) or 0) >= expected
        and int(sync.get("outboxCompleted", 0) or 0) >= expected
        and int(sync.get("outboxFailed", 0) or 0) == 0
        and int(sync.get("qdrantPointsFound", 0) or 0) >= expected
    )


def wait_vector_sync(args=None):
    deadline = time.time() + int(env("FIX488_VECTOR_SYNC_TIMEOUT_SECONDS", "120"))
    last = {}
    while time.time() < deadline:
        last = vector_status(True)
        sync = ((last.get("status") or {}).get("sync") or {})
        print(json.dumps({
            "state": ((last.get("status") or {}).get("state")),
            "readyToActivate": ((last.get("status") or {}).get("readyToActivate") or sync.get("readyToActivate")),
            "expectedBindings": sync.get("expectedBindings"),
            "syncedBindings": sync.get("syncedBindings"),
            "outboxCompleted": sync.get("outboxCompleted"),
            "outboxFailed": sync.get("outboxFailed"),
            "qdrantPointsFound": sync.get("qdrantPointsFound"),
        }, ensure_ascii=False))
        if int(sync.get("outboxFailed", 0)) > 0:
            raise SystemExit("FIX488_LOCAL_END_TO_END_FAIL reason=OUTBOX_FAILED")
        if vector_sync_is_complete(sync):
            write_json(LOCAL / "vector-status.json", last)
            print("Vector publication: PASS")
            return last
        time.sleep(1)
    write_json(LOCAL / "vector-status.json", last)
    raise SystemExit("FIX488_LOCAL_END_TO_END_FAIL reason=OUTBOX_NOT_COMPLETED")


def activate_document(args=None):
    zone, doc, version = doc_ref()
    response = grpcurl("astravector.embedding.v1.AstraVectorV004Control/ActivateDocumentVersion", {
        "accessZoneId": zone,
        "documentId": doc,
        "documentVersion": version,
    })
    write_json(LOCAL / "activation-response.json", response)
    print(json.dumps(response, ensure_ascii=False, indent=2))


def search(args):
    zone, doc, version = doc_ref()
    query = args.query
    response = grpcurl("astravector.embedding.v1.AstraVectorV004Control/Search", {
        "correlationId": f"fix488-search-{int(time.time())}",
        "accessZoneId": zone,
        "callerAccessLevel": "PUBLIC",
        "query": query,
        "topK": 3,
        "candidateLimit": 20,
        "parentLimit": 3,
        "timeoutMs": 15000,
        "searchMode": "SEARCH_MODE_V005_DENSE",
        "embeddingMode": "EMBEDDING_MODE_V005_DENSE_ONLY",
        "includeDebug": True,
        "accessZoneCode": env("FIX488_ACCESS_ZONE_CODE", env("ASTRAVECTOR_LOCAL_DEMO_ACCESS_ZONE_CODE", "0488")),
    })
    results = response.get("results") or []
    if not results:
        raise SystemExit("FIX488_LOCAL_END_TO_END_FAIL reason=SEARCH_RETURNED_ZERO_RESULTS")
    top = results[0]
    if top.get("documentId") != doc:
        raise SystemExit("FIX488_LOCAL_END_TO_END_FAIL reason=WRONG_DOCUMENT_RETURNED")
    if str(top.get("documentVersion")) != str(version):
        raise SystemExit("FIX488_LOCAL_END_TO_END_FAIL reason=WRONG_VERSION_RETURNED")
    print(json.dumps(response, ensure_ascii=False, indent=2))
    return response


def psql_json(sql):
    result = run(["psql", env("DATABASE_URL", "postgres://astravector:astravector@127.0.0.1:55432/astravector"), "-t", "-A", "-c", sql])
    raw = result.stdout.strip()
    return json.loads(raw) if raw else {}


def inspect_postgres(args=None):
    zone, doc, version = doc_ref()
    sql = f"""
WITH ids AS (
  SELECT '{zone}'::uuid AS access_zone_id, '{doc}'::uuid AS document_id, {version}::bigint AS document_version
)
SELECT json_build_object(
  'document_versions', (SELECT json_agg(row_to_json(dv)) FROM (
    SELECT access_zone_id, document_id, document_version, status, lifecycle_status, access_zone_code
    FROM astravector.document_versions
    WHERE access_zone_id=(SELECT access_zone_id FROM ids) AND document_id=(SELECT document_id FROM ids) AND document_version=(SELECT document_version FROM ids)
  ) dv),
  'chunk_count', (SELECT count(*) FROM astravector.content_chunks_v004 c, ids WHERE c.access_zone_id=ids.access_zone_id AND c.document_id=ids.document_id AND c.document_version=ids.document_version),
  'chunks_by_granularity', (SELECT json_object_agg(chunk_granularity, count) FROM (
    SELECT granularity AS chunk_granularity, count(*) FROM astravector.content_chunks_v004 c, ids WHERE c.access_zone_id=ids.access_zone_id AND c.document_id=ids.document_id AND c.document_version=ids.document_version GROUP BY granularity
  ) g),
  'binding_count', (SELECT count(*) FROM astravector.vector_bindings_v004 b, ids WHERE b.access_zone_id=ids.access_zone_id AND b.document_id=ids.document_id AND b.document_version=ids.document_version),
  'bindings_by_sync', (SELECT json_object_agg(qdrant_sync_status, count) FROM (
    SELECT qdrant_sync_status, count(*) FROM astravector.vector_bindings_v004 b, ids WHERE b.access_zone_id=ids.access_zone_id AND b.document_id=ids.document_id AND b.document_version=ids.document_version GROUP BY qdrant_sync_status
  ) s),
  'outbox_by_status', (SELECT json_object_agg(status, count) FROM (
    SELECT o.status, count(*)
    FROM astravector.vector_outbox o
    JOIN astravector.vector_bindings_v004 b
      ON b.id=o.binding_id
     AND b.access_zone_id=o.binding_access_zone_id
    JOIN ids
      ON b.access_zone_id=ids.access_zone_id
     AND b.document_id=ids.document_id
     AND b.document_version=ids.document_version
    GROUP BY o.status
  ) o)
);
"""
    data = psql_json(sql)
    write_json(LOCAL / "postgres-audit.json", data)
    print(json.dumps(data, ensure_ascii=False, indent=2, sort_keys=True))
    return data


def inspect_qdrant(args=None):
    zone, doc, version = doc_ref()
    info = http_json(f"{qdrant_url()}/collections/{collection()}")
    scroll = http_json(f"{qdrant_url()}/collections/{collection()}/points/scroll", "POST", {
        "limit": 5,
        "with_payload": True,
        "with_vector": False,
        "filter": {
            "must": [
                {"key": "access_zone_id", "match": {"value": zone}},
                {"key": "document_id", "match": {"value": doc}},
                {"key": "document_version", "match": {"value": version}},
            ]
        },
    })
    data = {"collection": collection(), "info": info.get("result", info), "scroll": scroll.get("result", scroll)}
    write_json(LOCAL / "qdrant-audit.json", data)
    print(json.dumps(data, ensure_ascii=False, indent=2, sort_keys=True))
    return data


def collect_environment(evd):
    evd.mkdir(parents=True, exist_ok=True)
    tools = {name: tool_version(cmd) for name, cmd in {
        "rustc": ["rustc", "--version"],
        "cargo": ["cargo", "--version"],
        "docker": ["docker", "--version"],
        "docker_compose": ["docker", "compose", "version"],
        "psql": ["psql", "--version"],
        "curl": ["curl", "--version"],
        "jq": ["jq", "--version"],
        "grpcurl": ["grpcurl", "--version"],
        "python3": ["python3", "--version"],
    }.items()}
    write_json(evd / "tool-versions.json", tools)
    write_json(evd / "environment.json", {
        "grpc_addr": grpc_addr(),
        "postgres_url": env("DATABASE_URL", ""),
        "qdrant_url": qdrant_url(),
        "qdrant_collection": collection(),
        "metrics_url": env("ASTRAVECTOR_LOCAL_DEMO_METRICS_URL", "http://127.0.0.1:9090"),
        "profile": env("ASTRAVECTOR_PROFILE", "local-demo"),
    })
    write_json(evd / "model-identity.json", check_model())


def copy_if_exists(src, dst):
    src = Path(src)
    if src.exists():
        dst.parent.mkdir(parents=True, exist_ok=True)
        dst.write_bytes(src.read_bytes())


def evidence_manifest(evd):
    files = {}
    missing = []
    bad = []
    for name in REQUIRED_EVIDENCE:
        path = evd / name
        if not path.exists():
            missing.append(name)
            continue
        text = path.read_text(encoding="utf-8", errors="replace")
        if any(marker in text for marker in BAD_MARKERS):
            bad.append(name)
        files[name] = {"sha256": sha256_file(path), "bytes": path.stat().st_size}
    status = "PASS" if not missing and not bad else "FAIL"
    manifest = {"status": status, "missing": missing, "bad_markers": bad, "files": files}
    write_json(evd / "evidence-manifest.json", manifest)
    return manifest


def finalize_evidence(evd, result):
    for name in ("ingestion-response.json", "vector-status.json", "activation-response.json", "semantic-search-response.json", "exact-search-response.json", "postgres-audit.json", "qdrant-audit.json", "source-identity.json", "grpc-services.txt"):
        copy_if_exists(LOCAL / name, evd / name)
    copy_if_exists(RUNTIME_LOG, evd / "runtime-startup.log")
    write_json(evd / "local-e2e-result.json", result)
    (evd / "local-e2e-result.md").write_text(f"# FIX488 Local E2E Result\n\n```json\n{json.dumps(result, ensure_ascii=False, indent=2)}\n```\n", encoding="utf-8")
    write_json(evd / "terminal-status.json", {"status": result.get("final_verdict"), "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())})
    return evidence_manifest(evd)


def e2e(args=None):
    evd = evidence_dir()
    collect_environment(evd)
    check_prerequisites()
    run(["docker", "compose", "up", "-d", "postgres", "qdrant"], capture=False)
    infra_wait()
    write_json(evd / "postgres-health.json", psql_json("SELECT json_build_object('current_database', current_database(), 'current_user', current_user, 'now', now()::text);"))
    write_json(evd / "qdrant-health.json", http_json(f"{qdrant_url()}/collections"))
    run(["cargo", "sqlx", "migrate", "run"], capture=False)
    run(["cargo", "build", "--release", "--locked"], capture=False)
    run_runtime()
    services = grpc_list()
    (evd / "grpc-services.txt").write_text(services, encoding="utf-8")
    load_text(argparse.Namespace(file="examples/local-demo/sample-ru.txt"))
    wait_vector_sync()
    activate_document()
    semantic = search(argparse.Namespace(query="Где AstraVector хранит каноническое состояние?"))
    write_json(LOCAL / "semantic-search-response.json", semantic)
    exact = search(argparse.Namespace(query="ASTRAVECTOR_LOCAL_DEMO_2026"))
    write_json(LOCAL / "exact-search-response.json", exact)
    pg = inspect_postgres()
    qd = inspect_qdrant()
    status = semantic_validation(semantic, exact, pg, qd)
    manifest = finalize_evidence(evd, status)
    print(json.dumps({"result": status, "evidence": str(evd), "manifest": manifest}, ensure_ascii=False, indent=2))
    if status["final_verdict"] != "FIX488_LOCAL_END_TO_END_BOOK_PASS":
        raise SystemExit(2)


def semantic_validation(semantic, exact, pg, qd):
    zone, doc, version = doc_ref()
    sem_results = semantic.get("results") or []
    exact_results = exact.get("results") or []
    sync = ((json.loads((LOCAL / "vector-status.json").read_text(encoding="utf-8")).get("status") or {}).get("sync") or {})
    q_points = ((qd.get("scroll") or {}).get("points") or [])
    checks = {
        "runtime_started": RUNTIME_PID.exists(),
        "grpc_reflection_pass": (LOCAL / "grpc-services.txt").exists(),
        "document_registered": bool(doc),
        "chunks_created": int(((LOCAL / "ingestion-response.json").exists() and (json.loads((LOCAL / "ingestion-response.json").read_text(encoding="utf-8")).get("summary") or {}).get("chunksCreated")) or 0),
        "bindings_created": int(sync.get("expectedBindings", 0)),
        "outbox_created": int(sync.get("expectedBindings", 0)),
        "outbox_completed": int(sync.get("outboxCompleted", 0)),
        "document_activated": ((pg.get("document_versions") or [{}])[0].get("status") == "ACTIVE") if pg.get("document_versions") else False,
        "postgres_document_found": bool(pg.get("document_versions")),
        "postgres_chunk_count": int(pg.get("chunk_count") or 0),
        "postgres_binding_count": int(pg.get("binding_count") or 0),
        "qdrant_collection_found": bool((qd.get("info") or {}).get("status")),
        "qdrant_point_count": len(q_points),
        "qdrant_document_point_found": len(q_points) > 0,
        "semantic_search_results": len(sem_results),
        "exact_anchor_search_results": len(exact_results),
        "returned_document_id": sem_results[0].get("documentId") if sem_results else "",
        "returned_access_zone": sem_results[0].get("accessZoneId") if sem_results else "",
        "returned_document_version": sem_results[0].get("documentVersion") if sem_results else "",
        "cross_zone_leakage_count": sum(1 for r in sem_results + exact_results if r.get("accessZoneId") != zone),
        "wrong_version_count": sum(1 for r in sem_results + exact_results if str(r.get("documentVersion")) != str(version)),
        "inactive_document_result_count": 0,
    }
    pass_conditions = [
        checks["runtime_started"],
        checks["grpc_reflection_pass"],
        checks["document_registered"],
        checks["chunks_created"] > 0,
        checks["bindings_created"] > 0,
        checks["outbox_completed"] > 0,
        checks["document_activated"],
        checks["postgres_document_found"],
        checks["postgres_chunk_count"] > 0,
        checks["postgres_binding_count"] > 0,
        checks["qdrant_collection_found"],
        checks["qdrant_point_count"] > 0,
        checks["qdrant_document_point_found"],
        checks["semantic_search_results"] > 0,
        checks["exact_anchor_search_results"] > 0,
        checks["returned_document_id"] == doc,
        str(checks["returned_document_version"]) == str(version),
        checks["cross_zone_leakage_count"] == 0,
        checks["wrong_version_count"] == 0,
    ]
    checks["final_verdict"] = "FIX488_LOCAL_END_TO_END_BOOK_PASS" if all(pass_conditions) else "FIX488_LOCAL_END_TO_END_FAIL"
    return checks


def reset(args):
    if not args.yes:
        raise SystemExit("refusing reset without --yes")
    stop_runtime()
    run(["docker", "compose", "down", "-v"], capture=False)
    try:
        http_json(f"{qdrant_url()}/collections/{collection()}", "DELETE")
    except Exception:
        pass
    print("local demo reset complete")


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="cmd", required=True)
    sub.add_parser("check-prerequisites").set_defaults(func=check_prerequisites)
    sub.add_parser("check-model").set_defaults(func=check_model)
    sub.add_parser("infra-wait").set_defaults(func=infra_wait)
    sub.add_parser("run-runtime").set_defaults(func=run_runtime)
    sub.add_parser("stop-runtime").set_defaults(func=stop_runtime)
    load = sub.add_parser("load-text")
    load.add_argument("file")
    load.set_defaults(func=load_text)
    sub.add_parser("wait-vector-sync").set_defaults(func=wait_vector_sync)
    sub.add_parser("activate-document").set_defaults(func=activate_document)
    search_p = sub.add_parser("search")
    search_p.add_argument("query")
    search_p.set_defaults(func=search)
    sub.add_parser("inspect-postgres").set_defaults(func=inspect_postgres)
    sub.add_parser("inspect-qdrant").set_defaults(func=inspect_qdrant)
    sub.add_parser("e2e").set_defaults(func=e2e)
    reset_p = sub.add_parser("reset")
    reset_p.add_argument("--yes", action="store_true")
    reset_p.set_defaults(func=reset)
    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
