# AstraVector Runtime Proof Result

Verdict:

```text
ASTRAVECTOR_RUNTIME_PROOF_BLOCKED
```

PASS is not claimed because authenticated Nexus model download and end-to-end runtime readiness were not executed in this Codex environment without exposing the reader secret.

## Tested State

| Field | Value |
| --- | --- |
| Branch | `agent/astravector-image-contract` |
| Tested Git SHA | `5aad9c03b93787776530ea414078f774fa87548c` before final result-doc amend; final branch HEAD is verified after push. |
| Tested pre-commit content | includes upstream task commit `bf4b3f911d4ccd144702df2eb3b98c353001fa48` plus scoped k8s/test alignment fixes |
| Tested image ref | `registry.astrabase.asia/astravector:sha-26288b4` |
| Observed local RepoDigest | `registry.astrabase.asia/astravector@sha256:77174cf14b1856b57f95ff96e96ee8c4c04df83034bd9af5127aaba287a6393a` |
| Host architecture | `uname -m = arm64` |
| Docker server architecture | `arm64` |
| Local image architecture | `linux/arm64` |
| PostgreSQL proof image | Runbook uses `pgvector/pgvector:pg16`; live runtime proof BLOCKED |
| Qdrant proof image | Runbook uses repository-pinned `qdrant/qdrant:v1.14.1`; live runtime proof BLOCKED |

Remote `docker buildx imagetools inspect registry.astrabase.asia/astravector:sha-26288b4` was attempted and failed in this environment with DNS resolution failure for `registry.astrabase.asia`.

## Audit Findings

| Gate | Result | Evidence |
| --- | --- | --- |
| Runtime model file names | PASS | `config/application.yaml` points to `/models/bge-m3/model.onnx`; bootstrap separately verifies adjacent `model.onnx_data`. |
| ONNX external data behavior | PASS_BY_INSPECTION | `src/inference/mod.rs` calls ONNX Runtime `commit_from_file(&cfg.model.path)`, so external data resolution is delegated to ONNX Runtime next to `model.onnx`; no alternate model path is introduced. |
| Rust checksum behavior | PASS | `src/main.rs` calls `checksum::verify` for model/tokenizer before inference initialization; bootstrap verifies model, external data and tokenizer before runtime starts. |
| gRPC health service name | PASS | Health reporter registers `AstraVectorRuntimeServer`; Kubernetes probes use `astravector.embedding.v1.AstraVectorRuntime`, matching proto package and service. |
| PostgreSQL startup semantics | PASS_BY_INSPECTION | Bootstrap only checks bounded TCP reachability; migrations and repository startup remain application-owned. |
| Qdrant startup/compatibility semantics | PASS_BY_INSPECTION | Bootstrap only checks bounded TCP reachability; Qdrant collection compatibility remains application/FIX491 logic. |
| FIX491 invariant | PASS_BY_INSPECTION | No changes to retrieval, persistence, vector_outbox, projection, recovery or Qdrant rebuild semantics. |
| Private registry pull secret | PASS | Added `imagePullSecrets: astravector-registry-pull` to runtime, migration, lifecycle and publisher manifests. |
| K8s image alignment | PASS | Runtime, migration, lifecycle and publisher manifests now use `registry.astrabase.asia/astravector:0.4.1-image-contract`; contract tests updated. |
| PVC semantics | PASS | `k8s/model-pvc.yaml` uses `ReadWriteMany`; runbook documents RWX requirement for `replicas: 2`. |
| YAML parse | PASS | Ruby YAML parsed configmap, secret example, PVC, deployment, migration job, lifecycle cronjob, publisher deployment and service. |
| Image contains no model artifacts | PASS | `docker run ... find / -name model.onnx -o -name model.onnx_data -o -name tokenizer.json -o -name pytorch_model.bin` found no baked model artifacts. |
| Runtime user | PASS | `docker run ... id` returned `uid=10001(astravector) gid=10001(astravector)`. |
| Dynamic libraries | PASS | `ldd /usr/local/bin/astravector-runtime` showed no unresolved dependencies. |
| Image history secret scan | PASS | `docker history --no-trunc` showed no reader/publisher credentials; only non-secret model URLs and checksums are in ENV. |
| Secret grep | PASS | Search over `.github`, Dockerfile, docker scripts, env example, k8s, container docs and modified tests found no committed password. |

