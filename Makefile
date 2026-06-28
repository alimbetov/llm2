.PHONY: fmt check test clippy release migrate run db-up db-down
fmt:
	cargo fmt --check
check:
	cargo check --all-targets
clippy:
	cargo clippy --all-targets --all-features -- -D warnings
test:
	cargo test --all-targets
release:
	cargo build --release
migrate:
	cargo run -- migrate
run:
	cargo run
db-up:
	docker compose up -d postgres
db-down:
	docker compose down
