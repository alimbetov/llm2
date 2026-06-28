use crate::{config::ProviderConfig, error::AstraError};
#[derive(Debug, Clone)]
pub struct SelectedProvider {
    pub name: String,
    pub fallback_used: bool,
}
pub fn candidates(c: &ProviderConfig) -> Result<Vec<SelectedProvider>, AstraError> {
    let mode = c.mode.to_uppercase();
    if mode != "AUTO" {
        let mut v = vec![SelectedProvider {
            name: normalize(&mode),
            fallback_used: false,
        }];
        if c.fallback_to_cpu && mode != "CPU" {
            v.push(SelectedProvider {
                name: "CPU".into(),
                fallback_used: true,
            })
        }
        return Ok(v);
    }
    let mut out = Vec::new();
    for (i, x) in c.preference.iter().enumerate() {
        let n = normalize(&x.to_uppercase());
        if n == "CPU" || compiled(&n) {
            out.push(SelectedProvider {
                name: n,
                fallback_used: i > 0,
            })
        }
    }
    if out.is_empty() {
        return Err(AstraError::Unavailable(
            "no execution provider candidate".into(),
        ));
    }
    Ok(out)
}
fn normalize(s: &str) -> String {
    if s == "TENSORRT" {
        "TENSOR_RT".into()
    } else {
        s.into()
    }
}
fn compiled(n: &str) -> bool {
    n == "CPU"
        || (n == "CUDA" && cfg!(feature = "cuda"))
        || (n == "TENSOR_RT" && cfg!(feature = "tensorrt"))
}
