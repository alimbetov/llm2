# GraphRAG Multi-zone Keys

GraphRAG and direct retrieval must use compound identities:

- parent context key: `(access_zone_id, parent_id)`;
- matched chunk key: `(access_zone_id, chunk_id)`;
- related chunk key: `(access_zone_id, chunk_id)`;
- seed expansion key: `(seed_access_zone_id, seed_chunk_id)`.

`RelatedChunk` carries both `access_zone_id` and `seed_access_zone_id` so final context assembly never guesses the zone by scanning requested zones.
