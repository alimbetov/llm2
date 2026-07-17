# v010/fix485 — RAG Search Reliability, Performance and Quality Hardening

## 1. Document status

```text
DOCUMENT_STATUS=IMPLEMENTATION_SPECIFICATION
BASE_BRANCH=main
BASE_COMMIT=c302a8341f64f6b31e0cf7aee97a966f554b3902
WORK_BRANCH=codex/fix485-rag-reliability-smoke-hardening
LOCAL_PROJECT=/Users/ruslanalimbetov/Documents/llm2/astravector
EXTERNAL_EVIDENCE_ROOT=/Users/ruslanalimbetov/Documents/llm2/astravector-evidence
CURRENT_PRODUCTION_STATUS=BLOCKED
```

This task is a hardening and proof task. It must not be treated as permission to add unrelated retrieval features, weaken security filters, tune quality fixtures to expected answers, or declare production readiness without evidence.

## 2. Primary objectives

Bring AstraVector to a state where the following properties are demonstrated on the same source, model, tokenizer, configuration, corpus, and packaged binary:

```text
RELIABILITY
LOW_AND_PREDICTABLE_LATENCY
HIGH_RETRIEVAL_QUALITY
SECURITY_ISOLATION
CORRECT_DEGRADATION
RECOVERY_REPEATABILITY
```

The work must prioritize correctness and operational safety over feature breadth.

## 3. Current architecture to preserve

AstraVector currently provides:

- PostgreSQL as canonical state;
- Qdrant as vector projection;
- dense retrieval;
- learned or deterministic sparse retrieval;
- PostgreSQL FTS;
- hybrid fusion;
- evidence-based no-answer;
- GraphRAG expansion;
- MMR diversification;
- lifecycle, TTL, legal-hold, deletion and version filtering;
- access-zone and access-level isolation;
- reconciliation and outbox recovery;
- tiered query processing up to 2048 canonical tokens;
- weighted admission, deadlines and cancellation;
- quality banks, failpoints, Testcontainers and load tooling.

The following public contracts must remain backward compatible unless a compile-time blocker proves otherwise:

- existing Search and RetrieveContext protobuf request fields;
- existing response fields and field numbers;
- PostgreSQL schema semantics;
- Qdrant collection and payload identities;
- production defaults that keep long-query Extended opt-in;
- legacy query-processing configuration aliases.

## 4. Non-goals

Do not do the following as part of fix485 without an explicit blocker and documented approval:

- change the embedding model;
- change vector dimension;
- reindex the production corpus;
- replace RRF with another ranking method;
- add an online LLM call for query decomposition;
- weaken no-answer thresholds to make tests pass;
- hardcode document IDs, fixture IDs or expected labels in production ranking;
- bypass PostgreSQL canonical-state checks;
- disable access-zone, access-level, active-version, TTL or lifecycle filters;
- merge automatically;
- claim `PRODUCTION_READY` from static or unit tests alone.

## 5. Required correctness workstreams

### 5.1. P0 — unified query normalization and offsets

Current risk: physical segmentation may normalize CRLF, blank lines and trailing whitespace before calculating byte offsets, while logical-intent extraction may operate on a different text representation.

Introduce one canonical representation:

```rust
pub struct NormalizedQuery {
    pub original_text: String,
    pub normalized_text: String,
    pub normalized_to_original_byte_map: Vec<usize>,
    pub token_offsets: Vec<TokenOffset>,
}
```

Requirements:

1. Normalize once in the planner or a dedicated normalization module.
2. Physical segmentation and logical-intent extraction use the same normalized text and offsets.
3. Diagnostics may report counts and hashes, but not raw user query text.
4. Preserve a reversible mapping sufficient for source-range diagnostics.
5. Reject invalid UTF-8 assumptions and unsafe byte slicing.
6. Add tests for CRLF, repeated blank lines, trailing spaces, Cyrillic, Kazakh letters, URLs, SQL and stack traces.

Acceptance:

```text
NORMALIZATION_COORDINATE_SYSTEM_PASS
NO_INTENT_OFFSET_DRIFT_PASS
UTF8_RANGE_SAFETY_PASS
```

### 5.2. P0 — candidate-to-intent evidence attribution

Current risk: a physical segment can contain multiple logical intents. A candidate found for that segment may be credited to all intents even when it supports only one.

Introduce explicit evidence:

