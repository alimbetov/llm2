.PHONY: fmt check test test-e2e e2e-network sqlx-prepare smoke-load smoke-load-blocking quality-fixtures quality-quick quality-quick-remote quality-runtime-quick quality-runtime-quick-remote quality-runtime-dense-quick quality-runtime-dense-quick-remote quality-runtime-sparse-quick-remote quality-runtime-hybrid-quick-remote quality-runtime-graph-quick-remote quality-runtime-rag-analysis-bank-remote quality-runtime-mmr-quick-remote quality-runtime-hard-negative-quick-remote quality-runtime-full-capability-quick-remote quality-runtime-tuning-remote quality-runtime-validation-remote quality-runtime-holdout-remote quality-runtime-confidence quality-runtime-confidence-remote quality-runtime-confidence-report quality-runtime-full quality-production-candidate quality-system-smoke-remote production-recovery-gate-m2 production-recovery-gate-m2-repeatability production-search-gate-m2 production-search-gate-m2-repeatability verify-fix463 verify-fix465 verify-fix467 verify-fix468 verify-fix480 verify-fix481 verify-fix482 verify-fix486b-runtime-baseline verify-fix486c-frozen-bank verify-fix486d-child-parent-runtime verify-fix486d-child-parent-runtime-proof verify-fix486f-stale-orphan-hydration-runtime verify-fix486f-stale-orphan-hydration-runtime-proof verify-fix486g-graph-parent-runtime verify-fix486g-graph-parent-runtime-proof verify-fix487a-retrieval-freeze verify-fix487b-contracts verify-fix487b-mixed-load-pilot verify-fix487b-existing-evidence verify-fix487bc-capacity-contracts verify-fix487bc-capacity-campaign verify-fix487bc-existing-capacity-evidence verify-fix487c-soak-contracts verify-fix487c-soak-60m verify-fix487c-existing-soak-evidence verify-fix489-live-capacity-contracts local-demo-check local-demo-infra-up local-demo-infra-wait local-demo-build local-demo-runtime-start local-demo-runtime-stop local-demo-load local-demo-search local-demo-inspect local-demo-e2e local-demo-down local-demo-reset verify-fix488-local-demo fix487b-cleanup fix487bc-cleanup fix481-prepare-judgments fix481-finalize-judgments fix482-structural-validator fix482-contract-tests fix482-prepare-judgments fix483-contracts fix483-long-query-quality fix483-short-regression fix483-integration fix483-load-smoke verify-fix483-production verify-rag-core smoke-rag-long-query smoke-rag-hybrid smoke-rag-failures smoke-rag-mixed-load verify-rag-production-candidate quality-fixtures-enriched clippy release migrate run run-runtime-local db-up db-down
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
	cargo run --bin astravector-runtime -- migrate
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

local-demo-check:
	./scripts/local-demo/check-prerequisites.sh
	./scripts/local-demo/check-model.sh

local-demo-infra-up:
	./scripts/local-demo/infra-up.sh

local-demo-infra-wait:
	./scripts/local-demo/infra-wait.sh

local-demo-build:
	./scripts/local-demo/build-runtime.sh

local-demo-runtime-start:
	./scripts/local-demo/run-runtime.sh

local-demo-runtime-stop:
	./scripts/local-demo/stop-runtime.sh

local-demo-load:
	./scripts/local-demo/load-text.sh examples/local-demo/sample-ru.txt

local-demo-search:
	./scripts/local-demo/search.sh 'Где AstraVector хранит каноническое состояние?'
	./scripts/local-demo/search.sh 'ASTRAVECTOR_LOCAL_DEMO_2026'

local-demo-inspect:
	./scripts/local-demo/inspect-postgres.sh
	./scripts/local-demo/inspect-qdrant.sh

local-demo-e2e:
	./scripts/local-demo/e2e.sh

local-demo-down:
	./scripts/local-demo/down.sh

local-demo-reset:
	./scripts/local-demo/reset.sh --yes

