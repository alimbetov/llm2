use crate::error::AstraError;
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    Source,
    Parent,
    Sub180,
    Sub260,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceChunkStorageMode {
    FullText,
    MetadataOnly,
    Disabled,
}
impl SourceChunkStorageMode {
    pub fn from_config(value: &str) -> Result<Self, AstraError> {
        match value {
            "FULL_TEXT" => Ok(Self::FullText),
            "METADATA_ONLY" => Ok(Self::MetadataOnly),
            "DISABLED" => Ok(Self::Disabled),
            other => Err(AstraError::InvalidArgument(format!(
                "unsupported source_chunk_storage_mode={other}; expected FULL_TEXT, METADATA_ONLY, or DISABLED"
            ))),
        }
    }
}
impl Granularity {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::Source => "SOURCE",
            Self::Parent => "PARENT",
            Self::Sub180 => "SUB_180",
            Self::Sub260 => "SUB_260",
        }
    }
}
#[derive(Debug, Clone)]
pub struct SizeProfile {
    pub target: usize,
    pub min: usize,
    pub max: usize,
    pub overlap: usize,
}
impl SizeProfile {
    pub fn validate(&self) -> Result<(), AstraError> {
        if self.min == 0
            || self.min >= self.target
            || self.target >= self.max
            || self.overlap >= self.min
        {
            return Err(AstraError::InvalidArgument(
                "invalid chunk profile: require 0 < min < target < max and overlap < min".into(),
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Clone)]
pub struct ChunkingProfile {
    pub version: String,
    pub parent: SizeProfile,
    pub sub180: SizeProfile,
    pub sub260: SizeProfile,
}
#[derive(Debug, Clone)]
pub struct GeneratedChunk {
    pub id: Uuid,
    pub root_id: Uuid,
    pub source_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub granularity: Granularity,
    pub sequence_no: u32,
    pub token_count: usize,
    pub content: String,
    pub content_hash: String,
    pub source_block_id: Option<String>,
    pub source_block_ids: Vec<String>,
    pub source_location: Value,
    pub source_links: Value,
    pub trace_relation_type: String,
    pub trace_quality: String,
}

#[derive(Debug, Clone)]
pub struct AnnotatedTextSegment {
    pub block_id: String,
    pub parent_block_id: Option<String>,
    pub block_type: String,
    pub text: String,
    pub source_location: Value,
    pub source_links: Value,
    pub metadata: Value,
    pub order_index: u32,
}

#[derive(Debug, Clone)]
pub struct ChunkSourceTrace {
    pub primary_block_id: Option<String>,
    pub source_block_ids: Vec<String>,
    pub source_location: Value,
    pub source_links: Value,
    pub relation_type: String,
    pub trace_quality: String,
}
impl ChunkSourceTrace {
    pub fn missing() -> Self {
        Self {
            primary_block_id: None,
            source_block_ids: Vec::new(),
            source_location: Value::Object(Default::default()),
            source_links: Value::Array(Vec::new()),
            relation_type: "SYNTHETIC".into(),
            trace_quality: "MISSING".into(),
        }
    }
    pub fn exact(segment: &AnnotatedTextSegment, relation_type: &str) -> Self {
        Self {
            primary_block_id: Some(segment.block_id.clone()),
            source_block_ids: vec![segment.block_id.clone()],
            source_location: segment.source_location.clone(),
            source_links: segment.source_links.clone(),
            relation_type: relation_type.into(),
            trace_quality: "EXACT".into(),
        }
    }
    pub fn parent_context(segments: &[AnnotatedTextSegment]) -> Self {
        if segments.is_empty() {
            return Self::missing();
        }
        let Some(primary) = segments
            .iter()
            .max_by_key(|s| specificity_score(&s.block_type) * 1_000_000 + s.text.len() as i32)
        else {
            return Self::missing();
        };
        Self {
            primary_block_id: Some(primary.block_id.clone()),
            source_block_ids: segments.iter().map(|s| s.block_id.clone()).collect(),
            source_location: merge_source_locations(segments),
            source_links: merge_source_links(segments),
            relation_type: "PARENT_CONTEXT".into(),
            trace_quality: if segments.len() == 1 {
                "EXACT"
            } else {
                "MERGED"
            }
            .into(),
        }
    }
}

fn specificity_score(block_type: &str) -> i32 {
    match block_type {
        "FAQ_ITEM" | "TABLE_ROW" | "LIST_ITEM" | "PARAGRAPH" => 50,
        "CODE_BLOCK" => 45,
        "TABLE" | "LIST" => 40,
        "SUBSECTION" => 30,
        "SECTION" => 20,
        "DOCUMENT" => 10,
        _ => 0,
    }
}

fn merge_source_locations(segments: &[AnnotatedTextSegment]) -> Value {
    let mut page_start: Option<u64> = None;
    let mut page_end: Option<u64> = None;
    let mut section_path = String::new();
    let mut heading = String::new();
    for seg in segments {
        if let Some(obj) = seg.source_location.as_object() {
            if let Some(v) = obj
                .get("page_start")
                .and_then(Value::as_u64)
                .filter(|v| *v > 0)
            {
                page_start = Some(page_start.map(|x| x.min(v)).unwrap_or(v));
            }
            if let Some(v) = obj
                .get("page_end")
                .and_then(Value::as_u64)
                .filter(|v| *v > 0)
            {
                page_end = Some(page_end.map(|x| x.max(v)).unwrap_or(v));
            }
            if section_path.is_empty() {
                section_path = obj
                    .get("section_path")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
            }
            if heading.is_empty() {
                heading = obj
                    .get("heading")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
            }
        }
    }
    serde_json::json!({
        "page_start": page_start.unwrap_or(0),
        "page_end": page_end.unwrap_or(page_start.unwrap_or(0)),
        "section_path": section_path,
        "heading": heading
    })
}

fn merge_source_links(segments: &[AnnotatedTextSegment]) -> Value {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for seg in segments {
        if let Some(arr) = seg.source_links.as_array() {
            for link in arr {
                let key = format!(
                    "{}|{}",
                    link.get("type").map(Value::to_string).unwrap_or_default(),
                    link.get("url").and_then(Value::as_str).unwrap_or("")
                );
                if seen.insert(key) {
                    out.push(link.clone());
                }
            }
        }
    }
    Value::Array(out)
}

pub trait TokenCounter: Send + Sync {
    fn count(&self, text: &str) -> usize;
    fn split_sentences(&self, text: &str) -> Vec<String>;
}
#[derive(Default)]
pub struct ConservativeTokenCounter;
impl TokenCounter for ConservativeTokenCounter {
    fn count(&self, text: &str) -> usize {
        text.split_whitespace().count()
    }
    fn split_sentences(&self, text: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut buf = String::new();
        for ch in text.chars() {
            buf.push(ch);
            if matches!(ch, '.' | '!' | '?' | '\n') {
                let t = buf.trim();
                if !t.is_empty() {
                    out.push(t.to_string())
                }
                buf.clear()
            }
        }
        let t = buf.trim();
        if !t.is_empty() {
            out.push(t.to_string())
        }
        out
    }
}

pub struct ChunkingEngine<T: TokenCounter> {
    counter: T,
    namespace: Uuid,
}
impl<T: TokenCounter> ChunkingEngine<T> {
    pub fn new(counter: T) -> Self {
        Self {
            counter,
            namespace: Uuid::NAMESPACE_URL,
        }
    }
    pub fn chunk(
        &self,
        access_zone: Uuid,
        document: Uuid,
        version: u64,
        text: &str,
        p: &ChunkingProfile,
        source_storage_mode: SourceChunkStorageMode,
    ) -> Result<Vec<GeneratedChunk>, AstraError> {
        p.parent.validate()?;
        p.sub180.validate()?;
        p.sub260.validate()?;
        if text.trim().is_empty() {
            return Err(AstraError::InvalidArgument("source_text is empty".into()));
        }
        let normalized = normalize(text);
        let root = self.id(
            access_zone,
            document,
            version,
            "ROOT",
            0,
            &normalized,
            &p.version,
        );
        let source_content_hash = hash(&normalized);
        let source_content = match source_storage_mode {
            SourceChunkStorageMode::FullText => normalized.clone(),
            SourceChunkStorageMode::MetadataOnly => String::new(),
            SourceChunkStorageMode::Disabled => String::new(),
        };
        let source_location = serde_json::json!({
            "has_full_text": source_storage_mode == SourceChunkStorageMode::FullText,
            "content_hash": source_content_hash.clone(),
            "original_byte_length": normalized.len(),
            "original_char_count": normalized.chars().count(),
            "source_chunk_storage_mode": match source_storage_mode { SourceChunkStorageMode::FullText => "FULL_TEXT", SourceChunkStorageMode::MetadataOnly => "METADATA_ONLY", SourceChunkStorageMode::Disabled => "DISABLED" },
        });
        let source = GeneratedChunk {
            id: root,
            root_id: root,
            source_id: root,
            parent_id: None,
            granularity: Granularity::Source,
            sequence_no: 0,
            token_count: self.counter.count(&normalized),
            content: source_content,
            content_hash: source_content_hash,
            source_block_id: None,
            source_block_ids: Vec::new(),
            source_location,
            source_links: Value::Array(Vec::new()),
            trace_relation_type: "SYNTHETIC".into(),
            trace_quality: "MISSING".into(),
        };
        let mut out = if source_storage_mode == SourceChunkStorageMode::Disabled {
            Vec::new()
        } else {
            vec![source]
        };
        let parents = self.build(
            root,
            root,
            None,
            Granularity::Parent,
            &normalized,
            &p.parent,
            access_zone,
            document,
            version,
            &p.version,
        );
        for parent in parents {
            let pid = parent.id;
            let content = parent.content.clone();
            out.push(parent);
            out.extend(self.build(
                root,
                root,
                Some(pid),
                Granularity::Sub180,
                &content,
                &p.sub180,
                access_zone,
                document,
                version,
                &p.version,
            ));
            out.extend(self.build(
                root,
                root,
                Some(pid),
                Granularity::Sub260,
                &content,
                &p.sub260,
                access_zone,
                document,
                version,
                &p.version,
            ));
        }
        Ok(out)
    }

    pub fn chunk_segments(
        &self,
        access_zone: Uuid,
        document: Uuid,
        version: u64,
        segments: &[AnnotatedTextSegment],
        p: &ChunkingProfile,
        source_storage_mode: SourceChunkStorageMode,
    ) -> Result<Vec<GeneratedChunk>, AstraError> {
        p.parent.validate()?;
        p.sub180.validate()?;
        p.sub260.validate()?;
        if segments.is_empty() {
            return Err(AstraError::InvalidArgument(
                "annotated segments are empty".into(),
            ));
        }
        let normalized = normalize(
            &segments
                .iter()
                .map(|s| s.text.as_str())
                .collect::<Vec<_>>()
                .join("\n\n"),
        );
        if normalized.trim().is_empty() {
            return Err(AstraError::InvalidArgument(
                "logical blocks contain no text".into(),
            ));
        }
        let root = self.id(
            access_zone,
            document,
            version,
            "ROOT",
            0,
            &normalized,
            &p.version,
        );
        let source_trace = ChunkSourceTrace::parent_context(segments);
        let source_content_hash = hash(&normalized);
        let source_content = match source_storage_mode {
            SourceChunkStorageMode::FullText => normalized.clone(),
            SourceChunkStorageMode::MetadataOnly => String::new(),
            SourceChunkStorageMode::Disabled => String::new(),
        };
        let mut source_location = source_trace.source_location.clone();
        if let Some(obj) = source_location.as_object_mut() {
            obj.insert(
                "has_full_text".into(),
                serde_json::Value::Bool(source_storage_mode == SourceChunkStorageMode::FullText),
            );
            obj.insert(
                "content_hash".into(),
                serde_json::Value::String(source_content_hash.clone()),
            );
            obj.insert(
                "original_byte_length".into(),
                serde_json::json!(normalized.len()),
            );
            obj.insert(
                "original_char_count".into(),
                serde_json::json!(normalized.chars().count()),
            );
            obj.insert(
                "source_chunk_storage_mode".into(),
                serde_json::Value::String(
                    match source_storage_mode {
                        SourceChunkStorageMode::FullText => "FULL_TEXT",
                        SourceChunkStorageMode::MetadataOnly => "METADATA_ONLY",
                        SourceChunkStorageMode::Disabled => "DISABLED",
                    }
                    .into(),
                ),
            );
        }
        let source = GeneratedChunk {
            id: root,
            root_id: root,
            source_id: root,
            parent_id: None,
            granularity: Granularity::Source,
            sequence_no: 0,
            token_count: self.counter.count(&normalized),
            content: source_content,
            content_hash: source_content_hash,
            source_block_id: source_trace.primary_block_id.clone(),
            source_block_ids: source_trace.source_block_ids.clone(),
            source_location,
            source_links: source_trace.source_links.clone(),
            trace_relation_type: source_trace.relation_type.clone(),
            trace_quality: source_trace.trace_quality.clone(),
        };
        let mut out = if source_storage_mode == SourceChunkStorageMode::Disabled {
            Vec::new()
        } else {
            vec![source]
        };
        let mut sequence_base = 0_u32;
        for segment in segments.iter().filter(|s| !s.text.trim().is_empty()) {
            let text = normalize(&segment.text);
            let parent_trace = ChunkSourceTrace::exact(segment, "PARENT_CONTEXT");
            let parents = self.build_traced(
                root,
                root,
                None,
                Granularity::Parent,
                &text,
                &p.parent,
                access_zone,
                document,
                version,
                &p.version,
                sequence_base,
                &parent_trace,
            );
            sequence_base += parents.len() as u32 + 1;
            for parent in parents {
                let pid = parent.id;
                let content = parent.content.clone();
                let child_relation = if self.counter.count(&text) > p.sub260.max {
                    "SPLIT_FROM_BLOCK"
                } else {
                    "DIRECT"
                };
                let child_trace = ChunkSourceTrace::exact(segment, child_relation);
                out.push(parent);
                let sub180 = self.build_traced(
                    root,
                    root,
                    Some(pid),
                    Granularity::Sub180,
                    &content,
                    &p.sub180,
                    access_zone,
                    document,
                    version,
                    &p.version,
                    sequence_base,
                    &child_trace,
                );
                sequence_base += sub180.len() as u32 + 1;
                out.extend(sub180);
                let sub260 = self.build_traced(
                    root,
                    root,
                    Some(pid),
                    Granularity::Sub260,
                    &content,
                    &p.sub260,
                    access_zone,
                    document,
                    version,
                    &p.version,
                    sequence_base,
                    &child_trace,
                );
                sequence_base += sub260.len() as u32 + 1;
                out.extend(sub260);
            }
        }
        Ok(out)
    }

    #[allow(clippy::too_many_arguments)]
    fn build_traced(
        &self,
        root: Uuid,
        source: Uuid,
        parent: Option<Uuid>,
        g: Granularity,
        text: &str,
        p: &SizeProfile,
        zone: Uuid,
        doc: Uuid,
        version: u64,
        profile: &str,
        sequence_offset: u32,
        trace: &ChunkSourceTrace,
    ) -> Vec<GeneratedChunk> {
        let mut chunks = self.build(
            root, source, parent, g, text, p, zone, doc, version, profile,
        );
        for chunk in chunks.iter_mut() {
            chunk.sequence_no += sequence_offset;
            let kind = format!("{}:{root}", chunk.granularity.as_db_str());
            chunk.id = self.id(
                zone,
                doc,
                version,
                &kind,
                chunk.sequence_no,
                &chunk.content,
                profile,
            );
            chunk.source_block_id = trace.primary_block_id.clone();
            chunk.source_block_ids = trace.source_block_ids.clone();
            chunk.source_location = trace.source_location.clone();
            chunk.source_links = trace.source_links.clone();
            chunk.trace_relation_type = trace.relation_type.clone();
            chunk.trace_quality = trace.trace_quality.clone();
        }
        chunks
    }

    #[allow(clippy::too_many_arguments)]
    fn build(
        &self,
        root: Uuid,
        source: Uuid,
        parent: Option<Uuid>,
        g: Granularity,
        text: &str,
        p: &SizeProfile,
        zone: Uuid,
        doc: Uuid,
        version: u64,
        profile: &str,
    ) -> Vec<GeneratedChunk> {
        let sentences = self.counter.split_sentences(text);
        let mut groups: Vec<String> = Vec::new();
        let mut current = String::new();
        for sentence in sentences {
            if self.counter.count(&sentence) > p.max {
                if !current.is_empty() {
                    groups.push(current);
                    current = String::new();
                }
                let mut piece = Vec::new();
                for word in sentence.split_whitespace() {
                    piece.push(word);
                    if piece.len() >= p.target {
                        groups.push(piece.join(" "));
                        piece.clear();
                    }
                }
                if !piece.is_empty() {
                    groups.push(piece.join(" "));
                }
                continue;
            }
            let candidate = if current.is_empty() {
                sentence.clone()
            } else {
                format!("{current} {sentence}")
            };
            if self.counter.count(&candidate) > p.max && !current.is_empty() {
                groups.push(current);
                current = sentence
            } else {
                current = candidate
            }
        }
        if !current.is_empty() {
            groups.push(current)
        }
        if groups.len() > 1 {
            let last_is_short = groups
                .last()
                .map(|last| self.counter.count(last) < p.min)
                .unwrap_or(false);
            if last_is_short {
                if let (Some(last), Some(prev)) = (groups.pop(), groups.pop()) {
                    let merged = format!("{prev} {last}");
                    if self.counter.count(&merged) <= p.max {
                        groups.push(merged);
                    } else {
                        groups.push(prev);
                        groups.push(last);
                    }
                }
            }
        }
        let mut out = Vec::new();
        for (i, content) in groups.into_iter().enumerate() {
            let c = normalize(&content);
            let kind = format!("{}:{root}", g.as_db_str());
            let id = self.id(zone, doc, version, &kind, i as u32, &c, profile);
            out.push(GeneratedChunk {
                id,
                root_id: root,
                source_id: source,
                parent_id: parent,
                granularity: g,
                sequence_no: i as u32,
                token_count: self.counter.count(&c),
                content_hash: hash(&c),
                content: c,
                source_block_id: None,
                source_block_ids: Vec::new(),
                source_location: Value::Object(Default::default()),
                source_links: Value::Array(Vec::new()),
                trace_relation_type: "SYNTHETIC".into(),
                trace_quality: "MISSING".into(),
            });
        }
        out
    }
    #[allow(clippy::too_many_arguments)]
    fn id(
        &self,
        zone: Uuid,
        doc: Uuid,
        version: u64,
        kind: &str,
        seq: u32,
        text: &str,
        profile: &str,
    ) -> Uuid {
        Uuid::new_v5(
            &self.namespace,
            format!(
                "{zone}:{doc}:{version}:{kind}:{seq}:{}:{profile}",
                hash(text)
            )
            .as_bytes(),
        )
    }
}
pub fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
pub fn hash(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

#[cfg(test)]
mod v007_fix2_trace_tests {
    use super::*;

    fn profile() -> ChunkingProfile {
        ChunkingProfile {
            version: "test".into(),
            parent: SizeProfile {
                target: 20,
                min: 1,
                max: 40,
                overlap: 0,
            },
            sub180: SizeProfile {
                target: 8,
                min: 1,
                max: 12,
                overlap: 0,
            },
            sub260: SizeProfile {
                target: 12,
                min: 1,
                max: 18,
                overlap: 0,
            },
        }
    }

    #[test]
    fn chunk_segments_preserves_source_block_id() {
        let engine = ChunkingEngine::new(ConservativeTokenCounter);
        let segments = vec![AnnotatedTextSegment {
            block_id: "block-1".into(),
            parent_block_id: None,
            block_type: "PARAGRAPH".into(),
            text: "Ежегодный отпуск составляет двадцать четыре календарных дня.".into(),
            source_location: serde_json::json!({"page_start": 3, "page_end": 3, "section_path": "Отпуска"}),
            source_links: serde_json::json!([{"type": 4, "url": "https://docs.example/doc?page=3", "label": "page 3"}]),
            metadata: serde_json::json!({}),
            order_index: 1,
        }];
        let chunks = engine
            .chunk_segments(
                Uuid::nil(),
                Uuid::nil(),
                1,
                &segments,
                &profile(),
                SourceChunkStorageMode::FullText,
            )
            .unwrap();
        assert!(chunks
            .iter()
            .any(|c| c.source_block_id.as_deref() == Some("block-1")));
        assert!(chunks.iter().any(|c| c
            .source_links
            .as_array()
            .map(|a| !a.is_empty())
            .unwrap_or(false)));
    }

    #[test]
    fn short_tail_merge_never_exceeds_profile_maximum() {
        let engine = ChunkingEngine::new(ConservativeTokenCounter);
        let profile = ChunkingProfile {
            version: "test".into(),
            parent: SizeProfile {
                target: 4,
                min: 3,
                max: 5,
                overlap: 0,
            },
            sub180: SizeProfile {
                target: 2,
                min: 1,
                max: 4,
                overlap: 0,
            },
            sub260: SizeProfile {
                target: 2,
                min: 1,
                max: 4,
                overlap: 0,
            },
        };
        let chunks = engine
            .chunk(
                Uuid::nil(),
                Uuid::nil(),
                1,
                "one two three four. five six.",
                &profile,
                SourceChunkStorageMode::FullText,
            )
            .expect("chunk text");
        assert!(chunks
            .iter()
            .filter(|chunk| chunk.granularity == Granularity::Parent)
            .all(|chunk| chunk.token_count <= profile.parent.max));
    }
}
