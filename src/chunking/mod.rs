use crate::error::AstraError;
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    Source,
    Parent,
    Sub180,
    Sub260,
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
        let source = GeneratedChunk {
            id: root,
            root_id: root,
            source_id: root,
            parent_id: None,
            granularity: Granularity::Source,
            sequence_no: 0,
            token_count: self.counter.count(&normalized),
            content: normalized.clone(),
            content_hash: hash(&normalized),
        };
        let mut out = vec![source];
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
                format!("{} {}", current, sentence)
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
        if groups.len() > 1 && self.counter.count(groups.last().unwrap()) < p.min {
            let last = groups.pop().unwrap();
            let prev = groups.pop().unwrap();
            groups.push(format!("{} {}", prev, last));
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
