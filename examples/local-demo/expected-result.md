# Expected Local Demo Result

Successful `make local-demo-e2e` produces real PostgreSQL rows, Qdrant points and search responses for `examples/local-demo/sample-ru.txt`.

The semantic response must return the loaded document and a `parentText` containing the PostgreSQL/Qdrant/outbox explanation from the sample text.

The exact response must return the loaded document for the anchor `ASTRAVECTOR_LOCAL_DEMO_2026`.