```rust
pub struct CandidateIntentEvidence {
    pub intent_id: usize,
    pub dense_score: Option<f32>,
    pub sparse_score: Option<f32>,
    pub lexical_score: Option<f32>,
    pub matched_term_count: usize,
    pub exact_technical_match_count: usize,
    pub evidence_passed: bool,
    pub reason_code: CandidateIntentEvidenceReason,
}
```

Required behavior:

1. Attribution is computed per candidate and per intent.
2. Dense-only attribution must not automatically cover all intents attached to the physical segment.
3. Sparse and lexical evidence may use normalized matched terms and exact technical identifiers.
4. A final intent is covered only when at least one final visible candidate has `evidence_passed=true` for that intent.
5. One candidate may legitimately cover multiple intents, but each attribution must pass independently.
6. Graph-derived candidates inherit only proven origin-intent provenance.

Acceptance scenarios:

- two questions in one segment, evidence only for question A → A covered, B uncovered, response `DEGRADED`;
- one candidate genuinely supports A and B → both covered;
- overlapping segments for one intent do not double count;
- technical context without a request does not create false full coverage.

### 5.3. P0 — retrieval failure semantics

No-answer must mean successful retrieval with no adequate evidence. It must not mean a backend could not be queried.

Add or complete branch status:

```rust
pub enum RetrievalBranchStatus {
    SuccessWithEvidence,
    SuccessNoEvidence,
    Timeout,
    BackendUnavailable,
    Cancelled,
    SkippedBudget,
}

pub enum SegmentRetrievalStatus {
    Success,
    PartialFailure,
    Failed,
    Skipped,
}
```

Rules:

- all required intents searched successfully and none has evidence → `INSUFFICIENT`;
- at least one required intent covered and another successful-without-evidence → `DEGRADED`;
- at least one required intent cannot be searched due to infrastructure failure → `DEGRADED` with explicit warning;
- all required intents fail due to infrastructure → `UNAVAILABLE` or `DEADLINE_EXCEEDED`;
- cancellation must return `CANCELLED` and release all permits;
- GraphRAG/MMR optional-stage timeout must not erase direct evidence.

### 5.4. P1 — deterministic multilingual intent classifier

The classifier must remain local, deterministic and rule-based for this task.

Ordering:

1. detect fenced code, SQL, stack trace, log and configuration evidence;
2. detect explicit questions;
3. detect polite requests and imperatives;
4. detect constraints attached to a request;
5. classify remaining text as context;
6. if no required intent exists, create one `ImplicitSearchIntent`.

Expand patterns for RU/KZ/EN, including polite and indirect requests such as:

- `Пожалуйста, объясните...`;
- `Подскажите, почему...`;
- `Мне нужно понять...`;
- `Можно ли сравнить...`;
- `Нужно определить...`;
- `Please explain...`;
- `Could you compare...`;
- `Маған түсіндіріңіз...`.

A technical line such as `must not be null` must not be treated as a user constraint merely because it starts with `must`.

Diagnostics must expose kind, required flag, confidence bucket/reason code and source segment IDs, but never raw intent text.

### 5.5. P1 — model-backed tokenizer boundary proof

WhitespaceCounter tests remain unit tests only. Add a suite that loads the actual configured tokenizer and produces exact canonical-token queries for:

```text
256
257
1024
1025
2048
2049
```

Test input categories:

- Russian;
- Kazakh;
- English;
- mixed RU/KZ/EN;
- snake_case and dotted identifiers;
- URLs and API paths;
- SQL;
- Java/Rust stack traces;
- Unicode punctuation;
- CJK safety case.

Assertions:

- correct tier;
- no silent truncation;
- token tail preserved;
- 2049 rejected before embedding/Qdrant/FTS calls;
- segment count and max segment tokens remain within selected profile.

## 6. Retrieval quality workstream

### 6.1. Executable retrieval mode proof

Replace the obsolete static-grep BM25/Hybrid smoke with executable runtime assertions. Prove the actual branches used by the production request path:

```text
DENSE_ONLY
SPARSE_ONLY
FTS_ONLY
DENSE_SPARSE
DENSE_SPARSE_FTS
```

If the public request API does not expose a mode selector, use controlled test configuration or test-only fixtures without adding an incompatible production API. The proof must show branch execution through diagnostics/metrics and validate returned identities.

Required query categories:

