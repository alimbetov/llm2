# fix486d Execution and Evidence Contract

## 1. Purpose

This contract defines how official Phase D evidence is executed, stored, normalized and accepted.

The evidence must demonstrate the real child-match/parent-hydration path. A shell exit code or a single response screenshot is insufficient.

## 2. Official runner

Phase D should provide one phase-owned runner, recommended name:

```text
scripts/fix486d-child-parent-runtime-proof.sh
```

Recommended Make target:

```text
verify-fix486d-child-parent-runtime
```

The runner must support:

```text
--verify-identities
--prepare
--ingest
--execute-search
--execute-retrieve-context
--repeat
--restart-proof
--execute-all
--evidence-root <path>
--run-id <id>
--keep-running
```

`--execute-all` must perform all mandatory stages and produce one aggregate verdict.

## 3. Shell and process safety

The runner must use fail-closed shell behavior:

```bash
set -Eeuo pipefail
```

It must:

- preserve the actual exit code of every command;
- record start/end timestamps and duration;
- record command identity without leaking secrets;
- terminate phase-owned runtime processes on exit;
- avoid killing unrelated processes;
- verify port ownership before startup;
- avoid success markers when a mandatory command failed;
- distinguish stage execution success from evidence collection success.

Required trap behavior:

```text
unexpected error → record current stage FAIL → collect bounded diagnostics → cleanup phase resources → final BLOCKED
```

## 4. Source and environment identity

Every official run must record:

```text
repository
branch
source SHA
origin/main SHA
bank manifest SHA-256
bank aggregate SHA-256
Cargo.lock SHA-256
resolved config SHA-256
release binary SHA-256
model SHA-256
tokenizer SHA-256
PostgreSQL image reference and ID/digest
Qdrant image reference and ID/digest
Rust version
Cargo version
OS and hardware
runtime ports
phase runner SHA-256
```

Any identity drift after stage `identity-verification` blocks the run.

## 5. Bank verification stage

Before infrastructure startup, run the Phase C verifier.

Required result:

```text
bank_id=fix486-hierarchical-bank
bank_version=1.0.0
status=FROZEN
query_count=11
case_count=10
aggregate_sha256=cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff
hash_mismatches=0
unexpected_files=0
missing_files=0
```

The Phase D runner must not rewrite the manifest or hashes.

## 6. Infrastructure stage

Use phase-owned PostgreSQL and Qdrant resources.

Record:

```text
container names
volume names
network name
image IDs/digests
health state
startup duration
ports
```

The runner must not destroy non-phase-owned containers or volumes.

Required assertions:

```text
POSTGRES_ACCEPTS_CONNECTIONS=true
QDRANT_READYZ=true
PREEXISTING_PORT_OWNER=false
MIGRATIONS_PASS=true
MIGRATION_HEAD_RECORDED=true
```

## 7. Runtime startup stage

Build and start a release runtime using locked dependencies and the resolved Phase D configuration.

Record:

```text
binary hash
PID
config hash
model/tokenizer hashes
resolved document ingestion deadline
reflection result
health result
metrics availability
expected service list
```

Required assertions:

```text
RUNTIME_ALIVE=true
HEALTH_SERVING=true
GRPC_REFLECTION_PASS=true
METRICS_ENDPOINT_PASS=true
```

## 8. Ingestion stage

Ingest the frozen bank fixture using the production path.

The stage must:

1. invoke the existing Phase C materialization contract;
2. preserve bank hashes;
3. wait for document operations to complete;
4. wait for outbox projection to reach a stable terminal state;
5. fail on any FAILED/dead-letter effect;
6. record active document/version identities;
7. generate the logical-to-runtime identity map;
8. audit canonical hierarchy and Qdrant points.

Required bounded waits:

```text
document operation deadline: resolved typed runtime value
outbox completion deadline: explicit and finite
Qdrant consistency wait: explicit and finite
```

Do not use unbounded polling.

## 9. Canonical audit artifacts

Required files:

```text
canonical-audit/document-versions.json
canonical-audit/chunks.json
canonical-audit/child-parent-bindings.json
canonical-audit/source-block-provenance.json
canonical-audit/visibility.json
canonical-audit/integrity-summary.json
```

Required summary fields:

```json
{
  "active_documents": 0,
  "active_versions": 0,
  "parent_chunks": 0,
  "child_chunks": 0,
  "orphan_children": 0,
  "cross_document_bindings": 0,
  "cross_version_bindings": 0,
  "cross_zone_bindings": 0,
  "duplicate_chunk_ids": 0,
  "duplicate_source_provenance_rows": 0
}
```

Counts depend on the materialized frozen bank, but all violation fields must be zero.

## 10. Qdrant audit artifacts

Required files:

```text
qdrant-audit/collection.json
qdrant-audit/points-summary.json
qdrant-audit/phase-d-child-points.json
qdrant-audit/payload-consistency.json
```

For each in-scope child selected by a query, evidence must show the corresponding synchronized projection.

## 11. Query execution artifacts

For each of three queries and two entry points, create a directory:

```text
<entry-point>/<query-id>/
```

Required files:

```text
request.json
response.json
normalized-result.json
ranking-trace.json
canonical-binding-audit.json
anchor-assertions.json
dedup-assertions.json
stage-result.json
```

