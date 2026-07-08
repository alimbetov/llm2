use crate::error::AstraError;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::OnceLock,
};

pub const TECHNICAL_SPARSE_MODE: &str = "LEXICAL_BASELINE_TECHNICAL";
pub const TECHNICAL_SPARSE_ENCODER_VERSION: &str = "technical-v2";
pub const TECHNICAL_SPARSE_INDEX_STRATEGY: &str = "stable_hash_sha256_u32";

const RAW_HASH_NAMESPACE_MIN: u32 = 1_000_000_000;
const RAW_HASH_NAMESPACE_SIZE: u32 = 1_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SparseTokenClass {
    Tokenizer,
    OrdinaryWord,
    NumericExact,
    Alphanumeric,
    ErrorCode,
    Uuid,
    IpOrPort,
    Path,
    Filename,
    UnderscoreIdentifier,
    GrpcMethod,
    VersionToken,
}

impl SparseTokenClass {
    pub fn as_str(self) -> &'static str {
        match self {
            SparseTokenClass::Tokenizer => "tokenizer",
            SparseTokenClass::OrdinaryWord => "ordinary_word",
            SparseTokenClass::NumericExact => "numeric_exact",
            SparseTokenClass::Alphanumeric => "alphanumeric",
            SparseTokenClass::ErrorCode => "error_code",
            SparseTokenClass::Uuid => "uuid",
            SparseTokenClass::IpOrPort => "ip_or_port",
            SparseTokenClass::Path => "path",
            SparseTokenClass::Filename => "filename",
            SparseTokenClass::UnderscoreIdentifier => "underscore_identifier",
            SparseTokenClass::GrpcMethod => "grpc_method",
            SparseTokenClass::VersionToken => "version_token",
        }
    }

    fn base_weight(self) -> f32 {
        match self {
            SparseTokenClass::Tokenizer | SparseTokenClass::OrdinaryWord => 1.0,
            SparseTokenClass::NumericExact => 2.5,
            SparseTokenClass::Alphanumeric => 2.2,
            SparseTokenClass::ErrorCode => 3.0,
            SparseTokenClass::Uuid => 2.5,
            SparseTokenClass::IpOrPort => 2.5,
            SparseTokenClass::Path => 2.5,
            SparseTokenClass::Filename => 2.2,
            SparseTokenClass::UnderscoreIdentifier => 2.4,
            SparseTokenClass::GrpcMethod => 2.5,
            SparseTokenClass::VersionToken => 2.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SparseVector {
    pub indices: Vec<u32>,
    pub values: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SparseToken {
    pub token: String,
    pub class: SparseTokenClass,
    pub index: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SparseAnalysis {
    pub tokens: Vec<SparseToken>,
    pub technical_token_count: usize,
    pub numeric_token_count: usize,
    pub alphanumeric_token_count: usize,
    pub special_token_count: usize,
}

#[derive(Debug, Clone, Copy)]
pub enum SparseEncodeMode {
    Document,
    Query,
}

#[derive(Debug, Clone)]
pub struct SparseTechnicalEncoder {
    min_weight: f32,
    max_non_zero: usize,
}

impl SparseTechnicalEncoder {
    pub fn new(min_weight: f32, max_non_zero: usize) -> Self {
        Self {
            min_weight,
            max_non_zero,
        }
    }

    pub fn version(&self) -> &'static str {
        TECHNICAL_SPARSE_ENCODER_VERSION
    }

    pub fn encode_document(&self, text: &str) -> Result<SparseVector, AstraError> {
        self.encode_core(text, SparseEncodeMode::Document)
    }

    pub fn encode_query(&self, text: &str) -> Result<SparseVector, AstraError> {
        self.encode_core(text, SparseEncodeMode::Query)
    }

    pub fn analyze(&self, text: &str) -> SparseAnalysis {
        let mut seen = HashSet::<(String, SparseTokenClass)>::new();
        let mut tokens = Vec::new();
        let mut analysis = SparseAnalysis::default();
        for (token, class) in extract_technical_tokens(text) {
            if !seen.insert((token.clone(), class)) {
                continue;
            }
            if class != SparseTokenClass::OrdinaryWord {
                analysis.technical_token_count += 1;
            }
            if class == SparseTokenClass::NumericExact {
                analysis.numeric_token_count += 1;
            }
            if class == SparseTokenClass::Alphanumeric {
                analysis.alphanumeric_token_count += 1;
            }
            if matches!(
                class,
                SparseTokenClass::ErrorCode
                    | SparseTokenClass::Uuid
                    | SparseTokenClass::IpOrPort
                    | SparseTokenClass::Path
                    | SparseTokenClass::Filename
                    | SparseTokenClass::UnderscoreIdentifier
                    | SparseTokenClass::GrpcMethod
                    | SparseTokenClass::VersionToken
            ) {
                analysis.special_token_count += 1;
            }
            tokens.push(SparseToken {
                index: stable_sparse_index(class, &token),
                token,
                class,
            });
        }
        tokens.sort_by(|a, b| {
            a.index
                .cmp(&b.index)
                .then_with(|| a.class.cmp(&b.class))
                .then_with(|| a.token.cmp(&b.token))
        });
        analysis.tokens = tokens;
        analysis
    }

    fn encode_core(&self, text: &str, _mode: SparseEncodeMode) -> Result<SparseVector, AstraError> {
        let mut merged = BTreeMap::<u32, f32>::new();
        let mut token_frequency = HashMap::<(String, SparseTokenClass), f32>::new();
        for (token, class) in extract_technical_tokens(text) {
            token_frequency
                .entry((token, class))
                .and_modify(|old| *old += 1.0)
                .or_insert(1.0);
        }
        for ((token, class), tf) in token_frequency {
            let weight = class.base_weight() * (1.0 + tf.ln());
            if !weight.is_finite() || weight <= self.min_weight {
                continue;
            }
            let index = stable_sparse_index(class, &token);
            merged
                .entry(index)
                .and_modify(|old| *old += weight)
                .or_insert(weight);
        }
        vector_from_weight_map(merged, self.max_non_zero)
    }
}

pub fn build_sparse(
    input_ids: &[u32],
    mask: &[u32],
    weights: &[f32],
    special: &HashSet<u32>,
    min_weight: f32,
    max_non_zero: usize,
) -> Result<(Vec<u32>, Vec<f32>), AstraError> {
    if input_ids.len() != mask.len() || mask.len() != weights.len() {
        return Err(AstraError::Internal("sparse tensor lengths differ".into()));
    }
    let mut merged = HashMap::<u32, f32>::new();
    for ((id, m), w) in input_ids.iter().zip(mask).zip(weights) {
        if !w.is_finite() {
            return Err(AstraError::Internal(
                "sparse vector contains NaN/Infinity".into(),
            ));
        }
        if *m == 0 || special.contains(id) || *w <= min_weight {
            continue;
        }
        merged
            .entry(*id)
            .and_modify(|old| *old = old.max(*w))
            .or_insert(*w);
    }
    let mut values: Vec<_> = merged.into_iter().collect();
    values.sort_by(|a, b| b.1.total_cmp(&a.1));
    values.truncate(max_non_zero);
    values.sort_by_key(|x| x.0);
    Ok((
        values.iter().map(|x| x.0).collect(),
        values.iter().map(|x| x.1).collect(),
    ))
}

pub fn build_lexical_sparse(
    text: &str,
    input_ids: &[u32],
    mask: &[u32],
    special: &HashSet<u32>,
    min_weight: f32,
    max_non_zero: usize,
) -> Result<(Vec<u32>, Vec<f32>), AstraError> {
    if input_ids.len() != mask.len() {
        return Err(AstraError::Internal(
            "lexical sparse tensor lengths differ".into(),
        ));
    }
    let mut term_frequency = HashMap::<u32, f32>::new();
    for (id, m) in input_ids.iter().zip(mask) {
        if *m == 0 || special.contains(id) {
            continue;
        }
        term_frequency
            .entry(*id)
            .and_modify(|old| *old += 1.0)
            .or_insert(1.0);
    }
    let mut merged = BTreeMap::<u32, f32>::new();
    for (id, tf) in term_frequency {
        let weight = SparseTokenClass::Tokenizer.base_weight() * (1.0 + tf.ln());
        if weight.is_finite() && weight > min_weight {
            merged.insert(id, weight);
        }
    }
    let technical = SparseTechnicalEncoder::new(min_weight, max_non_zero).encode_document(text)?;
    for (index, weight) in technical.indices.into_iter().zip(technical.values) {
        merged
            .entry(index)
            .and_modify(|old| *old += weight)
            .or_insert(weight);
    }
    vector_from_weight_map(merged, max_non_zero).map(|v| (v.indices, v.values))
}

fn vector_from_weight_map(
    merged: BTreeMap<u32, f32>,
    max_non_zero: usize,
) -> Result<SparseVector, AstraError> {
    let mut values = merged
        .into_iter()
        .filter(|(_, weight)| weight.is_finite() && *weight > 0.0)
        .collect::<Vec<_>>();
    let norm = values
        .iter()
        .map(|(_, weight)| (*weight as f64) * (*weight as f64))
        .sum::<f64>()
        .sqrt() as f32;
    if norm > 0.0 {
        for (_, weight) in &mut values {
            *weight /= norm;
        }
    }
    values.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    values.truncate(max_non_zero);
    values.sort_by_key(|x| x.0);
    Ok(SparseVector {
        indices: values.iter().map(|x| x.0).collect(),
        values: values.iter().map(|x| x.1).collect(),
    })
}

pub fn stable_sparse_index(class: SparseTokenClass, token: &str) -> u32 {
    let mut hasher = Sha256::new();
    hasher.update(b"astravector:sparse:technical:v1:");
    hasher.update(class.as_str().as_bytes());
    hasher.update(b":");
    hasher.update(token.as_bytes());
    let digest = hasher.finalize();
    let raw = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
    RAW_HASH_NAMESPACE_MIN + (raw % RAW_HASH_NAMESPACE_SIZE)
}

fn regexes() -> &'static [(SparseTokenClass, Regex)] {
    static REGEXES: OnceLock<Vec<(SparseTokenClass, Regex)>> = OnceLock::new();
    REGEXES.get_or_init(|| {
        [
            (
                SparseTokenClass::Uuid,
                r"\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b",
            ),
            (
                SparseTokenClass::IpOrPort,
                r"\b(?:\d{1,3}\.){3}\d{1,3}(?::\d{2,5})?\b|\blocalhost:\d{2,5}\b",
            ),
            (
                SparseTokenClass::GrpcMethod,
                r"\b[A-Z][A-Za-z0-9]+(?:Facade|Service)/[A-Z][A-Za-z0-9]+\b",
            ),
            (
                SparseTokenClass::Path,
                r"/[A-Za-z0-9._~:/?#\[\]@!$&'()*+,;=%-]+",
            ),
            (
                SparseTokenClass::Filename,
                r"\b[A-Za-z0-9_-]+\.[A-Za-z0-9]{2,}\b",
            ),
            (
                SparseTokenClass::ErrorCode,
                r"\b[A-Za-z]{1,12}(?:-[A-Za-z0-9]+)+\b",
            ),
            (
                SparseTokenClass::UnderscoreIdentifier,
                r"\b[A-Za-z0-9]+(?:_[A-Za-z0-9]+)+\b",
            ),
            (
                SparseTokenClass::VersionToken,
                r"\bv\d+(?:/fix\d+)?\b|\bfix\d+\b",
            ),
            (SparseTokenClass::NumericExact, r"\b\d{3,}\b"),
            (
                SparseTokenClass::Alphanumeric,
                r"\b(?:[A-Za-z]+\d+[A-Za-z0-9]*|\d+[A-Za-z]+[A-Za-z0-9]*)\b",
            ),
            (SparseTokenClass::OrdinaryWord, r"\b[[:alpha:]]{2,}\b"),
        ]
        .into_iter()
        .map(|(class, pattern)| {
            (
                class,
                Regex::new(pattern).expect("technical sparse regex must compile"),
            )
        })
        .collect()
    })
}

fn extract_technical_tokens(text: &str) -> Vec<(String, SparseTokenClass)> {
    let mut out = Vec::new();
    let mut seen_span_class = HashSet::<(usize, usize, SparseTokenClass)>::new();
    for (class, regex) in regexes() {
        for matched in regex.find_iter(text) {
            if !seen_span_class.insert((matched.start(), matched.end(), *class)) {
                continue;
            }
            let token = normalize_token(matched.as_str(), *class);
            if token.is_empty() {
                continue;
            }
            out.push((token.clone(), *class));
            for variant in token_variants(&token, *class) {
                if variant != token {
                    out.push((variant, *class));
                }
            }
        }
    }
    out
}

fn normalize_token(raw: &str, class: SparseTokenClass) -> String {
    let trimmed = raw.trim_matches(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                ',' | ';' | ':' | ')' | '(' | '[' | ']' | '{' | '}' | '"' | '\''
            )
    });
    match class {
        SparseTokenClass::OrdinaryWord => {
            let lower = trimmed.to_ascii_lowercase();
            if is_sparse_stopword(&lower) {
                String::new()
            } else {
                lower
            }
        }
        SparseTokenClass::Uuid => trimmed.to_ascii_lowercase(),
        _ => trimmed.to_string(),
    }
}

