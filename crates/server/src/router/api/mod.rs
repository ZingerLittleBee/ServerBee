pub mod agent;
pub mod auth;
pub mod server;
pub mod server_group;
pub mod setting;

use std::sync::Arc;

use axum::{Router, middleware};

use crate::middleware::auth::auth_middleware;
use crate::state::AppState;

pub fn router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .merge(auth::public_router())
        .merge(agent::public_router())
        .merge(
            Router::new()
                .merge(auth::protected_router())
                .merge(server::router())
                .merge(server_group::router())
                .merge(setting::router())
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    auth_middleware,
                )),
        )
}
