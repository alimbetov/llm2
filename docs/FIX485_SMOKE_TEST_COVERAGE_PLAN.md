# FIX485 Smoke Test Coverage Plan

## 1. Purpose

This document defines the protective smoke-test system for AstraVector after `fix485`. The objective is not merely to prove that the process starts. The smoke system must detect semantic retrieval defects, security leaks, incorrect degradation, latency regressions, resource leaks and unrecoverable state drift.

```text
PROJECT_ROOT=/Users/ruslanalimbetov/Documents/llm2/astravector
EVIDENCE_ROOT=/Users/ruslanalimbetov/Documents/llm2/astravector-evidence
WORK_BRANCH=codex/fix485-rag-reliability-smoke-hardening
BASE_COMMIT=c302a8341f64f6b31e0cf7aee97a966f554b3902
```

## 2. Result semantics

Every test and every aggregate profile returns exactly one status:

```text
PASS     assertion executed and satisfied
FAIL     assertion executed and failed
BLOCKED  mandatory prerequisite/capability unavailable
SKIPPED  test intentionally not executed
```

Rules:

- `BLOCKED` and `SKIPPED` are not PASS;
- an aggregate production-candidate gate cannot pass with any mandatory FAIL, BLOCKED or SKIPPED stage;
- all tests must emit machine-readable evidence;
- a shell exit code alone is not sufficient evidence;
- no test may modify source files or tracked quality labels during execution.

## 3. Evidence directory layout

Each run writes outside the repository:

```text
/Users/ruslanalimbetov/Documents/llm2/astravector-evidence/<run-id>/
├── environment/
├── static/
├── infrastructure/
├── startup/
├── lifecycle/
├── retrieval/
├── long-query/
├── graph-mmr/
├── security/
├── failures/
├── performance/
├── observability/
├── packaging/
├── quality/
├── metrics/
├── logs/
├── stage-failures.json
├── manifest.json
└── FIX485-SMOKE-RESULT.md
```

The manifest must include:

- source branch/SHA and origin/main SHA;
- clean-worktree assertion;
- Cargo.lock SHA;
- Rust/Cargo versions;
- hardware and OS values collected from the machine;
- Docker/PostgreSQL/Qdrant versions and image digests;
- binary/model/tokenizer/config/corpus/qrels hashes;
- stage commands, timestamps, duration and exit code;
- final status and failure codes.

## 4. Smoke tiers

```text
S0 STATIC
S1 STARTUP_AND_DEPENDENCIES
S2 DATA_LIFECYCLE
S3 RETRIEVAL_MODES
S4 LONG_QUERY_SEMANTICS
S5 GRAPH_AND_MMR
S6 SECURITY_AND_VISIBILITY
S7 FAILURE_AND_RECOVERY
S8 PERFORMANCE_AND_FAIRNESS
S9 OBSERVABILITY
S10 PACKAGING_DEPLOYMENT_ROLLBACK
```

## 5. S0 — Static and compile protection

### S0-01 Format

```bash
cargo fmt --all --check
```

Pass:

```text
exit=0
working_tree_unchanged=true
```

### S0-02 Locked compile

```bash
cargo check --locked --all-targets --all-features
```

Pass:

```text
exit=0
compile_errors=0
```

### S0-03 Full locked tests

```bash
cargo test --locked --all-targets --all-features
```

Pass:

```text
failed=0
ignored_mandatory=0
```

### S0-04 Clippy

```bash
cargo clippy --locked --all-targets --all-features -- -D warnings
```

Pass:

```text
warnings=0
```

### S0-05 SQLx metadata

```bash
cargo sqlx prepare --check -- --all-targets --all-features
```

Pass:

```text
metadata_drift=0
```

### S0-06 Query-processing contracts

```bash
cargo test --locked query_processing --lib -- --nocapture
cargo test --locked --test query_processing_contracts -- --nocapture
```

Must include boundaries, overlap, tail preservation, one Graph, one MMR, production defaults and legacy config aliases.

## 6. S1 — Startup and dependency proof

### S1-01 Infrastructure readiness

Start PostgreSQL and Qdrant using repository compose definitions.

Assertions:

- PostgreSQL accepts connections;
- Qdrant `/readyz` succeeds;
- actual image digest recorded;
- expected ports recorded;
- no pre-existing runtime owns the gRPC/metrics ports.

### S1-02 Migrations

Assertions:

- migrations apply from a clean database;
- reapplying is idempotent;
- migration head equals repository head;
- no failed `_sqlx_migrations` row;
- schema integrity SQL returns zero violations.

### S1-03 Model and tokenizer

Assertions:

