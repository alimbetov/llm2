# Build And Run

## Purpose

Показать реальные команды сборки и реальные binary names.

## Audience

Разработчики, DevOps.

## Short Summary

Не придумывайте binary names. Получайте их из `cargo metadata`.

## Verification Commands

```bash
cargo fmt --check
cargo check --all-targets --all-features
cargo test --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release --locked
```

## Actual Binary Names

Команда:

```bash
cargo metadata --no-deps --format-version=1 | jq '.packages[].targets[] | select(.kind[]=="bin") | .name'
```

Фактические binary names:

```text
astravector-enrichment  # experimental/dev-only; not copied to production Docker image in fix465
astravector-lifecycle
astravector-qdrant-publisher
astravector-reconciliation
astravector-runtime
```

## Run Template

```bash
DATABASE_URL='postgres://astravector:astravector_smoke_password@127.0.0.1:55432/astravector_smoke' \
ASTRAVECTOR_DB_URL='postgres://astravector:astravector_smoke_password@127.0.0.1:55432/astravector_smoke' \
QDRANT_HTTP_URL='http://127.0.0.1:56333' \
QDRANT_COLLECTION='astravector_smoke_v004' \
ASTRAVECTOR_QDRANT_URL='http://127.0.0.1:56333' \
ASTRAVECTOR_QDRANT_COLLECTION='astravector_smoke_v004' \
./target/release/astravector-runtime
```

For smoke config:

```bash
ASTRAVECTOR_CONFIG='smoke-tests/v004/config/application-smoke.yaml' \
./target/debug/astravector-runtime
```

## Expected Results

`astravector-runtime` должен поднять gRPC service на configured address. Проверка:

```bash
grpcurl -plaintext 127.0.0.1:55051 list
```

## Common Mistakes

- Запускать несуществующий binary.
- Использовать только `DATABASE_URL` без `ASTRAVECTOR_DB_URL`.
- Запускать release binary до применения migrations.
