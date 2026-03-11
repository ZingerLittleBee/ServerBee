use std::sync::Arc;

use axum::extract::State;
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::error::{ok, ApiResponse, AppError};
use crate::service::auth::AuthService;
use crate::service::config::ConfigService;
use crate::state::AppState;

const CONFIG_KEY_SETTINGS: &str = "system_settings";
const CONFIG_KEY_AUTO_DISCOVERY: &str = "auto_discovery_key";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SystemSettings {
    site_name: Option<String>,
    site_description: Option<String>,
    custom_css: Option<String>,
    custom_js: Option<String>,
}

#[derive(Debug, Serialize)]
struct AutoDiscoveryKeyResponse {
    key: String,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/settings", get(get_settings))
        .route("/settings", put(update_settings))
        .route("/settings/auto-discovery-key", get(get_auto_discovery_key))
        .route(
            "/settings/auto-discovery-key",
            put(regenerate_auto_discovery_key),
        )
}

async fn get_settings(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<SystemSettings>>, AppError> {
    let settings: SystemSettings = ConfigService::get_typed(&state.db, CONFIG_KEY_SETTINGS)
        .await?
        .unwrap_or_default();
    ok(settings)
}

async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SystemSettings>,
) -> Result<Json<ApiResponse<SystemSettings>>, AppError> {
    ConfigService::set_typed(&state.db, CONFIG_KEY_SETTINGS, &body).await?;
    ok(body)
}

async fn get_auto_discovery_key(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<AutoDiscoveryKeyResponse>>, AppError> {
    let key = ConfigService::get(&state.db, CONFIG_KEY_AUTO_DISCOVERY)
        .await?
        .unwrap_or_default();
    ok(AutoDiscoveryKeyResponse { key })
}

async fn regenerate_auto_discovery_key(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<AutoDiscoveryKeyResponse>>, AppError> {
    let new_key = AuthService::generate_session_token();
    ConfigService::set(&state.db, CONFIG_KEY_AUTO_DISCOVERY, &new_key).await?;
    ok(AutoDiscoveryKeyResponse { key: new_key })
}