- configured model and tokenizer exist;
- checksums match resolved production-candidate config;
- tokenizer loads and produces offsets;
- model warmup completes;
- dense dimension is 1024;
- sparse capability mode is explicitly recorded.

### S1-04 Runtime readiness

Assertions:

- process remains alive;
- gRPC reflection lists expected services;
- health check is SERVING;
- metrics endpoint is available;
- readiness fails when a required dependency is intentionally removed and recovers after restoration.

## 7. S2 — Data lifecycle and canonical-state protection

### S2-01 Idempotent ingestion

1. Ingest a document with a request ID.
2. Repeat the same request.

Pass:

```text
duplicate_documents=0
duplicate_chunks=0
duplicate_outbox_effects=0
```

### S2-02 Version activation

1. Ingest version 1.
2. Ingest and activate version 2.
3. Query content unique to each version.

Pass:

```text
version_2_searchable=true
version_1_searchable=false
wrong_version_results=0
```

### S2-03 TTL

Pass:

- expired version not returned by Dense, Sparse, FTS, Graph or Explain;
- PostgreSQL canonical status correct;
- Qdrant extra point removed or filtered until reconciliation;
- no non-expired sibling removed.

### S2-04 Legal hold

Pass:

- held document survives cleanup;
- retrieval remains correct according to lifecycle policy;
- release of hold allows expected cleanup;
- no unrelated document affected.

### S2-05 Delete and tombstone

Pass:

- deleted content is never returned;
- tombstone/audit semantics remain intact;
- reingestion policy behaves deterministically.

### S2-06 Reconciliation

Inject:

- missing Qdrant point;
- extra Qdrant point;
- stale payload/version;
- outbox item requiring retry.

Pass:

```text
missing_point_repaired=true
extra_point_removed_or_quarantined=true
stale_payload_repaired=true
canonical_postgres_state_unchanged=true
```

## 8. S3 — Executable retrieval mode coverage

Create `68-hybrid-runtime-retrieval.sh` or equivalent.

### S3-01 Dense semantic paraphrase

Pass when expected document/block appears in top-K and branch diagnostics prove Dense executed.

### S3-02 Sparse technical lookup

Queries include:

- `ORA-00904`;
- `LONG_QUERY_EXTENDED_NOT_ENABLED`;
- `/api/v1/documents`;
- `access_zone_id`;
- `runtime-quality-report.json`;
- leading-zero identifiers.

Pass:

```text
exact_identifier_recall=1.0 for mandatory cases
forbidden_results=0
```

### S3-03 PostgreSQL FTS

Pass only when:

- FTS branch actually executed;
- expected parent block found;
- statement timeout respected;
- access/lifecycle filters applied;
- no duplicate candidate identities after fusion.

### S3-04 Dense + Sparse

Pass when hybrid evidence improves or preserves expected top-K versus mandatory baseline and hard-negative FPR does not increase.

### S3-05 Dense + Sparse + FTS

Pass when diagnostics prove all enabled branches, local fusion and global fusion executed as configured.

### S3-06 Hard negatives

Mandatory outcome:

```text
false_positive_rate=0
forbidden_total_after=0
```

### S3-07 Multilingual

Cover RU, KZ, EN and mixed queries. Record Recall@K and exact identity, not only non-empty response.

### S3-08 Explain/Search parity

For identical caller, zone and query:

- same tier;
- same normalized token count;
- same security filters;
- same active-version/lifecycle eligibility;
- Explain cannot reveal hidden candidates.

## 9. S4 — Long-query model-backed semantic protection

Create `67-long-query-model-backed.sh` or equivalent.

### S4-01 Canonical boundaries

Generate exact tokenizer counts:

| Tokens | Extended flag | Expected |
|---:|---|---|
| 256 | off/on | SINGLE |
| 257 | off/on | SEGMENTED_STANDARD |
| 1024 | off/on | SEGMENTED_STANDARD |
| 1025 | off | LONG_QUERY_EXTENDED_NOT_ENABLED |
| 1025 | on | SEGMENTED_EXTENDED |
| 2048 | on | SEGMENTED_EXTENDED |
| 2049 | on | LONG_QUERY_TOO_LARGE |

For 2049 assert zero embedding, Qdrant, FTS and Graph calls through metrics or failpoint counters.

### S4-02 Normalization coordinate consistency

Cases:

- CRLF;
- multiple blank lines;
- trailing spaces;
- mixed Unicode punctuation;
- fenced code.

Pass when intent source ranges bind to the correct segments and all canonical tokens are covered.

### S4-03 Two intents in one segment

Corpus:

- document A answers PostgreSQL source-of-truth question;
- document B answers legal-hold/TTL question;
- optional document C answers both.

