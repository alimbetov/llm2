# FIX486B Runtime Baseline Result

## Identity

| Field | Value |
|---|---|
| Source branch | `codex/fix486b-reproducible-runtime-baseline` |
| Baseline/final SHA | `9e5250becad48583960c888f37c09ad32a6597ad` |
| origin/main and epic SHA | `e590cee8a7783b93084fb76c8eabc01e40d226bf` |
| Migration head | `39`, failed `0` |
| PostgreSQL image | `pgvector/pgvector:pg16` (`sha256:131dcf7ff6a900545df8e7e092c270aa8c6db2f2c818e408cb45ec21316b74e6`) |
| Qdrant image | `qdrant/qdrant:v1.14.1` (`sha256:419d72603f5346ee22ffc4606bdb7beb52fcb63077766fab678e6622ba247366`) |
| Dense model/tokenizer | BGE-M3, dimension `1024`; hashes recorded in the external manifest |

## Gate and Run Matrix

All required locked gates passed with exit code `0`: fmt, check, all-target tests, clippy, SQLx prepare, E2E Testcontainers, 50-concurrent Testcontainers and FIX486 bank contracts.

| Run | Result |
|---|---|
| R1 clean cold start | PASS |
| R2 independent clean repetition | PASS |
| R1/R2 normalized comparison | PASS: deterministic hierarchy and snapshot match |
| R3 restart and dependency recovery | PASS: state and normalized probes match R2 |

The R1/R2/R3 audit has `1` ACTIVE document, `7` chunks (`1` SOURCE, `2` PARENT, `4` children), `6` bindings all `SYNCED`, `6` completed outbox events, `6` Qdrant points, and zero dead letters, orphans and duplicates. Repeating the same ingestion left the snapshot unchanged.

Search and RetrieveContext each returned two contexts for the expected document/version. Their selected `(matched_chunk_id, parent_chunk_id)` identities were identical. During R3, stopping either Qdrant or PostgreSQL changed gRPC health to `NOT_SERVING`; health recovered after each dependency returned.

## Repairs

| Defect | Evidence and repair |
|---|---|
| Qdrant collection absent at initial readiness | Reproduced before `84dc1b4`; runtime now creates the configured collection before readiness. |
| Random auto-created access-zone identity | Reproduced before `36d77fc`; auto-created four-digit zone codes now derive a UUIDv5 identity. |

## Scope and Verdict

The Phase A bank remains `0.1.0-analysis-seed`; `1.0.0` was not frozen. This result proves the control-fixture runtime baseline only. It does not certify the full hierarchical bank, ranking quality, Graph/MMR quality, hybrid superiority, load SLOs, or production readiness.

External evidence: `/Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix486b/fix486b-final-9e5250b-20260718T060000Z`.

`FIX486_RUNTIME_BASELINE_PASS`
