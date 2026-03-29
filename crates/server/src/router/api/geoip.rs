use std::sync::Arc;
use std::sync::atomic::Ordering;

use axum::extract::State;
use axum::routing;
use axum::{Json, Router};
use serde::Serialize;

use crate::error::{ApiResponse, AppError, ok};
use crate::service::geoip;
use crate::state::AppState;

#[derive(Serialize)]
struct GeoIpStatus {
    installed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<String>,
}

#[derive(Serialize)]
struct DownloadResponse {
    success: bool,
    message: String,
}

async fn geoip_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<GeoIpStatus>>, AppError> {
    let guard = state.geoip.read().unwrap();
    let status = match guard.as_ref() {
        Some(service) => {
            let source = if !state.config.geoip.mmdb_path.is_empty() {
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
            GeoIpStatus {
                installed: true,
                source: Some(source.to_string()),
                file_size,
                updated_at,
            }
        }
        None => GeoIpStatus {
            installed: false,
            source: None,
            file_size: None,
            updated_at: None,
        },
    };
    ok(status)
}

async fn geoip_download(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<DownloadResponse>>, AppError> {
    // Concurrent download guard
    if state.geoip_downloading.swap(true, Ordering::SeqCst) {
        return ok(DownloadResponse {
            success: false,
            message: "Download already in progress".to_string(),
        });
    }

    let result = geoip::download_dbip(&state.config.server.data_dir).await;

    match result {
        Ok(service) => {
            let mut guard = state.geoip.write().unwrap();
            *guard = Some(service);
            state.geoip_downloading.store(false, Ordering::SeqCst);
            ok(DownloadResponse {
                success: true,
                message: "GeoIP database installed successfully".to_string(),
            })
        }
        Err(e) => {
            state.geoip_downloading.store(false, Ordering::SeqCst);
            ok(DownloadResponse {
                success: false,
                message: e,
            })
        }
    }
}

pub fn read_router() -> Router<Arc<AppState>> {
    Router::new().route("/geoip/status", routing::get(geoip_status))
}

pub fn write_router() -> Router<Arc<AppState>> {
    Router::new().route("/geoip/download", routing::post(geoip_download))
}
