use crate::config::QueryProcessingConfig;
use crate::query_processing::classification::QuerySegmentKind;
use crate::query_processing::planner::{
    build_segment, QueryPlanningError, QuerySegment, QueryTokenCounter,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QueryAtomKind {
    Paragraph,
    Sentence,
    ListItem,
    CodeBlock,
    HardTokenWindow,
}

#[derive(Debug, Clone)]
struct QueryAtom {
    text: String,
    token_count: usize,
    kind: QueryAtomKind,
    splittable: bool,
}

pub fn segment_query(
    query: &str,
    token_counter: &dyn QueryTokenCounter,
    config: &QueryProcessingConfig,
) -> Result<Vec<QuerySegment>, QueryPlanningError> {
    let normalized = normalize_query_text(query);
    let atoms = build_atoms(&normalized, token_counter, config)?;
    let packed = pack_atoms(&atoms, token_counter, config)?;
    let mut segments = Vec::with_capacity(packed.len());
    for (index, text) in packed.into_iter().enumerate() {
        let token_count = count_segment_tokens(&text, token_counter, config.segment_max_tokens)?;
        let mut segment = build_segment(index, &text, token_count, config, true);
        segment.required_for_coverage =
            segment.kind != QuerySegmentKind::Context || index + 1 == atoms.len();
        segments.push(segment);
    }
    if let Some(last) = segments.last_mut() {
        last.required_for_coverage = true;
    }
    Ok(segments)
}

fn normalize_query_text(text: &str) -> String {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::with_capacity(text.len());
    let mut empty_lines = 0usize;
    for line in text.trim().lines() {
        if line.trim().is_empty() {
            empty_lines += 1;
            if empty_lines <= 2 {
                out.push('\n');
            }
        } else {
            empty_lines = 0;
            out.push_str(line.trim_end());
            out.push('\n');
        }
    }
    out.trim().to_owned()
}

fn build_atoms(
    text: &str,
    token_counter: &dyn QueryTokenCounter,
    config: &QueryProcessingConfig,
) -> Result<Vec<QueryAtom>, QueryPlanningError> {
    let mut atoms = Vec::new();
    let mut current = String::new();
    let mut in_code = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            if !current.trim().is_empty() && !in_code {
                push_paragraph_atoms(&mut atoms, &current, token_counter, config)?;
                current.clear();
            }
            in_code = !in_code;
            current.push_str(line);
            current.push('\n');
            if !in_code {
                push_atom(
                    &mut atoms,
                    current.trim(),
                    QueryAtomKind::CodeBlock,
                    true,
                    token_counter,
                    config,
                )?;
                current.clear();
            }
            continue;
        }
        if in_code {
            current.push_str(line);
            current.push('\n');
            continue;
        }
        if line.trim().is_empty() {
            if !current.trim().is_empty() {
                push_paragraph_atoms(&mut atoms, &current, token_counter, config)?;
                current.clear();
            }
            continue;
        }
        if is_list_item(line) {
            if !current.trim().is_empty() {
                push_paragraph_atoms(&mut atoms, &current, token_counter, config)?;
                current.clear();
            }
            push_atom(
                &mut atoms,
                line.trim(),
                QueryAtomKind::ListItem,
                true,
                token_counter,
                config,
            )?;
        } else {
            current.push_str(line);
            current.push('\n');
        }
    }
    if !current.trim().is_empty() {
        if in_code {
            push_atom(
                &mut atoms,
                current.trim(),
                QueryAtomKind::CodeBlock,
                true,
                token_counter,
                config,
            )?;
        } else {
            push_paragraph_atoms(&mut atoms, &current, token_counter, config)?;
        }
    }
    if atoms.is_empty() {
        return Err(QueryPlanningError::SegmentationInvariant(
            "no atoms produced".into(),
        ));
    }
    Ok(atoms)
}

