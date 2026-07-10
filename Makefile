.PHONY: fmt check test test-e2e e2e-network sqlx-prepare smoke-load smoke-load-blocking quality-fixtures quality-quick quality-quick-remote quality-runtime-quick quality-runtime-quick-remote quality-runtime-dense-quick quality-runtime-dense-quick-remote quality-runtime-sparse-quick-remote quality-runtime-hybrid-quick-remote quality-runtime-graph-quick-remote quality-runtime-mmr-quick-remote quality-runtime-hard-negative-quick-remote quality-runtime-full-capability-quick-remote quality-runtime-confidence quality-runtime-confidence-remote quality-runtime-confidence-report quality-runtime-full quality-production-candidate verify-fix463 verify-fix465 verify-fix467 verify-fix468 quality-fixtures-enriched clippy release migrate run run-runtime-local db-up db-down
fmt:
	cargo fmt --check
check:
	cargo check --all-targets
clippy:
	cargo clippy --all-targets --all-features -- -D warnings
test:
	cargo test --all-targets --all-features

test-e2e:
	cargo test --features integration-tests --test e2e_testcontainers -- --nocapture
release:
	cargo build --release
migrate:
	cargo run -- migrate
run:
	cargo run
run-runtime-local:
	ASTRAVECTOR_DB_URL=$${ASTRAVECTOR_DB_URL:-postgres://astravector:astravector@127.0.0.1:55432/astravector} \
	DATABASE_URL=$${DATABASE_URL:-postgres://astravector:astravector@127.0.0.1:55432/astravector} \
	ASTRAVECTOR_QDRANT_URL=$${ASTRAVECTOR_QDRANT_URL:-http://127.0.0.1:6333} \
	ASTRAVECTOR_QDRANT_COLLECTION=$${ASTRAVECTOR_QDRANT_COLLECTION:-astravector_v004} \
	ASTRAVECTOR_MODEL_PATH=$${ASTRAVECTOR_MODEL_PATH:-/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/onnx/model.onnx} \
	ASTRAVECTOR_TOKENIZER_PATH=$${ASTRAVECTOR_TOKENIZER_PATH:-/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/tokenizer.json} \
	ASTRAVECTOR_GRAPH_TIMEOUT_MS=$${ASTRAVECTOR_GRAPH_TIMEOUT_MS:-500} \
	ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=$${ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION:-true} \
	ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_SEARCH=$${ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_SEARCH:-false} \
	cargo run --bin astravector-runtime
db-up:
	docker compose up -d postgres
db-down:
	docker compose down


sqlx-prepare:
	cargo sqlx prepare --check -- --all-targets --all-features

e2e-network: test-e2e

smoke-load:
	cargo test --features integration-tests --test smoke_load_retrieve_context -- --ignored --nocapture

smoke-load-blocking:
	cargo test --features integration-tests --test smoke_load_retrieve_context_testcontainers -- --nocapture

verify-fix463: fmt clippy check test sqlx-prepare test-e2e

verify-fix465: fmt clippy check test sqlx-prepare test-e2e smoke-load-blocking

quality-fixtures:
	cargo test --test quality_fixtures_contracts -- --nocapture

quality-fixtures-enriched:
	cargo test --test quality_fixtures_contracts -- --nocapture

quality-quick:
	cargo test --test quality_bench_quick -- --nocapture

quality-quick-remote:
	ASTRAVECTOR_QUALITY_ENDPOINT=http://localhost:50051 cargo test --test quality_bench_quick -- --nocapture

quality-runtime-quick:
	ASTRAVECTOR_QUALITY_RUNTIME_MODE=ingest-and-retrieve \
	cargo test --test quality_bench_runtime_quick -- --nocapture

quality-runtime-quick-remote:
	ASTRAVECTOR_QUALITY_ENDPOINT=http://localhost:50051 \
	ASTRAVECTOR_QUALITY_RUN_ID=$${ASTRAVECTOR_QUALITY_RUN_ID:-fix474i-quick-$$(date +%Y%m%d-%H%M%S)} \
	ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true \
	ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_SEARCH=false \
	ASTRAVECTOR_QUALITY_RUNTIME_MODE=ingest-and-retrieve \
	cargo test --test quality_bench_runtime_quick -- --nocapture

quality-runtime-dense-quick:
	ASTRAVECTOR_QUALITY_PROFILE=dense-only-quick \
	ASTRAVECTOR_QUALITY_RUNTIME_MODE=ingest-and-retrieve \
	cargo test --test quality_bench_runtime_quick -- --nocapture

quality-runtime-dense-quick-remote:
	ASTRAVECTOR_QUALITY_ENDPOINT=http://localhost:50051 \
	ASTRAVECTOR_QUALITY_RUN_ID=$${ASTRAVECTOR_QUALITY_RUN_ID:-fix474i-dense-$$(date +%Y%m%d-%H%M%S)} \
	ASTRAVECTOR_QUALITY_PROFILE=dense-only-quick \
	ASTRAVECTOR_QUALITY_REQUIRE_DENSE=true \
	ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true \
	ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_SEARCH=false \
	ASTRAVECTOR_QUALITY_RUNTIME_MODE=ingest-and-retrieve \
	cargo test --test quality_bench_runtime_quick -- --nocapture

quality-runtime-sparse-quick-remote:
	ASTRAVECTOR_QUALITY_ENDPOINT=http://localhost:50051 \
	ASTRAVECTOR_QUALITY_RUN_ID=$${ASTRAVECTOR_QUALITY_RUN_ID:-fix474i-sparse-$$(date +%Y%m%d-%H%M%S)} \
	ASTRAVECTOR_QUALITY_PROFILE=sparse-quick \
	ASTRAVECTOR_QUALITY_REQUIRE_DENSE=true \
	ASTRAVECTOR_QUALITY_REQUIRE_SPARSE=true \
	ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true \
	ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_SEARCH=false \
	ASTRAVECTOR_QUALITY_RUNTIME_MODE=ingest-and-retrieve \
	cargo test --test quality_bench_runtime_quick -- --nocapture

quality-runtime-hybrid-quick-remote:
	ASTRAVECTOR_QUALITY_ENDPOINT=http://localhost:50051 \
	ASTRAVECTOR_QUALITY_RUN_ID=$${ASTRAVECTOR_QUALITY_RUN_ID:-fix474i-hybrid-$$(date +%Y%m%d-%H%M%S)} \
	ASTRAVECTOR_QUALITY_PROFILE=hybrid-quick \
	ASTRAVECTOR_QUALITY_REQUIRE_DENSE=true \
	ASTRAVECTOR_QUALITY_REQUIRE_SPARSE=true \
	ASTRAVECTOR_QUALITY_REQUIRE_HYBRID=true \
	ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true \
	ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_SEARCH=false \
	ASTRAVECTOR_QUALITY_RUNTIME_MODE=ingest-and-retrieve \
	cargo test --test quality_bench_runtime_quick -- --nocapture

quality-runtime-graph-quick-remote:
	ASTRAVECTOR_QUALITY_ENDPOINT=http://localhost:50051 \
	ASTRAVECTOR_QUALITY_RUN_ID=$${ASTRAVECTOR_QUALITY_RUN_ID:-fix474i-graph-$$(date +%Y%m%d-%H%M%S)} \
	ASTRAVECTOR_QUALITY_PROFILE=graph-quick \
	ASTRAVECTOR_QUALITY_REQUIRE_DENSE=true \
	ASTRAVECTOR_QUALITY_REQUIRE_GRAPH=true \
	ASTRAVECTOR_GRAPH_MAX_RELATED_CHUNKS=8 \
	ASTRAVECTOR_GRAPH_CONTEXT_APPEND_LIMIT=5 \
	ASTRAVECTOR_GRAPH_EXPANSION_RESULT_LIMIT=12 \
	ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true \
	ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_SEARCH=false \
	ASTRAVECTOR_QUALITY_RUNTIME_MODE=ingest-and-retrieve \
	cargo test --test quality_bench_runtime_quick -- --nocapture

quality-runtime-mmr-quick-remote:
	ASTRAVECTOR_QUALITY_ENDPOINT=http://localhost:50051 \
	ASTRAVECTOR_QUALITY_RUN_ID=$${ASTRAVECTOR_QUALITY_RUN_ID:-fix477-mmr-$$(date +%Y%m%d-%H%M%S)} \
	ASTRAVECTOR_QUALITY_PROFILE=full-capability-quick \
	QUERY_FILTER=mmr \
	ASTRAVECTOR_QUALITY_REQUIRE_DENSE=true \
	ASTRAVECTOR_QUALITY_REQUIRE_SPARSE=true \
	ASTRAVECTOR_QUALITY_REQUIRE_HYBRID=true \
	ASTRAVECTOR_QUALITY_REQUIRE_MMR=true \
	ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true \
	ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_SEARCH=false \
	ASTRAVECTOR_QUALITY_RUNTIME_MODE=ingest-and-retrieve \
	cargo test --test quality_bench_runtime_quick -- --nocapture

quality-runtime-hard-negative-quick-remote:
	ASTRAVECTOR_QUALITY_ENDPOINT=http://localhost:50051 \
	ASTRAVECTOR_QUALITY_RUN_ID=$${ASTRAVECTOR_QUALITY_RUN_ID:-fix477-negative-$$(date +%Y%m%d-%H%M%S)} \
	ASTRAVECTOR_QUALITY_PROFILE=full-capability-quick \
	QUERY_FILTER=negative,technical-negative \
	ASTRAVECTOR_QUALITY_REQUIRE_DENSE=true \
	ASTRAVECTOR_QUALITY_REQUIRE_SPARSE=true \
	ASTRAVECTOR_QUALITY_REQUIRE_HYBRID=true \
	ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true \
	ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_SEARCH=false \
	ASTRAVECTOR_QUALITY_RUNTIME_MODE=ingest-and-retrieve \
	cargo test --test quality_bench_runtime_quick -- --nocapture

quality-runtime-full-capability-quick-remote:
	ASTRAVECTOR_QUALITY_ENDPOINT=http://localhost:50051 \
	ASTRAVECTOR_QUALITY_RUN_ID=$${ASTRAVECTOR_QUALITY_RUN_ID:-fix474i-full-$$(date +%Y%m%d-%H%M%S)} \
	ASTRAVECTOR_QUALITY_PROFILE=full-capability-quick \
	ASTRAVECTOR_QUALITY_REQUIRE_DENSE=true \
	ASTRAVECTOR_QUALITY_REQUIRE_SPARSE=true \
	ASTRAVECTOR_QUALITY_REQUIRE_HYBRID=true \
	ASTRAVECTOR_QUALITY_REQUIRE_GRAPH=true \
	ASTRAVECTOR_QUALITY_REQUIRE_MMR=true \
	ASTRAVECTOR_GRAPH_MAX_RELATED_CHUNKS=8 \
	ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true \
	ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_SEARCH=false \
	ASTRAVECTOR_QUALITY_RUNTIME_MODE=ingest-and-retrieve \
	cargo test --test quality_bench_runtime_quick -- --nocapture

quality-runtime-confidence:
	./scripts/quality-runtime-confidence.sh

quality-runtime-confidence-remote:
	ASTRAVECTOR_QUALITY_ENDPOINT=http://localhost:50051 \
	ASTRAVECTOR_DB_URL=$${ASTRAVECTOR_DB_URL:-postgres://astravector:astravector@127.0.0.1:55432/astravector} \
	DATABASE_URL=$${DATABASE_URL:-postgres://astravector:astravector@127.0.0.1:55432/astravector} \
	ASTRAVECTOR_QDRANT_URL=$${ASTRAVECTOR_QDRANT_URL:-http://127.0.0.1:6333} \
	ASTRAVECTOR_QDRANT_COLLECTION=$${ASTRAVECTOR_QDRANT_COLLECTION:-astravector_v004} \
	ASTRAVECTOR_MODEL_PATH=$${ASTRAVECTOR_MODEL_PATH:-/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/onnx/model.onnx} \
	ASTRAVECTOR_TOKENIZER_PATH=$${ASTRAVECTOR_TOKENIZER_PATH:-/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/tokenizer.json} \
	ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=$${ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION:-true} \
	ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_SEARCH=$${ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_SEARCH:-false} \
	ASTRAVECTOR_QUALITY_RUN_ID=$${ASTRAVECTOR_QUALITY_RUN_ID:-fix474f-$$(date +%Y%m%d-%H%M%S)} \
	CONFIDENCE_GATE_TIMEOUT_SECONDS=$${CONFIDENCE_GATE_TIMEOUT_SECONDS:-600} \
	./scripts/quality-runtime-confidence.sh

quality-runtime-confidence-report:
	jq . benchmarks/quality/reports/runtime-confidence-report.json

quality-runtime-full:
	ASTRAVECTOR_QUALITY_PROFILE=production-candidate \
	ASTRAVECTOR_QUALITY_RUNTIME_MODE=ingest-and-retrieve \
	cargo test --test quality_bench_runtime_quick -- --nocapture

quality-production-candidate:
	cargo test --features integration-tests --test quality_bench_production_candidate -- --nocapture

verify-fix467: fmt clippy check test quality-fixtures quality-quick


verify-fix468: fmt clippy check test quality-fixtures-enriched quality-quick
