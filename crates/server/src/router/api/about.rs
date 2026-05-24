use std::sync::Arc;

use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use utoipa::ToSchema;

use crate::error::{ApiResponse, AppError, ok};
use crate::state::AppState;

#[derive(Debug, Serialize, ToSchema)]
pub struct AboutInfo {
    pub version: String,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/about", get(get_about))
}

#[utoipa::path(
    get,
    path = "/api/about",
    tag = "about",
    responses(
        (status = 200, description = "Server build info", body = AboutInfo),
    )
)]
pub async fn get_about() -> Result<Json<ApiResponse<AboutInfo>>, AppError> {
    ok(AboutInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}
