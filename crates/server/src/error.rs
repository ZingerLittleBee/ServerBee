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

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message, details) = match self {
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, "BAD_REQUEST".to_string(), m, None),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED".to_string(), "Unauthorized".into(), None),
            AppError::Forbidden(m) => (StatusCode::FORBIDDEN, "FORBIDDEN".to_string(), m, None),
            AppError::TooManyRequests(m) => (StatusCode::TOO_MANY_REQUESTS, "TOO_MANY_REQUESTS".to_string(), m, None),
            AppError::NotFound(m) => (StatusCode::NOT_FOUND, "NOT_FOUND".to_string(), m, None),
            AppError::Conflict(m) => (StatusCode::CONFLICT, "CONFLICT".to_string(), m, None),
            AppError::Validation(m) => (StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION_ERROR".to_string(), m, None),
            AppError::RequestTimeout(m) => (StatusCode::REQUEST_TIMEOUT, "REQUEST_TIMEOUT".to_string(), m, None),
            AppError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR".to_string(), m, None),
            AppError::Domain { status, code, message, details } => (status, code.to_string(), message, details),
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
}
