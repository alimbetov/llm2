#!/usr/bin/env bash
set -euo pipefail
expected_version="0.4.1"
expected_tag="0.4.1-fix465-p2-production-hardening"

if ! grep -q "version = \"${expected_version}\"" Cargo.toml; then
  echo "Cargo.toml version must be ${expected_version}" >&2
  exit 1
fi

files=(
  .github/workflows/ci.yml
  k8s/deployment.yaml
  k8s/lifecycle-cronjob.yaml
  k8s/qdrant-publisher-deployment.yaml
  k8s/migration-job.yaml
  README.md
  docs/KUBERNETES_DEPLOYMENT.md
  docs/FIX463_PRODUCTION_CANDIDATE_STABILIZATION.md
  docs/FIX465_P2_PRODUCTION_HARDENING.md
)
for file in "${files[@]}"; do
  if ! grep -q "${expected_tag}" "$file"; then
    echo "$file must reference image tag ${expected_tag}" >&2
    exit 1
  fi
  if grep -q "0.4.0-fix463-production-candidate-stabilization" "$file"; then
    echo "$file still references old fix463 image tag" >&2
    exit 1
  fi
done

echo "version alignment OK: Cargo ${expected_version}, image tag ${expected_tag}"
