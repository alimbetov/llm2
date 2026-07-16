# fix484v Architecture Audit

## Query planning

- Exact token boundaries 256/257/1024/1025/2048/2049 are contract-tested.
- Queries above the hard limit fail closed; no silent truncation is used.
- Physical segments preserve the tail, bounded overlap, and deterministic ordering.
- Logical intents are independent from physical overlap/tail segments.
- Coverage is evaluated by required logical intent, not by duplicated physical segments.
- RRF keeps the best contribution per candidate and logical intent, preventing overlap amplification.

## Retrieval stages

- Graph seed selection reserves representation across required logical intents.
- Graph seeds are deterministic, deduplicated, and capped by the selected immutable tier limit.
- Graph expansion has one production invocation site per request.
- Final MMR selection has one production invocation site per request.
- `tests/query_processing_contracts.rs` locks these invocation-site contracts.

## Runtime control

- Admission is weighted by the frozen query tier: 1/3/6 permits.
- RAII releases all permits on success, error, deadline, or cancellation.
- Transport and server deadlines are measured from RPC receipt and use the earlier deadline.
- Cancellation propagates into admission, query scheduling, Qdrant/FTS work, graph expansion, and MMR hydration.

## Compatibility

- Legacy long-query configuration maps to Standard-tier fields only when new keys are absent.
- New configuration takes precedence and legacy environment aliases emit deprecation warnings.
- Extended remains disabled by default in the production profile.
