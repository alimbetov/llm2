# Local Development

## Purpose

Помочь разработчику собрать, проверить и запустить `AstraVector_v004` локально.

## Audience

Rust developers, backend developers, QA engineers.

## Short Summary

Нужны Rust/Cargo, Docker, PostgreSQL/Qdrant smoke infra, `psql`, `curl`, `jq`, `grpcurl`.

## Prerequisites

Проверить инструменты:

```bash
rustc --version
cargo --version
docker --version
docker compose version
psql --version
curl --version
jq --version
grpcurl --version
```

## Local Ports

| Component | Address |
|---|---|
| PostgreSQL | `127.0.0.1:55432` |
| Qdrant HTTP | `http://127.0.0.1:56333` |
| Qdrant gRPC | `127.0.0.1:56334` |
| AstraVector gRPC | `127.0.0.1:55051` |

## Typical Developer Workflow

```bash
cargo fmt --check
cargo check --all-targets --all-features
cargo test --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release --locked
```

## First Validation After Startup

Qdrant:

```bash
curl -s http://127.0.0.1:56333/health
```

PostgreSQL:

```bash
PGPASSWORD='astravector_smoke_password' \
psql -h 127.0.0.1 -p 55432 -U astravector -d astravector_smoke \
  -c "SELECT current_database(), current_user, now();"
```

gRPC reflection:

```bash
grpcurl -plaintext 127.0.0.1:55051 list
```

## Expected Results

- `cargo check --all-targets --all-features` должен завершиться без ошибок.
- Qdrant `/health` должен вернуть healthy response.
- PostgreSQL должен принять соединение к `astravector_smoke`.

## Common Mistakes

- Запускать smoke без загруженного `.env.smoke`.
- Считать, что `cargo test` заменяет smoke tests. Smoke проверяет PostgreSQL/Qdrant/gRPC контур.
- Оставлять smoke failpoints включенными после теста.
