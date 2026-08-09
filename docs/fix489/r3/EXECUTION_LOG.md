# FIX489-R3 Execution Log

## Current State

```text
implementation_status=PASS
official_discovery=PASS
official_soak=PASS
```

Static contract commands executed during implementation:

```bash
make verify-fix487a-retrieval-freeze
make verify-fix489-live-capacity-contracts
make verify-fix489r3-contracts
make verify-fix489r3-soak-contracts
python3 -m unittest -v tests/test_astravector_live_client.py tests/test_fix489_live_capacity.py tests/test_fix489_capacity_evidence.py tests/test_fix487bc_capacity_campaign.py tests/test_fix487c_soak.py
bash -n scripts/fix489r3-local-stable-floor.sh
bash -n scripts/fix489r3-soak-60m.sh
cargo fmt --all --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

Current static gate status:

```text
retrieval_freeze=PASS
fix489r3_contracts=PASS
fix489r3_soak_contracts=PASS
python_contracts=55/55 PASS
cargo_fmt=PASS
cargo_check=PASS
cargo_clippy=PASS
cargo_test=PASS
```

Official local stable floor discovery:

```bash
ASTRAVECTOR_FIX489R3_EXECUTE_DISCOVERY=true \
FIX489_LOAD_MODE=CLOSED_LOOP \
ASTRAVECTOR_MODEL_PATH=/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/onnx/model.onnx \
ASTRAVECTOR_TOKENIZER_PATH=/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/onnx/tokenizer.json \
ASTRAVECTOR_EVIDENCE_ROOT=/Users/ruslanalimbetov/Documents/llm2/astravector-evidence \
make verify-fix489r3-local-stable-floor
```

Result:

```text
run_id=fix489-r3-20260809T062307Z
tested_sha=00eeafaa465ea848c69dea9d7b70bd38aa75b785
terminal_status=PASS
verdict=FIX489_R3_LOCAL_STABLE_FLOOR_PASS
maximum_stable_concurrency=2
recommended_operating_concurrency=1
first_controlled_saturation_concurrency=3
evidence_manifest_sha256=5d2ecabd84e1c0051e5232a01e49a0e07fae74be8a7cdf2a1ffc44dd3700ff4a
```

First official soak attempt:

```text
tested_sha=d454de2e1ca1f85727ec1b587c7cf60c70fdd313
run_id=fix489-r3-soak-20260809T073245Z
terminal_status=FAILED
reason=DELETE_POOL_EXHAUSTED
UNKNOWN=206
```

Harness repair:

```text
tested_sha=623c75b65146d1ee9bda3ecd66636d9019accfce
repair=test(fix489r3): size soak delete pool from observed throughput
focused_validation=34/34 PASS
```

Official 60-minute soak repeat:

```bash
ASTRAVECTOR_FIX489R3_EXECUTE_SOAK=true \
FIX489_R3_CAPACITY_EVIDENCE_DIR=/Users/ruslanalimbetov/Documents/llm2/astravector-evidence/fix489-r3/fix489-r3-20260809T062307Z \
ASTRAVECTOR_MODEL_PATH=/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/onnx/model.onnx \
ASTRAVECTOR_TOKENIZER_PATH=/Users/ruslanalimbetov/Documents/llm2/models/bge-m3/onnx/tokenizer.json \
ASTRAVECTOR_EVIDENCE_ROOT=/Users/ruslanalimbetov/Documents/llm2/astravector-evidence \
make verify-fix489r3-soak-60m
```

Result:

```text
run_id=fix489-r3-soak-20260809T111138Z
tested_sha=623c75b65146d1ee9bda3ecd66636d9019accfce
terminal_status=PASS
verdict=FIX489_R3_SOAK_60M_PASS
completed_operations=7381
success_rate=1.0
grpc_statuses.OK=7381
UNKNOWN=0
hard_gates=0
memory_behavior_stable=true
queues_bounded=true
evidence_manifest_sha256=02ce12a365169b857ff5d963db6b6ed66d14b56ca3c8c1e9f8a9d7de11a94c55
```
