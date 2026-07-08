# fix462 Known Hardening Backlog

These items are intentionally outside the P0/P1 production-candidate closure scope.

## Load and chaos

- Full load test beyond smoke concurrency 50/100.
- Chaos tests for PostgreSQL, Qdrant, and network partitions.
- Soak test for TTL cleanup and outbox over 24+ hours.

## Architecture

- Full delete-outbox redesign for Qdrant delete operations.
- Formal admin force-unlock API for stale `delete_operation_id` repair.
- Dashboard pack for Prometheus/Grafana.

## Test expansion

- More realistic GraphRAG datasets with multiple edge types.
- Large-document semantic graph memory/capacity tests.
- End-to-end tests with gateway/auth integration.
