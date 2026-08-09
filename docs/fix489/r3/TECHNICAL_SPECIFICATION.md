# FIX489-R3 Local Stable Floor Discovery

## Scope

FIX489-R3 measures the local Mac CPU stable concurrency floor for the existing
FIX489 mixed-load runtime workload.

It is a hardware-aware local proof only:

```text
capacity_scope=LOCAL_MAC_CPU
production_capacity_claim=false
```

It does not replace the historical FIX489 full-capacity campaign and does not
change production retrieval, ranking, Graph, MMR, no-answer, lifecycle, access
control, model, tokenizer or timeout semantics.

## Campaign Mode

R3 uses an explicit opt-in mode:

```text
FIX489_CAMPAIGN_MODE=LOCAL_STABLE_FLOOR_DISCOVERY
```

The default remains:

```text
FULL_LOCAL_CAPACITY
```

Historical full-capacity levels remain:

```text
5,10,15,20,25,50
```

R3 discovery levels are:

```text
1,2,3,4
```

Each R3 level uses:

```text
runtime_warmup_seconds=30
load_warmup_seconds=60
measurement_seconds=300
cooldown_max_seconds=300
```

## Workload

R3 reuses the existing FIX489 live client and operation mix:

```text
25% SEARCH
35% RETRIEVE_CONTEXT
10% GRAPH_RETRIEVE_CONTEXT
15% INGEST_VERSION
5% DELETE_OR_EXPIRE
5% SYNC_STATUS
5% LIFECYCLE_STATUS
```

The load mode remains closed-loop. Concurrency N means no more than N
simultaneous production operations.

## Verdicts

Discovery success:

```text
FIX489_R3_LOCAL_STABLE_FLOOR_PASS
```

Soak success:

```text
FIX489_R3_SOAK_60M_PASS
```

The correct final language is local operational evidence, not production
capacity proof.
