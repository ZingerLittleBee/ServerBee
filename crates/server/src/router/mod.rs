pub mod api;
mod static_files;
mod system;
pub mod utils;
pub mod ws;

use std::sync::Arc;

use axum::Router;
use axum::http::HeaderValue;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::openapi::ApiDoc;
use crate::state::AppState;

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/healthz", axum::routing::get(|| async { "ok" }))
        .nest("/api", api::router(state.clone()))
        // Agent WS: /api/agent/ws?token=<token> (no auth middleware, uses token param)
        .nest("/api", ws::agent::router())
        // Browser WS: /api/ws/servers (auth checked inside handler)
        .nest("/api", ws::browser::router())
        // Terminal WS: /api/ws/terminal/:server_id (auth checked inside handler)
        .nest("/api", ws::terminal::router())
        // Docker logs WS: /api/ws/docker/logs/:server_id (auth checked inside handler)
        .nest("/api", ws::docker_logs::router())
        // Swagger UI
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        // System reserved routes must be mounted before the catch-all fallback so
        // they are never shadowed by custom theme packages (spec § 6.4).
        .nest("/__system", system::router())
        // Embedded frontend: theme-aware serve with cookie precedence (spec § 6.5).
        .fallback(static_files::theme_handler)
        // Security headers
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::REFERRER_POLICY,
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::HeaderName::from_static("x-permitted-cross-domain-policies"),
            HeaderValue::from_static("none"),
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
