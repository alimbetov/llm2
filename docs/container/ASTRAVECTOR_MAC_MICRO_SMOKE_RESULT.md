# AstraVector Mac Micro Smoke Result

Verdict:

```text
ASTRAVECTOR_MAC_MICRO_SMOKE_FAIL
```

The Russian ingestion and retrieval smoke passed on Mac with a verified cached BGE-M3 volume, but the full task cannot be marked PASS because fresh Nexus download from an empty volume did not complete and SIGTERM exited as `137`.

## Execution Plan

1. Pull the exact private image and inspect digest/architecture.
2. Start disposable `pgvector/pgvector:pg16` and `qdrant/qdrant:v1.14.1`.
3. Start AstraVector with an empty model volume and reader Nexus credentials.
4. If a real stopper appears, fix only the scoped image/bootstrap defect and publish a new immutable tag.
5. Verify SHA256 x3, ONNX/runtime startup, gRPC health, Russian ingestion, activation and retrieval evidence.
6. Verify cached restart, bad Nexus credentials and SIGTERM.
7. Record PASS/FAIL truthfully.

## Tested State

| Field | Value |
| --- | --- |
| Date/time | `2026-08-22` Asia/Almaty run; runtime logs use UTC `2026-08-21T19:*Z`. |
| Branch | `agent/astravector-image-contract` |
| Original image | `registry.astrabase.asia/astravector:sha-26288b4` |
| Original digest | `sha256:77174cf14b1856b57f95ff96e96ee8c4c04df83034bd9af5127aaba287a6393a` |
| Final tested image | `registry.astrabase.asia/astravector:sha-1cb6065` |
| Final tested digest | `sha256:b0567810b5ea3df752ff8ba559fcf16bc46b245878e798b8888dcf93426ee6ad` |
| Final image architecture | `linux/arm64` |
| PostgreSQL image | `pgvector/pgvector:pg16` |
| Qdrant image | `qdrant/qdrant:v1.14.1` |
| Model cache volume | `astravector-bge-m3-cache` |
| Runtime dense-only setting | `ASTRAVECTOR_SPARSE_REQUIRED=false` |

## Image Fixes Published

| Commit | Image tag | Digest | Reason |
| --- | --- | --- | --- |
| `7aed252` | `sha-7aed252` | `sha256:060832f8d9763eaf990b4a595db4b6d7cb231c54348f5837747977972ebe6436` | Force model downloads to HTTP/1.1 and retry transport errors. |
| `fa9729f` | `sha-fa9729f` | `sha256:05fefd2f869b0d6cb3d63c9f7ef1dd2d8dd922cc384196206c93c70a85c02a49` | Add curl continue support. |
| `1cb6065` | `sha-1cb6065` | `sha256:b0567810b5ea3df752ff8ba559fcf16bc46b245878e798b8888dcf93426ee6ad` | Use a stable `.part` file for resumable downloads. |

## Gates

| Gate | Result | Evidence |
| --- | --- | --- |
| Reader private pull, original image | PASS | `docker pull registry.astrabase.asia/astravector:sha-26288b4` returned expected digest. |
| Reader private pull, final image | PASS | `docker pull registry.astrabase.asia/astravector:sha-1cb6065` returned `sha256:b0567810b5ea3df752ff8ba559fcf16bc46b245878e798b8888dcf93426ee6ad`. |
| Mac/Docker architecture | PASS | `uname -m=arm64`; image inspect `ARCH=arm64 OS=linux`. |
| Disposable PostgreSQL | PASS | `pg_isready` accepted connections on Docker network alias `postgres`. |
| Disposable Qdrant | PASS | `curl http://qdrant:6333/collections` succeeded on Docker network alias `qdrant`. |
| Fresh Nexus model download | FAIL | Auth succeeded and download started, but `model.onnx_data` repeatedly failed with `curl: (92) HTTP/2 stream ... INTERNAL_ERROR` and then `curl: (18) end of response ... bytes missing`. Range probe returned `200 OK`, not `206 Partial Content`. |
| Verified cached model volume | PASS | SHA256 x3 matched inside Docker volume. |
| Cached bootstrap | PASS | Logs showed `cache valid` for `model.onnx`, `model.onnx_data`, and `tokenizer.json`. |
| PostgreSQL/Qdrant bootstrap reachability | PASS | Logs showed both reachable before application startup. |
| Application startup/migrations | PASS | Runtime started after SQLx migration notices. |
| ONNX/runtime init | PASS | Runtime reached `AstraVector v0.4.0 fix490 starting`, provider `CPU`. |
| gRPC health | PASS | `grpc.health.v1.Health/Check` for `astravector.embedding.v1.AstraVectorRuntime` returned `SERVING`. |
| Russian ingestion | PASS | `IndexLogicalDocument` accepted 2 blocks, created 7 chunks and scheduled 6 dense vectors. |
| Vector publication | PASS | `expectedBindings=6`, `syncedBindings=6`, `outboxCompleted=6`, `qdrantPointsFound=6`. |
| Activation | PASS_AFTER_CONFIG_FIX | With `ASTRAVECTOR_SPARSE_REQUIRED=false`, status became `READY_TO_ACTIVATE` and `ActivateDocumentVersion` returned `ACTIVE`. |
| Russian retrieval evidence | PASS | Search returned matched/parent text containing `AstraVector хранит каноническое состояние документов в PostgreSQL`. |
| Cached restart without re-download | PASS | Second start logged all three `cache valid` lines and repeated search returned the same document/evidence. |
| Bad Nexus credentials | PASS | Fresh bad-auth volume failed closed with `401`/`429` and `[astravector-bootstrap] FAIL: download failed for model.onnx`; no readiness reached. |
| SIGTERM | FAIL | `docker stop --time 45` stopped the runtime, but inspect returned `State=exited ExitCode=137 OOMKilled=false`. |

## Key Commands And Outputs

Final image pull:

```text
sha-1cb6065: Pulling from astravector
Digest: sha256:b0567810b5ea3df752ff8ba559fcf16bc46b245878e798b8888dcf93426ee6ad
Status: Image is up to date for registry.astrabase.asia/astravector:sha-1cb6065
```

Model SHA256 inside Docker volume:

```text
f84251230831afb359ab26d9fd37d5936d4d9bb5d1d5410e66442f630f24435b  /models/model.onnx
1eebfb28493f67bba03ce0ef64bfdc7fc5a3bd9d7493f818bb1d78cd798416b4  /models/model.onnx_data
21106b6d7dab2952c1d496fb21d5dc9db75c28ed361a05f5020bbba27810dd08  /models/tokenizer.json
```

Health:

```text
{
  "status": "SERVING"
}
```

Ingestion summary:

```text
blocksReceived=2
chunksCreated=7
denseVectorsCreated=6
qdrantPointsScheduled=6
```

Retrieval evidence:

```text
AstraVector хранит каноническое состояние документов в PostgreSQL.
```

SIGTERM:

```text
State=exited ExitCode=137 OOMKilled=false
```

Final verdict:

```text
ASTRAVECTOR_MAC_MICRO_SMOKE_FAIL
```
