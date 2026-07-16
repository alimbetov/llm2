#!/usr/bin/env python3
from __future__ import annotations

from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]


def load(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def save(path: str, content: str) -> None:
    (ROOT / path).write_text(content.rstrip() + "\n", encoding="utf-8")


def replace_once(content: str, old: str, new: str, label: str) -> str:
    count = content.count(old)
    if count != 1:
        raise RuntimeError(f"{label}: expected exactly one match, found {count}")
    return content.replace(old, new, 1)


def patch_config() -> None:
    path = "src/config/mod.rs"
    content = load(path)
    if "pub struct QueryProcessingTierConfig" not in content:
        pattern = re.compile(
            r"#\[derive\(Debug, Clone, Deserialize\)\]\npub struct QueryProcessingConfig \{.*?fn default_long_query_deadline_ms\(\) -> u64 \{\n    3_000\n\}\n",
            re.S,
        )
        replacement = '''#[derive(Debug, Clone, Deserialize)]
pub struct QueryProcessingTierConfig {
    pub max_tokens: usize,
    pub max_segments: usize,
    pub dense_candidate_limit: u32,
    pub sparse_candidate_limit: u32,
    pub lexical_candidate_limit: u32,
    pub local_fused_candidate_limit: u32,
    pub global_fused_candidate_limit: u32,
    pub max_parallel_segments: usize,
    pub max_parallel_lexical_segments: usize,
    pub deadline_ms: u64,
    pub max_graph_seeds: usize,
    pub admission_weight: u32,
}

impl QueryProcessingTierConfig {
    fn standard() -> Self {
        Self {
            max_tokens: 1_024,
            max_segments: 7,
            dense_candidate_limit: 18,
            sparse_candidate_limit: 18,
            lexical_candidate_limit: 12,
            local_fused_candidate_limit: 18,
            global_fused_candidate_limit: 100,
            max_parallel_segments: 3,
            max_parallel_lexical_segments: 2,
            deadline_ms: 3_000,
            max_graph_seeds: 8,
            admission_weight: 3,
        }
    }

    fn extended() -> Self {
        Self {
            max_tokens: 2_048,
            max_segments: 14,
            dense_candidate_limit: 10,
            sparse_candidate_limit: 10,
            lexical_candidate_limit: 8,
            local_fused_candidate_limit: 10,
            global_fused_candidate_limit: 140,
            max_parallel_segments: 3,
            max_parallel_lexical_segments: 2,
            deadline_ms: 6_000,
            max_graph_seeds: 10,
            admission_weight: 6,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueryProcessingConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub extended_enabled: bool,
    #[serde(default = "default_query_profile_version")]
    pub profile_version: String,
    #[serde(default = "default_long_query_absolute_max_tokens")]
    pub absolute_max_tokens: usize,
    #[serde(default = "default_long_query_absolute_max_bytes")]
    pub absolute_max_bytes: usize,
    #[serde(default = "default_query_segment_target_tokens")]
    pub segment_target_tokens: usize,
    #[serde(default = "default_query_segment_max_tokens")]
    pub segment_max_tokens: usize,
    #[serde(default = "default_query_segment_overlap_tokens")]
    pub segment_overlap_tokens: usize,
    // v008 compatibility aliases. Runtime limits resolve from standard/extended.
    #[serde(default = "default_query_max_segments")]
    pub max_segments: usize,
    #[serde(default = "default_query_max_parallel_segments")]
    pub max_parallel_segments: usize,
    #[serde(default = "default_query_max_parallel_lexical_segments")]
    pub max_parallel_lexical_segments: usize,
    #[serde(default = "default_query_per_segment_candidate_limit")]
    pub per_segment_candidate_limit: u32,
    #[serde(default = "default_query_global_candidate_limit")]
    pub global_candidate_limit: u32,
    #[serde(default = "default_query_segment_rrf_k")]
    pub segment_rrf_k: f32,
    #[serde(default = "default_question_segment_weight")]
    pub question_segment_weight: f32,
    #[serde(default = "default_technical_segment_weight")]
    pub technical_segment_weight: f32,
    #[serde(default = "default_context_segment_weight")]
    pub context_segment_weight: f32,
    #[serde(default = "default_long_query_deadline_ms")]
    pub long_query_deadline_ms: u64,
    #[serde(default = "default_single_query_deadline_ms")]
    pub single_deadline_ms: u64,
    #[serde(default = "default_single_graph_seeds")]
    pub single_graph_seeds: usize,
    #[serde(default = "default_single_admission_weight")]
    pub single_admission_weight: u32,
    #[serde(default = "QueryProcessingTierConfig::standard")]
    pub standard: QueryProcessingTierConfig,
    #[serde(default = "QueryProcessingTierConfig::extended")]
    pub extended: QueryProcessingTierConfig,
}

impl Default for QueryProcessingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            extended_enabled: false,
            profile_version: default_query_profile_version(),
            absolute_max_tokens: default_long_query_absolute_max_tokens(),
            absolute_max_bytes: default_long_query_absolute_max_bytes(),
            segment_target_tokens: default_query_segment_target_tokens(),
            segment_max_tokens: default_query_segment_max_tokens(),
            segment_overlap_tokens: default_query_segment_overlap_tokens(),
            max_segments: default_query_max_segments(),
            max_parallel_segments: default_query_max_parallel_segments(),
            max_parallel_lexical_segments: default_query_max_parallel_lexical_segments(),
            per_segment_candidate_limit: default_query_per_segment_candidate_limit(),
            global_candidate_limit: default_query_global_candidate_limit(),
            segment_rrf_k: default_query_segment_rrf_k(),
            question_segment_weight: default_question_segment_weight(),
            technical_segment_weight: default_technical_segment_weight(),
            context_segment_weight: default_context_segment_weight(),
            long_query_deadline_ms: default_long_query_deadline_ms(),
            single_deadline_ms: default_single_query_deadline_ms(),
            single_graph_seeds: default_single_graph_seeds(),
            single_admission_weight: default_single_admission_weight(),
            standard: QueryProcessingTierConfig::standard(),
            extended: QueryProcessingTierConfig::extended(),
        }
    }
}

fn default_query_profile_version() -> String { "tiered-query-v1".into() }
fn default_long_query_absolute_max_tokens() -> usize { 2_048 }
fn default_long_query_absolute_max_bytes() -> usize { 65_536 }
fn default_query_segment_target_tokens() -> usize { 180 }
fn default_query_segment_max_tokens() -> usize { 220 }
fn default_query_segment_overlap_tokens() -> usize { 24 }
fn default_query_max_segments() -> usize { 7 }
fn default_query_max_parallel_segments() -> usize { 3 }
fn default_query_max_parallel_lexical_segments() -> usize { 2 }
fn default_query_per_segment_candidate_limit() -> u32 { 18 }
fn default_query_global_candidate_limit() -> u32 { 100 }
fn default_query_segment_rrf_k() -> f32 { 60.0 }
fn default_question_segment_weight() -> f32 { 1.0 }
fn default_technical_segment_weight() -> f32 { 1.0 }
fn default_context_segment_weight() -> f32 { 0.5 }
fn default_long_query_deadline_ms() -> u64 { 3_000 }
fn default_single_query_deadline_ms() -> u64 { 1_000 }
fn default_single_graph_seeds() -> usize { 5 }
fn default_single_admission_weight() -> u32 { 1 }
'''
        content, count = pattern.subn(replacement, content, count=1)
        if count != 1:
            raise RuntimeError(f"config model replacement expected one match, found {count}")

    validation_marker = '''        anyhow::ensure!(
            qp.long_query_deadline_ms >= self.grpc.deadlines.query_ms,
            "search.query_processing.long_query_deadline_ms must be >= normal query deadline"
        );
'''
    validation_addition = '''        anyhow::ensure!(
            qp.standard.max_tokens > query_max
                && qp.standard.max_tokens < qp.extended.max_tokens
                && qp.extended.max_tokens <= 2_048,
            "query processing tiers must satisfy single < standard < extended <= 2048"
        );
        for (name, tier) in [("standard", &qp.standard), ("extended", &qp.extended)] {
            anyhow::ensure!(
                tier.max_segments >= 2 && tier.max_segments <= 16,
                "search.query_processing.{name}.max_segments must be between 2 and 16"
            );
            anyhow::ensure!(
                tier.max_parallel_segments > 0
                    && tier.max_parallel_segments <= tier.max_segments,
                "search.query_processing.{name}.max_parallel_segments is invalid"
            );
            anyhow::ensure!(
                tier.max_parallel_lexical_segments > 0
                    && tier.max_parallel_lexical_segments <= tier.max_segments,
                "search.query_processing.{name}.max_parallel_lexical_segments is invalid"
            );
            anyhow::ensure!(
                tier.local_fused_candidate_limit > 0
                    && tier.global_fused_candidate_limit >= tier.local_fused_candidate_limit
                    && tier.global_fused_candidate_limit <= self.limits.search_candidate_limit_max,
                "search.query_processing.{name} candidate limits are invalid"
            );
            anyhow::ensure!(
                tier.deadline_ms >= self.grpc.deadlines.query_ms,
                "search.query_processing.{name}.deadline_ms must be >= normal query deadline"
            );
            anyhow::ensure!(
                tier.admission_weight > 0,
                "search.query_processing.{name}.admission_weight must be positive"
            );
        }
'''
    if "query processing tiers must satisfy" not in content:
        content = replace_once(
            content,
            validation_marker,
            validation_marker + validation_addition,
            "config validation marker",
        )
    save(path, content)


def patch_yaml() -> None:
    path = "config/application.yaml"
    content = load(path)
    if "extended_enabled:" not in content:
        pattern = re.compile(r"  query_processing:\n.*?  hybrid_fusion_method:", re.S)
        block = '''  query_processing:
    enabled: ${ASTRAVECTOR_LONG_QUERY_ENABLED:-true}
    extended_enabled: ${ASTRAVECTOR_LONG_QUERY_EXTENDED_ENABLED:-false}
    profile_version: tiered-query-v1
    absolute_max_tokens: ${ASTRAVECTOR_LONG_QUERY_ABSOLUTE_MAX_TOKENS:-2048}
    absolute_max_bytes: ${ASTRAVECTOR_LONG_QUERY_ABSOLUTE_MAX_BYTES:-65536}
    segment_target_tokens: ${ASTRAVECTOR_LONG_QUERY_SEGMENT_TARGET_TOKENS:-180}
    segment_max_tokens: ${ASTRAVECTOR_LONG_QUERY_SEGMENT_MAX_TOKENS:-220}
    segment_overlap_tokens: ${ASTRAVECTOR_LONG_QUERY_SEGMENT_OVERLAP_TOKENS:-24}
    max_segments: ${ASTRAVECTOR_LONG_QUERY_MAX_SEGMENTS:-7}
    max_parallel_segments: ${ASTRAVECTOR_LONG_QUERY_MAX_PARALLEL_SEGMENTS:-3}
    max_parallel_lexical_segments: ${ASTRAVECTOR_LONG_QUERY_MAX_PARALLEL_FTS_SEGMENTS:-2}
    per_segment_candidate_limit: ${ASTRAVECTOR_LONG_QUERY_CANDIDATE_LIMIT:-18}
    global_candidate_limit: ${ASTRAVECTOR_LONG_QUERY_GLOBAL_CANDIDATE_LIMIT:-100}
    segment_rrf_k: ${ASTRAVECTOR_LONG_QUERY_RRF_K:-60}
    question_segment_weight: ${ASTRAVECTOR_LONG_QUERY_QUESTION_WEIGHT:-1.0}
    technical_segment_weight: ${ASTRAVECTOR_LONG_QUERY_TECHNICAL_WEIGHT:-1.0}
    context_segment_weight: ${ASTRAVECTOR_LONG_QUERY_CONTEXT_WEIGHT:-0.5}
    long_query_deadline_ms: ${ASTRAVECTOR_LONG_QUERY_DEADLINE_MS:-3000}
    single_deadline_ms: ${ASTRAVECTOR_SINGLE_QUERY_DEADLINE_MS:-1000}
    single_graph_seeds: ${ASTRAVECTOR_SINGLE_QUERY_GRAPH_SEEDS:-5}
    single_admission_weight: ${ASTRAVECTOR_SINGLE_QUERY_ADMISSION_WEIGHT:-1}
    standard:
      max_tokens: ${ASTRAVECTOR_LONG_QUERY_STANDARD_MAX_TOKENS:-1024}
      max_segments: ${ASTRAVECTOR_LONG_QUERY_STANDARD_MAX_SEGMENTS:-7}
      dense_candidate_limit: ${ASTRAVECTOR_LONG_QUERY_STANDARD_DENSE_LIMIT:-18}
      sparse_candidate_limit: ${ASTRAVECTOR_LONG_QUERY_STANDARD_SPARSE_LIMIT:-18}
      lexical_candidate_limit: ${ASTRAVECTOR_LONG_QUERY_STANDARD_FTS_LIMIT:-12}
      local_fused_candidate_limit: ${ASTRAVECTOR_LONG_QUERY_STANDARD_LOCAL_FUSED_LIMIT:-18}
      global_fused_candidate_limit: ${ASTRAVECTOR_LONG_QUERY_STANDARD_GLOBAL_FUSED_LIMIT:-100}
      max_parallel_segments: ${ASTRAVECTOR_LONG_QUERY_STANDARD_PARALLEL_SEGMENTS:-3}
      max_parallel_lexical_segments: ${ASTRAVECTOR_LONG_QUERY_STANDARD_PARALLEL_FTS:-2}
      deadline_ms: ${ASTRAVECTOR_LONG_QUERY_STANDARD_DEADLINE_MS:-3000}
      max_graph_seeds: ${ASTRAVECTOR_LONG_QUERY_STANDARD_GRAPH_SEEDS:-8}
      admission_weight: ${ASTRAVECTOR_LONG_QUERY_STANDARD_ADMISSION_WEIGHT:-3}
    extended:
      max_tokens: ${ASTRAVECTOR_LONG_QUERY_EXTENDED_MAX_TOKENS:-2048}
      max_segments: ${ASTRAVECTOR_LONG_QUERY_EXTENDED_MAX_SEGMENTS:-14}
      dense_candidate_limit: ${ASTRAVECTOR_LONG_QUERY_EXTENDED_DENSE_LIMIT:-10}
      sparse_candidate_limit: ${ASTRAVECTOR_LONG_QUERY_EXTENDED_SPARSE_LIMIT:-10}
      lexical_candidate_limit: ${ASTRAVECTOR_LONG_QUERY_EXTENDED_FTS_LIMIT:-8}
      local_fused_candidate_limit: ${ASTRAVECTOR_LONG_QUERY_EXTENDED_LOCAL_FUSED_LIMIT:-10}
      global_fused_candidate_limit: ${ASTRAVECTOR_LONG_QUERY_EXTENDED_GLOBAL_FUSED_LIMIT:-140}
      max_parallel_segments: ${ASTRAVECTOR_LONG_QUERY_EXTENDED_PARALLEL_SEGMENTS:-3}
      max_parallel_lexical_segments: ${ASTRAVECTOR_LONG_QUERY_EXTENDED_PARALLEL_FTS:-2}
      deadline_ms: ${ASTRAVECTOR_LONG_QUERY_EXTENDED_DEADLINE_MS:-6000}
      max_graph_seeds: ${ASTRAVECTOR_LONG_QUERY_EXTENDED_GRAPH_SEEDS:-10}
      admission_weight: ${ASTRAVECTOR_LONG_QUERY_EXTENDED_ADMISSION_WEIGHT:-6}
  hybrid_fusion_method:'''
        content, count = pattern.subn(block, content, count=1)
        if count != 1:
            raise RuntimeError(f"application.yaml query block replacement found {count}")
    content = content.replace(
        "query: { max_length: 256, truncation_allowed: true }",
        "query: { max_length: 256, truncation_allowed: false }",
        1,
    )
    save(path, content)

    prod_path = "config/application-prod.yaml"
    prod = load(prod_path)
    if "extended_enabled:" not in prod:
        prod = replace_once(
            prod,
            "  query_processing:\n    enabled: ${ASTRAVECTOR_LONG_QUERY_ENABLED:-false}\n",
            "  query_processing:\n    enabled: ${ASTRAVECTOR_LONG_QUERY_ENABLED:-false}\n    extended_enabled: ${ASTRAVECTOR_LONG_QUERY_EXTENDED_ENABLED:-false}\n",
            "production feature flags",
        )
    save(prod_path, prod)


def patch_grpc() -> None:
    path = "src/grpc/mod.rs"
    content = load(path)
    content = content.replace(
        "QueryProcessingMode,\n    },",
        "QueryProcessingMode, QueryProcessingTier,\n    },",
        1,
    )
    if "QueryPlanningError::ExtendedQueryNotEnabled" not in content:
        marker = '''        QueryPlanningError::LongQueryNotSupported => Status::out_of_range(
            "LONG_QUERY_NOT_SUPPORTED: query_processing.enabled=false rejects queries above tokenization.query.max_length",
        ),
'''
        addition = '''        QueryPlanningError::ExtendedQueryNotEnabled => Status::out_of_range(
            "LONG_QUERY_EXTENDED_NOT_ENABLED: enable query_processing.extended_enabled for queries above the Standard tier",
        ),
        QueryPlanningError::IntentExtraction(message) => Status::internal(format!(
            "QUERY_INTENT_EXTRACTION_FAILED: {message}"
        )),
'''
        content = replace_once(content, marker, marker + addition, "grpc planning errors")

    content = content.replace(
        '''            code: "LONG_QUERY_SEGMENTED".into(),
            message: format!(
                "query segmented into {} bounded segments",
                plan.segments.len()
            ),''',
        '''            code: match plan.tier {
                QueryProcessingTier::SegmentedExtended => "LONG_QUERY_SEGMENTED_EXTENDED",
                _ => "LONG_QUERY_SEGMENTED_STANDARD",
            }
            .into(),
            message: format!(
                "query processed as {} with {} bounded segments",
                plan.tier.code(),
                plan.segments.len()
            ),''',
        1,
    )
    content = content.replace(
        "max_length: self.cfg.search.query_processing.segment_max_tokens,",
        "max_length: plan.limits.segment_max_tokens,",
        1,
    )
    content = content.replace(
        "max_in_flight: self.cfg.search.query_processing.max_parallel_segments,",
        "max_in_flight: plan.limits.max_parallel_segments,",
        1,
    )
    old_limits = '''        let per_segment_limit = self
            .cfg
            .search
            .query_processing
            .per_segment_candidate_limit
            .min(candidate_limit)
            .min(self.cfg.limits.search_candidate_limit_max)
            .max(top_k.min(candidate_limit))
            .max(1) as usize;
        let global_limit = candidate_limit
            .min(self.cfg.search.query_processing.global_candidate_limit)
            .max(1) as usize;
'''
    new_limits = '''        let per_segment_limit = plan
            .limits
            .local_fused_candidate_limit
            .min(candidate_limit)
            .min(self.cfg.limits.search_candidate_limit_max)
            .max(top_k.min(candidate_limit))
            .max(1) as usize;
        let global_limit = candidate_limit
            .min(plan.limits.global_fused_candidate_limit)
            .max(1) as usize;
'''
    if old_limits in content:
        content = content.replace(old_limits, new_limits, 1)
    content = content.replace(
        '''            .buffer_unordered(
                self.cfg
                    .search
                    .query_processing
                    .max_parallel_segments
                    .max(1),
            )''',
        ".buffer_unordered(plan.limits.max_parallel_segments.max(1))",
        1,
    )
    content = content.replace(
        '''        let candidate_limit = if query_plan.mode == QueryProcessingMode::Segmented {
            candidate_limit
                .min(self.cfg.search.query_processing.global_candidate_limit)
                .max(top_k)
        } else {
            candidate_limit
        };''',
        '''        let candidate_limit = if query_plan.mode == QueryProcessingMode::Segmented {
            candidate_limit
                .min(query_plan.limits.global_fused_candidate_limit)
                .max(top_k)
        } else {
            candidate_limit
        };''',
        1,
    )
    content = content.replace(
        '''                    .len()
                    .min(self.cfg.search.query_processing.max_parallel_segments)
                    as f64,''',
        '''                    .len()
                    .min(query_plan.limits.max_parallel_segments)
                    as f64,''',
        1,
    )
    content = content.replace(
        "effective_query_timeout_ms(r.timeout_ms as u64, query_plan.mode, &self.cfg);",
        "effective_query_timeout_ms(r.timeout_ms as u64, query_plan.mode, &self.cfg)\n                .min(query_plan.limits.deadline_ms);",
        1,
    )
    save(path, content)


def patch_fusion() -> None:
    path = "src/query_processing/fusion.rs"
    content = load(path)
    content = content.replace(
        ".cmp(&left.identity.document_version)",
        ".cmp(&right.identity.document_version)",
        1,
    )
    save(path, content)


def main() -> None:
    patch_config()
    patch_yaml()
    patch_grpc()
    patch_fusion()
    print("fix484 existing-file patch applied")


if __name__ == "__main__":
    main()
