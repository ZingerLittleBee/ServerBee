pub mod api;

use std::sync::Arc;

use axum::Router;

use crate::state::AppState;

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/healthz", axum::routing::get(|| async { "ok" }))
        .nest("/api", api::router(state.clone()))
        .with_state(state)
}
