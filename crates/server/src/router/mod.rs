pub mod api;
mod static_files;
pub mod ws;

use std::sync::Arc;

use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::openapi::ApiDoc;
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
        // Terminal WS: /api/ws/terminal/:server_id (auth checked inside handler)
        .nest("/api", ws::terminal::router())
        // Swagger UI
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        // Embedded frontend: serve static files, SPA fallback to index.html
        .fallback(static_files::static_handler)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}