Queries:

- both questions in one short physical segment;
- both questions split by newline;
- polite indirect variants.

Pass:

- only A present → first intent covered, second uncovered, status DEGRADED;
- A+B present → both covered;
- C present → both attributions independently pass;
- no candidate gets automatic credit for unrelated intent.

### S4-04 Overlap

One intent spans two overlapping physical segments.

Pass:

- candidate score uses maximum contribution for the intent;
- matched segment provenance contains both;
- score is not doubled.

### S4-05 Tail evidence

Place decisive identifier/evidence near canonical token 1900–2048.

Pass:

- last segment reaches final original token;
- expected document found;
- diagnostics show no truncation.

### S4-06 Technical evidence + final question

Input contains long stack trace/SQL/config followed by a final question.

Pass:

- technical blocks are searchable but not independently required;
- final question is required;
- coverage corresponds to the question, not each log line.

### S4-07 Long hard negative

Pass:

```text
contexts=0 or explicit no-answer
false_positive=0
```

### S4-08 Intent classifier adversarial cases

Include:

- polite RU/KZ/EN requests;
- indirect questions without `?`;
- log line beginning with `must`;
- SQL containing `select` in lowercase;
- stack trace with question-like punctuation.

## 10. S5 — GraphRAG and MMR

### S5-01 One Graph execution

Instrument call count. Pass when Graph expansion executes at most once per request for Single, Standard and Extended.

### S5-02 Seed reservation

Pass when:

- maximum one primary seed per covered required intent before global fill;
- duplicate identities removed;
- tier seed cap respected;
- no seed from an uncovered intent.

### S5-03 Graph provenance

Pass when every graph candidate records origin seed, zone and intent IDs.

### S5-04 No evidence creation from graph-only neighbor

Without a direct evidence seed, graph neighbor cannot make an intent covered.

### S5-05 MMR one execution and diversity

Pass when:

- one MMR execution per request;
- covered required intents reserved;
- duplicates reduced;
- one candidate covering multiple intents may reduce context count;
- deterministic fallback when embedding fetch unavailable.

### S5-06 Optional-stage timeout

Graph/MMR timeout must preserve direct results and return explicit degradation warning.

## 11. S6 — Security and visibility

### S6-01 Cross-zone Dense/Sparse/FTS

Zone A query must never return Zone B through any branch or fusion stage.

### S6-02 Access level

PUBLIC caller must not receive RESTRICTED/CONFIDENTIAL content.

### S6-03 Missing/invalid identity

Pass when missing caller access and invalid API key are rejected fail-closed.

### S6-04 Gateway forwarding trust

Forwarded identity headers are ignored without a valid gateway trust token.

### S6-05 Lifecycle filters

Deleted, expired, inactive and superseded versions are hidden in Search, RetrieveContext, Explain and Graph expansion.

### S6-06 Diagnostics privacy

No raw query, raw intent, secret, API key or document text in metrics. Logs may contain correlation ID and bounded non-sensitive metadata only.

Hard acceptance:

```text
cross_zone_leakage=0
access_level_violations=0
wrong_versions=0
deleted_results=0
expired_results=0
```

## 12. S7 — Failure and recovery

Create `69-partial-backend-failure.sh` or equivalent.

### S7-01 Dense failure, Sparse/FTS available

Expected: response may be DEGRADED with valid evidence, not false no-answer.

### S7-02 Sparse failure, Dense/FTS available

Expected: explicit branch failure diagnostics and bounded fallback.

### S7-03 FTS timeout

Expected: statement cancelled within budget; direct vector evidence preserved.

### S7-04 Qdrant timeout/unavailable

Expected: no hang; bounded retry only with sufficient remaining budget; correct UNAVAILABLE/DEGRADED semantics.

### S7-05 PostgreSQL timeout/pool pressure

Expected: no indefinite wait; statement/acquire timeout; no visibility bypass.

### S7-06 Client cancellation

Expected:

```text
status=CANCELLED
pending_tasks_cancelled=true
permits_released=true
in_flight_gauges_return_zero=true
```

### S7-07 Shutdown during load

Expected graceful drain within configured timeout and no corrupt canonical state.

### S7-08 Admission overload

Expected RESOURCE_EXHAUSTED before unbounded queue/deadline accumulation.

### S7-09 Outbox fencing/dead-letter/recovery

Retain existing stale-worker and dead-letter assertions and include them in the aggregate reliability gate.

## 13. S8 — Performance and fairness

Create `70-mixed-tier-fairness.sh` or equivalent.

### S8-01 Isolated baselines

Run 100% Single, Standard and Extended separately after warmup.

