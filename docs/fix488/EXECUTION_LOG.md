# FIX488 Execution Log

Status: PASS

Run:

```text
run_id: fix488-20260804-075749
evidence_path: /Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix488/fix488-20260804-075749
command: make local-demo-e2e
verdict: FIX488_LOCAL_END_TO_END_BOOK_PASS
```

Model identity:

```text
model_path: /Users/ruslanalimbetov/Documents/llm2/models/bge-m3/onnx/model.onnx
model_sha256: f84251230831afb359ab26d9fd37d5936d4d9bb5d1d5410e66442f630f24435b
tokenizer_path: /Users/ruslanalimbetov/Documents/llm2/models/bge-m3/tokenizer.json
tokenizer_sha256: 21106b6d7dab2952c1d496fb21d5dc9db75c28ed361a05f5020bbba27810dd08
```

Source identity:

```text
source_path: /Users/ruslanalimbetov/Documents/llm2/astravector/examples/local-demo/sample-ru.txt
source_sha256: 3c2ccbd4a5d81c76e25e5622667ef1d2dc20d69ef14fbdbc4334aae79f0edc01
access_zone_code: 0488
access_zone_id: b4ec78f9-70c3-5264-8b75-1b85f1905e44
document_id: 175f2d7c-a5b8-573b-903f-f64eaaea903c
document_version: 1
logical_blocks: 7
```

Runtime proof:

```text
grpc_reflection_pass: true
document_registered: true
chunks_created: 22
bindings_created: 21
outbox_created: 21
outbox_completed: 21
document_activated: true
semantic_search_results: 1
exact_anchor_search_results: 1
cross_zone_leakage_count: 0
wrong_version_count: 0
inactive_document_result_count: 0
```

PostgreSQL audit:

```text
document_versions: ACTIVE / ACTIVE
content_chunks_v004: 22
SOURCE: 1
PARENT: 7
SUB_180: 7
SUB_260: 7
vector_bindings_v004: 21
SYNCED bindings: 21
vector_outbox COMPLETED: 21
```

Qdrant audit:

```text
collection: astravector_local_demo
collection_found: true
points_count: 21
document_point_found: true
scroll_payload_sample_count: 5
payload includes access_zone_id, binding_id, document_id, document_version, chunk_id, chunk_granularity, lifecycle_status, payload_version, model_version and tokenizer_version
```

Evidence manifest:

```text
status: PASS
missing: []
bad_markers: []
manifest_file: /Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix488/fix488-20260804-075749/evidence-manifest.json
local_e2e_result_sha256: 425ca0475dff98f1f0099bac88b535657724c55253c1e72c9faa04518ef1368d
postgres_audit_sha256: 745cfb3ecdf0ff5f4efe0d831b3d50654db25a5526af05f4b530a2741a0b0635
qdrant_audit_sha256: e8c4f9ed98921447e79bf56f521e14e3b52c8f028ccfedfc5e32e368ebd2450d
```