fn push_paragraph_atoms(
    atoms: &mut Vec<QueryAtom>,
    paragraph: &str,
    token_counter: &dyn QueryTokenCounter,
    config: &QueryProcessingConfig,
) -> Result<(), QueryPlanningError> {
    let paragraph = paragraph.trim();
    if paragraph.is_empty() {
        return Ok(());
    }
    let token_count = token_counter
        .count_tokens(paragraph, config.segment_max_tokens, false)
        .ok();
    if token_count.is_some_and(|count| count <= config.segment_max_tokens) {
        push_atom(
            atoms,
            paragraph,
            QueryAtomKind::Paragraph,
            true,
            token_counter,
            config,
        )?;
        return Ok(());
    }
    let sentences = split_sentences(paragraph);
    if sentences.len() > 1 {
        for sentence in sentences {
            push_atom(
                atoms,
                &sentence,
                QueryAtomKind::Sentence,
                true,
                token_counter,
                config,
            )?;
        }
    } else {
        push_hard_windows(atoms, paragraph, token_counter, config)?;
    }
    Ok(())
}

fn push_atom(
    atoms: &mut Vec<QueryAtom>,
    text: &str,
    kind: QueryAtomKind,
    splittable: bool,
    token_counter: &dyn QueryTokenCounter,
    config: &QueryProcessingConfig,
) -> Result<(), QueryPlanningError> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(());
    }
    match token_counter.count_tokens(text, config.segment_max_tokens, false) {
        Ok(token_count) => {
            atoms.push(QueryAtom {
                text: text.to_owned(),
                token_count,
                kind,
                splittable,
            });
            Ok(())
        }
        Err(_) if splittable => push_hard_windows(atoms, text, token_counter, config),
        Err(error) => Err(QueryPlanningError::SegmentationInvariant(format!(
            "unsplittable atom exceeds segment_max_tokens: {error}"
        ))),
    }
}

fn push_hard_windows(
    atoms: &mut Vec<QueryAtom>,
    text: &str,
    token_counter: &dyn QueryTokenCounter,
    config: &QueryProcessingConfig,
) -> Result<(), QueryPlanningError> {
    let offsets = token_counter
        .token_offsets(text)
        .map_err(QueryPlanningError::Tokenization)?;
    if offsets.is_empty() {
        return Ok(());
    }
    let mut start = 0usize;
    let window = config.segment_max_tokens.max(1);
    let stride = window.saturating_sub(config.segment_overlap_tokens).max(1);
    while start < offsets.len() {
        let mut end = (start + window).min(offsets.len());
        let start_byte = offsets[start].start_byte;
        let mut end_byte = offsets[end - 1].end_byte;
        let mut candidate = text[start_byte..end_byte].trim().to_owned();
        while token_counter
            .count_tokens(&candidate, config.segment_max_tokens, false)
            .is_err()
            && end > start + 1
        {
            end -= 1;
            end_byte = offsets[end - 1].end_byte;
            candidate = text[start_byte..end_byte].trim().to_owned();
        }
        let token_count =
            count_segment_tokens(&candidate, token_counter, config.segment_max_tokens)?;
        atoms.push(QueryAtom {
            text: candidate,
            token_count,
            kind: QueryAtomKind::HardTokenWindow,
            splittable: false,
        });
        if end == offsets.len() {
            break;
        }
        start = start.saturating_add(stride).min(end);
    }
    Ok(())
}

fn pack_atoms(
    atoms: &[QueryAtom],
    token_counter: &dyn QueryTokenCounter,
    config: &QueryProcessingConfig,
) -> Result<Vec<String>, QueryPlanningError> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut current_tokens = 0usize;
    for atom in atoms {
        let separator = if atom.kind == QueryAtomKind::CodeBlock || current.contains('\n') {
            "\n"
        } else {
            " "
        };
        let candidate = if current.is_empty() {
            atom.text.clone()
        } else {
            format!("{}{}{}", current, separator, atom.text)
        };
        let candidate_tokens = token_counter
            .count_tokens(&candidate, config.segment_max_tokens, false)
            .ok();
        let exceeds_target =
            current_tokens > 0 && current_tokens + atom.token_count > config.segment_target_tokens;
        if current_tokens > 0
            && (exceeds_target
                || candidate_tokens.is_none_or(|count| count > config.segment_max_tokens))
        {
            segments.push(current.trim().to_owned());
            if segments.len() >= config.max_segments {
                return repack_to_max_segments(segments, atoms, token_counter, config);
            }
            let previous_tail = if atom.kind == QueryAtomKind::HardTokenWindow {
                String::new()
            } else {
                overlap_tail(&current, config.segment_overlap_tokens)
            };
            let overlapped = if previous_tail.is_empty() {
                atom.text.clone()
            } else {
                format!("{} {}", previous_tail, atom.text)
            };
            current = if token_counter
                .count_tokens(&overlapped, config.segment_max_tokens, false)
                .is_ok()
            {
                overlapped
            } else {
                atom.text.clone()
            };
            current_tokens =
                count_segment_tokens(&current, token_counter, config.segment_max_tokens)?;
        } else {
            current = candidate;
            current_tokens = candidate_tokens.unwrap_or(current_tokens + atom.token_count);
        }
    }
    if !current.trim().is_empty() {
        segments.push(current.trim().to_owned());
    }
    if segments.len() > config.max_segments {
        return repack_to_max_segments(segments, atoms, token_counter, config);
    }
    Ok(segments)
}

