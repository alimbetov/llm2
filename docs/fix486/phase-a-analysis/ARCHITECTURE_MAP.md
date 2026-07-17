# FIX486A production architecture map

## Entry points

| API | Production implementation | Relationship |
|---|---|---|
| `Search` | `src/grpc/mod.rs::AstraVectorV004Control::search` | Authoritative retrieval pipeline |
| `RetrieveContext` | `src/grpc/mod.rs::AstraVectorRetrievalFacade::retrieve_context` | Maps facade request to `Search`, then maps `SearchResultV004` to contexts |
| `ExplainSearch` | `src/grpc/mod.rs::AstraVectorV004Control::explain_search` | Separate candidate-only diagnostic path |
| `ExplainRetrieve` | `src/grpc/mod.rs::AstraVectorRetrievalFacade::explain_retrieve` | Maps to `ExplainSearch`; it does not execute final Search selection |

`RetrieveContext` and `Search` therefore share runtime behavior. Explain does not currently prove
parent hydration, Graph expansion, MMR, token-budget truncation, final visibility, or final coverage.
It is diagnostic candidate visibility, not final-result parity.

## Ingestion and identity

`CreateMultiGranularityChunks` calls `ChunkingEngine::chunk` or `chunk_segments`, creates
`SOURCE -> PARENT -> SUB_180/SUB_260`, runs inference, and persists chunks, embeddings,
bindings, and outbox in the production repository transaction.

Chunk IDs are UUIDv5 over:

```text
access_zone_id : document_id : document_version : granularity/root : sequence :
SHA-256(normalized content) : chunking profile
```

Binding UUIDv5 includes the zone-scoped chunk/cache representation identity. Qdrant point UUIDv5
includes `access_zone_id` and `binding_id`. Equal logical labels in two zones are feasible without
assuming equal physical UUIDs.

Canonical PostgreSQL keys used by retrieval are composite:

```text
(access_zone_id, chunk_id)
(access_zone_id, document_id, document_version)
(access_zone_id, parent_chunk_id)
```

Qdrant is a projection. Search payload requires `access_zone_id`, `chunk_id`, granularity and
parent lineage; PostgreSQL remains authoritative.

## Direct retrieval

```text
Search request validation
-> canonical query plan using the loaded engine tokenizer
-> dense/sparse query embedding
-> Qdrant dense/sparse searches with canonical filter
-> branch failure/degradation handling
-> global fusion
-> parent grouping
-> one PostgreSQL batch hydration
-> direct SearchResultV004 candidates
```

The Qdrant filter in `src/qdrant/mod.rs::canonical_search_filter` applies zone, caller access
level, `ACTIVE` lifecycle, expiry, searchable granularities, quarantine, and optional version
identities. PostgreSQL hydration in
`Repository::fetch_hydrated_search_contexts_multi` uses one `unnest` query with ordinality and
rechecks matched child, parent, document/version, zone, access level, lifecycle, deletion and TTL.

Parent grouping key is `(access_zone_id, parent_chunk_id)`. PARENT hits use their own `chunk_id`;
SUB hits use payload `parent_chunk_id`. Input hits are already rank ordered, so the first hit for a
group is the winning child. Hydration preserves `matched_text` from that exact child separately
from canonical `parent_text`.

Missing rows are dropped. The aggregate metric
`astravector_parent_hydration_missing_total` records the count, but the ranking trace does not yet
record a per-candidate `HYDRATION_MISSING` reason.

## Graph path

```text
direct SearchResult candidates
-> graph seed selection keyed by (zone, child chunk)
-> Repository::expand_chunks_1hop_by_seed_keys
-> active edge and related-child filtering
-> batch fetch_contexts_for_graph_related_chunks_multi
-> related child resolves its own canonical PARENT
-> graph scoring/provenance
-> direct/Graph merge and dedup
```

The edge SQL is zone-scoped and checks active/non-quarantined/non-expired nodes, edges, related
chunks, and active document versions. The repaired graph hydration SQL requires an active,
non-deleted, non-expired `PARENT` in the same zone/document/version. It no longer substitutes a
child for a missing parent.

Graph result identity remains the related child. `parent_text` belongs to that child's parent;
metadata records seed zone/chunk, relation, hop, scores, source block and retrieval provenance.
Final dedup uses zone-aware result identity and merges secondary provenance.

## Final selection order

```text
candidate-intent evidence
-> direct required-segment reservation
-> Graph merge
-> strategy-aware MMR (one invocation)
-> post-MMR no-answer
-> hard token budget
-> final PostgreSQL visibility recheck
-> final required-intent coverage recomputation
-> response
```

Coverage is recomputed after direct retrieval, MMR, token budget, and final visibility. If final
segmented coverage is insufficient, results are cleared; degraded coverage is explicitly warned.
Ranking protection influences which candidate token truncation drops, but cannot bypass the hard
budget. A very large parent can still consume budget before smaller unique-aspect parents; the
bank must measure this in FIX486-10 rather than assume protection is optimal.

## Isolation and lifecycle matrix

| Stage | Zone | Access | Version/document | Chunk lifecycle/TTL | Notes |
|---|---|---|---|---|---|
| Dense/Sparse Qdrant | yes | yes | payload/version filters when requested | ACTIVE, expiry, quarantine | projection prefilter |
| PostgreSQL FTS | yes | yes | document ACTIVE | parent ACTIVE, deletion, expiry | PARENT/ORIGINAL only |
| Parent hydration | composite key | matched + parent | same document/version, document ACTIVE | matched + parent ACTIVE, deletion, expiry | one batch query |
| Graph edge expansion | composite seed | related child | document ACTIVE | nodes/edges/chunk ACTIVE, expiry, quarantine | one-hop only |
| Graph parent hydration | composite key | child + parent | same document/version, document ACTIVE | child + PARENT ACTIVE, deletion, expiry | fail closed after fix |
| MMR/token budget | inherited identity | no new authorization | no storage read | no storage read | destructive ranking stages |
| Final visibility | composite key | yes | document ACTIVE | matched chunk ACTIVE, deletion, expiry | after token budget |
| Explain | Qdrant filter only | yes | optional payload filters | Qdrant lifecycle filter | not final Search parity |

## Batching and resource shape

- Direct parent hydration is one SQL query for all candidate triples; no N+1 parent fetch.
- Graph expansion is one SQL query and Graph context hydration is one SQL query.
- MMR dense embeddings are fetched in a zone-scoped batch.
- Final visibility is one zone/chunk batch query.
- Query deadline, cancellation tokens, stage budgets and semaphores cover Qdrant, Graph and MMR.
- Parent hydration uses PostgreSQL `statement_timeout`; a total timeout returns an RPC error, not
  a successful empty/no-answer response.