- semantic paraphrase;
- exact error code;
- article or section number;
- API path;
- SQL/table/column identifier;
- typo/normalization;
- multilingual query;
- lexical distractor;
- hard negative;
- active-version and access-isolation cases.

For each query record:

- expected document/block identity;
- top-K identities;
- branch status;
- dense/sparse/lexical/fusion scores where available;
- final visible result;
- no-answer decision;
- latency by stage.

### 6.2. No ranking changes without A/B evidence

Do not modify dense/sparse/lexical weights, RRF constants, no-answer thresholds, Graph weights or MMR lambdas merely to make fixtures pass.

Any ranking change requires:

1. baseline captured from base commit;
2. candidate run on the same corpus/model/tokenizer/config;
3. comparison of Recall@K, Intent Recall@K, MRR, nDCG@10, hard-negative FPR and latency;
4. no access/lifecycle regression;
5. an explicit report explaining wins and losses.

### 6.3. Quality banks

Create or extend profiles:

```text
query-processing-model-backed-v1
multi-intent-v1
long-query-extended-v2
technical-identifiers-v2
hybrid-runtime-v2
failure-semantics-v1
```

Target minimum evaluated set:

| Category | Minimum |
|---|---:|
| Single semantic | 30 |
| Sparse/technical | 30 |
| Hybrid/FTS | 30 |
| Multi-intent | 30 |
| Long-query | 30 |
| Hard-negative | 30 |
| Security/isolation | 20 |
| GraphRAG/MMR | 20 |
| **Total** | **220** |

Blind judgment remains a separate gate. Do not use final runtime output to generate expected labels.

## 7. Performance and fairness workstream

### 7.1. Preliminary SLO

| Tier | p95 | p99 | Server deadline |
|---|---:|---:|---:|
| SINGLE | ≤1000 ms | ≤1500 ms | 1000–1500 ms |
| STANDARD | ≤3000 ms | ≤3500 ms | 3000 ms |
| EXTENDED | ≤5000 ms | ≤6000 ms | 6000 ms |

SLO values are local reference gates until a production deployment profile is measured.

### 7.2. Load profiles

Run:

```text
100% SINGLE
100% STANDARD
100% EXTENDED
70% SINGLE / 20% STANDARD / 10% EXTENDED
50% SINGLE / 50% EXTENDED
```

Assertions:

- mixed-load SINGLE p95 degradation ≤15% from isolated SINGLE baseline;
- Extended cannot starve Single;
- error rate ≤0.5% for accepted traffic;
- queue age remains bounded;
- admission rejects overload before unbounded deadline accumulation;
- semaphore permits and in-flight gauges return to zero;
- no panic, OOM, deadlock or runtime restart;
- post-spike latency recovers to baseline range.

### 7.3. Budget discipline

Verify for every tier:

- request receipt time is included in deadline;
- client timeout may reduce but not increase server cap;
- retries require sufficient remaining budget;
- PostgreSQL statement timeout respects remaining response reserve;
- optional Graph/MMR stages skip with explicit warning when budget is insufficient;
- cancellation propagates to pending inference, Qdrant, FTS, Graph and MMR fetch tasks.

## 8. Reliability and recovery workstream

Create deterministic failpoint or Testcontainers scenarios for:

- dense inference failure;
- sparse failure;
- FTS statement timeout;
- Qdrant timeout;
- Qdrant unavailable;
- PostgreSQL statement timeout;
- connection-pool exhaustion;
- Graph timeout;
- MMR embedding fetch timeout;
- client cancellation;
- shutdown during request;
- admission timeout;
- stale outbox worker;
- dead-letter recovery;
- missing Qdrant point reconciliation;
- extra Qdrant point reconciliation;
- active-version transition under concurrent search.

Hard requirements:

```text
panic=0
OOM=0
deadlock=0
permit_leak=0
false_no_answer=0
cross_zone_leakage=0
wrong_version=0
```

## 9. Security requirements

Prove identical security behavior for Search, RetrieveContext, ExplainSearch and Graph expansion:

- Zone A cannot retrieve Zone B;
- PUBLIC cannot retrieve RESTRICTED;
- missing caller access is rejected;
- invalid API key is rejected;
- forwarded identity is ignored unless the trusted gateway token is valid;
- deleted, expired, inactive and superseded versions are hidden;
- FTS cannot bypass canonical filters;
- Graph edges cannot cross access zones;
- diagnostics do not leak raw query, raw intent, document text or secrets.

Hard gate:

