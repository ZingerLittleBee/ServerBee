pub mod agent;
pub mod alert;
pub mod audit;
pub mod auth;
pub mod notification;
pub mod oauth;
pub mod ping;
pub mod server;
pub mod server_group;
pub mod setting;
pub mod status;
pub mod task;

use std::sync::Arc;

use axum::{Router, middleware};

use crate::middleware::auth::{auth_middleware, require_admin};
use crate::state::AppState;

pub fn router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .merge(auth::public_router())
        .merge(agent::public_router())
        .merge(oauth::router())
        .merge(status::router())
        .merge(
            Router::new()
                .merge(auth::protected_router())
                .merge(server::router())
                .merge(server_group::router())
                .merge(ping::router())
                // Admin-only routes
                .merge(
                    Router::new()
                        .merge(setting::router())
                        .merge(notification::router())
                        .merge(alert::router())
                        .merge(task::router())
                        .merge(audit::router())
                        .layer(middleware::from_fn(require_admin)),
                )
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    auth_middleware,
                )),
        )
}
