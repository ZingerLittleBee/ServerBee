use utoipa::openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "session_cookie",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new("serverbee_session"))),
        );
        components.add_security_scheme(
            "api_key",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("X-API-Key"))),
        );
        components.add_security_scheme(
            "bearer_token",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .build(),
            ),
        );
    }
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "ServerBee API",
        version = "0.2.1",
        description = "ServerBee VPS monitoring probe API. All responses are wrapped in `{\"data\": <value>}`. Errors return `{\"error\": {\"code\": \"...\", \"message\": \"...\"}}`.",
    ),
    modifiers(&SecurityAddon),
    paths(
        // auth
        crate::router::api::auth::login,
        crate::router::api::auth::logout,
        crate::router::api::auth::me,
        crate::router::api::auth::create_api_key,
        crate::router::api::auth::list_api_keys,
        crate::router::api::auth::delete_api_key,
        crate::router::api::auth::change_password,
        // 2FA
        crate::router::api::auth::totp_setup,
        crate::router::api::auth::totp_enable,
        crate::router::api::auth::totp_disable,
        crate::router::api::auth::totp_status,
        // OAuth account management
        crate::router::api::auth::list_oauth_accounts,
        crate::router::api::auth::unlink_oauth_account,
        // OAuth flow
        crate::router::api::oauth::list_providers,
        crate::router::api::oauth::oauth_authorize,
        crate::router::api::oauth::oauth_callback,
        // status (public)
        crate::router::api::status::public_status,
        // agent
        crate::router::api::agent::register,
        crate::router::api::agent::latest_version,
        // servers
        crate::router::api::server::list_servers,
        crate::router::api::server::get_server,
        crate::router::api::server::update_server,
        crate::router::api::server::delete_server,
        crate::router::api::server::batch_delete,
        crate::router::api::server::get_records,
        crate::router::api::server::get_gpu_records,
        crate::router::api::server::trigger_upgrade,
        crate::router::api::server::batch_update_capabilities,
        crate::router::api::server::cleanup_orphaned_servers,
        // server-groups
        crate::router::api::server_group::list_groups,
        crate::router::api::server_group::create_group,
        crate::router::api::server_group::update_group,
        crate::router::api::server_group::delete_group,
        // brand
        crate::router::api::brand::get_brand_config,
        crate::router::api::brand::update_brand_config,
        crate::router::api::brand::upload_logo,
        crate::router::api::brand::upload_favicon,
        crate::router::api::brand::serve_logo,
        crate::router::api::brand::serve_favicon,
        // settings
        crate::router::api::setting::get_settings,
        crate::router::api::setting::update_settings,
        crate::router::api::setting::get_auto_discovery_key,
        crate::router::api::setting::regenerate_auto_discovery_key,
        crate::router::api::setting::create_backup,
        crate::router::api::setting::restore_backup,
        // notifications
        crate::router::api::notification::list_notifications,
        crate::router::api::notification::get_notification,
        crate::router::api::notification::create_notification,
        crate::router::api::notification::update_notification,
        crate::router::api::notification::delete_notification,
        crate::router::api::notification::test_notification,
        crate::router::api::notification::list_groups,
        crate::router::api::notification::get_group,
        crate::router::api::notification::create_group,
        crate::router::api::notification::update_group,
        crate::router::api::notification::delete_group,
        // alert-rules
        crate::router::api::alert::list_rules,
        crate::router::api::alert::get_rule,
        crate::router::api::alert::create_rule,
        crate::router::api::alert::update_rule,
        crate::router::api::alert::delete_rule,
        crate::router::api::alert::list_states,
        crate::router::api::alert::list_alert_events,
        crate::router::api::alert::get_alert_event_detail,
        // tasks
        crate::router::api::task::list_tasks,
        crate::router::api::task::create_task,
        crate::router::api::task::get_task,
        crate::router::api::task::update_task,
        crate::router::api::task::delete_task,
        crate::router::api::task::run_task,
        crate::router::api::task::get_task_results,
        // audit
        crate::router::api::audit::list_audit_logs,
        // users
        crate::router::api::user::list_users,
        crate::router::api::user::get_user,
        crate::router::api::user::create_user,
        crate::router::api::user::update_user,
        crate::router::api::user::delete_user,
        // dashboards
        crate::router::api::dashboard::list_dashboards,
        crate::router::api::dashboard::get_default_dashboard,
        crate::router::api::dashboard::get_dashboard,
        crate::router::api::dashboard::create_dashboard,
        crate::router::api::dashboard::update_dashboard,
        crate::router::api::dashboard::delete_dashboard,
        // service-monitors
        crate::router::api::service_monitor::list_monitors,
        crate::router::api::service_monitor::get_monitor,
        crate::router::api::service_monitor::create_monitor,
        crate::router::api::service_monitor::update_monitor,
        crate::router::api::service_monitor::delete_monitor,
        crate::router::api::service_monitor::get_records,
        crate::router::api::service_monitor::trigger_check,
        // status-pages
        crate::router::api::status_page::get_public_status_page,
        crate::router::api::status_page::list_status_pages,
        crate::router::api::status_page::create_status_page,
        crate::router::api::status_page::update_status_page,
        crate::router::api::status_page::delete_status_page,
        // incidents
        crate::router::api::incident::list_incidents,
        crate::router::api::incident::create_incident,
        crate::router::api::incident::update_incident,
        crate::router::api::incident::delete_incident,
        crate::router::api::incident::add_incident_update,
        // maintenances
        crate::router::api::maintenance_api::list_maintenances,
        crate::router::api::maintenance_api::create_maintenance,
        crate::router::api::maintenance_api::update_maintenance,
        crate::router::api::maintenance_api::delete_maintenance,
        // ping-tasks
        crate::router::api::ping::list_tasks,
        crate::router::api::ping::get_task,
        crate::router::api::ping::create_task,
        crate::router::api::ping::update_task,
        crate::router::api::ping::delete_task,
        crate::router::api::ping::get_records,
        // uptime
        crate::router::api::uptime::get_uptime_daily,
        // traceroute
        crate::router::api::traceroute::trigger_traceroute,
        crate::router::api::traceroute::get_traceroute_result,
        // traffic
        crate::router::api::traffic::get_traffic,
        crate::router::api::traffic::get_traffic_overview,
        crate::router::api::traffic::get_traffic_overview_daily,
        crate::router::api::traffic::get_traffic_cycle,
        // mobile-auth
        crate::router::api::mobile::mobile_login,
        crate::router::api::mobile::mobile_refresh,
        crate::router::api::mobile::mobile_logout,
        crate::router::api::mobile::list_devices,
        crate::router::api::mobile::revoke_device,
        crate::router::api::mobile::generate_pair_code,
        crate::router::api::mobile::mobile_pair_redeem,
        crate::router::api::mobile::push_register,
        crate::router::api::mobile::push_unregister,
        // files
        crate::router::api::file::list_files,
        crate::router::api::file::stat_file,
        crate::router::api::file::read_file,
        crate::router::api::file::download_file,
        crate::router::api::file::list_transfers,
        crate::router::api::file::write_file,
        crate::router::api::file::delete_file,
        crate::router::api::file::mkdir,
        crate::router::api::file::move_file,
        crate::router::api::file::start_download,
        crate::router::api::file::upload_file,
        crate::router::api::file::cancel_transfer,
    ),
    components(
        schemas(
            // error
            crate::error::ErrorBody,
            crate::error::ErrorDetail,
            // auth
            crate::router::api::auth::LoginRequest,
            crate::router::api::auth::LoginResponse,
            crate::router::api::auth::MeResponse,
            crate::router::api::auth::CreateApiKeyRequest,
            crate::router::api::auth::ApiKeyResponse,
            crate::router::api::auth::ChangePasswordRequest,
            // 2FA
            crate::router::api::auth::TotpSetupResponse,
            crate::router::api::auth::TotpVerifyRequest,
            crate::router::api::auth::TotpDisableRequest,
            crate::router::api::auth::TotpStatusResponse,
            // OAuth
            crate::entity::oauth_account::Model,
            crate::router::api::oauth::OAuthProvidersResponse,
            // agent
            crate::router::api::agent::RegisterResponse,
            crate::service::upgrade_release::LatestAgentVersionResponse,
            // servers
            crate::router::api::server::ServerResponse,
            crate::router::api::server::BatchDeleteRequest,
            crate::router::api::server::BatchDeleteResponse,
            crate::router::api::server::UpgradeRequest,
            crate::router::api::server::BatchCapabilitiesRequest,
            crate::router::api::server::BatchCapabilitiesResponse,
            crate::router::api::server::CleanupResponse,
            crate::service::server::UpdateServerInput,
            // server-groups
            crate::router::api::server_group::CreateGroupRequest,
            crate::router::api::server_group::UpdateGroupRequest,
            // brand
            crate::router::api::brand::BrandConfig,
            crate::router::api::brand::UploadResponse,
            // settings
            crate::router::api::setting::SystemSettings,
            crate::router::api::setting::AutoDiscoveryKeyResponse,
            // notifications
            crate::service::notification::CreateNotification,
            crate::service::notification::UpdateNotification,
            crate::service::notification::CreateNotificationGroup,
            crate::service::notification::UpdateNotificationGroup,
            // alert-rules
            crate::service::alert::AlertRuleItem,
            crate::service::alert::CreateAlertRule,
            crate::service::alert::UpdateAlertRule,
            crate::service::alert::AlertStateResponse,
            crate::service::alert::AlertEventResponse,
            crate::router::api::alert::AlertEventDetailResponse,
            // tasks
            crate::router::api::task::CreateTaskRequest,
            crate::router::api::task::UpdateTaskRequest,
            crate::router::api::task::TaskResponse,
            // dashboards
            crate::entity::dashboard::Model,
            crate::entity::dashboard_widget::Model,
            crate::service::dashboard::DashboardWithWidgets,
            crate::service::dashboard::CreateDashboardInput,
            crate::service::dashboard::UpdateDashboardInput,
            crate::service::dashboard::WidgetInput,
            // service-monitors
            crate::service::service_monitor::CreateServiceMonitor,
            crate::service::service_monitor::UpdateServiceMonitor,
            crate::entity::service_monitor::Model,
            crate::entity::service_monitor_record::Model,
            crate::router::api::service_monitor::MonitorWithRecord,
            // ping-tasks
            crate::service::ping::CreatePingTask,
            crate::service::ping::UpdatePingTask,
            // status
            crate::router::api::status::StatusPageResponse,
            crate::router::api::status::StatusServer,
            crate::router::api::status::StatusMetrics,
            crate::router::api::status::StatusGroup,
            // audit
            crate::router::api::audit::AuditLogEntry,
            crate::router::api::audit::AuditListResponse,
            // users
            crate::service::user::UserResponse,
            crate::service::user::CreateUserInput,
            crate::service::user::UpdateUserInput,
            // status-pages
            crate::router::api::status_page::StatusPageInfo,
            crate::router::api::status_page::ServerStatusInfo,
            crate::router::api::status_page::IncidentWithUpdates,
            crate::router::api::status_page::PublicStatusPageData,
            crate::service::status_page::CreateStatusPage,
            crate::service::status_page::UpdateStatusPage,
            crate::entity::status_page::Model,
            // incidents
            crate::service::incident::CreateIncident,
            crate::service::incident::UpdateIncident,
            crate::service::incident::CreateIncidentUpdate,
            crate::entity::incident::Model,
            crate::entity::incident_update::Model,
            // maintenances
            crate::service::maintenance::CreateMaintenance,
            crate::service::maintenance::UpdateMaintenance,
            crate::entity::maintenance::Model,
            // uptime
            crate::entity::uptime_daily::Model,
            crate::service::uptime::UptimeDailyEntry,
            // entity models used as responses
            crate::entity::server_group::Model,
            crate::entity::notification::Model,
            crate::entity::notification_group::Model,
            crate::entity::alert_rule::Model,
            crate::entity::ping_task::Model,
            crate::entity::ping_record::Model,
            crate::entity::record::Model,
            crate::entity::record_hourly::Model,
            crate::entity::gpu_record::Model,
            crate::entity::task_result::Model,
            // traffic
            crate::router::api::traffic::TrafficResponse,
            crate::router::api::traffic::CycleResponse,
            crate::service::traffic::TrafficPrediction,
            crate::service::traffic::DailyTraffic,
            crate::service::traffic::HourlyTraffic,
            crate::service::traffic::ServerTrafficOverview,
            crate::service::traffic::CycleTraffic,
            // mobile-auth
            crate::router::api::mobile::MobileLoginRequest,
            crate::router::api::mobile::MobileRefreshRequest,
            crate::router::api::mobile::MobilePairRedeemRequest,
            crate::router::api::mobile::MobilePairCodeResponse,
            crate::router::api::mobile::MobileDeviceResponse,
            crate::router::api::mobile::PushRegisterRequest,
            crate::service::mobile_auth::MobileTokenResponse,
            crate::service::mobile_auth::MobileUserResponse,
            // files
            serverbee_common::types::FileEntry,
            serverbee_common::types::FileType,
            crate::router::api::file::ListFilesRequest,
            crate::router::api::file::ListFilesResponse,
            crate::router::api::file::StatRequest,
            crate::router::api::file::StatResponse,
            crate::router::api::file::ReadRequest,
            crate::router::api::file::ReadResponse,
            crate::router::api::file::WriteRequest,
            crate::router::api::file::DeleteRequest,
            crate::router::api::file::MkdirRequest,
            crate::router::api::file::MoveRequest,
            crate::router::api::file::DownloadRequest,
            crate::router::api::file::DownloadResponse,
            crate::router::api::file::SuccessResponse,
            crate::router::api::file::TransfersResponse,
            crate::service::file_transfer::TransferInfo,
            // traceroute
            crate::router::api::traceroute::TriggerTracerouteRequest,
            crate::router::api::traceroute::TriggerTracerouteResponse,
            crate::router::api::traceroute::TracerouteResultResponse,
            serverbee_common::types::TracerouteHop,
        ),
    ),
    tags(
        (name = "auth", description = "Authentication & API keys"),
        (name = "mobile-auth", description = "Mobile authentication & device management"),
        (name = "2fa", description = "Two-factor authentication (TOTP)"),
        (name = "oauth", description = "OAuth login & account linking"),
        (name = "agent", description = "Agent registration"),
        (name = "servers", description = "Server management"),
        (name = "server-groups", description = "Server group management"),
        (name = "brand", description = "Custom branding (logo, favicon, site title)"),
        (name = "settings", description = "System settings"),
        (name = "notifications", description = "Notification channels"),
        (name = "notification-groups", description = "Notification groups"),
        (name = "alert-rules", description = "Alert rules"),
        (name = "status", description = "Public server status page"),
        (name = "status-pages", description = "Status page management & public view"),
        (name = "incidents", description = "Incident management"),
        (name = "maintenances", description = "Maintenance window management"),
        (name = "audit", description = "Audit logs (admin only)"),
        (name = "users", description = "User management (admin only)"),
        (name = "tasks", description = "Remote command execution"),
        (name = "dashboards", description = "Custom dashboard management"),
        (name = "service-monitors", description = "Server-side service monitoring (SSL/DNS/HTTP/TCP/WHOIS)"),
        (name = "ping-tasks", description = "Ping probe tasks"),
        (name = "traceroute", description = "Traceroute diagnostics"),
        (name = "traffic", description = "Traffic statistics & billing cycle overview"),
        (name = "uptime", description = "Uptime statistics"),
        (name = "files", description = "File management"),
    ),
    security(
        ("session_cookie" = []),
        ("api_key" = []),
        ("bearer_token" = []),
    ),
)]
pub struct ApiDoc;
