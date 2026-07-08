# AstraVector Grafana dashboards

Dashboards are stored in:

```text
observability/grafana/
```

## Dashboards

- `astravector-overview.json` — service-level health and backlog overview.
- `astravector-retrieval.json` — dense/sparse/hybrid/GraphRAG/MMR retrieval indicators.
- `astravector-consistency.json` — outbox and reconciliation consistency indicators.
- `astravector-ttl.json` — TTL deletion and backlog indicators.
- `astravector-runtime.json` — runtime pressure and retention indicators.

## Import

Import these JSON files into Grafana through **Dashboards → New → Import** or provision them through a ConfigMap in Kubernetes.

## Validation

`tests/fix465_p2_hardening_contracts.rs` validates that dashboards are syntactically valid JSON and reference AstraVector metrics.
