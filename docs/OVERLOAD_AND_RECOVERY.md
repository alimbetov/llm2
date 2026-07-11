# Overload And Recovery

## Error Semantics

`RESOURCE_EXHAUSTED` is intentional load shedding: admission timeout, full query queue, or excessive queue age. AstraVector does not retry overload internally.

`DEADLINE_EXCEEDED` means the caller deadline expired or remaining inference budget was insufficient. `UNAVAILABLE` means the scheduler or a required downstream is unavailable; canonical PostgreSQL visibility validation is never bypassed.

## Tuning

Tune retrieval concurrency first, then query queue capacity. A larger queue increases stale backlog and is not a capacity improvement. Graph and MMR use separate permits and may degrade to direct retrieval only when partial fallback is enabled.

`application-load-m2.yaml` is local Apple M2 evidence, not a production capacity claim.

## Retry Amplification

Online query timeout is not retried. `UNAVAILABLE` retries only when delay, operation budget, and safety margin fit inside the deadline. Document ingestion retains a longer retry policy.

## Time To Recovery

The gate runs 10-second windows after spike. Recovery is the end of the third consecutive healthy window and must occur within 60 seconds. A five-minute stabilized run must then pass.

## Failed Gate Actions

Keep `NOT_PRODUCTION_READY`; inspect admission, queue age/depth, deadline, and retry metrics; do not weaken 97/97 quality, GraphRAG 13/13, isolation, or hard-negative gates.
