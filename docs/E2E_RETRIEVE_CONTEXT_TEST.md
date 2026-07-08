# fix462 RetrieveContext Network E2E

`test_e2e_retrieve_context_full_rag_lifecycle_over_tonic_network` is the release-blocking fix462 E2E.

## What it proves

The test must start real dependencies and call the generated tonic client over a local network socket:

1. PostgreSQL testcontainer;
2. Qdrant testcontainer;
3. all SQL migrations;
4. Qdrant collection creation;
5. production-like indexing/persistence + outbox publish;
6. tonic `AstraVectorRetrievalFacadeServer` on a random local port;
7. generated `AstraVectorRetrievalFacadeClient`;
8. network `retrieve_context` before TTL;
9. TTL cleanup;
10. network `retrieve_context` after TTL returns zero contexts.

## Forbidden replacement

The test must not replace the network proof with direct calls such as:

```rust
repo.persist(...)
qdrant_client.search_dense(...)
repo.fetch_chunk_texts_by_ids_multi(...)
trait.retrieve_context(...)
```

Those calls are allowed only as secondary assertions.

## Run

```bash
cargo test --features integration-tests --test e2e_testcontainers -- --nocapture
```