`dedup-assertions.json` may contain `not_applicable=true` for cases other than `q-parent-dedup`.

## 12. Trace requirements

Ranking trace must be bounded and sanitized. It should include:

```text
candidate identity
ranking stage
rank
dense score when present
sparse score when present
lexical score when present
fusion score when present
final score when present
retrieval sources
exact technical match flag
primary direct flag
graph expanded flag
parent dedup/drop reason
```

Graph-specific proof is not part of Phase D. Any Graph-expanded candidate must be excluded from the positive direct-child verdict or clearly identified as non-authoritative for this phase.

## 13. Normalization rules

The normalizer may remove only volatile fields such as:

```text
timestamps
run IDs
request IDs
trace IDs
PIDs
container IDs
latency values
non-semantic floating-point formatting
```

It must not remove:

```text
physical or logical chunk identities
parent identity
document identity
version
zone
source block identity
anchor results
retrieval-source flags
dedup reasons
status or failure codes
```

## 14. Warm-repeat artifacts

Create:

```text
comparisons/warm-repeat-search.json
comparisons/warm-repeat-retrieve-context.json
```

Compare the first and second execution without reingestion.

Required stable fields:

```text
logical child
logical parent
required anchors
forbidden anchors
dedup outcome
status
```

## 15. Restart artifacts

After runtime restart without reingestion, create:

```text
restart/health.json
restart/search-results/
restart/retrieve-context-results/
comparisons/pre-post-restart.json
```

The post-restart result must preserve the logical child/parent proof.

## 16. Stage-results schema

Write `stage-results.json` with one row per stage:

```json
{
  "schema_version": 1,
  "phase": "fix486d",
  "run_id": "string",
  "source_sha": "hex",
  "bank_version": "1.0.0",
  "bank_aggregate_sha256": "hex",
  "stages": [
    {
      "stage": "identity-verification",
      "status": "PASS",
      "started_at": "RFC3339",
      "finished_at": "RFC3339",
      "duration_ms": 0,
      "exit_code": 0,
      "failure_codes": [],
      "artifacts": []
    }
  ],
  "verdict": "FIX486_CHILD_PARENT_RUNTIME_PROOF_PASS"
}
```

Mandatory stages:

```text
identity-verification
bank-verification
static-gates
infrastructure-start
migrations
runtime-start
production-ingestion
identity-map
canonical-audit
qdrant-audit
search-proof
retrieve-context-proof
entry-point-comparison
warm-repeatability
restart-repeatability
evidence-completeness
cleanup
final-verdict
```

All mandatory stages except `cleanup` must be PASS. Cleanup failure also blocks if it leaves phase-owned resources or invalidates evidence.

## 17. Query-results schema

Write six mandatory rows to `query-results.jsonl`:

```text
3 queries × 2 entry points = 6 rows
```

Optional warm/restart rows must be stored separately or marked with an execution dimension so they cannot replace the six mandatory rows.

Required uniqueness key:

```text
(query_id, entry_point, execution_kind)
```

## 18. Manifest and integrity

`manifest.json` must enumerate every official evidence file with:

```text
relative path
size bytes
SHA-256
artifact class
mandatory flag
```

The completeness audit must:

- fail on a missing mandatory file;
- fail on a hash mismatch;
- fail on duplicate manifest paths;
- fail if an artifact points outside the run root;
- fail if stage results reference a non-manifest artifact;
- record total file count and aggregate evidence hash.

## 19. Defect register

`defect-register.json` must include:

```json
{
  "schema_version": 1,
  "unresolved_in_scope_p0": 0,
  "unresolved_in_scope_p1": 0,
  "defects": []
}
```

Every defect entry must record:

```text
id
severity
category
first failing run
root cause
regression test
fix commit
rerun evidence
status
```

## 20. Cleanup

At the end of an official run:

- stop the phase-owned runtime;
- stop/remove phase-owned containers when `--keep-running` is absent;
- record final port ownership;
- record leaked process count;
- preserve the external evidence directory;
- never delete evidence because the final verdict is BLOCKED.

Required assertions:

```text
LEAKED_RUNTIME_PROCESSES=0
LEAKED_PORT_OWNERS=0
EVIDENCE_DIRECTORY_PRESERVED=true
```

## 21. Repository outputs

The implementation may add:

```text
scripts/fix486d-child-parent-runtime-proof.sh
scripts/fix486d-child-parent-audit.sql
scripts/fix486d-normalize-result.py or equivalent
tests/fix486d_child_parent_contracts.rs
Makefile target verify-fix486d-child-parent-runtime
compact result/evidence summaries under docs/fix486/phase-d-child-parent-runtime/
```

Do not commit:

- full runtime logs;
- local model/tokenizer artifacts;
- database volumes;
- Qdrant storage;
- secrets;
- machine-specific absolute evidence bundles except in a compact manifest pointer.

## 22. Final evidence rule

The final PASS is valid only when:

```text
six mandatory query rows PASS
all mandatory stages PASS
manifest integrity PASS
no unresolved P0/P1
bank aggregate unchanged
source and runtime identities complete
```

Anything less produces `FIX486_CHILD_PARENT_RUNTIME_PROOF_BLOCKED`.