```text
cross_zone_leakage=0
access_level_violation=0
wrong_version=0
deleted_result=0
expired_result=0
```

## 10. Observability requirements

Ensure metrics exist and are tested for:

```text
query_total{tier,status}
query_duration_seconds{tier}
query_planning_duration_seconds{tier}
query_segment_count{tier}
query_intent_count{tier}
retrieval_branch_total{branch,status,tier}
retrieval_branch_duration_seconds{branch,tier}
intent_coverage_ratio{tier,stage}
query_degraded_total{tier,reason}
admission_wait_seconds{tier}
admission_rejected_total{tier}
work_units_in_flight{tier}
graph_seed_count{tier}
graph_skipped_total{reason}
mmr_skipped_total{reason}
```

Do not add high-cardinality labels such as query text/hash, document ID, user ID, correlation ID or access-zone ID.

Smoke assertions:

- metrics endpoint is available;
- counters change after relevant requests;
- in-flight gauges return to zero;
- timeout/rejection/degradation reason labels are present;
- logs contain correlation ID but no raw query or secret;
- readiness changes correctly when a required dependency is lost.

## 11. Reproducible evidence requirements

All official evidence must be external to the git worktree:

```text
/Users/ruslanalimbetov/Documents/llm2/astravector-evidence/<run-id>/
```

Every run must capture:

- source branch and SHA;
- origin/main SHA;
- clean-worktree result;
- Cargo.lock SHA;
- rustc and cargo versions;
- host hardware from `system_profiler`/`sysctl` rather than hardcoded values;
- macOS version;
- Docker version;
- PostgreSQL and Qdrant image digests and runtime versions;
- binary SHA;
- model SHA;
- tokenizer SHA;
- resolved configuration and SHA;
- corpus snapshot and SHA;
- quality-bank/qrels SHA;
- all command lines, start/end timestamps and exit codes;
- stage failure registry;
- metrics snapshots;
- machine-readable final report.

All Cargo verification and build commands must use `--locked`.

A dirty worktree, source SHA mismatch, missing model/tokenizer, unavailable infrastructure or skipped mandatory stage yields `BLOCKED`, never PASS.

## 12. Required new tests and scripts

Target files:

```text
src/query_processing/normalization.rs
src/query_processing/evidence.rs
src/query_processing/status.rs

tests/query_tokenizer_model_backed.rs
tests/query_normalization_offsets.rs
tests/multi_intent_evidence.rs
tests/retrieval_failure_semantics.rs
tests/mixed_tier_fairness.rs
tests/graph_intent_provenance.rs
tests/explain_search_parity.rs

smoke-tests/v004/scripts/67-long-query-model-backed.sh
smoke-tests/v004/scripts/68-hybrid-runtime-retrieval.sh
smoke-tests/v004/scripts/69-partial-backend-failure.sh
smoke-tests/v004/scripts/70-mixed-tier-fairness.sh
smoke-tests/v004/scripts/71-query-observability.sh
smoke-tests/v004/scripts/72-deployment-container.sh
```

Names may change if the repository has a better established convention, but the capability coverage must remain.

## 13. Required Make targets

Add or align:

```makefile
verify-rag-core:
	cargo fmt --all --check
	cargo check --locked --all-targets --all-features
	cargo test --locked --all-targets --all-features
	cargo clippy --locked --all-targets --all-features -- -D warnings
	cargo sqlx prepare --check -- --all-targets --all-features

smoke-rag-long-query:
	./smoke-tests/v004/scripts/run-full-smoke.sh --only long-query-model-backed --keep-running

smoke-rag-hybrid:
	./smoke-tests/v004/scripts/run-full-smoke.sh --only hybrid-runtime-retrieval --keep-running

smoke-rag-failures:
	./smoke-tests/v004/scripts/run-full-smoke.sh --only partial-backend-failure --keep-running

smoke-rag-mixed-load:
	./smoke-tests/v004/scripts/run-full-smoke.sh --only mixed-tier-fairness --keep-running

verify-rag-production-candidate:
	$(MAKE) verify-rag-core
	$(MAKE) smoke-rag-long-query
	$(MAKE) smoke-rag-hybrid
	$(MAKE) smoke-rag-failures
	$(MAKE) quality-runtime-confidence-remote
```

## 14. CI strategy

### Per PR mandatory

