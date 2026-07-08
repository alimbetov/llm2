use crate::{
    config::AppConfig,
    dense::l2_normalize,
    error::AstraError,
    sparse::{build_lexical_sparse, build_sparse},
    tokenizer::{CanonicalTokenizer, TokenizedItem},
};
use async_trait::async_trait;
use ndarray::Array2;
use ort::{session::Session, value::TensorRef};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};
#[derive(Debug, Clone)]
pub struct EmbeddingResult {
    pub dense: Option<Vec<f32>>,
    pub sparse_indices: Option<Vec<u32>>,
    pub sparse_values: Option<Vec<f32>>,
    pub token_count: usize,
    pub truncated: bool,
}
#[derive(Debug, Clone)]
pub struct InferenceInput {
    pub text: String,
    pub max_length: usize,
    pub allow_truncation: bool,
    pub want_dense: bool,
    pub want_sparse: bool,
    pub token_count_hint: usize,
}
#[async_trait]
pub trait InferenceEngine: Send + Sync {
    async fn encode_batch(
        &self,
        inputs: Vec<InferenceInput>,
    ) -> Result<Vec<EmbeddingResult>, AstraError>;
    fn dense_available(&self) -> bool;
    fn sparse_available(&self) -> bool;
    fn count_tokens(
        &self,
        text: &str,
        max_length: usize,
        allow_truncation: bool,
    ) -> Result<usize, AstraError>;
    async fn self_test(&self) -> Result<(), AstraError>;
}
pub struct OnnxBgeM3Engine {
    cfg: Arc<AppConfig>,
    tokenizer: CanonicalTokenizer,
    session: Arc<Mutex<Session>>,
    sparse_capability: bool,
    neural_sparse_capability: bool,
}
impl OnnxBgeM3Engine {
    pub fn load(
        cfg: Arc<AppConfig>,
        tokenizer: CanonicalTokenizer,
        _provider: &str,
    ) -> Result<Self, AstraError> {
        let mut b = Session::builder().map_err(oe)?;
        #[cfg(feature = "cuda")]
        if _provider == "CUDA" {
            use ort::execution_providers::CUDAExecutionProvider;
            b = b
                .with_execution_providers([CUDAExecutionProvider::default().build()])
                .map_err(oe)?;
        }
        #[cfg(feature = "tensorrt")]
        if _provider == "TENSOR_RT" || _provider == "TENSORRT" {
            use ort::execution_providers::TensorRTExecutionProvider;
            b = b
                .with_execution_providers([TensorRTExecutionProvider::default().build()])
                .map_err(oe)?;
        }
        let s = b.commit_from_file(&cfg.model.path).map_err(oe)?;
        let output_names: HashSet<String> =
            s.outputs().iter().map(|x| x.name().to_owned()).collect();
        let dense = output_names.contains(&cfg.model.dense_output_name)
            || output_names.contains(&cfg.model.token_output_name);
        if !dense {
            return Err(AstraError::FailedPrecondition(format!(
                "ONNX has neither {} nor {}",
                cfg.model.dense_output_name, cfg.model.token_output_name
            )));
        }
        let neural_sparse =
            output_names.contains(&cfg.model.sparse_output_name) && cfg.sparse.enabled;
        let sparse = cfg.sparse.enabled;
        Ok(Self {
            cfg,
            tokenizer,
            session: Arc::new(Mutex::new(s)),
            sparse_capability: sparse,
            neural_sparse_capability: neural_sparse,
        })
    }
    #[allow(clippy::type_complexity)]
    fn tokenize_batch(
        &self,
        inputs: &[InferenceInput],
    ) -> Result<(Vec<TokenizedItem>, Array2<i64>, Array2<i64>), AstraError> {
        let mut toks = Vec::with_capacity(inputs.len());
        for i in inputs {
            toks.push(
                self.tokenizer
                    .encode(&i.text, i.max_length, i.allow_truncation)?,
            )
        }
        let seq = toks.iter().map(|x| x.input_ids.len()).max().unwrap_or(1);
        let mut ids = Array2::<i64>::zeros((toks.len(), seq));
        let mut mask = Array2::<i64>::zeros((toks.len(), seq));
        for (r, t) in toks.iter().enumerate() {
            for (c, v) in t.input_ids.iter().enumerate() {
                ids[(r, c)] = *v as i64;
                mask[(r, c)] = t.attention_mask[c] as i64
            }
        }
        Ok((toks, ids, mask))
    }
    fn run_sync(&self, inputs: Vec<InferenceInput>) -> Result<Vec<EmbeddingResult>, AstraError> {
        let (toks, ids, mask) = self.tokenize_batch(&inputs)?;
        let batch = toks.len();
        let seq = ids.shape()[1];
        let mut session = self
            .session
            .lock()
            .map_err(|_| AstraError::Internal("ONNX session mutex poisoned".into()))?;
        let outputs = session
            .run(ort::inputs![
                TensorRef::from_array_view(&ids).map_err(oe)?,
                TensorRef::from_array_view(&mask).map_err(oe)?
            ])
            .map_err(oe)?;
        let dense_ready = outputs.get(&self.cfg.model.dense_output_name);
        let hidden = outputs.get(&self.cfg.model.token_output_name);
        let lexical = outputs.get(&self.cfg.model.sparse_output_name);
        let dense_data = if let Some(v) = dense_ready {
            let (_, d) = v.try_extract_tensor::<f32>().map_err(oe)?;
            Some((d.to_vec(), false))
        } else if let Some(v) = hidden {
            let (_, d) = v.try_extract_tensor::<f32>().map_err(oe)?;
            Some((d.to_vec(), true))
        } else {
            None
        };
        let lexical_data = if let Some(v) = lexical {
            let (_, d) = v.try_extract_tensor::<f32>().map_err(oe)?;
            Some(d.to_vec())
        } else {
            None
        };
        let mut out = Vec::with_capacity(batch);
        for row in 0..batch {
            let input = &inputs[row];
            let dense = if input.want_dense {
                let (data, token_level) = dense_data.as_ref().ok_or_else(|| {
                    AstraError::FailedPrecondition("dense output unavailable".into())
                })?;
                let raw = if *token_level {
                    let start = row * seq * self.cfg.dense.dimension;
                    data.get(start..start + self.cfg.dense.dimension)
                        .ok_or_else(|| {
                            AstraError::Internal("hidden-state output shape mismatch".into())
                        })?
                        .to_vec()
                } else {
                    let start = row * self.cfg.dense.dimension;
                    data.get(start..start + self.cfg.dense.dimension)
                        .ok_or_else(|| {
                            AstraError::Internal("sentence embedding output shape mismatch".into())
                        })?
                        .to_vec()
                };
                Some(l2_normalize(raw, self.cfg.dense.dimension)?)
            } else {
                None
            };
            let (si, sv) = if input.want_sparse {
                if !self.sparse_capability {
                    return Err(AstraError::FailedPrecondition(
                        "sparse requested but sparse encoder unavailable".into(),
                    ));
                }
                let (i, v) = if self.neural_sparse_capability {
                    let all = lexical_data.as_ref().ok_or_else(|| {
                        AstraError::Internal(
                            "sparse lexical_data missing while sparse output requested".into(),
                        )
                    })?;
                    let start = row * seq;
                    let weights = all.get(start..start + seq).ok_or_else(|| {
                        AstraError::Internal("lexical output shape mismatch".into())
                    })?;
                    build_sparse(
                        &toks[row].input_ids,
                        &toks[row].attention_mask,
                        &weights[..toks[row].input_ids.len()],
                        self.tokenizer.special_ids(),
                        self.cfg.sparse.min_weight,
                        self.cfg.sparse.max_non_zero,
                    )?
                } else {
                    build_lexical_sparse(
                        &input.text,
                        &toks[row].input_ids,
                        &toks[row].attention_mask,
                        self.tokenizer.special_ids(),
                        self.cfg.sparse.min_weight,
                        self.cfg.sparse.max_non_zero,
                    )?
                };
                (Some(i), Some(v))
            } else {
                (None, None)
            };
            out.push(EmbeddingResult {
                dense,
                sparse_indices: si,
                sparse_values: sv,
                token_count: toks[row].token_count,
                truncated: toks[row].truncated,
            })
        }
        Ok(out)
    }
}
#[async_trait]
impl InferenceEngine for OnnxBgeM3Engine {
    async fn encode_batch(
        &self,
        inputs: Vec<InferenceInput>,
    ) -> Result<Vec<EmbeddingResult>, AstraError> {
        let this = self.session.clone();
        let cfg = self.cfg.clone();
        let tok = self.tokenizer.clone();
        let sparse = self.sparse_capability;
        let neural_sparse = self.neural_sparse_capability;
        tokio::task::spawn_blocking(move || {
            OnnxBgeM3Engine {
                cfg,
                tokenizer: tok,
                session: this,
                sparse_capability: sparse,
                neural_sparse_capability: neural_sparse,
            }
            .run_sync(inputs)
        })
        .await
        .map_err(|e| AstraError::Internal(format!("inference task: {e}")))?
    }
    fn dense_available(&self) -> bool {
        true
    }
    fn sparse_available(&self) -> bool {
        self.sparse_capability
    }
    fn count_tokens(
        &self,
        text: &str,
        max_length: usize,
        allow_truncation: bool,
    ) -> Result<usize, AstraError> {
        Ok(self
            .tokenizer
            .encode(text, max_length, allow_truncation)?
            .token_count)
    }
    async fn self_test(&self) -> Result<(), AstraError> {
        let r = self
            .encode_batch(vec![InferenceInput {
                text: "AstraVector self test".into(),
                max_length: 32,
                allow_truncation: false,
                want_dense: true,
                want_sparse: self.sparse_capability,
                token_count_hint: 8,
            }])
            .await?;
        let x = r
            .first()
            .ok_or_else(|| AstraError::Internal("empty self-test result".into()))?;
        if x.dense.as_ref().map(|v| v.len()) != Some(self.cfg.dense.dimension) {
            return Err(AstraError::Internal(
                "self-test dense dimension mismatch".into(),
            ));
        }
        Ok(())
    }
}
fn oe<E: std::fmt::Display>(e: E) -> AstraError {
    AstraError::Unavailable(format!("ONNX Runtime: {e}"))
}
