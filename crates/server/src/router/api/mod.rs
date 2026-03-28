pub mod agent;
pub mod alert;
pub mod audit;
pub mod auth;
pub mod brand;
pub mod dashboard;
pub mod docker;
pub mod file;
pub mod geoip;
pub mod incident;
pub mod maintenance_api;
pub mod mobile_alert;
pub mod mobile_auth;
pub mod mobile_device;
pub mod network_probe;
pub mod notification;
pub mod oauth;
pub mod ping;
pub mod service_monitor;
pub mod server;
pub mod server_group;
pub mod setting;
pub mod status;
pub mod status_page;
pub mod task;
pub mod traceroute;
pub mod traffic;
pub mod uptime;
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
        .merge(status_page::public_router())
        .merge(brand::public_router())
        // Mobile auth endpoints (public — login/refresh/logout are entry points)
        .merge(mobile_auth::router())
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
                .merge(traceroute::read_router())
                .merge(service_monitor::read_router())
                .merge(status_page::read_router())
                .merge(uptime::read_router())
                .merge(dashboard::read_router())
                .merge(alert::alert_events_router())
                .merge(geoip::read_router())
                // Mobile device + alert endpoints (authenticated)
                .merge(mobile_device::router())
                .merge(mobile_alert::router())
                // Admin-only routes (write operations + management)
                .merge(
                    Router::new()
                        .merge(server::write_router())
                        .merge(server_group::write_router())
                        .merge(ping::write_router())
                        .merge(network_probe::write_router())
                        .merge(file::write_router(state.config.file.max_upload_size.try_into().unwrap_or(usize::MAX)))
                        .merge(docker::write_router())
                        .merge(service_monitor::write_router())
                        .merge(traceroute::write_router())
                        .merge(dashboard::write_router())
                        .merge(setting::router())
                        .merge(brand::write_router())
                        .merge(notification::router())
                        .merge(alert::router())
                        .merge(task::router())
                        .merge(audit::router())
                        .merge(user::router())
                        .merge(incident::router())
                        .merge(geoip::write_router())
                        .merge(maintenance_api::router())
                        .merge(status_page::write_router())
                        .layer(middleware::from_fn(require_admin)),
                )
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    auth_middleware,
                )),
        )
}
