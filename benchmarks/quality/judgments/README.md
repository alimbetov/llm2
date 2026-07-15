# Quality Judgment Workflow

`candidate-pools/` contains the auditable union of candidates from dense, sparse,
PostgreSQL FTS, hybrid, graph, and final ranking stages. `blind-judgments/` hides
rank, retrieval source, and document identity. `adjudicated/` is generated only
after every blind candidate has a relevance value from 0 through 3.

`scripts/fix481_judgment_pool.py` fails closed on missing traces, truncated traces,
unknown candidate text, incomplete judgments, identity mismatch, or insufficient
pool-source coverage. Runtime validation consumes only manifests whose status is
`ADJUDICATED`; a prepared template is not accepted as qrels.