Record:

- achieved RPS;
- p50/p95/p99;
- error/status distribution;
- stage latencies;
- CPU/RSS/swap;
- PostgreSQL pool and Qdrant metrics;
- queue age and admission wait.

### S8-02 Mixed profile

```text
70% SINGLE
20% STANDARD
10% EXTENDED
```

Pass:

```text
single_p95_regression<=15%
accepted_error_rate<=0.5%
positive_empty_count=0
critical_wrong_result_count=0
```

### S8-03 Adversarial fairness

```text
50% SINGLE
50% EXTENDED
```

Pass when Single requests remain admitted and Extended cannot consume all work units indefinitely.

### S8-04 Spike and recovery

Run a bounded spike above stable RPS, then recovery windows.

Pass:

- runtime PID/start time unchanged;
- health remains/re-becomes SERVING;
- no permit leak;
- p95 returns to declared recovery SLO for consecutive windows.

### S8-05 Soak

Run at a conservative fraction of stable RPS for 60 minutes.

Pass:

- no monotonic RSS leak beyond declared threshold;
- swap stable/bounded;
- error and correctness gates maintained;
- post-soak quality unchanged;
- integrity audit zero.

### S8-06 Three-run repeatability

Three consecutive runs must use identical source, Cargo.lock, binary, model, tokenizer, config, corpus and qrels hashes.

## 14. S9 — Observability

Create `71-query-observability.sh` or equivalent.

Assert:

- metrics endpoint ready;
- query counters increase by tier/status;
- branch counters increase by branch/status;
- coverage metrics change by stage;
- admission reject and wait metrics exist;
- Graph/MMR skip metrics exist;
- all in-flight gauges return to zero after completion/cancellation;
- readiness transitions when required dependency is unavailable;
- alerts/dashboards reference existing metric names.

## 15. S10 — Packaging, deployment and rollback

Create `72-deployment-container.sh` or equivalent.

### S10-01 Docker build

Use locked dependency resolution and record image digest.

Pass:

- expected release binary present;
- container starts with external PostgreSQL/Qdrant;
- health, gRPC and metrics work;
- graceful stop succeeds;
- secrets absent from image history/logs.

### S10-02 Kubernetes validation

Pass:

- manifests render;
- `kubectl apply --dry-run=server` succeeds in test cluster;
- readiness/liveness correct;
- resource requests/limits present;
- security context reviewed;
- ConfigMap/Secret wiring valid;
- rolling update and rollback tested.

### S10-03 Feature rollback

Disable Extended and/or long-query feature flags without DB migration or Qdrant reindex.

Pass:

- Single remains operational;
- Standard remains operational when only Extended disabled;
- Extended requests fail with explicit contract;
- no silent truncation returns.

## 16. Aggregate profiles

### `rag-fast`

Mandatory on every PR:

```text
S0
S1 startup with Testcontainers
query-processing contracts
normalization/intent/evidence unit tests
security contracts
```

### `rag-model-backed`

Mandatory before review-ready:

```text
S1
S3
S4
S5
S6
quality confidence
```

### `rag-reliability`

Mandatory before production-candidate verdict:

```text
S2
S6
S7
S9
existing reliability-closing profile
```

### `rag-release-candidate`

Mandatory before production-candidate verdict:

```text
S0-S10 mandatory tests
fresh quality/holdout status
load/soak/recovery
packaging/deployment/rollback
```

## 17. CI cadence

| Cadence | Required coverage |
|---|---|
| Every PR | S0 + fast Testcontainers + query semantics + security contracts |
| Push to main | PR set + concurrency smoke |
| Protected/manual | model-backed S3–S6 |
| Nightly | failure matrix subset + quality bank |
| Release candidate | full S0–S10 + 60-minute soak |
| Production promotion | three-run repeatability + deployment + rollback |

## 18. Final acceptance

The smoke plan is complete only when:

```text
STATIC_GATE_PASS
STARTUP_GATE_PASS
DATA_LIFECYCLE_PASS
DENSE_SPARSE_FTS_HYBRID_PASS
LONG_QUERY_MODEL_BACKED_PASS
MULTI_INTENT_EVIDENCE_PASS
GRAPH_MMR_PROVENANCE_PASS
SECURITY_ISOLATION_PASS
FAILURE_RECOVERY_PASS
MIXED_TIER_FAIRNESS_PASS
OBSERVABILITY_PASS
PACKAGING_DEPLOYMENT_ROLLBACK_PASS
THREE_RUN_REPEATABILITY_PASS
```

Any missing mandatory proof produces:

```text
FIX485_PRODUCTION_BLOCKED
```

with exact failure codes and evidence paths.