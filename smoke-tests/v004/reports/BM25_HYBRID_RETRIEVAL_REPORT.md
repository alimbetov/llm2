# AstraVector_v004 BM25 / Sparse / Hybrid Retrieval Report

## Verdict
BM25_RETRIEVAL_BLOCKED

## Detection

| Check | Value |
|---|---:|
| SearchRequest retrieval mode | no |
| SearchResponse sparse/lexical scores | no |
| Query sparse embedding requested | no |
| Qdrant sparse/BM25 search method | no |
| Hybrid fusion path | no |
| PostgreSQL smoke DB check | available |
| embedding_sparse rows | 0 |
| ACTIVE Civil Code versions in Zone A | 9 |

## Dense Baseline

- dense_only passed: 0
- dense_only failed: 0

## BM25 / Hybrid

BM25/sparse/hybrid retrieval is blocked, not passed.

Blocked reasons:
- SearchRequestV004 has no retrieval/search mode field
- SearchResponseV004 has no sparse/lexical/BM25 score fields
- Search query embedding uses want_sparse=false
- Qdrant client has no sparse/BM25 search method
- No hybrid fusion path found in src
- embedding_sparse has no indexed rows

## Artifacts

- Results JSON: /Users/ruslanalimbetov/Documents/llm2/AstraVector_v004/smoke-tests/v004/reports/bm25-hybrid-results.json
- Candidates JSONL: /Users/ruslanalimbetov/Documents/llm2/AstraVector_v004/smoke-tests/v004/reports/bm25-hybrid-candidates.jsonl
