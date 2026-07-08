# AstraVector fix465 known hardening backlog

## Enrichment worker scope

`astravector-enrichment` remains out of production scope in `fix465`.

The binary source can remain in the repository as experimental/development code, but the production Docker image must not copy `/usr/local/bin/astravector-enrichment` until the worker has a real queue, persistence model, shutdown handling, metrics and tests.

Required before production scope:

- durable enrichment task source;
- scan/claim/complete/retry lifecycle;
- `CancellationToken` shutdown;
- metrics and alerts;
- Kubernetes Deployment/CronJob;
- functional tests.
