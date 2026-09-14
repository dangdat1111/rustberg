use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use rustberg_core::error::RustbergError;

/// Wraps `RustbergError` so it can be returned directly from axum handlers.
/// Status-code mapping follows the Iceberg REST Catalog spec's error
/// model (404 for missing namespace/table, 409 for commit conflicts,
/// 400 for bad requests).
pub struct ApiError(pub RustbergError);

impl From<RustbergError> for ApiError {
    fn from(e: RustbergError) -> Self {
        ApiError(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self.0 {
            RustbergError::NamespaceNotFound(m) | RustbergError::TableNotFound(m) | RustbergError::CatalogNotFound(m) => {
                (StatusCode::NOT_FOUND, m.clone())
            }
            RustbergError::AlreadyExists(m) => (StatusCode::CONFLICT, m.clone()),
            RustbergError::CommitConflict { .. } => (StatusCode::CONFLICT, self.0.to_string()),
            RustbergError::InvalidRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            RustbergError::Storage(_) | RustbergError::Optimize(_) | RustbergError::LogStore(_) | RustbergError::Internal(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".to_string())
            }
        };

        if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = %self.0, "request failed");
        }

        (status, Json(json!({ "error": message }))).into_response()
    }
}
