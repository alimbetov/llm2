# Build verification

## Source provenance

- Base archive: `AstraVector_v001.zip`
- Verified base SHA-256: `1a0fe1cc628973ebdcbec5edefc6a5438e0075863061658f455517e837e3e7af`
- Target: `AstraVector_v002`

## Checks performed during generation

- Base archive SHA-256 verification: passed.
- YAML configuration parse: passed.
- Required migrations and project files: present.
- Search for deterministic production fallback / TODO / unimplemented macros: no production deterministic fallback found.
- Static delimiter smoke-check: performed; the only apparent brace mismatch is caused by brace literals in environment expansion format strings.

## Checks not executable in generation environment

The environment did not contain `cargo`, `rustc`, `rustfmt`, `clippy`, Docker, the BGE-M3 tokenizer/model artifacts, PostgreSQL, CUDA, or TensorRT. Therefore the following must run in CI/target infrastructure:

```bash
cargo fmt --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

Runtime acceptance also requires:

- PostgreSQL migration integration test;
- real dense-only and dense+sparse ONNX tests;
- official Python BGE-M3 parity tests;
- multi-pod claim/lease/fencing test;
- load and failure tests.
