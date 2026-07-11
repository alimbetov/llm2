# graph-extra-006 root cause analysis

`tech-ttl-clean-002` is a real direct lexical/vector candidate. The controlled relation
`tech-ttl-clean-002 -> tech-recon-run-004` is semantically correct and is a one-hop relation;
the fixture does not require two-hop GraphRAG.

The candidate was admitted to the bounded graph seed pool after lexical-aware seed ranking,
and relation lookup returned `tech-recon-run-004`. The remaining failure occurred during
`GRAPH_DIRECT_MERGE`: the graph seed was a SUB chunk while the returned direct evidence was
its PARENT chunk. Chunk-ID-only provenance matching therefore failed, and global MMR removed
the lower-scored direct seed from the direct context budget.

The production fix carries `graph_seed_source_block_id` through graph expansion and reserves
a bounded direct provenance budget for relation-producing seeds, matching by chunk ID or
source block ID. Remaining direct slots still use ordinary MMR. No query, fixture, document,
or block ID is present in production conditions.

Run `wave10-1-graph-extra-006-004` passed 1/1 with direct block
`tech-ttl-clean-002`, related block `tech-recon-run-004`, graph hit rate 1.0, zero graph
timeouts/errors, and zero access-zone/access-level violations.
