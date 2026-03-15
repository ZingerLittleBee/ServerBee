use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "ServerBee API",
        version = "0.2.1",
        description = "ServerBee VPS monitoring probe API. All responses are wrapped in `{\"data\": <value>}`. Errors return `{\"error\": {\"code\": \"...\", \"message\": \"...\"}}`.",
    ),
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
        // server-groups
        crate::router::api::server_group::list_groups,
        crate::router::api::server_group::create_group,
        crate::router::api::server_group::update_group,
        crate::router::api::server_group::delete_group,
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
        // tasks
        crate::router::api::task::create_task,
        crate::router::api::task::get_task,
        crate::router::api::task::get_task_results,
        // audit
        crate::router::api::audit::list_audit_logs,
        // users
        crate::router::api::user::list_users,
        crate::router::api::user::get_user,
        crate::router::api::user::create_user,
        crate::router::api::user::update_user,
        crate::router::api::user::delete_user,
        // ping-tasks
        crate::router::api::ping::list_tasks,
        crate::router::api::ping::get_task,
        crate::router::api::ping::create_task,
        crate::router::api::ping::update_task,
        crate::router::api::ping::delete_task,
        crate::router::api::ping::get_records,
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
            // servers
            crate::router::api::server::ServerResponse,
            crate::router::api::server::BatchDeleteRequest,
            crate::router::api::server::BatchDeleteResponse,
            crate::router::api::server::UpgradeRequest,
            crate::router::api::server::BatchCapabilitiesRequest,
            crate::router::api::server::BatchCapabilitiesResponse,
            crate::service::server::UpdateServerInput,
            // server-groups
            crate::router::api::server_group::CreateGroupRequest,
            crate::router::api::server_group::UpdateGroupRequest,
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
            // tasks
            crate::router::api::task::CreateTaskRequest,
            crate::router::api::task::TaskResponse,
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
            // entity models used as responses
            crate::entity::server_group::Model,
            crate::entity::notification::Model,
            crate::entity::notification_group::Model,
            crate::entity::alert_rule::Model,
            crate::entity::ping_task::Model,
            crate::entity::ping_record::Model,
            crate::entity::gpu_record::Model,
            crate::entity::task_result::Model,
        ),
    ),
    tags(
        (name = "auth", description = "Authentication & API keys"),
        (name = "2fa", description = "Two-factor authentication (TOTP)"),
        (name = "oauth", description = "OAuth login & account linking"),
        (name = "agent", description = "Agent registration"),
        (name = "servers", description = "Server management"),
        (name = "server-groups", description = "Server group management"),
        (name = "settings", description = "System settings"),
        (name = "notifications", description = "Notification channels"),
        (name = "notification-groups", description = "Notification groups"),
        (name = "alert-rules", description = "Alert rules"),
        (name = "status", description = "Public server status page"),
        (name = "audit", description = "Audit logs (admin only)"),
        (name = "users", description = "User management (admin only)"),
        (name = "tasks", description = "Remote command execution"),
        (name = "ping-tasks", description = "Ping probe tasks"),
    ),
    security(
        ("session_cookie" = []),
        ("api_key" = []),
        ("bearer_token" = []),
    ),
)]
pub struct ApiDoc;
