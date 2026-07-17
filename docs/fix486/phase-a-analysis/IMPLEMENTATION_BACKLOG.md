# FIX486 implementation backlog

Codex must populate this file from the analysis. Do not implement non-blocking items in fix486a.

## Backlog item schema

```text
ID=
TITLE=
CATEGORY=
SEVERITY=
ROOT_CAUSE=
AFFECTED_FILES=
AFFECTED_PRODUCTION_PATH=
REQUIRED_PRODUCTION_CHANGE=
REQUIRED_REGRESSION_TEST=
REQUIRED_RUNTIME_SMOKE=
REQUIRED_FIXTURE=
REQUIRED_METRIC_OR_TRACE=
BACKWARD_COMPATIBILITY_IMPACT=
TARGET_PHASE=
BLOCKING=
```

## Categories

```text
PRODUCTION_DEFECT
TESTABILITY_GAP
OBSERVABILITY_GAP
FIXTURE_GAP
FAILPOINT_GAP
PERFORMANCE_GAP
DOCUMENTATION_GAP
CI_GATE_GAP
```

## Initial phase mapping

| Target phase | Planned scope |
|---|---|
| fix486b | reproducible baseline and environment identity |
| fix486c | frozen hierarchical bank 1.0.0 and ingestion fixture tooling |
| fix486d | child/parent correctness, evidence preservation and dedup |
| fix486e | isolation, active version, TTL, deletion and orphan defense |
| fix486f | hydration failpoints and degradation semantics |
| fix486g | GraphRAG child-to-parent and provenance |
| fix486h | MMR, token budget and multi-intent protection |
| fix486i | MacBook load, N+1 and resource stability |
| fix486j | aggregate evidence and final verdict |

## Discovered items

TBD by Codex.