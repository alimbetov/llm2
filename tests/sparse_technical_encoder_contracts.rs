use astravector_runtime::sparse::{
    stable_sparse_index, SparseTechnicalEncoder, SparseTokenClass,
    TECHNICAL_SPARSE_ENCODER_VERSION, TECHNICAL_SPARSE_INDEX_STRATEGY, TECHNICAL_SPARSE_MODE,
};

fn encoder() -> SparseTechnicalEncoder {
    SparseTechnicalEncoder::new(0.0, 512)
}

fn index_for_token(text: &str, token: &str) -> u32 {
    let analysis = encoder().analyze(text);
    analysis
        .tokens
        .iter()
        .find(|t| t.token == token)
        .unwrap_or_else(|| panic!("token `{token}` not extracted from `{text}`"))
        .index
}

fn assert_document_query_same_index(token: &str, document: &str, query: &str) {
    let doc_index = index_for_token(document, token);
    let query_index = index_for_token(query, token);
    assert_eq!(
        doc_index, query_index,
        "document/query sparse index mismatch for {token}"
    );
    assert_eq!(doc_index, index_for_token(document, token));
    assert_eq!(query_index, index_for_token(query, token));
}

#[test]
fn technical_sparse_metadata_is_explicit() {
    assert_eq!(TECHNICAL_SPARSE_MODE, "LEXICAL_BASELINE_TECHNICAL");
    assert_eq!(TECHNICAL_SPARSE_ENCODER_VERSION, "technical-v2");
    assert_eq!(TECHNICAL_SPARSE_INDEX_STRATEGY, "stable_hash_sha256_u32");
    assert_eq!(encoder().version(), TECHNICAL_SPARSE_ENCODER_VERSION);
}

#[test]
fn document_and_query_paths_share_stable_indices_for_required_tokens() {
    let cases = [
        (
            "00000445543",
            "Document ticket id 00000445543 is persisted.",
            "Find 00000445543",
        ),
        (
            "A00017457",
            "Customer reference A00017457 was imported.",
            "Lookup A00017457",
        ),
        (
            "ORA-00904",
            "Database returned Code: ORA-00904, invalid identifier.",
            "Explain ORA-00904",
        ),
        (
            "ERR-1045",
            "Worker emitted ERR-1045 during retry.",
            "Search ERR-1045",
        ),
        (
            "CC_HOME_REQUESTS",
            "Table CC_HOME_REQUESTS stores requests.",
            "Where is CC_HOME_REQUESTS used?",
        ),
        (
            "access_zone_id",
            "Payload field access_zone_id is mandatory.",
            "Filter by access_zone_id",
        ),
        (
            "/api/v1/documents",
            "Endpoint path=/api/v1/documents accepts uploads.",
            "Call /api/v1/documents",
        ),
        (
            "runtime-quality-report.json",
            "Open file runtime-quality-report.json.",
            "Read runtime-quality-report.json",
        ),
        (
            "127.0.0.1:6333",
            "Qdrant endpoint 127.0.0.1:6333 is local.",
            "Connect to 127.0.0.1:6333",
        ),
        (
            "v007/fix474",
            "Release v007/fix474 enabled sparse.",
            "Status of v007/fix474",
        ),
        (
            "AstraVectorIngestionFacade/IndexLogicalDocument",
            "gRPC method AstraVectorIngestionFacade/IndexLogicalDocument ingests blocks.",
            "Invoke AstraVectorIngestionFacade/IndexLogicalDocument",
        ),
    ];
    for (token, document, query) in cases {
        assert_document_query_same_index(token, document, query);
    }
}

#[test]
fn leading_zero_numeric_token_is_preserved() {
    let analysis = encoder().analyze("identifier 00000445543 should stay exact");
    assert!(analysis.tokens.iter().any(|t| t.token == "00000445543"));
    assert!(!analysis.tokens.iter().any(|t| t.token == "445543"));
}

#[test]
fn lowercase_variants_are_generated_consistently() {
    let document = encoder().analyze("Table CC_HOME_REQUESTS and Code ORA-00904");
    let query = encoder().analyze("Find cc_home_requests and ora-00904");
    for token in ["cc_home_requests", "ora-00904"] {
        let doc = document
            .tokens
            .iter()
            .find(|t| t.token == token)
            .expect("document lowercase variant");
        let qry = query
            .tokens
            .iter()
            .find(|t| t.token == token)
            .expect("query lowercase variant");
        assert_eq!(doc.index, qry.index);
    }
}

#[test]
fn l2_normalization_produces_non_zero_vectors() {
    let vector = encoder()
        .encode_document("ORA-00904 access_zone_id /api/v1/documents 00000445543")
        .expect("encode document");
    assert!(!vector.indices.is_empty());
    assert_eq!(vector.indices.len(), vector.values.len());
    let norm = vector
        .values
        .iter()
        .map(|v| (*v as f64) * (*v as f64))
        .sum::<f64>()
        .sqrt();
    assert!((norm - 1.0).abs() < 0.0001, "norm={norm}");
}

#[test]
fn sparse_indices_are_stable_across_encoder_instances() {
    let one = SparseTechnicalEncoder::new(0.0, 512);
    let two = SparseTechnicalEncoder::new(0.0, 512);
    let text = "AstraVectorIngestionFacade/IndexLogicalDocument ORA-00904 00000445543";
    assert_eq!(
        one.encode_document(text).expect("one").indices,
        two.encode_query(text).expect("two").indices
    );
}

#[test]
fn stable_hash_uses_class_namespace() {
    let numeric = stable_sparse_index(SparseTokenClass::NumericExact, "1700");
    let alpha = stable_sparse_index(SparseTokenClass::Alphanumeric, "A1700");
    assert_ne!(numeric, alpha);
    assert_eq!(
        numeric,
        stable_sparse_index(SparseTokenClass::NumericExact, "1700")
    );
}
