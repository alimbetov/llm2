# AstraVector_v004 RAG Retrieval Expert Report

## 1. Summary

- Verdict: RAG_CORE_E2E_CANDIDATE
- Questions total: 15
- Valid questions: 9
- Passed: 9
- Failed: 0
- Recall@10: 1

## 2. Search Pipeline

question -> validation -> ONNX query embedding -> Qdrant dense search -> parent grouping -> PostgreSQL batch parent fetch -> SearchResponse

## 3. Corpus Indexing State

| Metric | Value |
|---|---:|
| Active document versions | 9 |
| PARENT chunks | 347 |
| SUB_180 chunks | 650 |
| SUB_260 chunks | 583 |

## Expert Interpretation

PASS means expected original parent context was retrieved for valid smoke questions. It does not prove full legal-answer correctness, reranker quality, or production reliability.
