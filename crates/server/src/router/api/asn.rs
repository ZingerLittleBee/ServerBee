use std::sync::Arc;
use std::sync::atomic::Ordering;

use axum::extract::State;
use axum::routing;
use axum::{Json, Router};
use serde::Serialize;
use utoipa::ToSchema;

use crate::error::{ApiResponse, AppError, ok};
use crate::service::asn;
use crate::state::AppState;

#[derive(Serialize, ToSchema)]
pub struct AsnStatus {
    pub installed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct AsnDownloadResponse {
    pub success: bool,
    pub message: String,
}

#[utoipa::path(
    get,
    path = "/api/asn/status",
    tag = "asn",
    responses(
        (status = 200, description = "ASN database install status", body = AsnStatus),
    )
)]
pub async fn asn_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<AsnStatus>>, AppError> {
    let guard = state.asn.read().unwrap();
    let status = match guard.as_ref() {
        Some(service) => {
            let source = if !state.config.asn.mmdb_path.is_empty() {
                "custom"
            } else {
                "downloaded"
            };
            let (file_size, updated_at) = std::fs::metadata(&service.source_path)
                .map(|m| {
                    let size = m.len() as i64;
                    let modified = m.modified().ok().map(|t| {
                        let dt: chrono::DateTime<chrono::Utc> = t.into();
                        dt.to_rfc3339()
                    });
                    (Some(size), modified)
                })
                .unwrap_or((None, None));
            AsnStatus {
                installed: true,
                source: Some(source.to_string()),
                file_size,
                updated_at,
            }
        }
        None => AsnStatus {
            installed: false,
            source: None,
            file_size: None,
            updated_at: None,
        },
    };
    ok(status)
}

#[utoipa::path(
    post,
    path = "/api/asn/download",
    tag = "asn",
    responses(
        (status = 200, description = "ASN download result", body = AsnDownloadResponse),
    )
)]
pub async fn asn_download(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<AsnDownloadResponse>>, AppError> {
    // Concurrent download guard
    if state.asn_downloading.swap(true, Ordering::SeqCst) {
        return ok(AsnDownloadResponse {
            success: false,
            message: "Download already in progress".to_string(),
        });
    }

    let result = asn::download_dbip_asn(&state.config.server.data_dir).await;

    match result {
        Ok(service) => {
            let mut guard = state.asn.write().unwrap();
            *guard = Some(service);
            state.asn_downloading.store(false, Ordering::SeqCst);
            ok(AsnDownloadResponse {
                success: true,
                message: "ASN database installed successfully".to_string(),
            })
        }
        Err(e) => {
            state.asn_downloading.store(false, Ordering::SeqCst);
            ok(AsnDownloadResponse {
                success: false,
                message: e,
            })
        }
    }
}

pub fn read_router() -> Router<Arc<AppState>> {
    Router::new().route("/asn/status", routing::get(asn_status))
}

pub fn write_router() -> Router<Arc<AppState>> {
    Router::new().route("/asn/download", routing::post(asn_download))
}
