pub mod agent;
pub mod alert;
pub mod audit;
pub mod auth;
pub mod dashboard;
pub mod docker;
pub mod file;
pub mod network_probe;
pub mod notification;
pub mod oauth;
pub mod ping;
pub mod service_monitor;
pub mod server;
pub mod server_group;
pub mod setting;
pub mod status;
pub mod task;
pub mod traffic;
pub mod user;

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
                // Read-only routes accessible to all authenticated users
                .merge(server::read_router())
                .merge(server_group::read_router())
                .merge(ping::read_router())
                .merge(network_probe::read_router())
                .merge(file::read_router())
                .merge(docker::read_router())
                .merge(traffic::read_router())
                .merge(service_monitor::read_router())
                .merge(dashboard::read_router())
                .merge(alert::alert_events_router())
                // Admin-only routes (write operations + management)
                .merge(
                    Router::new()
                        .merge(server::write_router())
                        .merge(server_group::write_router())
                        .merge(ping::write_router())
                        .merge(network_probe::write_router())
                        .merge(file::write_router())
                        .merge(docker::write_router())
                        .merge(service_monitor::write_router())
                        .merge(dashboard::write_router())
                        .merge(setting::router())
                        .merge(notification::router())
                        .merge(alert::router())
                        .merge(task::router())
                        .merge(audit::router())
                        .merge(user::router())
                        .layer(middleware::from_fn(require_admin)),
                )
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    auth_middleware,
                )),
        )
}
