# fix486c bank freeze contract

## Purpose

Define the immutable identity and mutation rules for `fix486-hierarchical-bank` version `1.0.0`.

## Frozen payload

Exactly five payload files are covered by the bank identity:

1. `corpus/hierarchical-fixture-v1.json`
2. `queries/hierarchical-queries-v1.jsonl`
3. `qrels/hierarchical-qrels-v1.jsonl`
4. `graph-relations/hierarchical-graph-v1.json`
5. `lifecycle/hierarchical-lifecycle-v1.json`

`bank-manifest.json` records, but is not part of, the aggregate hash.

## Version transition

Allowed transition:

```text
0.1.0-analysis-seed / NOT_FROZEN
→
1.0.0 / FROZEN
```

The transition is permitted only after structural, canonical-byte and hash verification pass.

## Immutability rules

After version `1.0.0` is frozen:

- payload bytes must not be edited in place;
- query wording must not be changed in place;
- qrel expectations must not be changed in place;
- logical IDs must not be reassigned;
- files must not be added to or removed from the frozen payload;
- formatting-only changes are still identity changes and are forbidden;
- generated runtime IDs must never be written into the bank;
- fixes to runtime behavior must not modify bank `1.0.0`;
- any intentional payload change requires a new bank version and migration note.

## Canonical byte rules

All payload files must:

- use UTF-8 without BOM;
- use LF line endings;
- end with exactly one LF;
- contain no NUL bytes;
- contain no trailing whitespace;
- parse according to their JSON or JSONL format;
- preserve array order and JSONL row order as committed.

The verifier must not rewrite files during verification.

## Hash rules

Per-file SHA-256 is calculated over exact bytes.

Aggregate input order:

```text
corpus/hierarchical-fixture-v1.json
queries/hierarchical-queries-v1.jsonl
qrels/hierarchical-qrels-v1.jsonl
graph-relations/hierarchical-graph-v1.json
lifecycle/hierarchical-lifecycle-v1.json
```

Aggregate stream line format:

```text
<relative-path>\t<lowercase-sha256>\n
```

The aggregate SHA-256 is calculated over the concatenated UTF-8 stream.

## Fail-closed behavior

Verification must fail when:

- any manifest hash is null or malformed;
- any payload hash differs;
- aggregate hash differs;
- any payload file is missing;
- any unlisted file exists below the frozen bank root, except `bank-manifest.json`;
- JSON/JSONL parsing fails;
- canonical byte rules fail;
- query, qrel or logical identity contracts fail;
- bank version/status is not exactly `1.0.0 / FROZEN`.

## Change policy

A required post-freeze change creates a new version, for example:

```text
1.0.1 — correction that preserves intended case semantics
1.1.0 — additive case/query capability
2.0.0 — incompatible schema or evaluation semantics
```

No version may reuse the aggregate hash of a semantically different payload.
