use crate::error::AstraError;
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;

pub async fn verify(path: &str, expected: &str, required: bool) -> Result<String, AstraError> {
    if expected.is_empty() {
        if required {
            tracing::error!(path = %path, "model/tokenizer checksum is required but missing");
            return Err(AstraError::FailedPrecondition(
                "model/tokenizer checksum required".into(),
            ));
        }
        return Ok(String::new());
    }
    let mut f = tokio::fs::File::open(path)
        .await
        .map_err(|e| {
            tracing::error!(path = %path, error = %e, "failed to open model/tokenizer file for checksum verification");
            AstraError::Unavailable("model/tokenizer file is unavailable".into())
        })?;
    let mut h = Sha256::new();
    let mut b = vec![0u8; 1024 * 1024];
    loop {
        let n = f
            .read(&mut b)
            .await
            .map_err(|e| {
                tracing::error!(path = %path, error = %e, "failed to read model/tokenizer file for checksum verification");
                AstraError::Unavailable("model/tokenizer file is unreadable".into())
            })?;
        if n == 0 {
            break;
        }
        h.update(&b[..n]);
    }
    let actual = hex::encode(h.finalize());
    if !actual.eq_ignore_ascii_case(expected) {
        tracing::error!(path = %path, actual_sha256 = %actual, "model/tokenizer checksum mismatch");
        return Err(AstraError::FailedPrecondition(
            "model/tokenizer checksum mismatch".into(),
        ));
    }
    Ok(actual)
}
