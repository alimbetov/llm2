use thiserror::Error;
#[derive(Debug, Error, Clone)]
pub enum AstraError {
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("failed precondition: {0}")]
    FailedPrecondition(String),
    #[error("already exists: {0}")]
    AlreadyExists(String),
    #[error("input too long: {0}")]
    OutOfRange(String),
    #[error("resource exhausted: {0}")]
    ResourceExhausted(String),
    #[error("deadline exceeded: {0}")]
    DeadlineExceeded(String),
    #[error("cancelled: {0}")]
    Cancelled(String),
    #[error("ownership lost: {0}")]
    OwnershipLost(String),
    #[error("unauthenticated: {0}")]
    Unauthenticated(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("unavailable: {0}")]
    Unavailable(String),
    #[error("internal error: {0}")]
    Internal(String),
}
impl From<AstraError> for tonic::Status {
    fn from(v: AstraError) -> Self {
        match v {
            AstraError::InvalidArgument(m) => tonic::Status::invalid_argument(m),
            AstraError::FailedPrecondition(m) => tonic::Status::failed_precondition(m),
            AstraError::AlreadyExists(m) => tonic::Status::already_exists(m),
            AstraError::OutOfRange(m) => tonic::Status::out_of_range(m),
            AstraError::ResourceExhausted(m) => tonic::Status::resource_exhausted(m),
            AstraError::DeadlineExceeded(m) => tonic::Status::deadline_exceeded(m),
            AstraError::Cancelled(m) => tonic::Status::cancelled(m),
            AstraError::OwnershipLost(m) => tonic::Status::aborted(m),
            AstraError::Unauthenticated(m) => tonic::Status::unauthenticated(m),
            AstraError::PermissionDenied(m) => tonic::Status::permission_denied(m),
            AstraError::NotFound(m) => tonic::Status::not_found(m),
            AstraError::Unavailable(m) => tonic::Status::unavailable(m),
            AstraError::Internal(m) => tonic::Status::internal(m),
        }
    }
}
