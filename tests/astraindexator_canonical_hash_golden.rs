use astravector_runtime::pb;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

const FIXTURE: &str = include_str!("fixtures/astraindexator/canonical-hash-v1.json");

fn source_location_from_json(value: &Value) -> Option<pb::SourceLocation> {
    let obj = value.as_object()?;
    if obj.is_empty() {
        return None;
    }
    Some(pb::SourceLocation {
        page_start: obj.get("page_start").and_then(Value::as_u64).unwrap_or(0) as u32,
        page_end: obj.get("page_end").and_then(Value::as_u64).unwrap_or(0) as u32,
        char_start: obj.get("char_start").and_then(Value::as_u64).unwrap_or(0) as u32,
        char_end: obj.get("char_end").and_then(Value::as_u64).unwrap_or(0) as u32,
        section_path: obj
            .get("section_path")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        heading: obj
            .get("heading")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        table_id: obj
            .get("table_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        row_index: obj.get("row_index").and_then(Value::as_u64).unwrap_or(0) as u32,
        column_index: obj
            .get("column_index")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
    })
}

fn string_map(value: Option<&Value>) -> HashMap<String, String> {
    value
        .and_then(Value::as_object)
        .map(|obj| {
            obj.iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        value.as_str().unwrap_or_default().to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn source_links_from_json(value: &Value) -> Vec<pb::SourceLink> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .map(|link| pb::SourceLink {
            r#type: link.get("type").and_then(Value::as_i64).unwrap_or(0) as i32,
            url: link
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            label: link
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            mime_type: link
                .get("mime_type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            requires_auth: link
                .get("requires_auth")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            expires_at: link
                .get("expires_at")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            attributes: string_map(link.get("attributes")),
        })
        .collect()
}

fn block_from_json(value: &Value) -> pb::LogicalBlock {
    pb::LogicalBlock {
        block_id: value
            .get("block_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        parent_block_id: value
            .get("parent_block_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        block_type: value
            .get("block_type")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        text: value
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        order_index: value
            .get("order_index")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
        source_location: source_location_from_json(&value["source_location"]),
        source_links: source_links_from_json(&value["source_links"]),
        metadata: string_map(value.get("metadata")),
    }
}

fn logical_block_to_json(block: &pb::LogicalBlock) -> Value {
    let source_location = block
        .source_location
        .as_ref()
        .map(|location| {
            json!({
                "page_start": location.page_start,
                "page_end": location.page_end,
                "char_start": location.char_start,
                "char_end": location.char_end,
                "section_path": &location.section_path,
                "heading": &location.heading,
                "table_id": &location.table_id,
                "row_index": location.row_index,
                "column_index": location.column_index,
            })
        })
        .unwrap_or_else(|| Value::Object(Default::default()));
    let source_links = Value::Array(
        block
            .source_links
            .iter()
            .map(|link| {
                json!({
                    "type": link.r#type,
                    "url": &link.url,
                    "label": &link.label,
                    "mime_type": &link.mime_type,
                    "requires_auth": link.requires_auth,
                    "expires_at": &link.expires_at,
                    "attributes": &link.attributes,
                })
            })
            .collect(),
    );
    json!({
        "block_id": &block.block_id,
        "parent_block_id": &block.parent_block_id,
        "block_type": block.block_type,
        "text": &block.text,
        "order_index": block.order_index,
        "metadata": &block.metadata,
        "source_location": source_location,
        "source_links": source_links,
    })
}

fn compute_batch_content_hash(blocks: &[pb::LogicalBlock]) -> String {
    let values = blocks.iter().map(logical_block_to_json).collect::<Vec<_>>();
    let bytes = serde_json::to_vec(&values).expect("serialize canonical logical block array");
    format!("{:x}", Sha256::digest(bytes))
}

fn render_final_content_text(blocks: &[pb::LogicalBlock]) -> String {
    let mut out = String::new();
    for block in blocks {
        if let Some(location) = block.source_location.as_ref() {
            if !location.heading.trim().is_empty() {
                out.push_str(location.heading.trim());
                out.push('\n');
            }
        }
        out.push_str(block.text.trim());
        out.push_str("\n\n");
    }
    out
}

#[test]
fn astraindexator_and_astravector_share_byte_exact_hash_vectors() {
    let fixture: Value = serde_json::from_str(FIXTURE).expect("parse golden fixture");
    let blocks = fixture["blocks"]
        .as_array()
        .expect("blocks array")
        .iter()
        .map(block_from_json)
        .collect::<Vec<_>>();

    let expected_batch = fixture["expected"]["batchContentHash"]
        .as_str()
        .expect("expected batch hash");
    assert_eq!(compute_batch_content_hash(&blocks), expected_batch);

    let rendered = render_final_content_text(&blocks);
    let expected_rendered = fixture["expected"]["finalRenderedText"]
        .as_str()
        .expect("expected final rendered text");
    assert_eq!(rendered, expected_rendered);

    let expected_final = fixture["expected"]["finalContentHash"]
        .as_str()
        .expect("expected final hash");
    assert_eq!(format!("{:x}", Sha256::digest(rendered.as_bytes())), expected_final);
}