fn repack_to_max_segments(
    _existing: Vec<String>,
    atoms: &[QueryAtom],
    token_counter: &dyn QueryTokenCounter,
    config: &QueryProcessingConfig,
) -> Result<Vec<String>, QueryPlanningError> {
    let full = atoms
        .iter()
        .map(|atom| atom.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let words = full.split_whitespace().collect::<Vec<_>>();
    if words.is_empty() {
        return Ok(Vec::new());
    }
    let windows = config.max_segments.max(1);
    let mut out = Vec::with_capacity(windows);
    let mut start = 0usize;
    while start < words.len() && out.len() < windows {
        let remaining_windows = windows - out.len();
        let remaining_words = words.len() - start;
        let mut take = remaining_words.div_ceil(remaining_windows);
        take = take.min(config.segment_max_tokens.max(1));
        let mut end = (start + take).min(words.len());
        let mut candidate = words[start..end].join(" ");
        while token_counter
            .count_tokens(&candidate, config.segment_max_tokens, false)
            .is_err()
            && end > start + 1
        {
            end -= 1;
            candidate = words[start..end].join(" ");
        }
        out.push(candidate);
        start = end;
    }
    if start < words.len() {
        return Err(QueryPlanningError::SegmentationInvariant(
            "query cannot be packed within max_segments and segment_max_tokens".into(),
        ));
    }
    Ok(out)
}

fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut start = 0usize;
    let mut chars = text.char_indices().peekable();
    while let Some((idx, ch)) = chars.next() {
        let next_is_boundary = chars
            .peek()
            .map(|(_, next)| next.is_whitespace())
            .unwrap_or(true);
        if matches!(ch, '!' | '?') || (ch == '.' && next_is_boundary) {
            let end = idx + ch.len_utf8();
            let piece = text[start..end].trim();
            if !piece.is_empty() {
                sentences.push(piece.to_owned());
            }
            start = end;
        }
    }
    let tail = text[start..].trim();
    if !tail.is_empty() {
        sentences.push(tail.to_owned());
    }
    if sentences.is_empty() {
        vec![text.trim().to_owned()]
    } else {
        sentences
    }
}

fn overlap_tail(text: &str, overlap_tokens: usize) -> String {
    if overlap_tokens == 0 {
        return String::new();
    }
    let words = text.split_whitespace().collect::<Vec<_>>();
    let start = words.len().saturating_sub(overlap_tokens);
    words[start..].join(" ")
}

fn is_list_item(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("- ")
        || trimmed.starts_with("* ")
        || trimmed.chars().next().is_some_and(|c| c.is_ascii_digit()) && trimmed.contains(". ")
}

