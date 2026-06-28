# Known limitations v004

1. The v004 control-plane protobuf is defined as a separate compatibility service, but its full Tonic service implementation is not yet wired into `main.rs`.
2. The legacy v1 runtime path remains available and still uses legacy request fields until cutover is completed.
3. The migration helper requires an explicit `(tenant_id, workspace_id) -> access_zone_id` mapping.
4. Full reconciliation scans, Qdrant scroll/orphan discovery and scheduled repair loops are not yet complete; a per-binding repair primitive is present.
5. External/local LLM enrichment providers are not implemented; disabled provider and rule-based validation are present.
6. Cross-encoder reranker and NLI consistency are not implemented.
7. No `Cargo.lock` could be generated because Rust tooling is unavailable in this environment.
8. Compilation and integration with PostgreSQL/Qdrant/ONNX remain mandatory before deployment.
