use crate::config::AppConfig;
use crate::pb::{ContractVersions, RuntimeMetadata};

pub const CONTRACT_VERSION: &str = "astravector_embedding_contract_v4_0";
pub const POOLING_VERSION: &str = "cls_v1";
pub const NORMALIZATION_VERSION: &str = "l2_v1";

pub fn versions(cfg: &AppConfig) -> ContractVersions {
    ContractVersions {
        contract_version: CONTRACT_VERSION.into(),
        model_version: cfg.model.version.clone(),
        tokenizer_version: cfg.tokenizer.version.clone(),
        dense_version: cfg.dense.version.clone(),
        sparse_version: cfg.sparse.version.clone(),
        pooling_version: POOLING_VERSION.into(),
        normalization_version: NORMALIZATION_VERSION.into(),
    }
}

pub fn runtime_metadata(provider: &str, fallback_used: bool) -> RuntimeMetadata {
    RuntimeMetadata {
        execution_provider: provider.into(),
        onnxruntime_version: "runtime-detected".into(),
        device_name: provider.into(),
        fallback_used,
    }
}
