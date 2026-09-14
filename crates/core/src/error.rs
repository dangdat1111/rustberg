use thiserror::Error;

/// Unified error type shared by every rustberg crate.
///
/// Mirrors the coarse error taxonomy Amoro's Java `AMS` exposes over its
/// Thrift/REST boundary, so the API layer can map these 1:1 onto HTTP
/// status codes (404, 409, 400, 503, 500).
#[derive(Debug, Error)]
pub enum RustbergError {
    #[error("namespace not found: {0}")]
    NamespaceNotFound(String),

    #[error("table not found: {0}")]
    TableNotFound(String),

    #[error("catalog not found: {0}")]
    CatalogNotFound(String),

    #[error("already exists: {0}")]
    AlreadyExists(String),

    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("commit conflict on {table}: expected base snapshot {expected:?}, found {found:?}")]
    CommitConflict {
        table: String,
        expected: Option<i64>,
        found: Option<i64>,
    },

    #[error("storage backend error: {0}")]
    Storage(String),

    #[error("optimize job error: {0}")]
    Optimize(String),

    #[error("logstore error: {0}")]
    LogStore(String),

    #[error("internal error: {0}")]
    Internal(String),
}

pub type RustbergResult<T> = Result<T, RustbergError>;
