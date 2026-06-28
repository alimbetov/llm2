use crate::config::SecurityConfig;
use subtle::ConstantTimeEq;
use tonic::{Request, Status};
#[derive(Clone)]
pub struct ApiKeyAuth {
    enabled: bool,
    key: Vec<u8>,
    protect_health: bool,
}
impl ApiKeyAuth {
    pub fn new(c: &SecurityConfig) -> Self {
        Self {
            enabled: c.enabled,
            key: c.api_key.as_bytes().to_vec(),
            protect_health: c.protect_health,
        }
    }
    #[allow(clippy::result_large_err)]
    pub fn interceptor(&self, req: Request<()>) -> Result<Request<()>, Status> {
        if !self.enabled {
            return Ok(req);
        };
        let path = req
            .extensions()
            .get::<tonic::GrpcMethod>()
            .map(|m| m.method().to_string())
            .unwrap_or_default();
        if !self.protect_health
            && (path == "Health" || path == "GetContract" || path == "GetCapabilities")
        {
            return Ok(req);
        }
        let got = req
            .metadata()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .as_bytes();
        if got.len() == self.key.len() && bool::from(got.ct_eq(&self.key)) {
            Ok(req)
        } else {
            Err(Status::unauthenticated("invalid x-api-key"))
        }
    }
}
