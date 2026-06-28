use crate::{config::AppConfig, error::AstraError};
use std::{collections::HashSet, sync::Arc};
use tokenizers::{Encoding, Tokenizer};

#[derive(Clone)]
pub struct CanonicalTokenizer {
    inner: Arc<Tokenizer>,
    special_ids: Arc<HashSet<u32>>,
}

#[derive(Debug, Clone)]
pub struct TokenizedItem {
    pub input_ids: Vec<u32>,
    pub attention_mask: Vec<u32>,
    pub token_count: usize,
    pub truncated: bool,
}

impl CanonicalTokenizer {
    pub fn load(cfg: &AppConfig) -> Result<Self, AstraError> {
        let tokenizer = Tokenizer::from_file(&cfg.tokenizer.path)
            .map_err(|e| AstraError::Unavailable(format!("load tokenizer: {e}")))?;
        let mut special = HashSet::new();
        for token in ["<s>", "</s>", "<pad>", "[CLS]", "[SEP]", "[PAD]"] {
            if let Some(id) = tokenizer.token_to_id(token) {
                special.insert(id);
            }
        }
        Ok(Self {
            inner: Arc::new(tokenizer),
            special_ids: Arc::new(special),
        })
    }

    pub fn encode(
        &self,
        text: &str,
        max_length: usize,
        allow_truncation: bool,
    ) -> Result<TokenizedItem, AstraError> {
        let encoding: Encoding = self
            .inner
            .encode(text, true)
            .map_err(|e| AstraError::InvalidArgument(format!("tokenization failed: {e}")))?;
        let original = encoding.get_ids().len();
        if original > max_length && !allow_truncation {
            return Err(AstraError::OutOfRange(format!(
                "{original} tokens exceeds max_length={max_length}"
            )));
        }
        let take = original.min(max_length);
        Ok(TokenizedItem {
            input_ids: encoding.get_ids()[..take].to_vec(),
            attention_mask: encoding.get_attention_mask()[..take].to_vec(),
            token_count: take,
            truncated: original > max_length,
        })
    }

    pub fn special_ids(&self) -> &HashSet<u32> {
        self.special_ids.as_ref()
    }
}
