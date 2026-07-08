# fix462 Smoke Load Test

The smoke load test is not a full capacity test. It verifies that RetrieveContext with HYBRID + GraphRAG + MMR survives moderate concurrency before the release is called `PRODUCTION CANDIDATE`.

## Run

```bash
ASTRA_VECTOR_SMOKE_RETRIEVE_ENDPOINT=http://127.0.0.1:50051 \
ASTRA_VECTOR_SMOKE_ACCESS_ZONE_ID=<zone-uuid> \
ASTRA_VECTOR_SMOKE_QUERY="production candidate retrieve context smoke" \
ASTRA_VECTOR_SMOKE_CONCURRENCY=50 \
cargo test --features integration-tests --test smoke_load_retrieve_context -- --ignored --nocapture
```

## Criteria

- concurrency 50 must pass in CI/staging;
- concurrency 100 may run as nightly/manual;
- success rate must be at least 99%;
- no semaphore leak or timeout storm.
