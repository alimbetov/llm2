# Smoke Tests

## Purpose

Описать smoke profiles, отдельные tests и правила интерпретации.

## Audience

QA, разработчики, операторы, аналитики.

## Short Summary

Smoke tests являются доказательным контуром. PASS ставится только при фактическом assertion. BLOCKED не является PASS.

## Required Profile

```bash
./smoke-tests/v004/scripts/run-full-smoke.sh --profile reliability-closing --keep-running
```

Current documented result:

```text
PASS 9 / FAIL 0 / BLOCKED 0 / SKIPPED 0
```

## Individual Tests

```bash
./smoke-tests/v004/scripts/run-full-smoke.sh --only atomicity-failpoints --keep-running
./smoke-tests/v004/scripts/run-full-smoke.sh --only outbox-fencing --keep-running
./smoke-tests/v004/scripts/run-full-smoke.sh --only dead-letter-qdrant-failure --keep-running
./smoke-tests/v004/scripts/run-full-smoke.sh --only bm25-hybrid-retrieval --keep-running
```

## Current Results

```text
reliability-closing: PASS 9 / FAIL 0 / BLOCKED 0 / SKIPPED 0
bm25-hybrid-retrieval: BM25_RETRIEVAL_BLOCKED
```

## reliability-closing

Includes:

- build
- migrations
- full-power-wave1
- access-security
- consistency
- atomicity-failpoints
- outbox-fencing
- dead-letter-qdrant-failure
- data-integrity-audit

## atomicity-failpoints

Проверяет controlled failures до/после критических persistence steps.

Критичные признаки:

- no partial committed state;
- retry completes;
- duplicate chunks отсутствуют;
- synced bindings/completed outbox/Qdrant points восстановлены после retry;
- after_commit_before_response не создает дубли при повторе.

Report:

```bash
cat smoke-tests/v004/reports/ATOMICITY_FAILPOINTS_REPORT.md
```

## outbox-fencing

Проверяет stale worker completion protection через `locked_by + lock_generation`.

Expected evidence:

```text
stale_rows=0
current_rows=1
```

## dead-letter-qdrant-failure

Проверяет:

- Qdrant always-fail приводит к `DEAD_LETTER`;
- false `SYNCED` не появляется;
- transient failure восстанавливается до `COMPLETED/SYNCED/Qdrant point`.

## bm25-hybrid-retrieval

Сейчас ожидаемый verdict:

```text
BM25_RETRIEVAL_BLOCKED
```

Причина: production BM25/sparse/hybrid retrieval path не реализован в Search API. Это корректный результат и не должен трактоваться как PASS.

The current Search smoke proves dense retrieval with mandatory access-zone, caller access-level, lifecycle, active-document-version, expiry, and searchable-granularity constraints. It does not prove arbitrary `SearchRequestV004.filters`; that proto field is reserved for future extension.

## PASS / FAIL / BLOCKED / SKIPPED

- `PASS`: assertion выполнен.
- `FAIL`: assertion не выполнен.
- `BLOCKED`: тест существует, но production capability отсутствует.
- `SKIPPED`: тест не выполнялся.

## Common Mistakes

- Считать `BM25_RETRIEVAL_BLOCKED` успешным retrieval quality proof.
- Запускать `--strict` и удивляться, что BLOCKED останавливает профиль.
- Читать только summary, не открывая evidence JSONL.
- Трактовать наличие `SearchRequestV004.filters` в proto как доказательство реализованной фильтрации по произвольным caller filters.
