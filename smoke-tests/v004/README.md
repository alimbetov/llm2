# AstraVector v004 Smoke Tests

Run from the project root:

```bash
./smoke-tests/v004/scripts/run-full-smoke.sh
```

Useful variants:

```bash
./smoke-tests/v004/scripts/run-full-smoke.sh --keep-running
./smoke-tests/v004/scripts/run-full-smoke.sh --only corpus
./smoke-tests/v004/scripts/run-full-smoke.sh --skip-build
./smoke-tests/v004/scripts/run-full-smoke.sh --strict
```

The runner writes:

- per-step JSON: `smoke-tests/v004/results/*.json`
- summary JSON: `smoke-tests/v004/reports/smoke-results.json`
- markdown report: `smoke-tests/v004/reports/SMOKE_REPORT.md`
- JUnit XML: `smoke-tests/v004/reports/smoke-junit.xml`
- process logs: `smoke-tests/v004/logs/*.log`

Exit codes per scenario:

- `0`: PASS
- `1`: FAIL
- `2`: BLOCKED
- `3`: SKIPPED

The smoke suite is intentionally evidence-driven. A started process or open TCP port is not a PASS unless a scenario also checks the expected response, database state, or Qdrant state.
