# FIX488 Result

Verdict:

```text
FIX488_LOCAL_END_TO_END_BOOK_PASS
```

The local end-to-end book and scripts were verified with a real local AstraVector runtime, PostgreSQL, Qdrant and the BGE-M3 ONNX model. The run used production gRPC ingestion, waited for vector publication, activated the document version and executed dense search against Qdrant-backed points.

Canonical evidence:

```text
/Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix488/fix488-20260804-075749
```

Key proof counters:

```text
runtime_started: true
grpc_reflection_pass: true
document_registered: true
document_activated: true
chunks_created: 22
bindings_created: 21
outbox_completed: 21
postgres_chunk_count: 22
postgres_binding_count: 21
qdrant_collection_found: true
qdrant_document_point_found: true
semantic_search_results: 1
exact_anchor_search_results: 1
cross_zone_leakage_count: 0
wrong_version_count: 0
inactive_document_result_count: 0
```

Returned document:

```text
access_zone_id: b4ec78f9-70c3-5264-8b75-1b85f1905e44
document_id: 175f2d7c-a5b8-573b-903f-f64eaaea903c
document_version: 1
```

The semantic query returned original PostgreSQL parent text about PostgreSQL as the canonical state store. The exact-anchor query returned the original parent text `ASTRAVECTOR_LOCAL_DEMO_2026`.

Static validation completed for the committed helper surface:

```text
python3 -m py_compile scripts/local-demo/local_demo.py: PASS
python3 -m unittest tests/test_fix488_local_scripts.py tests/test_fix488_local_request_builders.py tests/test_fix488_local_evidence.py: PASS
make verify-fix488-local-demo: PASS
cargo fmt --all --check: PASS
cargo check --locked --all-targets --all-features: PASS
cargo clippy --locked --all-targets --all-features -- -D warnings: PASS
cargo test --locked --all-targets --all-features: PASS
make local-demo-e2e: PASS
```
