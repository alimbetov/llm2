use crate::error::AstraError;
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;
pub async fn verify(path: &str, expected: &str, required: bool) -> Result<String, AstraError> {
    if expected.is_empty() {
        if required {
            return Err(AstraError::FailedPrecondition(format!(
                "checksum required for {path}"
            )));
        }
        return Ok(String::new());
    }
    let mut f = tokio::fs::File::open(path)
        .await
        .map_err(|e| AstraError::Unavailable(format!("open {path}: {e}")))?;
    let mut h = Sha256::new();
    let mut b = vec![0u8; 1024 * 1024];
    loop {
        let n = f
            .read(&mut b)
            .await
            .map_err(|e| AstraError::Unavailable(format!("read {path}: {e}")))?;
        if n == 0 {
            break;
        }
        h.update(&b[..n]);
    }
    let actual = hex::encode(h.finalize());
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(AstraError::FailedPrecondition(format!(
            "checksum mismatch for {path}: expected={expected}, actual={actual}"
        )));
    }
    Ok(actual)
}