## Static Gates

| Command | Result |
| --- | --- |
| `bash -n docker/model-bootstrap.sh` | PASS |
| `bash -n docker/entrypoint.sh` | PASS |
| `cargo fmt --all --check` | PASS |
| `cargo check --locked --all-targets --all-features` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked --test fix463_stabilization_contracts` | PASS |
| `cargo test --locked --test fix465_p2_hardening_contracts` | PASS |
| `cargo test --locked --all-targets --all-features` | PASS |

The earlier image-contract run observed one flaky E2E provenance failure. In this runtime-proof run, the full suite passed, including `tests/e2e_testcontainers.rs`.

## FIX491 Status

`make verify-fix491-persistence-recovery` produced:

```text
run_id: fix491-20260821-225320
final_verdict: FIX491_PERSISTENCE_RECOVERY_BLOCKED
```

Passed stages:

```text
cargo-fmt
cargo-check
fix491-projection-contracts
fix491-postgres-contracts
fix491-recovery-testcontainers
postgres-audit
qdrant-compatibility
qdrant-audit
```

Blocked stages:

```text
retrieval-before = 127
qdrant-rebuild = 127
retrieval-after = 127
```

`retrieval-before.stderr` states:

```text
retrieval parity not requested; set FIX491_RUN_RETRIEVAL_PARITY=1 with a running local-demo runtime
```

## Live Runtime Gates

| Gate | Result | Reason |
| --- | --- | --- |
| Reader can pull image from private registry | BLOCKED | Remote registry DNS failed in Codex environment; current local cache has the image and digest, but this is not a fresh private pull proof. |
| Exact image digest recorded | PARTIAL | Local RepoDigest recorded as `sha256:77174cf14b1856b57f95ff96e96ee8c4c04df83034bd9af5127aaba287a6393a`; fresh remote pull blocked by DNS. |
| First start downloads model from Nexus | BLOCKED | Requires reader password injection; available tool channel would expose secret input in transcript. |
| Model SHA-256 x3 | BLOCKED | Requires successful Nexus download into Docker volume. |
| ONNX initialization | BLOCKED | Requires downloaded model and live runtime start. |
| PostgreSQL migrations/startup | BLOCKED | Requires live runtime start against disposable `pgvector/pgvector:pg16`. |
| Qdrant compatibility/startup | BLOCKED | Requires live runtime start against disposable Qdrant. |
| gRPC health/readiness | BLOCKED | Requires live runtime start; runbook uses `grpc.health.v1.Health/Check` for `astravector.embedding.v1.AstraVectorRuntime`. |
| Cached restart without re-download | BLOCKED | Requires first successful runtime start and persistent model volume. |
| Corruption recovery | BLOCKED | Requires populated model volume. |
| Invalid Nexus credentials | BLOCKED | Requires safe live execution without leaking secrets and bounded negative scenario. |
| Nexus unavailable | BLOCKED | Requires live execution with fresh empty model volume. |
| PostgreSQL unavailable | BLOCKED | Requires valid cached model volume. |
| Qdrant unavailable | BLOCKED | Requires valid cached model volume and PostgreSQL. |
| SIGTERM | BLOCKED | Requires healthy running container. |

## Scope-Limited Fixes Applied

1. Added `imagePullSecrets` to Kubernetes workloads that use the private registry image.
2. Aligned runtime, migration, lifecycle and publisher Kubernetes image references to `registry.astrabase.asia/astravector:0.4.1-image-contract`.
3. Updated CI image tag and version-alignment contract tests from the obsolete fix465 tag to the image-contract tag.

No retrieval, persistence, model loading, migration, Qdrant projection or FIX491 semantics were changed.
