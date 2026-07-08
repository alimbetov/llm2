# V007 fix4.5.5 — Access Zone Auto-Provisioning, Bind Fix & Registry Lifecycle Hardening

Base: `AstraVector_v007_interface_simplification_fix15_graph_rag_lite_fix454_access_zone_registry_code_matrix.zip`.

## Scope

This patch closes the StartLogicalDocumentIngestion bind-order defect and adds controlled ingestion-time auto-provisioning for `access_zone_code`.

## Key rules

- `access_zone_id` remains the internal UUID-backed source of truth.
- `access_zone_code` remains an external/audit alias in the `0000`–`9999` range.
- Search and RetrieveContext must never auto-create access zones.
- Auto-create is allowed only in ingestion path and only when `access_zone_registry.auto_create_on_ingestion=true`.

## Bind fix

The `ingestion_sessions_v004` INSERT includes `access_zone_code`; therefore the Rust bind sequence must be:

```rust
.bind(session_id)                 // $1 ingestion_session_id
.bind(access_zone_id)             // $2 access_zone_id UUID
.bind(&access_zone_code)          // $3 access_zone_code audit alias
.bind(document_id)                // $4 document_id
.bind(r.document_version as i64)  // $5 document_version
```

## Auto-provisioning flow

1. Client sends `access_zone_code="1500"`.
2. AstraVector validates `^[0-9]{4}$`.
3. AstraVector resolves the code in `astravector.access_zones`.
4. If found and ACTIVE, it uses the existing UUID `access_zone_id`.
5. If missing and `auto_create_on_ingestion=false`, it returns `FAILED_PRECONDITION / ACCESS_ZONE_NOT_FOUND`.
6. If missing and `auto_create_on_ingestion=true`, it creates a registry row with:
   - generated UUID `access_zone_id`;
   - `access_zone_code`;
   - `default_ttl_days` from Code Matrix TTL Policy;
   - `ttl_policy_source='CODE_MATRIX'`;
   - `auto_created=true`;
   - `created_reason='INGESTION_AUTO_CREATE'`;
   - `first_seen_at` / `last_seen_at`.
7. Ingestion continues internally by UUID.

## Config

```yaml
access_zone_registry:
  enabled: true
  cache_ttl_seconds: 60
  fail_if_zone_missing: true
  auto_create_on_ingestion: false
  auto_create_on_search: false
  auto_create_default_status: ACTIVE
  auto_create_require_internal_auth: true
```

`auto_create_on_search` is validated to remain false.

## Database changes

Migration `0031_v007_fix455_access_zone_auto_provisioning.sql` adds audit columns to `astravector.access_zones`:

- `auto_created`
- `created_by`
- `created_reason`
- `first_seen_at`
- `last_seen_at`

## Metrics

- `access_zone_auto_create_attempt_total`
- `access_zone_auto_create_success_total`
- `access_zone_auto_create_failed_total`
- `access_zone_auto_create_denied_total`
- `access_zone_auto_created_total`
- `access_zone_code_matrix_ttl_assigned_total`

## Acceptance points

- Existing code resolves without creation.
- Missing code with auto-create disabled returns `FAILED_PRECONDITION`.
- Missing code with auto-create enabled creates one UUID-backed registry row.
- Concurrent ingestion with the same missing code creates only one registry row.
- Search/RetrieveContext never creates zones.
- `access_zone_code` is stored in audit fields; all internal filtering remains UUID-backed.
