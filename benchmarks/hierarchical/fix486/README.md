# FIX486 hierarchical retrieval bank

This directory contains the versioned evidence bank for AstraVector hierarchical retrieval.

## Current status

```text
BANK_ID=fix486-hierarchical-bank
BANK_VERSION=0.1.0-analysis-seed
STATUS=NOT_FROZEN
```

The seed bank defines logical fixtures and expected behavior. Codex must verify that the identities can be reproduced through the real ingestion path before publishing version `1.0.0`.

## Contents

```text
bank-manifest.json
corpus/hierarchical-fixture-v1.json
queries/hierarchical-queries-v1.jsonl
qrels/hierarchical-qrels-v1.jsonl
graph-relations/hierarchical-graph-v1.json
lifecycle/hierarchical-lifecycle-v1.json
schemas/*.schema.json
```

## Critical proof cases

```text
FIX486-01 child → correct parent
FIX486-02 parent deduplication
FIX486-03 cross-zone logical-ID isolation
FIX486-04 inactive version filtering
FIX486-05 deleted/orphan parent rejection
FIX486-06 hydration failure semantics
FIX486-07 exact child evidence preservation
FIX486-08 Graph child → own parent
FIX486-09 unique-intent token-budget protection
FIX486-10 large-parent multi-aspect pressure
```

## Immutability

Once version `1.0.0` is frozen:

- expected labels may not be changed to match runtime output;
- query and qrel changes require a new bank version;
- fixture changes and production fixes must be separate commits;
- every evidence run must record all file hashes;
- a bank identity mismatch blocks the phase verdict.

## Seed anchors

The corpus uses unique markers such as:

```text
ASTRA_CANONICAL_STATE_A1
ASTRA_LEGAL_HOLD_A2
ASTRA_RECONCILIATION_A3
ORA-00904
content_chunks_v004
/api/v1/search
```

These anchors are test data only and must never be hardcoded into production ranking logic.

## Expected execution model

```text
bank fixture
→ public ingestion facade
→ actual deterministic chunks
→ PostgreSQL/Qdrant/Graph
→ public retrieval facade
→ assertion against independent qrels
```

Direct insertion into internal tables is allowed only for deliberate stale/orphan/failure scenarios and must be explicitly marked as fault injection.