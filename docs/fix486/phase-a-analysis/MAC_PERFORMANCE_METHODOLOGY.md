# FIX486 Mac performance methodology

## Fixed identity

Record source SHA, bank manifest and aggregate hash, Cargo.lock, release binary, config, model,
tokenizer, PostgreSQL/Qdrant image digests, macOS, CPU/core count, RAM, free disk and power mode.
Run on AC power with a stable thermal state. A changed identity starts a new run.

## Corpora

| Tier | Purpose | Minimum shape |
|---|---|---|
| correctness | deterministic hierarchy | FIX486 1.0.0, both zones, all lifecycle traps |
| medium | realistic steady state | >=10k parents and both child granularities |
| stress | saturation/backpressure | >=100k searchable bindings plus Graph edges |

Large-parent documents are generated before the run and token-counted by the production tokenizer.
Generated corpus seed and hash are recorded.

## Protocol

1. Start clean isolated PostgreSQL/Qdrant volumes and apply migrations.
2. Ingest, complete outbox, verify point/binding parity and activate versions.
3. Warm model and retrieval with at least 100 unmeasured queries.
4. Run correctness at concurrency 1, then closed-loop 4/8/16 and open-loop target arrival rates.
5. Use a mixed query distribution covering direct child, dedup, zone negative, lifecycle, Graph,
   multi-intent and large parent.
6. Measure at least 15 minutes after warmup; run three repetitions per load point.
7. Stop increasing load at sustained admission rejection, swap growth, OOM, error-rate breach or
   p99 deadline breach.

## Measurements and assertions

- latency p50/p95/p99 and timeout/error/degraded rates by query class;
- CPU, RSS, peak RSS, swap, file descriptors, threads/tasks and semaphore saturation;
- PostgreSQL query count/request, hydration p95/p99, pool wait and statement timeout count;
- Qdrant dense/sparse latency, request/error count and point parity;
- Graph edges visited, Graph latency and Graph candidates/request;
- MMR fetch/compute latency, candidate count and fallback rate;
- token budget before/after, dropped contexts and final intent coverage;
- cross-zone/access/version/lifecycle violations, empty parent contexts and duplicate parents.

Acceptance thresholds are frozen before execution. Repeatability reports median plus maximum
relative deviation across three runs. Phase A defines methodology only; it does not claim load
certification.
