# AstraVector v007 fix4.5.4 — Access Zone Registry, Code Matrix TTL Policy & UUID-backed Zone Resolution

## Purpose

This patch standardizes access-zone handling for AstraVector after fix4.5.3 TTL lifecycle hardening.

AstraVector keeps `access_zone_id` as the internal immutable UUID-backed identifier, while adding an external stable `access_zone_code` in the `0000..9999` range for ingestion service and ai_bro integrations.

## Final model

| Layer | Representation |
|---|---|
| Proto public contract | `string access_zone_id`, `string access_zone_code` |
| PostgreSQL authoritative key | `access_zone_id UUID` |
| PostgreSQL registry alias | `access_zone_code CHAR(4)` |
| Qdrant payload | `access_zone_id` UUID string + `access_zone_code` diagnostic alias |
| Search filtering | UUID `access_zone_id`, never code-only |
| GraphRAG filtering | UUID `access_zone_id`, never code-only |
| TTL cleanup | UUID `access_zone_id` + `document_id` + `document_version` |

## Access zone registry

New table:

```sql
astravector.access_zones(
  access_zone_id UUID PRIMARY KEY,
  access_zone_code CHAR(4) UNIQUE NOT NULL,
  status TEXT NOT NULL,
  default_ttl_days INTEGER NOT NULL,
  ttl_policy_source TEXT NOT NULL,
  allow_never_expire BOOLEAN NOT NULL
)
```

Only `ACTIVE` zones are accepted by the resolver for ingestion/search in fix4.5.4.

## Code matrix TTL policy

The code matrix is used only when a zone is created or backfilled. The calculated value is stored explicitly in `access_zones.default_ttl_days`; AstraVector must not recompute TTL dynamically for existing zones.

| Code range | Default TTL |
|---|---:|
| `0000–0999` | `0`, never-expire |
| `1000–1499` | `182` days |
| `1500–1999` | `365` days |
| `2000–2499` | `547` days |
| `2500–2999` | `730` days |
| `3000–3499` | `912` days |
| `3500–3999` | `1095` days |
| `4000–4499` | `1277` days |
| `4500–4999` | `1460` days |
| `5000–5499` | `1642` days |
| `5500–5999` | `1825` days |
| `6000–6499` | `2007` days |
| `6500–6999` | `2190` days |
| `7000–7499` | `2372` days |
| `7500–7999` | `2555` days |
| `8000–8499` | `2737` days |
| `8500–8999` | `2920` days |
| `9000–9499` | `3102` days |
| `9500–9999` | `3650` days, special max-retention bucket |

## Ingestion resolution

External clients may send either:

- legacy `access_zone_id` UUID string; or
- new `access_zone_code` such as `1500`.

If both are supplied, AstraVector verifies that the code resolves to the same UUID. Mismatch returns `INVALID_ARGUMENT`.

The effective TTL is chosen as follows:

1. request `ttl_days > 0` wins if within configured min/max;
2. request `ttl_days == 0` is allowed only if both global and zone-level never-expire policies allow it;
3. missing/zero proto default uses `access_zones.default_ttl_days` from the resolved zone.

## Search/RetrieveContext resolution

External clients may send:

- legacy `access_zone_id`; or
- legacy `access_zone_ids`; or
- new `access_zone_code`; or
- new `access_zone_codes`.

All codes are resolved to UUIDs before Qdrant/PostgreSQL/GraphRAG filtering. Qdrant filtering remains UUID-based:

```json
{
  "key": "access_zone_id",
  "match": { "any": ["uuid-1", "uuid-2"] }
}
```

`access_zone_code` is stored in Qdrant payload only for diagnostics and explainability.

## Payload drift mitigation

PostgreSQL remains the source of truth. Qdrant payload may drift temporarily. Search still performs PostgreSQL context re-check after Qdrant candidate fetch, enforcing:

- access zone UUID list;
- `lifecycle_status = ACTIVE`;
- TTL validity;
- access level.

A full reconciliation worker remains a P2 hardening item.

## Metrics

New metric names:

- `access_zone_registry_resolve_total`
- `access_zone_registry_resolve_failed_total`
- `access_zone_registry_cache_hit_total`
- `access_zone_registry_cache_miss_total`
- `access_zone_code_invalid_total`
- `access_zone_code_uuid_mismatch_total`
- `access_zone_registry_reload_duration_ms`

## Limitations

- This patch keeps internal UUID-backed storage.
- Code-based filtering is intentionally not used as the primary security boundary.
- Access-zone admin CRUD is out of scope; registry rows are managed by migration/manual SQL for this patch.
- Rust cargo verification was not executed in the patching environment.
