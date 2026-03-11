pub mod api;
pub mod ws;

// TODO: Add utoipa OpenAPI documentation (P1)

use std::sync::Arc;

use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::state::AppState;

pub fn create_router(state: Arc<AppState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/healthz", axum::routing::get(|| async { "ok" }))
        .nest("/api", api::router(state.clone()))
        // Agent WS: /api/agent/ws?token=<token> (no auth middleware, uses token param)
        .nest("/api", ws::agent::router())
        // Browser WS: /api/ws/servers (auth checked inside handler)
        .nest("/api", ws::browser::router())
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}
