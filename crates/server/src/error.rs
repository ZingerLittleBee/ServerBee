use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ApiResponse<T: Serialize> {
    pub data: T,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct PaginatedResponse<T: Serialize> {
    pub data: Vec<T>,
    pub total: u64,
    pub page: u64,
    pub page_size: u64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Unauthorized")]
    Unauthorized,
    #[error("Forbidden: {0}")]
    Forbidden(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Too many requests: {0}")]
    TooManyRequests(String),
    #[error("Conflict: {0}")]
    Conflict(String),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Request timeout: {0}")]
    RequestTimeout(String),
    /// An upstream/external dependency (webhook target, push provider, mail
    /// API, etc.) failed or was unreachable. This is not a ServerBee fault, so
    /// it must not surface as "Internal error" (500) — that misleads the user
    /// into thinking the server broke when their own config or a third party is
    /// at fault. The message is bare (no prefix) because delivery messages are
    /// already self-descriptive (e.g. "Webhook request failed: ...").
    #[error("{0}")]
    BadGateway(String),
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("{message}")]
    Domain {
        status: StatusCode,
        code: &'static str,
        message: String,
        details: Option<serde_json::Value>,
    },
}

impl AppError {
    /// Bare error message for audit-log `deny_reason` details (no status-code
    /// prefix). `None` for `Unauthorized`, which is deliberately opaque.
    pub fn audit_reason(&self) -> Option<&str> {
        match self {
            AppError::Forbidden(message)
            | AppError::BadRequest(message)
            | AppError::NotFound(message)
            | AppError::Conflict(message)
            | AppError::RequestTimeout(message)
            | AppError::Validation(message)
            | AppError::TooManyRequests(message)
            | AppError::BadGateway(message)
            | AppError::Internal(message) => Some(message.as_str()),
            AppError::Unauthorized => None,
            AppError::Domain { message, .. } => Some(message.as_str()),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let message = self.to_string();
        let (status, code, details) = match self {
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, "BAD_REQUEST".to_string(), None),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED".to_string(), None),
            AppError::Forbidden(_) => (StatusCode::FORBIDDEN, "FORBIDDEN".to_string(), None),
            AppError::TooManyRequests(_) => (StatusCode::TOO_MANY_REQUESTS, "TOO_MANY_REQUESTS".to_string(), None),
            AppError::NotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND".to_string(), None),
            AppError::Conflict(_) => (StatusCode::CONFLICT, "CONFLICT".to_string(), None),
            AppError::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION_ERROR".to_string(), None),
            AppError::RequestTimeout(_) => (StatusCode::REQUEST_TIMEOUT, "REQUEST_TIMEOUT".to_string(), None),
            AppError::BadGateway(_) => (StatusCode::BAD_GATEWAY, "BAD_GATEWAY".to_string(), None),
            AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR".to_string(), None),
            AppError::Domain { status, code, details, .. } => (status, code.to_string(), details),
        };
        let body = ErrorBody { error: ErrorDetail { code, message, details } };
        (status, Json(body)).into_response()
    }
}

impl From<sea_orm::DbErr> for AppError {
    fn from(err: sea_orm::DbErr) -> Self {
        tracing::error!("Database error: {err}");
        AppError::Internal("Database error".to_string())
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        tracing::error!("Internal error: {err}");
        AppError::Internal("Internal error".to_string())
    }
}

#[allow(dead_code)]
pub type ApiResult<T> = Result<Json<ApiResponse<T>>, AppError>;

pub fn ok<T: Serialize>(data: T) -> Result<Json<ApiResponse<T>>, AppError> {
    Ok(Json(ApiResponse { data }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    #[tokio::test]
    async fn domain_error_serializes_with_code_and_details() {
        let err = AppError::Domain {
            status: StatusCode::BAD_REQUEST,
            code: "ZIP_SLIP",
            message: "package contains unsafe path".to_string(),
            details: Some(serde_json::json!({ "entry": "../etc/passwd" })),
        };

        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let body = to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "ZIP_SLIP");
        assert_eq!(json["error"]["message"], "package contains unsafe path");
        assert_eq!(json["error"]["details"]["entry"], "../etc/passwd");
    }

    #[tokio::test]
    async fn existing_variant_response_unchanged() {
        let err = AppError::BadRequest("test".into());
        let resp = err.into_response();
        let body = to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "BAD_REQUEST");
        assert!(json["error"].get("details").is_none(), "details must be omitted when absent");
    }

    #[tokio::test]
    async fn bad_request_preserves_prefixed_message() {
        let err = AppError::BadRequest("foo".to_string());
        let resp = err.into_response();
        let body = to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["message"], "Bad request: foo");
    }

    #[tokio::test]
    async fn bad_gateway_maps_to_502_with_bare_message() {
        // Upstream/external delivery failures (webhook, push, mail) must be 502,
        // not 500 "Internal error", and carry the self-descriptive message
        // verbatim so the user learns it is their endpoint that is unreachable.
        let err = AppError::BadGateway("Webhook request failed: connection refused".to_string());
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);

        let body = to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "BAD_GATEWAY");
        assert_eq!(json["error"]["message"], "Webhook request failed: connection refused");
    }

    #[tokio::test]
    async fn domain_uses_bare_message() {
        let err = AppError::Domain {
            status: StatusCode::BAD_REQUEST,
            code: "TEST",
            message: "bare message".to_string(),
            details: None,
        };
        let resp = err.into_response();
        let body = to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["message"], "bare message");
    }
}