fn token_variants(token: &str, class: SparseTokenClass) -> Vec<String> {
    let mut variants = Vec::new();
    if class == SparseTokenClass::OrdinaryWord {
        let stem = sparse_word_stem(token);
        if stem != token && !is_sparse_stopword(&stem) {
            variants.push(stem);
        }
    }
    if matches!(
        class,
        SparseTokenClass::Alphanumeric
            | SparseTokenClass::ErrorCode
            | SparseTokenClass::Filename
            | SparseTokenClass::GrpcMethod
            | SparseTokenClass::Path
            | SparseTokenClass::UnderscoreIdentifier
            | SparseTokenClass::VersionToken
    ) {
        let lower = token.to_ascii_lowercase();
        if lower != token {
            variants.push(lower);
        }
    }
    if class == SparseTokenClass::UnderscoreIdentifier {
        variants.extend(
            token
                .split('_')
                .filter(|p| p.len() >= 2)
                .map(|p| p.to_ascii_lowercase()),
        );
    }
    if class == SparseTokenClass::Path {
        variants.extend(
            token
                .split('/')
                .filter(|p| p.len() >= 2)
                .map(|p| p.to_ascii_lowercase()),
        );
    }
    if class == SparseTokenClass::GrpcMethod {
        variants.extend(
            token
                .split('/')
                .filter(|p| p.len() >= 2)
                .map(|p| p.to_string()),
        );
    }
    variants
}

fn sparse_word_stem(token: &str) -> String {
    if token.len() > 5 && token.ends_with("ies") {
        return format!("{}y", &token[..token.len() - 3]);
    }
    for suffix in ["ing", "ed", "es", "s"] {
        if token.len() > suffix.len() + 3 && token.ends_with(suffix) {
            return token[..token.len() - suffix.len()].to_string();
        }
    }
    token.to_string()
}

fn is_sparse_stopword(token: &str) -> bool {
    matches!(
        token,
        "a" | "an"
            | "and"
            | "are"
            | "as"
            | "at"
            | "be"
            | "by"
            | "did"
            | "do"
            | "does"
            | "during"
            | "for"
            | "from"
            | "how"
            | "if"
            | "in"
            | "is"
            | "it"
            | "must"
            | "of"
            | "on"
            | "or"
            | "should"
            | "the"
            | "to"
            | "what"
            | "when"
            | "where"
            | "while"
            | "with"
    )
}
