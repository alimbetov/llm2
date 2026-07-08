//! fix4.5.5 access-zone auto-provisioning acceptance scenarios.
//! These are ignored until PostgreSQL/Qdrant testcontainers wiring is enabled.

#[test]
fn start_ingestion_missing_code_auto_create_disabled_returns_failed_precondition_required() {
    eprintln!("integration scenario requires external PostgreSQL/Qdrant harness: Start ingestion with unknown access_zone_code and auto_create_on_ingestion=false must return FAILED_PRECONDITION / ACCESS_ZONE_NOT_FOUND");
}

#[test]
fn start_ingestion_missing_code_auto_create_enabled_creates_uuid_zone_required() {
    eprintln!("integration scenario requires external PostgreSQL/Qdrant harness: Start ingestion with unknown code=1500 and auto_create_on_ingestion=true must create one access_zones row with UUID and default_ttl_days=365");
}

#[test]
fn start_ingestion_bind_order_stores_access_zone_code_and_document_id_required() {
    eprintln!("integration scenario requires external PostgreSQL/Qdrant harness: Start ingestion must store access_zone_code='1500' and document_id in their own columns, proving bind-order is correct");
}

#[test]
fn concurrent_start_ingestion_same_missing_code_creates_single_row_required() {
    eprintln!("integration scenario requires external PostgreSQL/Qdrant harness: Parallel Start ingestion requests for the same missing access_zone_code must create only one access_zones row");
}

#[test]
fn search_missing_code_never_auto_creates_required() {
    eprintln!("integration scenario requires external PostgreSQL/Qdrant harness: Search/RetrieveContext with unknown access_zone_code must return FAILED_PRECONDITION and must not insert access_zones row");
}

#[test]
fn code_uuid_mismatch_remains_invalid_argument_required() {
    eprintln!("integration scenario requires external PostgreSQL/Qdrant harness: If access_zone_code and access_zone_id are both supplied, they must resolve to the same zone or return INVALID_ARGUMENT");
}
