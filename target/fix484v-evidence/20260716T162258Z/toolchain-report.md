# fix484v Toolchain Report

- Locked dependency graph minimum supported Rust version: 1.88.
- `Cargo.toml` `rust-version`: 1.88.
- GitHub Actions toolchain: 1.88.0.
- Docker builder: `rust:1.88-trixie`.
- Host verification toolchain: Rust/Cargo 1.96.0.
- All Cargo verification commands use `--locked`.
- Docker release build under Rust 1.88: PASS.

Rust 1.88 was selected instead of the host's 1.96 because it is the minimum version
required by the locked dependency graph. This keeps package metadata, CI, and Docker
aligned without raising the project MSRV unnecessarily.
