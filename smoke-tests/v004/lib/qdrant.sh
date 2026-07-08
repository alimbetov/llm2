#!/usr/bin/env bash

qdrant_json() {
  local method="$1" path="$2" body="${3:-}"
  if [[ -n "$body" ]]; then
    curl -sS -X "$method" -H 'content-type: application/json' --data "$body" "${QDRANT_HTTP_URL}${path}"
  else
    curl -sS -X "$method" "${QDRANT_HTTP_URL}${path}"
  fi
}

ensure_qdrant_collection() {
  local body
  if qdrant_json GET "/collections/${QDRANT_COLLECTION}" \
    | jq -e '.result.config.params.vectors.dense.size == 1024' >/dev/null 2>&1; then
    return 0
  fi
  qdrant_json DELETE "/collections/${QDRANT_COLLECTION}" >/dev/null 2>&1 || true
  body="$(jq -n '{vectors:{dense:{size:1024,distance:"Cosine"}}}')"
  qdrant_json PUT "/collections/${QDRANT_COLLECTION}" "$body" >/dev/null
  qdrant_json GET "/collections/${QDRANT_COLLECTION}" \
    | jq -e '.result.status != null and .result.config.params.vectors.dense.size == 1024' >/dev/null
}
