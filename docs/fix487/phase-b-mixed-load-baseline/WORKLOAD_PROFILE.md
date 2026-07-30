# FIX487B Workload Profile

## Dataset

```text
version = fix487b-dataset-v1
seed = 487205
documents = 60
zones = 4871, 4872, 4873
access levels = PUBLIC, INTERNAL, CONFIDENTIAL, RESTRICTED
languages = EN, RU, KZ
```

The dataset is synthetic and never modifies FIX486 or quality benchmark banks.

## Operation Cycle

The deterministic 100-operation cycle is:

```text
Search                         25
RetrieveContext                35
Graph-enabled RetrieveContext  10
Ingestion/new version          15
Delete/expiry                   5
Sync/outbox status              5
Lifecycle/recovery status       5
```

The runner uses a bounded worker pool. For the pilot:

```text
workers = 5
queue maxsize = workers * 2
max attempts = 2
```

Retryable statuses:

```text
UNAVAILABLE
RESOURCE_EXHAUSTED
DEADLINE_EXCEEDED
```

Non-retryable statuses:

```text
INVALID_ARGUMENT
FAILED_PRECONDITION
PERMISSION_DENIED
UNAUTHENTICATED
INTERNAL
UNKNOWN
```