fn count_segment_tokens(
    text: &str,
    token_counter: &dyn QueryTokenCounter,
    max_tokens: usize,
) -> Result<usize, QueryPlanningError> {
    token_counter
        .count_tokens(text, max_tokens, false)
        .map_err(QueryPlanningError::Tokenization)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct WhitespaceCounter;

    impl QueryTokenCounter for WhitespaceCounter {
        fn count_tokens(
            &self,
            text: &str,
            max_length: usize,
            allow_truncation: bool,
        ) -> Result<usize, String> {
            let count = text.split_whitespace().count();
            if count > max_length && !allow_truncation {
                return Err(format!("{count} tokens exceeds max_length={max_length}"));
            }
            Ok(count.min(max_length))
        }
    }

    fn config() -> QueryProcessingConfig {
        QueryProcessingConfig {
            segment_target_tokens: 8,
            segment_max_tokens: 10,
            segment_overlap_tokens: 2,
            max_segments: 6,
            ..Default::default()
        }
    }

    #[test]
    fn preserves_paragraph_boundaries() {
        let query = "one two three four.\n\nfive six seven eight nine ten eleven twelve?";
        let segments = segment_query(query, &WhitespaceCounter, &config()).unwrap();
        assert!(segments.len() >= 2);
        assert!(segments[0].text.contains("one two three four"));
    }

    #[test]
    fn preserves_sentence_boundaries() {
        let query = "alpha beta gamma delta. What does legal hold do to TTL cleanup?";
        let segments = segment_query(query, &WhitespaceCounter, &config()).unwrap();
        assert!(segments.iter().any(|s| s.text.contains("legal hold")));
    }

    #[test]
    fn preserves_list_items() {
        let query = "- first item with context\n- second item asks what happens next?";
        let segments = segment_query(query, &WhitespaceCounter, &config()).unwrap();
        assert!(segments.iter().any(|s| s.text.contains("first item")));
        assert!(segments.iter().any(|s| s.text.contains("second item")));
    }

    #[test]
    fn preserves_fenced_code_blocks() {
        let query = "Context\n```text\nORA-00904 in /api/v1/documents\n```\nWhat failed?";
        let segments = segment_query(query, &WhitespaceCounter, &config()).unwrap();
        assert!(segments.iter().any(|s| s.text.contains("ORA-00904")));
    }

    #[test]
    fn oversized_code_block_is_split() {
        let body = (0..40)
            .map(|index| format!("line_{index}"))
            .collect::<Vec<_>>()
            .join(" ");
        let query = format!("```text\n{body}\n```");
        let segments = segment_query(&query, &WhitespaceCounter, &config()).unwrap();
        assert!(segments.len() > 1);
        assert!(segments.iter().all(|segment| segment.token_count <= 10));
    }

    #[test]
    fn filename_is_not_split_as_sentence() {
        assert_eq!(
            split_sentences("Inspect runtime-quality-report.json before rollout.").len(),
            1
        );
    }

    #[test]
    fn version_is_not_split_as_sentence() {
        assert_eq!(
            split_sentences("Compare v1.2.3 with service.example.com.").len(),
            1
        );
    }

    #[test]
    fn preserves_api_path() {
        let segments = segment_query(
            "Explain /api/v1/documents failure",
            &WhitespaceCounter,
            &config(),
        )
        .unwrap();
        assert!(segments[0].has_technical_identifier);
    }

    #[test]
    fn preserves_error_code() {
        let segments =
            segment_query("ORA-00904 appears in logs", &WhitespaceCounter, &config()).unwrap();
        assert!(segments[0].has_technical_identifier);
    }

    #[test]
    fn preserves_file_name() {
        let segments = segment_query(
            "Check src/grpc/mod.rs for timeout",
            &WhitespaceCounter,
            &config(),
        )
        .unwrap();
        assert!(segments[0].has_technical_identifier);
    }

    #[test]
    fn preserves_snake_case_identifier() {
        let segments = segment_query(
            "The field access_zone_id appears",
            &WhitespaceCounter,
            &config(),
        )
        .unwrap();
        assert!(segments[0].has_technical_identifier);
    }

    #[test]
    fn respects_max_tokens() {
        let query = (0..40)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let segments = segment_query(&query, &WhitespaceCounter, &config()).unwrap();
        assert!(segments.iter().all(|s| s.token_count <= 10));
    }

    #[test]
    fn does_not_create_empty_segments() {
        let query = "alpha beta\n\n\n gamma delta";
        let segments = segment_query(query, &WhitespaceCounter, &config()).unwrap();
        assert!(segments.iter().all(|s| !s.text.trim().is_empty()));
    }

    #[test]
    fn is_deterministic() {
        let query = (0..35)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let left = segment_query(&query, &WhitespaceCounter, &config()).unwrap();
        let right = segment_query(&query, &WhitespaceCounter, &config()).unwrap();
        assert_eq!(
            left.iter().map(|s| &s.sha256).collect::<Vec<_>>(),
            right.iter().map(|s| &s.sha256).collect::<Vec<_>>()
        );
    }

    #[test]
    fn is_utf8_safe() {
        let query = "Контекст по удержанию документов. What does legal hold do?";
        let segments = segment_query(query, &WhitespaceCounter, &config()).unwrap();
        assert!(!segments.is_empty());
    }
}
