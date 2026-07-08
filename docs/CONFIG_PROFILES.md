# AstraVector configuration profiles

AstraVector uses a Spring-like profile layout:

```text
config/application.yaml       # base configuration
config/application-dev.yaml   # local development overlay
config/application-test.yaml  # automated/integration test overlay
config/application-prod.yaml  # production overlay
```

Loading order:

```text
1. ASTRAVECTOR_CONFIG, default config/application.yaml
2. profile overlay: application-{profile}.yaml
3. environment expansion inside YAML values
```

Active profile resolution:

```text
ASTRAVECTOR_PROFILE takes precedence.
If missing, ASTRAVECTOR_ENV is used.
Default profile is dev.
```

Aliases:

```text
development/local -> dev
testing/tests     -> test
production        -> prod
```

Examples:

```bash
ASTRAVECTOR_ENV=dev cargo run
ASTRAVECTOR_ENV=test cargo test
ASTRAVECTOR_ENV=prod ./astravector-runtime
```

Kubernetes should set:

```yaml
env:
  - name: ASTRAVECTOR_ENV
    value: production
```

Production secrets must be provided through Kubernetes Secret, Vault, or another secret manager:

```text
ASTRAVECTOR_DB_URL
ASTRAVECTOR_API_KEY
ASTRAVECTOR_QDRANT_URL
ASTRAVECTOR_QDRANT_API_KEY
ASTRAVECTOR_MODEL_SHA256
ASTRAVECTOR_TOKENIZER_SHA256
```

`ASTRAVECTOR_CONFIG` can still point to a complete custom config file. If that file is not named `application.yaml`, no automatic profile overlay is applied unless `ASTRAVECTOR_PROFILE_CONFIG` is explicitly set.
