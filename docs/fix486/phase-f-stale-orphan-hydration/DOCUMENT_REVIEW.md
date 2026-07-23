# FIX486F Document Review

## Review Identity

```text
branch: codex/fix486f-stale-orphan-hydration-proof
specification head reviewed: 254b915486aff7ad6276010424c3e034a29c23ed
base: f989eae11176d3f9137b0d3d4fb5418159b90713
frozen bank: 1.0.0 / FROZEN
frozen aggregate: cc699d929226f928eb2e92aa97d51d82d78e20f69440f04229e9bec9f83164ff
```

## Findings Resolved in Documentation

### F-DR-001 — Partial timeout could force N+1 hydration

Severity: HIGH

The original text required parent-scoped timeout without protecting the existing
batch SQL invariant. Implementing it literally could replace one canonical batch
fetch with N parent queries. The contract now forbids proof-only N+1 behavior and
requires the capability audit to distinguish real independently bounded hydration
from a post-batch orchestration failpoint.

### F-DR-002 — Qdrant parent-ID tamper had the wrong expected reason

Severity: HIGH

Changing only `parent_chunk_id` while retaining the original binding creates a
payload/binding disagreement. The correct first classification is
`BINDING_INVALID`, not `HYDRATION_MISSING`. The mandatory missing-parent proof now
uses request-scoped `RETURN_NOT_FOUND_SELECTED` after binding validation. Payload
tamper remains an optional, separately classified diagnostic.

### F-DR-003 — Ranking non-interference could pass vacuously

Severity: HIGH

The frozen stale query may have no valid survivor, making unchanged empty result
sets meaningless. The ranking control now requires a runner-owned query with at
least one known-valid survivor and proof that the injected candidate entered the
raw candidate window.

### F-DR-004 — Transport errors were treated as response messages

Severity: MEDIUM

`UNAVAILABLE` and `DEADLINE_EXCEEDED` may terminate gRPC without a normal protobuf
body. The contracts now separate transport outcome, optional structured status
details, protected trace, and normalized proof fields. No response-only field may
be claimed when no response body was emitted.

### F-DR-005 — Failpoint activation boundary was underspecified

Severity: HIGH

The original matching used an ambiguous request ID and prohibited a public API
without defining a safe alternative. The contract now requires caller
`correlation_id`, an explicit non-production startup capability, bounded
activation, and a local phase-owned control mechanism or startup plan.

### F-DR-006 — Metrics across restart lacked process-epoch semantics

Severity: MEDIUM

In-process Prometheus counters may reset after runtime restart. Metric evidence is
now compared within process epochs, with a new baseline after restart.

### F-DR-007 — Hard-gate counter names diverged across documents

Severity: MEDIUM

The technical specification now carries canonical detailed names for concurrency
and response/trace/metric propagation. Aggregate aliases may be additional output
but cannot replace the detailed counters.

## Review Verdict

```text
APPROVED_FOR_CAPABILITY_AUDIT
```

This verdict approves production capability inspection only. It is not runtime
PASS and does not authorize ranking, qrel, tokenizer, Graph, MMR, or frozen-bank
changes. Implementation design must be revised if the capability audit disproves
any assumed boundary.
