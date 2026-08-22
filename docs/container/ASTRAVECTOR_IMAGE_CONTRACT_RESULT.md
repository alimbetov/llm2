# AstraVector Image Contract Result

Git branch: `agent/astravector-image-contract`

Git SHA: `26288b4`

Pushed images:

```text
registry.astrabase.asia/astravector:0.4.1-image-contract
registry.astrabase.asia/astravector:sha-26288b4
digest: sha256:77174cf14b1856b57f95ff96e96ee8c4c04df83034bd9af5127aaba287a6393a
```

## Implementation Evidence

| Gate | Result | Evidence |
| --- | --- | --- |
| Dockerfile remains locked release multi-stage build | PASS | `cargo build --locked --release --bins` preserved. |
| Large model bundle excluded from final image | PASS | Docker build context is `astravector/`; final stage copies binaries, config, migrations and scripts only. |
| Runtime model path compatibility | PASS | `src/inference/mod.rs` loads `cfg.model.path`; config now points to `/models/bge-m3/model.onnx`; `model.onnx_data` is verified next to it for ONNX external data. |
| Runtime checksum contract | PASS | Bootstrap verifies `model.onnx`, `model.onnx_data`, `tokenizer.json`; Rust also verifies model/tokenizer checksum before startup. |
| Nexus credentials runtime-only | PASS | Only `ASTRAVECTOR_NEXUS_USERNAME` and `ASTRAVECTOR_NEXUS_PASSWORD` are referenced; no committed secret values. |
| PostgreSQL externalized | PASS | Bootstrap requires `ASTRAVECTOR_DB_URL`; shell performs TCP reachability only. |
| Qdrant externalized | PASS | Bootstrap requires `ASTRAVECTOR_QDRANT_URL`; collection compatibility remains application/FIX491 logic. |
| Shared model cache concurrency | PASS | Bootstrap uses an atomic `mkdir` lock in the writable model cache. |
| SIGTERM/SIGINT propagation | PASS | Entrypoint ends with `exec "$@"`. |
| Non-root runtime | PASS | Runtime user/group `10001` added; Kubernetes security context matches. |
| Kubernetes minimal production example | PASS | Updated `k8s/deployment.yaml`, `k8s/configmap.yaml`, `k8s/secret.example.yaml`, added `k8s/model-pvc.yaml`. |
| Documentation | PASS | `docs/container/ASTRAVECTOR_IMAGE_CONTRACT.md` created. |
| Image build | PASS | `docker build --pull=false -t astravector:0.4.1-image-contract .` completed. |
| Image push | PASS | Both immutable tags pushed to `registry.astrabase.asia/astravector`; digest recorded above. |
| Final image model exclusion | PASS | `docker run ... find / -name model.onnx -o -name model.onnx_data -o -name tokenizer.json -o -name pytorch_model.bin` found no model artifacts in the image. |
| ONNX Runtime packaging | PASS | Builder contains static `libonnxruntime.a`; final `ldd /usr/local/bin/astravector-runtime` resolves all dynamic dependencies inside the image and has no missing `libonnxruntime.so` dependency. |
| Bootstrap fail-closed env validation | PASS | Running `astravector-model-bootstrap` without mandatory runtime env failed with `required environment variable is missing: ASTRAVECTOR_DB_URL`. |

## Local Commands Executed

```text
bash -n docker/model-bootstrap.sh
bash -n docker/entrypoint.sh
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo test --locked --features integration-tests --test e2e_testcontainers test_e2e_retrieve_context_full_rag_lifecycle_over_tonic_network -- --nocapture
make verify-fix491-persistence-recovery
docker build --pull=false -t astravector:0.4.1-image-contract .
docker tag astravector:0.4.1-image-contract registry.astrabase.asia/astravector:0.4.1-image-contract
docker tag astravector:0.4.1-image-contract registry.astrabase.asia/astravector:sha-26288b4
docker push registry.astrabase.asia/astravector:0.4.1-image-contract
docker push registry.astrabase.asia/astravector:sha-26288b4
```

Static gates completed with exit code `0`.

`cargo test --locked --all-targets --all-features` completed unit/bin/checksum/dense tests but the full run failed once in `tests/e2e_testcontainers.rs::test_e2e_retrieve_context_full_rag_lifecycle_over_tonic_network` because the expected `GRAPH_EXPANDED` provenance was absent. The same test was rerun directly and passed. No retrieval semantics were changed in this image-contract branch.

`make verify-fix491-persistence-recovery` produced `docs/fix491/evidence/fix491-20260821-222437` and `FIX491_PERSISTENCE_RECOVERY_BLOCKED`: static/contracts, PostgreSQL canonical audit, Qdrant compatibility and Qdrant audit passed; retrieval-before, qdrant-rebuild and retrieval-after returned `127`.

## Gates Not Fully Executed In This Environment

| Gate | Result | Reason |
| --- | --- | --- |
| Authenticated Nexus download | BLOCKED | Reader credentials were supplied after image push, but live download was not executed because the available command interface would expose secret values in shell input/arguments. The image contract is ready for operator execution with runtime env vars. |
| Full runtime model initialization in final image | BLOCKED | Requires live Nexus model download or preloaded verified model bundle plus external PostgreSQL/Qdrant endpoints. |
| Invalid Nexus credentials / Nexus unavailable live tests | BLOCKED | Requires controlled external Nexus credential/network scenarios. |
| PostgreSQL + Qdrant live startup | BLOCKED | External endpoints were not provided for this image-contract run. |
| Kubernetes dry-run validation | BLOCKED | `kubectl --dry-run=client --validate=false apply ...` attempted API discovery at `localhost:8080` and failed because no Kubernetes API was available in this environment. |
| Full `cargo test --locked --all-targets --all-features` | FLAKY_FAIL_THEN_TARGETED_PASS | One testcontainers retrieval provenance assertion failed in the full suite, then passed when rerun directly. |
| FIX491 canonical proof | BLOCKED | `FIX491_PERSISTENCE_RECOVERY_BLOCKED`; PostgreSQL/Qdrant canonical gates passed, retrieval parity/rebuild command stages returned `127`. |

## Verdict

ASTRAVECTOR_IMAGE_CONTRACT_BLOCKED