verify-fix488-local-demo:
	$(MAKE) verify-fix487a-retrieval-freeze
	bash -n scripts/local-demo/*.sh
	python3 -m unittest -v tests/test_fix488_local_request_builders.py tests/test_fix488_local_evidence.py tests/test_fix488_local_scripts.py

verify-fix486b-runtime-baseline:
	./scripts/fix486b-runtime-baseline.sh

verify-fix486c-frozen-bank:
	./scripts/fix486c-frozen-bank.sh --dry-run

verify-fix486d-child-parent-runtime:
	./scripts/fix486d-child-parent-runtime-proof.sh --execute-all

verify-fix486d-child-parent-runtime-proof: verify-fix486d-child-parent-runtime

verify-rag-core:
	cargo fmt --all --check
	cargo check --locked --all-targets --all-features
	cargo test --locked --all-targets --all-features
	cargo clippy --locked --all-targets --all-features -- -D warnings
	cargo sqlx prepare --check -- --all-targets --all-features

smoke-rag-long-query:
	SMOKE_ARTIFACT_DIR=$${SMOKE_ARTIFACT_DIR:-$${ASTRAVECTOR_EVIDENCE_ROOT:-../astravector-evidence}/$${FIX485_RUN_ID:-fix485-local}/smoke} bash ./smoke-tests/v004/scripts/run-full-smoke.sh --only long-query-model-backed --keep-running --strict

smoke-rag-hybrid:
	SMOKE_ARTIFACT_DIR=$${SMOKE_ARTIFACT_DIR:-$${ASTRAVECTOR_EVIDENCE_ROOT:-../astravector-evidence}/$${FIX485_RUN_ID:-fix485-local}/smoke} bash ./smoke-tests/v004/scripts/run-full-smoke.sh --only hybrid-runtime-retrieval --keep-running --strict

smoke-rag-failures:
	SMOKE_ARTIFACT_DIR=$${SMOKE_ARTIFACT_DIR:-$${ASTRAVECTOR_EVIDENCE_ROOT:-../astravector-evidence}/$${FIX485_RUN_ID:-fix485-local}/smoke} bash ./smoke-tests/v004/scripts/run-full-smoke.sh --only partial-backend-failure --keep-running --strict

smoke-rag-mixed-load:
	SMOKE_ARTIFACT_DIR=$${SMOKE_ARTIFACT_DIR:-$${ASTRAVECTOR_EVIDENCE_ROOT:-../astravector-evidence}/$${FIX485_RUN_ID:-fix485-local}/smoke} bash ./smoke-tests/v004/scripts/run-full-smoke.sh --only mixed-tier-fairness --keep-running --strict

verify-rag-production-candidate: verify-rag-core
	$(MAKE) smoke-rag-long-query
	$(MAKE) smoke-rag-hybrid
	$(MAKE) smoke-rag-failures
	$(MAKE) smoke-rag-mixed-load
	$(MAKE) quality-runtime-confidence-remote


sqlx-prepare:
	cargo sqlx prepare --check -- --all-targets --all-features

e2e-network: test-e2e

smoke-load:
	cargo test --features integration-tests --test smoke_load_retrieve_context -- --ignored --nocapture

smoke-load-blocking:
	cargo test --features integration-tests --test smoke_load_retrieve_context_testcontainers -- --nocapture

fix483-contracts:
	cargo test query_processing --lib -- --nocapture
	cargo test --test query_processing_contracts -- --nocapture

fix483-long-query-quality:
	ASTRAVECTOR_QUALITY_ENDPOINT=$${ASTRAVECTOR_QUALITY_ENDPOINT:-http://localhost:50051} \
	ASTRAVECTOR_QUALITY_PROFILE=long-query-v1 \
	ASTRAVECTOR_QUALITY_RUNTIME_MODE=ingest-and-retrieve \
	cargo test --test quality_bench_runtime_quick -- --nocapture

fix483-short-regression:
	ASTRAVECTOR_QUALITY_ENDPOINT=$${ASTRAVECTOR_QUALITY_ENDPOINT:-http://localhost:50051} \
	ASTRAVECTOR_QUALITY_PROFILE=rag-quality-bank-v1 \
	ASTRAVECTOR_QUALITY_RUNTIME_MODE=ingest-and-retrieve \
	cargo test --test quality_bench_runtime_quick -- --nocapture

fix483-integration:
	cargo test --features integration-tests --test smoke_load_retrieve_context_testcontainers -- --nocapture
	cargo test --features integration-tests --test lexical_retrieval_integration -- --nocapture

fix483-load-smoke:
	ASTRA_VECTOR_SMOKE_RETRIEVE_ENDPOINT=$${ASTRAVECTOR_QUALITY_ENDPOINT:-http://127.0.0.1:50051} \
	ASTRA_VECTOR_SMOKE_CONCURRENCY=50 \
	cargo test --features integration-tests --test smoke_load_retrieve_context -- --ignored --nocapture

verify-fix483-production:
	cargo fmt --check
	cargo check --all-targets --all-features
	cargo clippy --all-targets --all-features -- -D warnings
	$(MAKE) fix483-contracts
	$(MAKE) fix483-integration

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
	ASTRAVECTOR_GRAPH_MAX_SEED_CHUNKS=16 \
	ASTRAVECTOR_GRAPH_CONTEXT_APPEND_LIMIT=5 \
	ASTRAVECTOR_GRAPH_EXPANSION_RESULT_LIMIT=12 \
	ASTRAVECTOR_GRAPH_MERGE_STRATEGY=GRAPH_AS_CONTEXT_APPEND \
	ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true \
	ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_SEARCH=false \
	ASTRAVECTOR_QUALITY_RUNTIME_MODE=ingest-and-retrieve \
	cargo test --test quality_bench_runtime_quick -- --nocapture

quality-runtime-rag-analysis-bank-remote:
	ASTRAVECTOR_QUALITY_ENDPOINT=$${ASTRAVECTOR_QUALITY_ENDPOINT:-http://localhost:50051} \
	ASTRAVECTOR_QUALITY_RUN_ID=$${ASTRAVECTOR_QUALITY_RUN_ID:-rag-analysis-bank-$$(date +%Y%m%d-%H%M%S)} \
	ASTRAVECTOR_QUALITY_PROFILE=rag-analysis-bank \
	ASTRAVECTOR_QUALITY_REQUIRE_DENSE=true \
	ASTRAVECTOR_QUALITY_REQUIRE_SPARSE=true \
	ASTRAVECTOR_QUALITY_REQUIRE_HYBRID=true \
	ASTRAVECTOR_QUALITY_REQUIRE_GRAPH=true \
	ASTRAVECTOR_QUALITY_REQUIRE_MMR=true \
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
	ASTRAVECTOR_GRAPH_MAX_SEED_CHUNKS=16 \
	ASTRAVECTOR_GRAPH_CONTEXT_APPEND_LIMIT=5 \
	ASTRAVECTOR_GRAPH_EXPANSION_RESULT_LIMIT=12 \
	ASTRAVECTOR_GRAPH_MERGE_STRATEGY=GRAPH_AS_CONTEXT_APPEND \
	ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true \
	ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_SEARCH=false \
	ASTRAVECTOR_QUALITY_RUNTIME_MODE=ingest-and-retrieve \
	cargo test --test quality_bench_runtime_quick -- --nocapture

quality-runtime-tuning-remote:
	ASTRAVECTOR_QUALITY_ENDPOINT=$${ASTRAVECTOR_QUALITY_ENDPOINT:-http://localhost:50051} \
	ASTRAVECTOR_QUALITY_RUN_ID=$${ASTRAVECTOR_QUALITY_RUN_ID:-fix480-tuning-$$(date +%Y%m%d-%H%M%S)} \
	ASTRAVECTOR_QUALITY_PROFILE=tuning ASTRAVECTOR_QUALITY_RUNTIME_MODE=ingest-and-retrieve \
	ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true \
	cargo test --test quality_bench_runtime_quick -- --nocapture

quality-runtime-validation-remote:
	ASTRAVECTOR_QUALITY_ENDPOINT=$${ASTRAVECTOR_QUALITY_ENDPOINT:-http://localhost:50051} \
	ASTRAVECTOR_QUALITY_RUN_ID=$${ASTRAVECTOR_QUALITY_RUN_ID:-fix480-validation-$$(date +%Y%m%d-%H%M%S)} \
	ASTRAVECTOR_QUALITY_PROFILE=validation ASTRAVECTOR_QUALITY_RUNTIME_MODE=ingest-and-retrieve \
	ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true \
	cargo test --test quality_bench_runtime_quick -- --nocapture

quality-runtime-holdout-remote:
	ASTRAVECTOR_QUALITY_ENDPOINT=$${ASTRAVECTOR_QUALITY_ENDPOINT:-http://localhost:50051} \
	ASTRAVECTOR_QUALITY_RUN_ID=$${ASTRAVECTOR_QUALITY_RUN_ID:-fix480-holdout-$$(date +%Y%m%d-%H%M%S)} \
	ASTRAVECTOR_QUALITY_PROFILE=holdout ASTRAVECTOR_QUALITY_RUNTIME_MODE=ingest-and-retrieve \
	ASTRAVECTOR_ACCESS_ZONE_REGISTRY_AUTO_CREATE_ON_INGESTION=true \
	cargo test --test quality_bench_runtime_quick -- --nocapture

quality-system-smoke-remote:
	./scripts/quality-system-smoke.sh $${SYSTEM_SMOKE_ARGS:---external-runtime}

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
	jq . $${ASTRAVECTOR_QUALITY_OUTPUT_DIR:-target/quality-reports}/runtime-confidence-report.json

quality-runtime-full:
	ASTRAVECTOR_QUALITY_PROFILE=production-candidate \
	ASTRAVECTOR_QUALITY_RUNTIME_MODE=ingest-and-retrieve \
	cargo test --test quality_bench_runtime_quick -- --nocapture

quality-production-candidate:
	cargo test --features integration-tests --test quality_bench_production_candidate -- --nocapture

production-recovery-gate-m2:
	ASTRAVECTOR_PROFILE=load-m2 ./scripts/macbook-model-backed-load.sh

production-recovery-gate-m2-repeatability:
	./scripts/run_fix478_repeatability_gate.sh

production-search-gate-m2:
	ASTRAVECTOR_PROFILE=search-production-candidate LOAD_RUN_ID=$${LOAD_RUN_ID:-fix480-$$(git rev-parse --short=7 HEAD)-run-1} ./scripts/macbook-model-backed-load.sh

production-search-gate-m2-repeatability:
	./scripts/run_fix480_repeatability_gate.sh

verify-fix480: fmt clippy check test sqlx-prepare test-e2e quality-fixtures-enriched
	cargo test --features integration-tests --test lexical_retrieval_integration -- --nocapture

verify-fix481:
	cargo fmt --check
	cargo check --all-targets --all-features
	cargo clippy --all-targets --all-features -- -D warnings
	cargo test --all-targets --all-features
	cargo sqlx prepare --check -- --all-targets --all-features
	cargo test --features integration-tests --test lexical_retrieval_integration -- --nocapture
	cargo test --features integration-tests --test ranking_evidence_preservation -- --nocapture
	cargo test --features integration-tests --test e2e_testcontainers -- --nocapture

fix481-prepare-judgments:
	test -n "$${FIX481_REPORT_DIR}" || (echo "FIX481_REPORT_DIR is required" >&2; exit 2)
	python3 scripts/fix481_judgment_pool.py prepare --profile $${FIX481_PROFILE:-validation} --report-dir "$${FIX481_REPORT_DIR}"

fix481-finalize-judgments:
	python3 scripts/fix481_judgment_pool.py finalize --profile $${FIX481_PROFILE:-validation}

fix482-structural-validator:
	python3 scripts/validate_rag_quality_bank_v1.py

fix482-contract-tests:
	cargo test --test quality_fixtures_contracts -- --nocapture

fix482-prepare-judgments: fix482-structural-validator
	python3 scripts/prepare_fix482_rag_quality_bank_judgments.py

verify-fix482:
	cargo fmt --check
	cargo check --all-targets --all-features
	cargo clippy --all-targets --all-features -- -D warnings
	cargo build --bin astravector-runtime
	python3 scripts/validate_rag_quality_bank_v1.py
	python3 scripts/prepare_fix482_rag_quality_bank_judgments.py
	cargo test --test quality_fixtures_contracts -- --nocapture

verify-fix467: fmt clippy check test quality-fixtures quality-quick


verify-fix468: fmt clippy check test quality-fixtures-enriched quality-quick

verify-fix486e-isolation-lifecycle-runtime:
	./scripts/fix486e-isolation-lifecycle-runtime-proof.sh --execute-all

verify-fix486e-isolation-lifecycle-runtime-proof: verify-fix486e-isolation-lifecycle-runtime

verify-fix486f-stale-orphan-hydration-runtime:
	./scripts/fix486f-stale-orphan-hydration-proof.sh --execute-all

verify-fix486f-stale-orphan-hydration-runtime-proof: verify-fix486f-stale-orphan-hydration-runtime

verify-fix486g-graph-parent-runtime:
	./scripts/fix486g-graph-parent-runtime-proof.sh --execute-all

verify-fix486g-graph-parent-runtime-proof: verify-fix486g-graph-parent-runtime

verify-fix487a-retrieval-freeze:
	python3 -m py_compile scripts/fix487_retrieval_freeze_guard.py
	python3 -m unittest -v tests/test_fix487_retrieval_freeze_guard.py
	python3 scripts/fix487_retrieval_freeze_guard.py --repo .

verify-fix487b-contracts:
	$(MAKE) verify-fix487a-retrieval-freeze
	python3 -m py_compile scripts/fix487b_dataset.py scripts/fix487b_mixed_load.py scripts/fix487b_evidence.py scripts/fix487b_audit.py
	python3 -m unittest -v tests/test_fix487b_dataset.py tests/test_fix487b_mixed_load.py tests/test_fix487b_evidence.py tests/test_fix487b_audit.py
	bash -n scripts/fix487b-mixed-load-pilot.sh

verify-fix487b-mixed-load-pilot:
	@if [ "$$ASTRAVECTOR_FIX487B_EXECUTE_PILOT" != "true" ]; then echo "FIX487B_BLOCKED=EXPLICIT_PILOT_OPT_IN_REQUIRED"; exit 2; fi
	bash scripts/fix487b-mixed-load-pilot.sh

verify-fix487b-existing-evidence:
	python3 scripts/fix487b_evidence.py --root "$${FIX487B_EVIDENCE_DIR:?set FIX487B_EVIDENCE_DIR}"

fix487b-cleanup:
	docker compose -p astravector_fix487b -f docker-compose.fix487b.yml down

verify-fix487bc-capacity-contracts:
	$(MAKE) verify-fix487b-contracts
	python3 -m py_compile scripts/fix487bc_capacity_campaign.py scripts/fix487bc_capacity_evidence.py
	python3 -m unittest -v tests/test_fix487bc_capacity_campaign.py tests/test_fix487bc_capacity_evidence.py
	bash -n scripts/fix487bc-capacity-campaign.sh

verify-fix487bc-capacity-campaign:
	@if [ "$$ASTRAVECTOR_FIX487BC_EXECUTE_CAPACITY" != "true" ]; then echo "FIX487BC_BLOCKED=EXPLICIT_CAPACITY_OPT_IN_REQUIRED"; exit 2; fi
	bash scripts/fix487bc-capacity-campaign.sh

verify-fix487bc-existing-capacity-evidence:
	python3 scripts/fix487bc_capacity_evidence.py --root "$${FIX487BC_CAPACITY_EVIDENCE_DIR:?set FIX487BC_CAPACITY_EVIDENCE_DIR}"

verify-fix487c-soak-contracts:
	$(MAKE) verify-fix487a-retrieval-freeze
	python3 -m py_compile scripts/fix487c_soak.py
	python3 -m unittest -v tests/test_fix487c_soak.py
	bash -n scripts/fix487c-soak-60m.sh

verify-fix487c-soak-60m:
	@if [ "$$ASTRAVECTOR_FIX487C_EXECUTE_SOAK" != "true" ]; then echo "FIX487C_BLOCKED=EXPLICIT_SOAK_OPT_IN_REQUIRED"; exit 2; fi
	bash scripts/fix487c-soak-60m.sh

verify-fix487c-existing-soak-evidence:
	python3 scripts/fix487c_soak.py --verify-evidence-root "$${FIX487C_SOAK_EVIDENCE_DIR:?set FIX487C_SOAK_EVIDENCE_DIR}"

fix487bc-cleanup:
	docker compose -p astravector_fix487b -f docker-compose.fix487b.yml down

verify-fix489-live-capacity-contracts:
	$(MAKE) verify-fix487a-retrieval-freeze
	python3 -m py_compile scripts/astravector_live_client.py scripts/fix489_live_capacity.py
	python3 -m unittest -v tests/test_fix489_live_capacity.py
	bash -n scripts/fix487bc-capacity-campaign.sh scripts/fix487c-soak-60m.sh
