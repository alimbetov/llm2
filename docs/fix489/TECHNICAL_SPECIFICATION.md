# FIX489 Technical Specification

FIX489 converts the FIX487 capacity and soak harnesses from dry-run planning to real local AstraVector runtime execution.

Scope:

- reuse the FIX488 production-path live client;
- execute mixed load through real gRPC calls;
- collect per-operation latency/status evidence;
- collect process/Docker resource samples;
- preserve PostgreSQL/Qdrant integrity audits;
- keep retrieval behavior frozen.

Out of scope:

- retrieval ranking changes;
- Graph, RRF, MMR or no-answer tuning;
- chunking profile changes;
- fixture/qrel edits;
- Kubernetes, backup/restore or recovery-at-scale proof.

Official commands:

```bash
ASTRAVECTOR_FIX487BC_EXECUTE_CAPACITY=true make verify-fix487bc-capacity-campaign
FIX487BC_CAPACITY_EVIDENCE_DIR=<capacity evidence dir> \
ASTRAVECTOR_FIX487C_EXECUTE_SOAK=true make verify-fix487c-soak-60m
```

Contract-only command:

```bash
make verify-fix489-live-capacity-contracts
```

Evidence root:

```text
${ASTRAVECTOR_EVIDENCE_ROOT:-../astravector-evidence}/fix489-capacity/<run-id>
${ASTRAVECTOR_EVIDENCE_ROOT:-../astravector-evidence}/fix487c/<run-id>
```

For local developer smoke, durations can be shortened with:

```text
FIX489_CAPACITY_MEASUREMENT_SECONDS
FIX489_CAPACITY_COOLDOWN_SECONDS
FIX489_SOAK_MEASUREMENT_SECONDS
FIX489_MIN_COMPLETED_25
FIX489_MIN_COMPLETED_50
FIX489_MIN_COMPLETED_100
FIX489_MIN_COMPLETED_200
FIX489_CAPACITY_LEVELS
```

The official defaults remain the FIX487 capacity/soak contract.

The official live capacity and soak entrypoints use the `fix489-capacity`
profile by default when no explicit `ASTRAVECTOR_PROFILE` is supplied. This
profile changes only bounded operational budgets for CPU model-backed local
execution: request deadlines, PostgreSQL/Qdrant timeouts and query queue
budgets. It must not change retrieval ranking, Graph, RRF, MMR, frozen queries,
qrels or fixtures.
