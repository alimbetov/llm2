use crate::chunking::GeneratedChunk;
use chrono::{DateTime, Utc};
use ordered_float::OrderedFloat;
use rayon::prelude::*;
use rustc_hash::FxHashSet;
use serde_json::{json, Value};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap, HashSet};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum GraphRagError {
    #[error("graph rebuild timed out after {timeout_ms} ms")]
    RebuildTimeout { timeout_ms: u64 },
    #[error("semantic graph skipped: too many chunks, chunks={chunks}, max={max}")]
    TooManyChunks { chunks: usize, max: usize },
    #[error("orphan edge detected: node_id={node_id}")]
    OrphanEdge { node_id: Uuid },
    #[error("self-loop edge detected: node_id={node_id}")]
    SelfLoopEdge { node_id: Uuid },
    #[error(
        "duplicate edge detected: {source_node_id} -> {target_node_id}, relation={relation_type}"
    )]
    DuplicateEdge {
        source_node_id: Uuid,
        target_node_id: Uuid,
        relation_type: String,
    },
    #[error("invalid graph config: {message}")]
    InvalidConfig { message: String },
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("failed to build dedicated rayon pool: {message}")]
    ParallelPoolBuild { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GraphNodeType {
    Document,
    LogicalBlock,
    Chunk,
    EntityTag,
}
impl GraphNodeType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Document => "DOCUMENT",
            Self::LogicalBlock => "LOGICAL_BLOCK",
            Self::Chunk => "CHUNK",
            Self::EntityTag => "ENTITY_TAG",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GraphRelationType {
    DocumentContainsBlock,
    BlockContainsBlock,
    BlockProducedChunk,
    ChunkProducedByBlock,
    ChunkHasParent,
    ChunkPreviousSibling,
    ChunkNextSibling,
    ChunkSameTable,
    ChunkSemanticSimilar,
}
impl GraphRelationType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DocumentContainsBlock => "DOCUMENT_CONTAINS_BLOCK",
            Self::BlockContainsBlock => "BLOCK_CONTAINS_BLOCK",
            Self::BlockProducedChunk => "BLOCK_PRODUCED_CHUNK",
            Self::ChunkProducedByBlock => "CHUNK_PRODUCED_BY_BLOCK",
            Self::ChunkHasParent => "CHUNK_HAS_PARENT",
            Self::ChunkPreviousSibling => "CHUNK_PREVIOUS_SIBLING",
            Self::ChunkNextSibling => "CHUNK_NEXT_SIBLING",
            Self::ChunkSameTable => "CHUNK_SAME_TABLE",
            Self::ChunkSemanticSimilar => "CHUNK_SEMANTIC_SIMILAR",
        }
    }
    pub fn boost(self) -> f32 {
        match self {
            Self::ChunkHasParent => 0.90,
            Self::ChunkPreviousSibling | Self::ChunkNextSibling => 0.75,
            Self::ChunkSameTable => 0.60,
            Self::ChunkSemanticSimilar => 0.60,
            Self::BlockProducedChunk | Self::ChunkProducedByBlock => 0.70,
            _ => 0.50,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GraphNode {
    pub access_zone_id: Uuid,
    pub node_id: Uuid,
    pub node_type: GraphNodeType,
    pub external_id: String,
    pub document_id: Option<Uuid>,
    pub document_version: Option<i64>,
    pub chunk_id: Option<Uuid>,
    pub block_id: Option<String>,
    pub label: Option<String>,
    pub properties: Value,
    pub lifecycle_status: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub quarantined: bool,
    pub access_level: i16,
}

#[derive(Debug, Clone)]
pub struct GraphEdge {
    pub access_zone_id: Uuid,
    pub edge_id: Uuid,
    pub source_node_type: GraphNodeType,
    pub source_node_id: Uuid,
    pub target_node_type: GraphNodeType,
    pub target_node_id: Uuid,
    pub relation_type: GraphRelationType,
    pub relation_score: f32,
    pub relation_source: String,
    pub relation_rank: Option<i32>,
    pub document_id: Option<Uuid>,
    pub document_version: Option<i64>,
    pub lifecycle_status: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub quarantined: bool,
    pub properties: Value,
}

#[derive(Debug, Clone)]
pub struct RelatedChunk {
    /// Access zone of the related chunk. Required for multi-zone GraphRAG identity.
    pub access_zone_id: Uuid,
    pub chunk_id: Uuid,
    /// Access zone of the seed chunk that produced this expansion edge.
    pub seed_access_zone_id: Uuid,
    pub seed_chunk_id: Uuid,
    pub relation_type: GraphRelationType,
    pub relation_score: f32,
    pub relation_rank: Option<i32>,
    pub hop_distance: u32,
}

#[derive(Debug, Clone)]
pub struct GraphBuildLimits {
    pub max_document_graph_nodes: usize,
    pub max_document_graph_edges: usize,
    pub max_block_nodes: usize,
    pub max_chunk_nodes: usize,
    pub max_children_per_block: usize,
    pub max_same_parent_edges: usize,
    pub max_same_table_edges: usize,
    pub semantic_edges_enabled: bool,
    pub semantic_top_k_per_chunk: usize,
    pub semantic_min_score: f32,
    pub semantic_max_edges_per_document: usize,
    pub semantic_max_chunks_for_in_memory: usize,
    pub semantic_large_document_policy: String,
    pub semantic_normalize_embeddings: bool,
    pub semantic_parallel_enabled: bool,
    pub semantic_parallelism: usize,
    pub semantic_warn_build_time_ms: u64,
    pub semantic_rebuild_timeout_ms: u64,
}
impl Default for GraphBuildLimits {
    fn default() -> Self {
        Self {
            max_document_graph_nodes: 5000,
            max_document_graph_edges: 20000,
            max_block_nodes: 3000,
            max_chunk_nodes: 2000,
            max_children_per_block: 100,
            max_same_parent_edges: 5000,
            max_same_table_edges: 2000,
            semantic_edges_enabled: true,
            semantic_top_k_per_chunk: 3,
            semantic_min_score: 0.70,
            semantic_max_edges_per_document: 3000,
            semantic_max_chunks_for_in_memory: 500,
            semantic_large_document_policy: "SKIP_SEMANTIC".into(),
            semantic_normalize_embeddings: true,
            semantic_parallel_enabled: false,
            semantic_parallelism: 0,
            semantic_warn_build_time_ms: 3000,
            semantic_rebuild_timeout_ms: 30_000,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChunkEmbeddingForGraph {
    pub chunk_id: Uuid,
    pub embedding: Vec<f32>,
}

#[derive(Debug, Clone, Default)]
pub struct SemanticGraphBuildSummary {
    pub semantic_edges_created: usize,
    pub semantic_edges_skipped_by_score: usize,
    pub semantic_edges_skipped_by_limit: usize,
    pub semantic_edges_skipped_duplicate: usize,
    pub semantic_edges_skipped_no_embedding: usize,
    pub semantic_build_duration_ms: u128,
    pub semantic_avg_weight: Option<f32>,
    pub semantic_min_weight: Option<f32>,
    pub semantic_max_weight: Option<f32>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GraphScoringOptions {
    pub relation_weights: HashMap<String, f32>,
    pub default_structural_relation_weight: f32,
    pub default_semantic_relation_weight: f32,
    pub graph_hop_penalty: HashMap<String, f32>,
    pub graph_min_score: f32,
    pub structural_seed_score_floor: f32,
    pub semantic_power: f32,
}

impl Default for GraphScoringOptions {
    fn default() -> Self {
        Self {
            relation_weights: HashMap::from([
                ("CHUNK_HAS_PARENT".into(), 0.95),
                ("CHUNK_PREVIOUS_SIBLING".into(), 0.90),
                ("CHUNK_NEXT_SIBLING".into(), 0.90),
                ("CHUNK_SAME_TABLE".into(), 0.85),
                ("CHUNK_SEMANTIC_SIMILAR".into(), 0.60),
            ]),
            default_structural_relation_weight: 0.90,
            default_semantic_relation_weight: 0.60,
            graph_hop_penalty: HashMap::from([
                ("hop_1".into(), 1.00),
                ("hop_2".into(), 0.70),
                ("hop_3".into(), 0.50),
            ]),
            graph_min_score: 0.05,
            structural_seed_score_floor: 0.10,
            semantic_power: 1.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GraphBuildResult {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct LogicalBlockLite {
    block_id: String,
    parent_block_id: Option<String>,
    block_type: String,
    text: String,
    order_index: u32,
    block_level: Option<i32>,
    source_location: Value,
}

pub fn build_limited_structural_graph(
    access_zone_id: Uuid,
    document_id: Uuid,
    document_version: i64,
    metadata: &Value,
    chunks: &[GeneratedChunk],
    access_level: i16,
    ttl_days: Option<i32>,
    limits: &GraphBuildLimits,
) -> GraphBuildResult {
    let mut warnings = Vec::new();
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let expires_at = ttl_days.map(|days| Utc::now() + chrono::Duration::days(days as i64));

    let document_node_id = node_id(
        access_zone_id,
        GraphNodeType::Document,
        &format!("{}:{}", document_id, document_version),
    );
    nodes.push(GraphNode {
        access_zone_id,
        node_id: document_node_id,
        node_type: GraphNodeType::Document,
        external_id: format!("{}:{}", document_id, document_version),
        document_id: Some(document_id),
        document_version: Some(document_version),
        chunk_id: None,
        block_id: None,
        label: metadata
            .get("document_title")
            .and_then(Value::as_str)
            .map(str::to_string),
        properties: json!({"graph_version":"fix3_graph_lite"}),
        lifecycle_status: "ACTIVE".into(),
        expires_at,
        quarantined: false,
        access_level,
    });

    let blocks = logical_blocks_from_metadata(metadata);
    let selected_blocks = select_blocks_for_graph(&blocks, chunks, limits, &mut warnings);
    let mut block_node_ids = HashMap::new();
    for block in &selected_blocks {
        let id = node_id(
            access_zone_id,
            GraphNodeType::LogicalBlock,
            &format!("{}:{}:{}", document_id, document_version, block.block_id),
        );
        block_node_ids.insert(block.block_id.clone(), id);
        nodes.push(GraphNode {
            access_zone_id,
            node_id: id,
            node_type: GraphNodeType::LogicalBlock,
            external_id: format!("{}:{}:{}", document_id, document_version, block.block_id),
            document_id: Some(document_id),
            document_version: Some(document_version),
            chunk_id: None,
            block_id: Some(block.block_id.clone()),
            label: block.source_location.get("heading").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string).or_else(|| Some(block.text.chars().take(80).collect())),
            properties: json!({"block_type": block.block_type, "order_index": block.order_index, "block_level": block.block_level}),
            lifecycle_status: "ACTIVE".into(),
            expires_at,
            quarantined: false,
            access_level,
        });
    }

    for block in &selected_blocks {
        let Some(target) = block_node_ids.get(&block.block_id).copied() else {
            continue;
        };
        if let Some(parent) = &block.parent_block_id {
            if let Some(source) = block_node_ids.get(parent).copied() {
                push_edge(
                    &mut edges,
                    access_zone_id,
                    document_id,
                    document_version,
                    GraphNodeType::LogicalBlock,
                    source,
                    GraphNodeType::LogicalBlock,
                    target,
                    GraphRelationType::BlockContainsBlock,
                    1.0,
                    None,
                    expires_at,
                    &mut warnings,
                    limits.max_document_graph_edges,
                );
            } else {
                push_edge(
                    &mut edges,
                    access_zone_id,
                    document_id,
                    document_version,
                    GraphNodeType::Document,
                    document_node_id,
                    GraphNodeType::LogicalBlock,
                    target,
                    GraphRelationType::DocumentContainsBlock,
                    1.0,
                    None,
                    expires_at,
                    &mut warnings,
                    limits.max_document_graph_edges,
                );
            }
        } else {
            push_edge(
                &mut edges,
                access_zone_id,
                document_id,
                document_version,
                GraphNodeType::Document,
                document_node_id,
                GraphNodeType::LogicalBlock,
                target,
                GraphRelationType::DocumentContainsBlock,
                1.0,
                None,
                expires_at,
                &mut warnings,
                limits.max_document_graph_edges,
            );
        }
    }

    let mut chunk_node_ids = HashMap::new();
    for chunk in chunks
        .iter()
        .filter(|c| c.granularity.as_db_str() != "SOURCE")
        .take(limits.max_chunk_nodes)
    {
        let id = node_id(access_zone_id, GraphNodeType::Chunk, &chunk.id.to_string());
        chunk_node_ids.insert(chunk.id, id);
        nodes.push(GraphNode {
            access_zone_id,
            node_id: id,
            node_type: GraphNodeType::Chunk,
            external_id: chunk.id.to_string(),
            document_id: Some(document_id),
            document_version: Some(document_version),
            chunk_id: Some(chunk.id),
            block_id: chunk.source_block_id.clone(),
            label: Some(chunk.content.chars().take(80).collect()),
            properties: json!({"granularity": chunk.granularity.as_db_str(), "sequence_no": chunk.sequence_no, "trace_quality": chunk.trace_quality}),
            lifecycle_status: "ACTIVE".into(),
            expires_at,
            quarantined: false,
            access_level,
        });
    }
    if chunks.len() > limits.max_chunk_nodes {
        warnings.push("GRAPH_CHUNK_NODE_LIMIT_REACHED".into());
    }

    for chunk in chunks
        .iter()
        .filter(|c| c.granularity.as_db_str() != "SOURCE")
    {
        let Some(chunk_node) = chunk_node_ids.get(&chunk.id).copied() else {
            continue;
        };
        if let Some(block_id) = &chunk.source_block_id {
            if let Some(block_node) = block_node_ids.get(block_id).copied() {
                push_edge(
                    &mut edges,
                    access_zone_id,
                    document_id,
                    document_version,
                    GraphNodeType::LogicalBlock,
                    block_node,
                    GraphNodeType::Chunk,
                    chunk_node,
                    GraphRelationType::BlockProducedChunk,
                    1.0,
                    None,
                    expires_at,
                    &mut warnings,
                    limits.max_document_graph_edges,
                );
                push_edge(
                    &mut edges,
                    access_zone_id,
                    document_id,
                    document_version,
                    GraphNodeType::Chunk,
                    chunk_node,
                    GraphNodeType::LogicalBlock,
                    block_node,
                    GraphRelationType::ChunkProducedByBlock,
                    1.0,
                    None,
                    expires_at,
                    &mut warnings,
                    limits.max_document_graph_edges,
                );
            }
        }
        if let Some(parent_id) = chunk.parent_id {
            if let Some(parent_node) = chunk_node_ids.get(&parent_id).copied() {
                push_edge(
                    &mut edges,
                    access_zone_id,
                    document_id,
                    document_version,
                    GraphNodeType::Chunk,
                    chunk_node,
                    GraphNodeType::Chunk,
                    parent_node,
                    GraphRelationType::ChunkHasParent,
                    1.0,
                    Some(0),
                    expires_at,
                    &mut warnings,
                    limits.max_document_graph_edges,
                );
            }
        }
    }
    add_adjacent_sibling_edges(
        access_zone_id,
        document_id,
        document_version,
        chunks,
        &chunk_node_ids,
        expires_at,
        limits,
        &mut edges,
        &mut warnings,
    );
    add_same_table_edges(
        access_zone_id,
        document_id,
        document_version,
        chunks,
        &chunk_node_ids,
        expires_at,
        limits,
        &mut edges,
        &mut warnings,
    );

    if nodes.len() > limits.max_document_graph_nodes {
        nodes.truncate(limits.max_document_graph_nodes);
        warnings.push("GRAPH_NODE_LIMIT_REACHED".into());
    }
    filter_edges_to_existing_nodes(&nodes, &mut edges, &mut warnings);
    if edges.len() > limits.max_document_graph_edges {
        edges.truncate(limits.max_document_graph_edges);
        warnings.push("GRAPH_EDGE_LIMIT_REACHED".into());
    }
    GraphBuildResult {
        nodes,
        edges,
        warnings,
    }
}

fn logical_blocks_from_metadata(metadata: &Value) -> Vec<LogicalBlockLite> {
    let raw = metadata.get("logical_blocks").and_then(Value::as_str);
    let parsed = raw.and_then(|s| serde_json::from_str::<Value>(s).ok());
    let Some(items) = parsed.as_ref().and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let block_id = item
                .get("block_id")
                .and_then(Value::as_str)?
                .trim()
                .to_string();
            if block_id.is_empty() {
                return None;
            }
            let metadata = item.get("metadata").cloned().unwrap_or_else(|| json!({}));
            let block_level = metadata
                .get("block_level")
                .and_then(Value::as_i64)
                .map(|v| v as i32)
                .or_else(|| {
                    metadata
                        .get("level")
                        .and_then(Value::as_i64)
                        .map(|v| v as i32)
                });
            Some(LogicalBlockLite {
                block_id,
                parent_block_id: item
                    .get("parent_block_id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
                block_type: item
                    .get("block_type_name")
                    .and_then(Value::as_str)
                    .unwrap_or("UNSPECIFIED")
                    .to_string(),
                text: item
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                order_index: item.get("order_index").and_then(Value::as_u64).unwrap_or(0) as u32,
                block_level,
                source_location: item
                    .get("source_location")
                    .cloned()
                    .unwrap_or_else(|| json!({})),
            })
        })
        .collect()
}

fn select_blocks_for_graph<'a>(
    blocks: &'a [LogicalBlockLite],
    chunks: &[GeneratedChunk],
    limits: &GraphBuildLimits,
    warnings: &mut Vec<String>,
) -> Vec<&'a LogicalBlockLite> {
    let mut by_parent: HashMap<Option<String>, Vec<&LogicalBlockLite>> = HashMap::new();
    for b in blocks {
        by_parent
            .entry(b.parent_block_id.clone())
            .or_default()
            .push(b);
    }
    for v in by_parent.values_mut() {
        v.sort_by_key(|b| b.order_index);
    }
    let chunk_blocks: HashSet<String> = chunks
        .iter()
        .filter_map(|c| c.source_block_id.clone())
        .collect();
    let mut depth_cache = HashMap::new();
    let mut scored = blocks
        .iter()
        .map(|b| {
            let depth = b
                .block_level
                .unwrap_or_else(|| compute_depth(&b.block_id, blocks, &mut depth_cache));
            let base = match b.block_type.as_str() {
                "DOCUMENT" => 0,
                "SECTION" => 10,
                "SUBSECTION" => 20,
                "TABLE" => 30,
                "PARAGRAPH" | "TABLE_ROW" | "LIST_ITEM" | "FAQ_ITEM" => 40,
                _ => 90,
            };
            let has_chunk_penalty = if chunk_blocks.contains(&b.block_id) {
                0
            } else {
                1
            };
            (base + depth, has_chunk_penalty, b.order_index, b)
        })
        .collect::<Vec<_>>();
    scored.sort_by_key(|x| (x.0, x.1, x.2));
    let mut parent_counts: HashMap<Option<String>, usize> = HashMap::new();
    let mut selected = Vec::new();
    for (_, _, _, block) in scored {
        let count = parent_counts
            .entry(block.parent_block_id.clone())
            .or_default();
        if *count >= limits.max_children_per_block
            && !matches!(
                block.block_type.as_str(),
                "DOCUMENT" | "SECTION" | "SUBSECTION"
            )
        {
            continue;
        }
        *count += 1;
        selected.push(block);
        if selected.len() >= limits.max_block_nodes {
            break;
        }
    }
    if blocks.len() > selected.len() {
        warnings.push("GRAPH_NODE_LIMIT_REACHED".into());
    }
    selected
}

fn compute_depth(
    block_id: &str,
    blocks: &[LogicalBlockLite],
    cache: &mut HashMap<String, i32>,
) -> i32 {
    if let Some(v) = cache.get(block_id) {
        return *v;
    }
    let map: HashMap<&str, Option<&str>> = blocks
        .iter()
        .map(|b| (b.block_id.as_str(), b.parent_block_id.as_deref()))
        .collect();
    let mut depth = 0;
    let mut cur = block_id;
    let mut seen = HashSet::new();
    while let Some(Some(parent)) = map.get(cur) {
        if !seen.insert(cur.to_string()) {
            break;
        }
        depth += 1;
        cur = parent;
    }
    cache.insert(block_id.to_string(), depth);
    depth
}

#[allow(clippy::too_many_arguments)]
fn push_edge(
    edges: &mut Vec<GraphEdge>,
    access_zone_id: Uuid,
    document_id: Uuid,
    document_version: i64,
    source_node_type: GraphNodeType,
    source_node_id: Uuid,
    target_node_type: GraphNodeType,
    target_node_id: Uuid,
    relation_type: GraphRelationType,
    score: f32,
    rank: Option<i32>,
    expires_at: Option<DateTime<Utc>>,
    warnings: &mut Vec<String>,
    max_edges: usize,
) {
    if edges.len() >= max_edges {
        if !warnings.iter().any(|w| w == "GRAPH_EDGE_LIMIT_REACHED") {
            warnings.push("GRAPH_EDGE_LIMIT_REACHED".into());
        }
        return;
    }
    let edge_key = format!(
        "{}:{}:{}:{}",
        source_node_id,
        target_node_id,
        relation_type.as_str(),
        rank.unwrap_or(0)
    );
    edges.push(GraphEdge {
        access_zone_id,
        edge_id: Uuid::new_v5(&Uuid::NAMESPACE_URL, edge_key.as_bytes()),
        source_node_type,
        source_node_id,
        target_node_type,
        target_node_id,
        relation_type,
        relation_score: score,
        relation_source: "STRUCTURAL".into(),
        relation_rank: rank,
        document_id: Some(document_id),
        document_version: Some(document_version),
        lifecycle_status: "ACTIVE".into(),
        expires_at,
        quarantined: false,
        properties: json!({}),
    });
}

fn add_adjacent_sibling_edges(
    access_zone_id: Uuid,
    document_id: Uuid,
    document_version: i64,
    chunks: &[GeneratedChunk],
    chunk_node_ids: &HashMap<Uuid, Uuid>,
    expires_at: Option<DateTime<Utc>>,
    limits: &GraphBuildLimits,
    edges: &mut Vec<GraphEdge>,
    warnings: &mut Vec<String>,
) {
    let mut groups: BTreeMap<Uuid, Vec<&GeneratedChunk>> = BTreeMap::new();
    for c in chunks
        .iter()
        .filter(|c| matches!(c.granularity.as_db_str(), "SUB_180" | "SUB_260"))
    {
        if let Some(parent) = c.parent_id {
            groups.entry(parent).or_default().push(c);
        }
    }
    let mut count = 0usize;
    for group in groups.values_mut() {
        group.sort_by_key(|c| c.sequence_no);
        for pair in group.windows(2) {
            if count + 2 > limits.max_same_parent_edges {
                warnings.push("GRAPH_EDGE_LIMIT_REACHED".into());
                return;
            }
            let Some(a) = chunk_node_ids.get(&pair[0].id).copied() else {
                continue;
            };
            let Some(b) = chunk_node_ids.get(&pair[1].id).copied() else {
                continue;
            };
            push_edge(
                edges,
                access_zone_id,
                document_id,
                document_version,
                GraphNodeType::Chunk,
                a,
                GraphNodeType::Chunk,
                b,
                GraphRelationType::ChunkNextSibling,
                1.0,
                Some(1),
                expires_at,
                warnings,
                limits.max_document_graph_edges,
            );
            push_edge(
                edges,
                access_zone_id,
                document_id,
                document_version,
                GraphNodeType::Chunk,
                b,
                GraphNodeType::Chunk,
                a,
                GraphRelationType::ChunkPreviousSibling,
                1.0,
                Some(-1),
                expires_at,
                warnings,
                limits.max_document_graph_edges,
            );
            count += 2;
        }
    }
}

fn is_row_style_table_chunk(c: &GeneratedChunk) -> bool {
    let has_table_id = c
        .source_location
        .get("table_id")
        .and_then(Value::as_str)
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let has_row_index = c.source_location.get("row_index").is_some();
    let block_type = c
        .source_location
        .get("block_type")
        .and_then(Value::as_str)
        .unwrap_or("");
    has_table_id && (has_row_index || block_type.eq_ignore_ascii_case("TABLE_ROW"))
}

fn filter_edges_to_existing_nodes(
    nodes: &[GraphNode],
    edges: &mut Vec<GraphEdge>,
    warnings: &mut Vec<String>,
) {
    let node_ids: HashSet<Uuid> = nodes.iter().map(|n| n.node_id).collect();
    let before = edges.len();
    edges.retain(|e| {
        e.source_node_id != e.target_node_id
            && node_ids.contains(&e.source_node_id)
            && node_ids.contains(&e.target_node_id)
    });
    if edges.len() != before {
        warnings.push(format!(
            "GRAPH_ORPHAN_OR_SELF_LOOP_EDGES_DROPPED:{}",
            before - edges.len()
        ));
    }
}

pub fn l2_normalize(input: &[f32]) -> Option<Vec<f32>> {
    if input.is_empty() {
        return None;
    }
    let norm_sq: f32 = input.iter().map(|v| v * v).sum();
    if norm_sq <= f32::EPSILON {
        return None;
    }
    let norm = norm_sq.sqrt();
    Some(input.iter().map(|v| *v / norm).collect())
}

pub fn dot_product(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.is_empty() || a.len() != b.len() {
        return None;
    }
    Some(a.iter().zip(b.iter()).map(|(x, y)| x * y).sum())
}

pub fn relation_weight(scoring: &GraphScoringOptions, relation_type: GraphRelationType) -> f32 {
    scoring
        .relation_weights
        .get(relation_type.as_str())
        .copied()
        .unwrap_or_else(|| {
            if relation_type == GraphRelationType::ChunkSemanticSimilar {
                scoring.default_semantic_relation_weight
            } else {
                scoring.default_structural_relation_weight
            }
        })
}

pub fn hop_penalty(scoring: &GraphScoringOptions, hop_distance: u32) -> f32 {
    scoring
        .graph_hop_penalty
        .get(&format!("hop_{hop_distance}"))
        .copied()
        .unwrap_or(0.50)
}

pub fn score_graph_candidate_with_options(
    seed_score: f32,
    relation_type: GraphRelationType,
    relation_score: f32,
    hop_distance: u32,
    scoring: &GraphScoringOptions,
) -> f32 {
    let semantic_relation = relation_type == GraphRelationType::ChunkSemanticSimilar;
    let adjusted_edge = if semantic_relation {
        relation_score.powf(scoring.semantic_power)
    } else {
        relation_score
    };
    let effective_seed_score = if semantic_relation {
        seed_score
    } else {
        seed_score.max(scoring.structural_seed_score_floor)
    };
    effective_seed_score
        * adjusted_edge
        * relation_weight(scoring, relation_type)
        * hop_penalty(scoring, hop_distance)
}

pub fn build_semantic_edges_in_memory(
    access_zone_id: Uuid,
    document_id: Uuid,
    document_version: i64,
    nodes: &[GraphNode],
    embeddings: &[ChunkEmbeddingForGraph],
    access_level: i16,
    ttl_days: Option<i32>,
    limits: &GraphBuildLimits,
) -> Result<(Vec<GraphEdge>, SemanticGraphBuildSummary), GraphRagError> {
    let started = std::time::Instant::now();
    let mut summary = SemanticGraphBuildSummary::default();
    if !limits.semantic_edges_enabled {
        return Ok((Vec::new(), summary));
    }
    if embeddings.len() > limits.semantic_max_chunks_for_in_memory {
        match limits.semantic_large_document_policy.as_str() {
            "FAIL_INDEXING" => {
                metrics::counter!(
                    "graph_semantic_documents_skipped_large_total",
                    "policy" => limits.semantic_large_document_policy.clone()
                )
                .increment(1);
                return Err(GraphRagError::TooManyChunks {
                    chunks: embeddings.len(),
                    max: limits.semantic_max_chunks_for_in_memory,
                });
            }
            "QDRANT_BACKEND" => summary
                .warnings
                .push("SEMANTIC_GRAPH_QDRANT_BACKEND_NOT_IMPLEMENTED".into()),
            "STRUCTURAL_ONLY" => summary
                .warnings
                .push("SEMANTIC_GRAPH_STRUCTURAL_ONLY_TOO_MANY_CHUNKS".into()),
            _ => summary
                .warnings
                .push("SEMANTIC_GRAPH_SKIPPED_TOO_MANY_CHUNKS".into()),
        }
        metrics::counter!(
            "graph_semantic_documents_skipped_large_total",
            "policy" => limits.semantic_large_document_policy.clone()
        )
        .increment(1);
        tracing::warn!(
            chunks = embeddings.len(),
            max = limits.semantic_max_chunks_for_in_memory,
            policy = limits.semantic_large_document_policy.as_str(),
            "SEMANTIC_GRAPH_SKIPPED_TOO_MANY_CHUNKS"
        );
        return Ok((Vec::new(), summary));
    }
    let chunk_node_by_chunk_id: HashMap<Uuid, Uuid> = nodes
        .iter()
        .filter(|n| n.node_type == GraphNodeType::Chunk)
        .filter_map(|n| n.chunk_id.map(|chunk_id| (chunk_id, n.node_id)))
        .collect();

    let mut prepared: Vec<(Uuid, Uuid, Vec<f32>)> = Vec::with_capacity(embeddings.len());
    for e in embeddings {
        let Some(node_id) = chunk_node_by_chunk_id.get(&e.chunk_id).copied() else {
            continue;
        };
        let vector_opt = if limits.semantic_normalize_embeddings {
            l2_normalize(&e.embedding)
        } else {
            Some(e.embedding.clone())
        };
        let Some(vector) = vector_opt else {
            summary.semantic_edges_skipped_no_embedding += 1;
            continue;
        };
        prepared.push((e.chunk_id, node_id, vector));
    }
    if prepared.len() < 2 {
        summary
            .warnings
            .push("SEMANTIC_GRAPH_SKIPPED_NO_EMBEDDINGS".into());
        return Ok((Vec::new(), summary));
    }
    let expires_at = ttl_days.map(|days| Utc::now() + chrono::Duration::days(days as i64));
    let estimated_capacity = (prepared.len() * limits.semantic_top_k_per_chunk)
        .min(limits.semantic_max_edges_per_document);
    let source_edges: Vec<(Vec<GraphEdge>, usize)> = if !limits.semantic_parallel_enabled {
        metrics::counter!("graph_semantic_parallel_mode_total", "mode" => "sequential")
            .increment(1);
        prepared
            .iter()
            .map(|source| {
                build_semantic_edges_for_source(
                    access_zone_id,
                    document_id,
                    document_version,
                    source,
                    &prepared,
                    access_level,
                    expires_at,
                    limits,
                )
            })
            .collect()
    } else if limits.semantic_parallelism == 0 {
        metrics::counter!("graph_semantic_parallel_mode_total", "mode" => "global_pool")
            .increment(1);
        prepared
            .par_iter()
            .map(|source| {
                build_semantic_edges_for_source(
                    access_zone_id,
                    document_id,
                    document_version,
                    source,
                    &prepared,
                    access_level,
                    expires_at,
                    limits,
                )
            })
            .collect()
    } else {
        metrics::counter!("graph_semantic_parallel_mode_total", "mode" => "dedicated_pool")
            .increment(1);
        metrics::counter!("graph_semantic_dedicated_rayon_pool_used_total").increment(1);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(limits.semantic_parallelism)
            .build()
            .map_err(|e| GraphRagError::ParallelPoolBuild {
                message: e.to_string(),
            })?;
        pool.install(|| {
            prepared
                .par_iter()
                .map(|source| {
                    build_semantic_edges_for_source(
                        access_zone_id,
                        document_id,
                        document_version,
                        source,
                        &prepared,
                        access_level,
                        expires_at,
                        limits,
                    )
                })
                .collect()
        })
    };
    let mut edges = Vec::with_capacity(estimated_capacity);
    let mut seen: FxHashSet<(Uuid, Uuid, &'static str)> = FxHashSet::default();
    let mut weights = Vec::with_capacity(estimated_capacity);
    for (candidate_edges, skipped_by_score) in source_edges {
        summary.semantic_edges_skipped_by_score += skipped_by_score;
        for edge in candidate_edges {
            if edges.len() >= limits.semantic_max_edges_per_document {
                summary.semantic_edges_skipped_by_limit += 1;
                break;
            }
            let key = (
                edge.source_node_id,
                edge.target_node_id,
                GraphRelationType::ChunkSemanticSimilar.as_str(),
            );
            if !seen.insert(key) {
                summary.semantic_edges_skipped_duplicate += 1;
                continue;
            }
            weights.push(edge.relation_score);
            edges.push(edge);
        }
        if edges.len() >= limits.semantic_max_edges_per_document {
            break;
        }
    }
    summary.semantic_edges_created = edges.len();
    summary.semantic_build_duration_ms = started.elapsed().as_millis();
    if !weights.is_empty() {
        let sum: f32 = weights.iter().sum();
        summary.semantic_avg_weight = Some(sum / weights.len() as f32);
        summary.semantic_min_weight = weights.iter().copied().reduce(f32::min);
        summary.semantic_max_weight = weights.iter().copied().reduce(f32::max);
    }
    if summary.semantic_build_duration_ms > limits.semantic_warn_build_time_ms as u128 {
        summary.warnings.push("SEMANTIC_GRAPH_BUILD_SLOW".into());
    }
    Ok((edges, summary))
}

fn build_semantic_edges_for_source(
    access_zone_id: Uuid,
    document_id: Uuid,
    document_version: i64,
    source: &(Uuid, Uuid, Vec<f32>),
    prepared: &[(Uuid, Uuid, Vec<f32>)],
    access_level: i16,
    expires_at: Option<DateTime<Utc>>,
    limits: &GraphBuildLimits,
) -> (Vec<GraphEdge>, usize) {
    let (source_chunk_id, source_node_id, source_vec) = source;
    let mut skipped_by_score = 0usize;
    let k = limits.semantic_top_k_per_chunk.max(1);
    let mut heap: BinaryHeap<Reverse<(OrderedFloat<f32>, Uuid)>> = BinaryHeap::with_capacity(k);
    for (target_chunk_id, target_node_id, target_vec) in prepared {
        if source_chunk_id == target_chunk_id {
            continue;
        }
        let Some(score) = dot_product(source_vec, target_vec) else {
            continue;
        };
        if score < limits.semantic_min_score {
            skipped_by_score += 1;
            continue;
        }
        let ordered = OrderedFloat(score);
        if heap.len() < k {
            heap.push(Reverse((ordered, *target_node_id)));
        } else if ordered
            > heap
                .peek()
                .map(|x| x.0 .0)
                .unwrap_or(OrderedFloat(f32::NEG_INFINITY))
        {
            heap.pop();
            heap.push(Reverse((ordered, *target_node_id)));
        }
    }
    metrics::counter!("graph_semantic_topk_heap_used_total").increment(1);
    let mut candidates: Vec<(Uuid, f32)> = heap
        .into_iter()
        .map(|Reverse((score, target_node_id))| (target_node_id, score.0))
        .collect();
    candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut edges = Vec::with_capacity(k);
    for (rank, (target_node_id, score)) in candidates.into_iter().enumerate() {
        let edge_key = format!(
            "{}:{}:{}",
            source_node_id,
            target_node_id,
            GraphRelationType::ChunkSemanticSimilar.as_str()
        );
        edges.push(GraphEdge {
            access_zone_id,
            edge_id: Uuid::new_v5(&Uuid::NAMESPACE_URL, edge_key.as_bytes()),
            source_node_type: GraphNodeType::Chunk,
            source_node_id: *source_node_id,
            target_node_type: GraphNodeType::Chunk,
            target_node_id,
            relation_type: GraphRelationType::ChunkSemanticSimilar,
            relation_score: score,
            relation_source: if limits.semantic_parallel_enabled {
                "SEMANTIC_IN_MEMORY_RAYON"
            } else {
                "SEMANTIC_IN_MEMORY"
            }
            .into(),
            relation_rank: Some(rank as i32 + 1),
            document_id: Some(document_id),
            document_version: Some(document_version),
            lifecycle_status: "ACTIVE".into(),
            expires_at,
            quarantined: false,
            properties: json!({
                "semantic_backend":"IN_MEMORY",
                "parallel_enabled": limits.semantic_parallel_enabled,
                "normalized":limits.semantic_normalize_embeddings,
                "semantic_min_score":limits.semantic_min_score,
                "access_level":access_level
            }),
        });
    }
    (edges, skipped_by_score)
}

fn add_same_table_edges(
    access_zone_id: Uuid,
    document_id: Uuid,
    document_version: i64,
    chunks: &[GeneratedChunk],
    chunk_node_ids: &HashMap<Uuid, Uuid>,
    expires_at: Option<DateTime<Utc>>,
    limits: &GraphBuildLimits,
    edges: &mut Vec<GraphEdge>,
    warnings: &mut Vec<String>,
) {
    let mut groups: BTreeMap<String, Vec<&GeneratedChunk>> = BTreeMap::new();
    for c in chunks.iter().filter(|c| is_row_style_table_chunk(c)) {
        let table_id = c
            .source_location
            .get("table_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if !table_id.is_empty() {
            groups.entry(table_id.to_string()).or_default().push(c);
        }
    }
    let mut count = 0usize;
    for group in groups.values_mut() {
        group.sort_by_key(|c| {
            c.source_location
                .get("row_index")
                .and_then(Value::as_u64)
                .unwrap_or(c.sequence_no as u64)
        });
        for pair in group.windows(2) {
            if count + 2 > limits.max_same_table_edges {
                warnings.push("GRAPH_SAME_TABLE_EDGE_LIMIT_REACHED".into());
                return;
            }
            let Some(a) = chunk_node_ids.get(&pair[0].id).copied() else {
                continue;
            };
            let Some(b) = chunk_node_ids.get(&pair[1].id).copied() else {
                continue;
            };
            push_edge(
                edges,
                access_zone_id,
                document_id,
                document_version,
                GraphNodeType::Chunk,
                a,
                GraphNodeType::Chunk,
                b,
                GraphRelationType::ChunkSameTable,
                1.0,
                Some(1),
                expires_at,
                warnings,
                limits.max_document_graph_edges,
            );
            push_edge(
                edges,
                access_zone_id,
                document_id,
                document_version,
                GraphNodeType::Chunk,
                b,
                GraphNodeType::Chunk,
                a,
                GraphRelationType::ChunkSameTable,
                1.0,
                Some(-1),
                expires_at,
                warnings,
                limits.max_document_graph_edges,
            );
            count += 2;
        }
    }
}

fn node_id(zone: Uuid, t: GraphNodeType, external_id: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!(
            "astravector-graph-node:{}:{}:{}",
            zone,
            t.as_str(),
            external_id
        )
        .as_bytes(),
    )
}

pub fn score_graph_candidate(
    seed_score: f32,
    relation_type: GraphRelationType,
    relation_score: f32,
) -> f32 {
    seed_score * relation_type.boost() * relation_score
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunking::{GeneratedChunk, Granularity};

    fn chunk(id: Uuid, parent: Option<Uuid>, seq: u32, block: &str) -> GeneratedChunk {
        GeneratedChunk {
            id,
            root_id: Uuid::nil(),
            source_id: Uuid::nil(),
            parent_id: parent,
            granularity: Granularity::Sub260,
            sequence_no: seq,
            token_count: 10,
            content: "text".into(),
            content_hash: "h".into(),
            source_block_id: Some(block.into()),
            source_block_ids: vec![block.into()],
            source_location: json!({}),
            source_links: json!([]),
            trace_relation_type: "DIRECT".into(),
            trace_quality: "EXACT".into(),
        }
    }

    #[test]
    fn graph_score_uses_boost() {
        let s = score_graph_candidate(1.0, GraphRelationType::ChunkNextSibling, 0.8);
        assert!((s - 0.6).abs() < 0.0001);
    }

    #[test]
    fn structural_graph_score_uses_seed_floor() {
        let scoring = GraphScoringOptions::default();
        let s = score_graph_candidate_with_options(
            0.01,
            GraphRelationType::ChunkNextSibling,
            0.92,
            1,
            &scoring,
        );
        assert!(s >= scoring.graph_min_score);
    }

    #[test]
    fn semantic_graph_score_does_not_use_structural_seed_floor() {
        let scoring = GraphScoringOptions::default();
        let s = score_graph_candidate_with_options(
            0.01,
            GraphRelationType::ChunkSemanticSimilar,
            0.92,
            1,
            &scoring,
        );
        assert!(s < scoring.graph_min_score);
    }

    #[test]
    fn adjacent_sibling_edges_are_linear() {
        let zone = Uuid::new_v4();
        let doc = Uuid::new_v4();
        let parent = Uuid::new_v4();
        let chunks = (0..5)
            .map(|i| chunk(Uuid::new_v4(), Some(parent), i, "b"))
            .collect::<Vec<_>>();
        let mut ids = HashMap::new();
        for c in &chunks {
            ids.insert(c.id, Uuid::new_v4());
        }
        let mut edges = Vec::new();
        let mut warnings = Vec::new();
        add_adjacent_sibling_edges(
            zone,
            doc,
            1,
            &chunks,
            &ids,
            None,
            &GraphBuildLimits::default(),
            &mut edges,
            &mut warnings,
        );
        assert_eq!(edges.len(), 8);
    }
}
