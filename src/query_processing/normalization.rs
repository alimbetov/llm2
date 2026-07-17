use crate::query_processing::planner::QueryTokenCounter;
use crate::tokenizer::TokenOffset;

#[derive(Debug, Clone)]
pub struct NormalizedQuery {
    pub original_text: String,
    pub normalized_text: String,
    /// Maps every normalized byte boundary to the corresponding original byte boundary.
    pub normalized_to_original_byte_map: Vec<usize>,
    pub token_offsets: Vec<TokenOffset>,
}

impl NormalizedQuery {
    pub fn original_byte_range(
        &self,
        normalized_start: usize,
        normalized_end: usize,
    ) -> Option<(usize, usize)> {
        if normalized_start > normalized_end || normalized_end > self.normalized_text.len() {
            return None;
        }
        Some((
            *self.normalized_to_original_byte_map.get(normalized_start)?,
            *self.normalized_to_original_byte_map.get(normalized_end)?,
        ))
    }
}

#[derive(Debug, Clone, Copy)]
struct MappedChar {
    ch: char,
    original_start: usize,
    original_end: usize,
}

pub fn normalize_query(
    original: &str,
    token_counter: &dyn QueryTokenCounter,
) -> Result<NormalizedQuery, String> {
    let canonical = canonical_chars(original);
    let trimmed = trim_outer_whitespace(&canonical);
    let normalized_chars = normalize_lines(trimmed);
    let (normalized_text, normalized_to_original_byte_map) =
        render(original.len(), &normalized_chars);
    let token_offsets = token_counter.token_offsets(&normalized_text)?;
    validate_offsets(&normalized_text, &token_offsets)?;
    Ok(NormalizedQuery {
        original_text: original.to_owned(),
        normalized_text,
        normalized_to_original_byte_map,
        token_offsets,
    })
}

fn canonical_chars(original: &str) -> Vec<MappedChar> {
    let mut chars = Vec::new();
    let mut iter = original.char_indices().peekable();
    while let Some((start, ch)) = iter.next() {
        if ch == '\r' {
            let end = if iter.peek().is_some_and(|(_, next)| *next == '\n') {
                let (newline_start, newline) = iter.next().expect("peeked CRLF newline");
                newline_start + newline.len_utf8()
            } else {
                start + ch.len_utf8()
            };
            chars.push(MappedChar {
                ch: '\n',
                original_start: start,
                original_end: end,
            });
        } else {
            chars.push(MappedChar {
                ch,
                original_start: start,
                original_end: start + ch.len_utf8(),
            });
        }
    }
    chars
}

fn trim_outer_whitespace(chars: &[MappedChar]) -> &[MappedChar] {
    let start = chars
        .iter()
        .position(|mapped| !mapped.ch.is_whitespace())
        .unwrap_or(chars.len());
    let end = chars
        .iter()
        .rposition(|mapped| !mapped.ch.is_whitespace())
        .map_or(start, |index| index + 1);
    &chars[start..end]
}

fn normalize_lines(chars: &[MappedChar]) -> Vec<MappedChar> {
    let mut out = Vec::with_capacity(chars.len());
    let mut line_start = 0usize;
    let mut blank_lines = 0usize;
    for index in 0..=chars.len() {
        let at_end = index == chars.len();
        if !at_end && chars[index].ch != '\n' {
            continue;
        }
        let mut line_end = index;
        while line_end > line_start
            && chars[line_end - 1].ch.is_whitespace()
            && chars[line_end - 1].ch != '\n'
        {
            line_end -= 1;
        }
        let blank = chars[line_start..line_end]
            .iter()
            .all(|mapped| mapped.ch.is_whitespace());
        if blank {
            blank_lines += 1;
        } else {
            blank_lines = 0;
            out.extend_from_slice(&chars[line_start..line_end]);
        }
        if !at_end && (!blank || blank_lines <= 2) {
            out.push(chars[index]);
        }
        line_start = index.saturating_add(1);
    }
    while out.last().is_some_and(|mapped| mapped.ch.is_whitespace()) {
        out.pop();
    }
    out
}

fn render(original_len: usize, chars: &[MappedChar]) -> (String, Vec<usize>) {
    let mut text = String::new();
    let mut map = Vec::new();
    for mapped in chars {
        if map.is_empty() {
            map.push(mapped.original_start);
        } else if let Some(boundary) = map.last_mut() {
            *boundary = mapped.original_start;
        }
        let mut encoded = [0_u8; 4];
        let bytes = mapped.ch.encode_utf8(&mut encoded).as_bytes();
        text.push(mapped.ch);
        for boundary in 1..=bytes.len() {
            map.push(if boundary == bytes.len() {
                mapped.original_end
            } else {
                mapped.original_start + boundary
            });
        }
    }
    if map.is_empty() {
        map.push(original_len);
    }
    (text, map)
}

fn validate_offsets(text: &str, offsets: &[TokenOffset]) -> Result<(), String> {
    for offset in offsets {
        if offset.start_byte >= offset.end_byte
            || offset.end_byte > text.len()
            || !text.is_char_boundary(offset.start_byte)
            || !text.is_char_boundary(offset.end_byte)
        {
            return Err(format!(
                "token {} has invalid UTF-8 byte range {}..{}",
                offset.token_index, offset.start_byte, offset.end_byte
            ));
        }
    }
    Ok(())
}