- format;
- locked all-target/all-feature check;
- query-processing unit tests;
- query-processing contracts;
- normalization/intent/evidence tests;
- locked all-target/all-feature tests;
- clippy with warnings denied;
- SQLx metadata check;
- Testcontainers concurrency smoke;
- security isolation contracts.

### Protected model-backed workflow

- real tokenizer boundaries;
- dense/sparse/FTS/hybrid runtime quality;
- multi-intent quality;
- hard negatives;
- Graph/MMR provenance;
- fresh confidence report.

### Scheduled or release-candidate workflow

- mixed-tier load;
- spike/recovery;
- 60-minute soak;
- post-load quality and integrity;
- three-run repeatability;
- packaged image/deployment/rollback proof.

## 15. Acceptance criteria

### Correctness

```text
[ ] Normalization and intent offsets share one coordinate system
[ ] Real-tokenizer boundaries 256/257/1024/1025/2048/2049 pass
[ ] Multi-intent false coverage = 0
[ ] Infrastructure failure is never interpreted as no-answer
[ ] 2049-token query performs zero retrieval backend calls
```

### Retrieval quality

```text
[ ] Dense baseline has no material regression
[ ] Sparse exact-technical recall has no material regression
[ ] Hybrid MRR/nDCG has no material regression
[ ] hard-negative false-positive rate = 0 for mandatory bank
[ ] wrong-version rate = 0
[ ] forbidden-result rate = 0
```

### Reliability

```text
[ ] panic = 0
[ ] OOM = 0
[ ] deadlock = 0
[ ] semaphore leak = 0
[ ] cancellation passes
[ ] graceful shutdown passes
[ ] overload recovery passes
```

### Performance

```text
[ ] SINGLE p95 <=1000 ms on declared local reference profile
[ ] STANDARD p95 <=3000 ms
[ ] EXTENDED p95 <=5000 ms
[ ] mixed-load SINGLE degradation <=15%
[ ] accepted-request error rate <=0.5%
```

### Security

```text
[ ] cross-zone leakage = 0
[ ] access violation = 0
[ ] Graph cross-zone leakage = 0
[ ] FTS access bypass = 0
[ ] Explain/Search security parity passes
```

### Evidence

```text
[ ] Fresh model-backed quality report exists for branch head
[ ] Fresh holdout/blind-judgment status is explicit
[ ] Three repeatable clean load runs pass or status remains BLOCKED
[ ] Docker/deployment smoke passes
[ ] Rollback proof exists
```

## 16. Definition of Done

```text
RAG_QUERY_SEMANTIC_CORRECTNESS_PASS
NORMALIZATION_OFFSET_MAPPING_PASS
CANONICAL_TOKENIZER_BOUNDARIES_PASS
MULTI_INTENT_EVIDENCE_PASS
NO_FALSE_INTENT_COVERAGE_PASS

DENSE_RUNTIME_PASS
SPARSE_RUNTIME_PASS
FTS_RUNTIME_PASS
HYBRID_RUNTIME_PASS
NO_ANSWER_SEMANTICS_PASS
PARTIAL_FAILURE_SEMANTICS_PASS

GRAPH_INTENT_PROVENANCE_PASS
ONE_GRAPH_PER_REQUEST_PASS
ONE_MMR_PER_REQUEST_PASS

ACCESS_ISOLATION_PASS
LIFECYCLE_FILTERING_PASS
EXPLAIN_SEARCH_PARITY_PASS

SINGLE_LATENCY_PASS
STANDARD_LATENCY_PASS
EXTENDED_LATENCY_PASS
MIXED_TIER_FAIRNESS_PASS

FAILPOINT_RECOVERY_PASS
OVERLOAD_RECOVERY_PASS
SHUTDOWN_PASS

MODEL_BACKED_QUALITY_PASS
BLIND_JUDGMENT_STATUS_RECORDED
THREE_RUN_REPEATABILITY_PASS

PACKAGED_IMAGE_PASS
DEPLOYMENT_SMOKE_PASS
ROLLBACK_PASS

RAG_SEARCH_ENGINE_PRODUCTION_CANDIDATE
```

## 17. Mandatory reporting rule

Codex must end with one of:

```text
FIX485_PRODUCTION_CANDIDATE
```

or:

```text
FIX485_PRODUCTION_BLOCKED
```

`FIX485_PRODUCTION_BLOCKED` must include exact remaining blockers and evidence paths. A partial implementation with green static checks is not production-ready.