//! Global Tailnet / Headscale Control Plane service, runtime configuration
//! persistence (`configs` key `"tailscale_config"`), SSRF-validated upstream
//! device fetching with short-lived caching, device-to-server auto-linking, and
//! local `tailscaled` Unix socket discovery.

use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Deserializer, Serialize};
use tokio::sync::RwLock;
use url::Url;

use crate::config::TailscaleConfig;
use crate::entity::server;
use crate::error::AppError;
use crate::service::auth::{AuthInvalidationEvent, notify_auth_invalidated};
use crate::service::config::ConfigService;
use crate::service::server::ServerService;
use serverbee_common::ssrf;
use serverbee_common::tailscale_types::is_cgnat_ipv4;

pub const CONFIG_KEY_TAILSCALE: &str = "tailscale_config";
const DEVICE_CACHE_TTL: Duration = Duration::from_secs(30);
const LOCAL_HINT_CACHE_TTL: Duration = Duration::from_secs(30);
const EXPIRING_SOON_DAYS: i64 = 14;

// ---------------------------------------------------------------------------
// Stored & Effective Configuration
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct StoredTailscaleConfig {
    pub enabled: Option<bool>,
    pub provider: Option<String>,
    pub api_base_url: Option<String>,
    pub tailnet: Option<String>,
    pub api_key: Option<String>,
    pub preferred_server_url: Option<String>,
    pub auth_enabled: Option<bool>,
    pub auto_provision_role: Option<String>,
}

impl std::fmt::Debug for StoredTailscaleConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredTailscaleConfig")
            .field("enabled", &self.enabled)
            .field("provider", &self.provider)
            .field("api_base_url", &self.api_base_url)
            .field("tailnet", &self.tailnet)
            .field(
                "api_key",
                &self
                    .api_key
                    .as_ref()
                    .map(|k| if k.is_empty() { "" } else { "<redacted>" }),
            )
            .field("preferred_server_url", &self.preferred_server_url)
            .field("auth_enabled", &self.auth_enabled)
            .field("auto_provision_role", &self.auto_provision_role)
            .finish()
    }
}

#[derive(Clone)]
pub struct EffectiveTailscaleConfig {
    pub enabled: bool,
    pub provider: String,
    pub api_base_url: String,
    pub tailnet: String,
    pub api_key: String,
    pub preferred_server_url: String,
    pub socket_path: String,
    pub auth_enabled: bool,
    pub auto_provision_role: String,
}

impl std::fmt::Debug for EffectiveTailscaleConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EffectiveTailscaleConfig")
            .field("enabled", &self.enabled)
            .field("provider", &self.provider)
            .field("api_base_url", &self.api_base_url)
            .field("tailnet", &self.tailnet)
            .field(
                "api_key",
                &if self.api_key.is_empty() {
                    ""
                } else {
                    "<redacted>"
                },
            )
            .field("preferred_server_url", &self.preferred_server_url)
            .field("socket_path", &self.socket_path)
            .field("auth_enabled", &self.auth_enabled)
            .field("auto_provision_role", &self.auto_provision_role)
            .finish()
    }
}

impl EffectiveTailscaleConfig {
    pub fn from_layers(base: &TailscaleConfig, stored: Option<&StoredTailscaleConfig>) -> Self {
        let Some(stored) = stored else {
            return Self {
                enabled: base.enabled,
                provider: normalize_provider_or_default(&base.provider),
                api_base_url: base.api_base_url.trim().to_string(),
                tailnet: normalize_tailnet_or_default(&base.tailnet),
                api_key: base.api_key.clone(),
                preferred_server_url: base.preferred_server_url.trim().to_string(),
                socket_path: base.socket_path.trim().to_string(),
                auth_enabled: base.auth_enabled,
                auto_provision_role: base.auto_provision_role.trim().to_string(),
            };
        };

        Self {
            enabled: stored.enabled.unwrap_or(base.enabled),
            provider: normalize_provider_or_default(
                stored.provider.as_deref().unwrap_or(&base.provider),
            ),
            api_base_url: stored
                .api_base_url
                .as_deref()
                .unwrap_or(&base.api_base_url)
                .trim()
                .to_string(),
            tailnet: normalize_tailnet_or_default(
                stored.tailnet.as_deref().unwrap_or(&base.tailnet),
            ),
            api_key: stored
                .api_key
                .clone()
                .unwrap_or_else(|| base.api_key.clone()),
            preferred_server_url: stored
                .preferred_server_url
                .as_deref()
                .unwrap_or(&base.preferred_server_url)
                .trim()
                .to_string(),
            socket_path: base.socket_path.trim().to_string(),
            auth_enabled: stored.auth_enabled.unwrap_or(base.auth_enabled),
            auto_provision_role: stored
                .auto_provision_role
                .as_deref()
                .unwrap_or(&base.auto_provision_role)
                .trim()
                .to_string(),
        }
    }

    pub fn is_control_configured(&self) -> bool {
        self.enabled && !self.api_key.trim().is_empty() && !self.api_base_url.trim().is_empty()
    }

    pub fn to_admin_response(&self) -> TailscaleAdminConfigResponse {
        TailscaleAdminConfigResponse {
            enabled: self.enabled,
            provider: self.provider.clone(),
            api_base_url: self.api_base_url.clone(),
            tailnet: self.tailnet.clone(),
            has_api_key: !self.api_key.trim().is_empty(),
            preferred_server_url: self.preferred_server_url.clone(),
            auth_enabled: self.auth_enabled,
            auto_provision_role: self.auto_provision_role.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Public API DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct TailnetDeviceItem {
    pub id: String,
    pub name: String,
    pub hostname: String,
    pub os: Option<String>,
    pub addresses: Vec<String>,
    pub authorized: bool,
    pub online: bool,
    pub last_seen: Option<String>,
    pub key_expiry_disabled: bool,
    pub expires: Option<String>,
    pub client_version: Option<String>,
    pub tags: Vec<String>,
    pub matched_server_id: Option<String>,
    pub matched_server_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TailnetOverviewResponse {
    pub configured: bool,
    pub provider: String,
    pub tailnet: String,
    pub server_tailnet_url: Option<String>,
    pub total_devices: usize,
    pub online_devices: usize,
    pub linked_servers: usize,
    pub expiring_keys_count: usize,
    pub fetched_at: Option<String>,
    pub error: Option<String>,
    pub devices: Vec<TailnetDeviceItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TailscaleAdminConfigResponse {
    pub enabled: bool,
    pub provider: String,
    pub api_base_url: String,
    pub tailnet: String,
    pub has_api_key: bool,
    pub preferred_server_url: String,
    pub auth_enabled: bool,
    pub auto_provision_role: String,
}

fn default_tailnet_field() -> String {
    "-".to_string()
}

#[derive(Clone, Deserialize, utoipa::ToSchema)]
pub struct UpdateTailscaleConfigRequest {
    pub enabled: bool,
    pub provider: String,
    pub api_base_url: String,
    #[serde(default = "default_tailnet_field")]
    pub tailnet: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub preferred_server_url: Option<String>,
    #[serde(default)]
    pub auth_enabled: Option<bool>,
    #[serde(default)]
    pub auto_provision_role: Option<String>,
}

impl std::fmt::Debug for UpdateTailscaleConfigRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpdateTailscaleConfigRequest")
            .field("enabled", &self.enabled)
            .field("provider", &self.provider)
            .field("api_base_url", &self.api_base_url)
            .field("tailnet", &self.tailnet)
            .field(
                "api_key",
                &self
                    .api_key
                    .as_ref()
                    .map(|k| if k.is_empty() { "" } else { "<redacted>" }),
            )
            .field("preferred_server_url", &self.preferred_server_url)
            .field("auth_enabled", &self.auth_enabled)
            .field("auto_provision_role", &self.auto_provision_role)
            .finish()
    }
}

#[derive(Clone, Deserialize, Default, utoipa::ToSchema)]
pub struct TestTailscaleConfigRequest {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub api_base_url: Option<String>,
    #[serde(default)]
    pub tailnet: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
}

impl std::fmt::Debug for TestTailscaleConfigRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestTailscaleConfigRequest")
            .field("provider", &self.provider)
            .field("api_base_url", &self.api_base_url)
            .field("tailnet", &self.tailnet)
            .field(
                "api_key",
                &self
                    .api_key
                    .as_ref()
                    .map(|k| if k.is_empty() { "" } else { "<redacted>" }),
            )
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TailscaleConnectionTestResponse {
    pub success: bool,
    pub device_count: usize,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Upstream Transport Abstraction (SSRF-enforced in production)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct UpstreamQuery {
    pub provider: String,
    pub base_url: Url,
    pub tailnet: String,
    pub api_key: String,
}

#[async_trait]
pub trait TailscaleUpstreamTransport: Send + Sync {
    async fn fetch_devices(&self, query: &UpstreamQuery) -> Result<Vec<TailnetDeviceItem>, String>;
}

pub struct ReqwestTailscaleTransport;

#[async_trait]
impl TailscaleUpstreamTransport for ReqwestTailscaleTransport {
    async fn fetch_devices(&self, query: &UpstreamQuery) -> Result<Vec<TailnetDeviceItem>, String> {
        let host = query
            .base_url
            .host_str()
            .ok_or_else(|| "API base URL has no host".to_string())?;
        let clean_host = host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_string();
        let port = query.base_url.port_or_known_default().unwrap_or(443);

        let validated_addrs = ssrf::resolve_and_check_monitor_async(&clean_host, port)
            .await
            .map_err(|e| e.to_string())?;

        let mut builder = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .danger_accept_invalid_certs(false)
            .redirect(reqwest::redirect::Policy::none());

        if clean_host.parse::<IpAddr>().is_err() {
            builder = builder.resolve_to_addrs(&clean_host, &validated_addrs);
        }

        let client = builder
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

        let base = query.base_url.as_str().trim_end_matches('/');
        match query.provider.as_str() {
            "tailscale" => {
                let endpoint = format!("{base}/api/v2/tailnet/{}/devices", query.tailnet);
                let resp = client
                    .get(&endpoint)
                    .bearer_auth(&query.api_key)
                    .header(reqwest::header::ACCEPT, "application/json")
                    .send()
                    .await
                    .map_err(|e| format!("Upstream Tailscale request failed: {e}"))?;

                let status = resp.status();
                let body = resp
                    .text()
                    .await
                    .map_err(|e| format!("Failed to read Tailscale response: {e}"))?;

                if !status.is_success() {
                    return Err(format!("Tailscale API returned HTTP {status}"));
                }
                parse_tailscale_devices_json(&body)
            }
            "headscale" => {
                let node_endpoint = format!("{base}/api/v1/node");
                let resp = client
                    .get(&node_endpoint)
                    .bearer_auth(&query.api_key)
                    .header(reqwest::header::ACCEPT, "application/json")
                    .send()
                    .await
                    .map_err(|e| format!("Upstream Headscale request failed: {e}"))?;

                if resp.status() == reqwest::StatusCode::NOT_FOUND {
                    let machine_endpoint = format!("{base}/api/v1/machine");
                    let fallback = client
                        .get(&machine_endpoint)
                        .bearer_auth(&query.api_key)
                        .header(reqwest::header::ACCEPT, "application/json")
                        .send()
                        .await
                        .map_err(|e| format!("Upstream Headscale request failed: {e}"))?;

                    let status = fallback.status();
                    let body = fallback
                        .text()
                        .await
                        .map_err(|e| format!("Failed to read Headscale response: {e}"))?;
                    if !status.is_success() {
                        return Err(format!("Headscale API returned HTTP {status}"));
                    }
                    return parse_headscale_devices_json(&body);
                }

                let status = resp.status();
                let body = resp
                    .text()
                    .await
                    .map_err(|e| format!("Failed to read Headscale response: {e}"))?;
                if !status.is_success() {
                    return Err(format!("Headscale API returned HTTP {status}"));
                }
                parse_headscale_devices_json(&body)
            }
            other => Err(format!("Unsupported provider '{other}'")),
        }
    }
}

// ---------------------------------------------------------------------------
// Service Implementation
// ---------------------------------------------------------------------------

struct CachedDeviceList {
    cache_key: String,
    fetched_at: DateTime<Utc>,
    cached_instant: Instant,
    devices: Vec<TailnetDeviceItem>,
}

pub struct TailscaleControlService {
    base_config: TailscaleConfig,
    listen_addr: String,
    transport: Arc<dyn TailscaleUpstreamTransport>,
    devices_cache: RwLock<Option<CachedDeviceList>>,
    local_hint_cache: RwLock<Option<(Instant, Option<String>)>>,
}

impl TailscaleControlService {
    pub fn new(base_config: TailscaleConfig, listen_addr: String) -> Self {
        Self::with_transport(
            base_config,
            listen_addr,
            Arc::new(ReqwestTailscaleTransport),
        )
    }

    pub fn with_transport(
        base_config: TailscaleConfig,
        listen_addr: String,
        transport: Arc<dyn TailscaleUpstreamTransport>,
    ) -> Self {
        Self {
            base_config,
            listen_addr,
            transport,
            devices_cache: RwLock::new(None),
            local_hint_cache: RwLock::new(None),
        }
    }

    pub async fn effective_config(
        &self,
        db: &DatabaseConnection,
    ) -> Result<EffectiveTailscaleConfig, AppError> {
        let stored: Option<StoredTailscaleConfig> =
            ConfigService::get_typed(db, CONFIG_KEY_TAILSCALE).await?;
        Ok(EffectiveTailscaleConfig::from_layers(
            &self.base_config,
            stored.as_ref(),
        ))
    }

    pub async fn get_admin_config(
        &self,
        db: &DatabaseConnection,
    ) -> Result<TailscaleAdminConfigResponse, AppError> {
        Ok(self.effective_config(db).await?.to_admin_response())
    }

    /// Update persisted runtime Tailscale settings. Returns `(response, api_key_modified)`.
    pub async fn update_config(
        &self,
        db: &DatabaseConnection,
        req: UpdateTailscaleConfigRequest,
    ) -> Result<(TailscaleAdminConfigResponse, bool), AppError> {
        let existing: StoredTailscaleConfig = ConfigService::get_typed(db, CONFIG_KEY_TAILSCALE)
            .await?
            .unwrap_or_default();

        let provider = validate_provider(&req.provider)?;
        let tailnet = validate_tailnet(&req.tailnet)?;

        let raw_base = req.api_base_url.trim();
        let api_base_url = if raw_base.is_empty() {
            if provider == "tailscale" {
                "https://api.tailscale.com".to_string()
            } else if req.enabled {
                return Err(AppError::Validation(
                    "api_base_url is required when Headscale is enabled".to_string(),
                ));
            } else {
                String::new()
            }
        } else {
            let parsed = validate_api_base_url(raw_base)?;
            parsed.as_str().trim_end_matches('/').to_string()
        };

        let (api_key_slot, api_key_modified) = match req.api_key {
            None => (existing.api_key, false),
            Some(ref k) if k.is_empty() => (Some(String::new()), true),
            Some(k) => (Some(k.trim().to_string()), true),
        };

        let preferred_server_url = match req.preferred_server_url {
            Some(url) => Some(validate_preferred_server_url(&url)?),
            None => existing.preferred_server_url,
        };

        let auth_enabled = match req.auth_enabled {
            Some(flag) => Some(flag),
            None => existing.auth_enabled,
        };

        let auto_provision_role = match req.auto_provision_role {
            Some(role) => Some(validate_auto_provision_role(&role)?),
            None => existing.auto_provision_role,
        };

        let updated_stored = StoredTailscaleConfig {
            enabled: Some(req.enabled),
            provider: Some(provider),
            api_base_url: Some(api_base_url),
            tailnet: Some(tailnet),
            api_key: api_key_slot,
            preferred_server_url,
            auth_enabled,
            auto_provision_role,
        };

        ConfigService::set_typed(db, CONFIG_KEY_TAILSCALE, &updated_stored).await?;
        self.invalidate_cache().await;
        notify_auth_invalidated(AuthInvalidationEvent::All);

        let effective =
            EffectiveTailscaleConfig::from_layers(&self.base_config, Some(&updated_stored));
        Ok((effective.to_admin_response(), api_key_modified))
    }

    pub async fn invalidate_cache(&self) {
        *self.devices_cache.write().await = None;
        *self.local_hint_cache.write().await = None;
    }

    pub async fn test_connection(
        &self,
        db: &DatabaseConnection,
        req: TestTailscaleConfigRequest,
    ) -> Result<TailscaleConnectionTestResponse, AppError> {
        let effective = self.effective_config(db).await?;

        let provider_raw = req
            .provider
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&effective.provider);
        let provider = validate_provider(provider_raw)?;

        let base_raw = req
            .api_base_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&effective.api_base_url);
        if base_raw.is_empty() {
            return Err(AppError::Validation(
                "api_base_url is required to test connection".to_string(),
            ));
        }
        let base_url = validate_api_base_url(base_raw)?;

        let tailnet_raw = req
            .tailnet
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&effective.tailnet);
        let tailnet = validate_tailnet(tailnet_raw)?;

        let api_key = req
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(effective.api_key.trim())
            .to_string();
        if api_key.is_empty() {
            return Err(AppError::Validation(
                "api_key is required to test connection".to_string(),
            ));
        }

        let query = UpstreamQuery {
            provider,
            base_url,
            tailnet,
            api_key,
        };

        match self.transport.fetch_devices(&query).await {
            Ok(devices) => {
                let count = devices.len();
                Ok(TailscaleConnectionTestResponse {
                    success: true,
                    device_count: count,
                    message: format!("Connected successfully ({count} devices found)"),
                })
            }
            Err(error) => {
                if error.contains("SSRF guard") {
                    return Err(AppError::Validation(error));
                }
                Ok(TailscaleConnectionTestResponse {
                    success: false,
                    device_count: 0,
                    message: error,
                })
            }
        }
    }

    pub async fn get_overview(
        &self,
        db: &DatabaseConnection,
    ) -> Result<TailnetOverviewResponse, AppError> {
        let effective = self.effective_config(db).await?;
        let server_tailnet_url = self.resolve_server_tailnet_url(&effective).await;

        if !effective.is_control_configured() {
            return Ok(TailnetOverviewResponse {
                configured: false,
                provider: effective.provider,
                tailnet: effective.tailnet,
                server_tailnet_url,
                total_devices: 0,
                online_devices: 0,
                linked_servers: 0,
                expiring_keys_count: 0,
                fetched_at: None,
                error: None,
                devices: Vec::new(),
            });
        }

        let base_url = match validate_api_base_url(&effective.api_base_url) {
            Ok(url) => url,
            Err(err) => {
                return Ok(TailnetOverviewResponse {
                    configured: true,
                    provider: effective.provider,
                    tailnet: effective.tailnet,
                    server_tailnet_url,
                    total_devices: 0,
                    online_devices: 0,
                    linked_servers: 0,
                    expiring_keys_count: 0,
                    fetched_at: None,
                    error: Some(err.to_string()),
                    devices: Vec::new(),
                });
            }
        };

        let tailnet = match validate_tailnet(&effective.tailnet) {
            Ok(t) => t,
            Err(err) => {
                return Ok(TailnetOverviewResponse {
                    configured: true,
                    provider: effective.provider,
                    tailnet: effective.tailnet,
                    server_tailnet_url,
                    total_devices: 0,
                    online_devices: 0,
                    linked_servers: 0,
                    expiring_keys_count: 0,
                    fetched_at: None,
                    error: Some(err.to_string()),
                    devices: Vec::new(),
                });
            }
        };

        let cache_key = format!("{}|{}|{}", effective.provider, base_url.as_str(), tailnet);
        let cached = {
            let guard = self.devices_cache.read().await;
            guard.as_ref().and_then(|entry| {
                (entry.cache_key == cache_key && entry.cached_instant.elapsed() < DEVICE_CACHE_TTL)
                    .then(|| (entry.devices.clone(), entry.fetched_at))
            })
        };

        let (raw_devices, fetched_at) = match cached {
            Some(hit) => hit,
            None => {
                let query = UpstreamQuery {
                    provider: effective.provider.clone(),
                    base_url,
                    tailnet: tailnet.clone(),
                    api_key: effective.api_key.trim().to_string(),
                };
                match self.transport.fetch_devices(&query).await {
                    Ok(devices) => {
                        let now = Utc::now();
                        *self.devices_cache.write().await = Some(CachedDeviceList {
                            cache_key,
                            fetched_at: now,
                            cached_instant: Instant::now(),
                            devices: devices.clone(),
                        });
                        (devices, now)
                    }
                    Err(err) => {
                        return Ok(TailnetOverviewResponse {
                            configured: true,
                            provider: effective.provider,
                            tailnet,
                            server_tailnet_url,
                            total_devices: 0,
                            online_devices: 0,
                            linked_servers: 0,
                            expiring_keys_count: 0,
                            fetched_at: None,
                            error: Some(err),
                            devices: Vec::new(),
                        });
                    }
                }
            }
        };

        let servers = ServerService::list_servers(db).await?;
        let devices = link_devices_to_servers(raw_devices, &servers);

        let now = Utc::now();
        let total_devices = devices.len();
        let online_devices = devices.iter().filter(|d| d.online).count();
        let linked_servers = devices
            .iter()
            .filter_map(|d| d.matched_server_id.as_deref())
            .collect::<HashSet<_>>()
            .len();
        let expiring_keys_count = devices
            .iter()
            .filter(|d| is_key_expiring_soon(d, now))
            .count();

        Ok(TailnetOverviewResponse {
            configured: true,
            provider: effective.provider,
            tailnet,
            server_tailnet_url,
            total_devices,
            online_devices,
            linked_servers,
            expiring_keys_count,
            fetched_at: Some(fetched_at.to_rfc3339()),
            error: None,
            devices,
        })
    }

    pub async fn resolve_server_tailnet_url(
        &self,
        effective: &EffectiveTailscaleConfig,
    ) -> Option<String> {
        let preferred = effective.preferred_server_url.trim();
        if !preferred.is_empty() {
            return Some(preferred.trim_end_matches('/').to_string());
        }

        {
            let guard = self.local_hint_cache.read().await;
            if let Some((instant, ref hint)) = *guard
                && instant.elapsed() < LOCAL_HINT_CACHE_TTL
            {
                return hint.clone();
            }
        }

        let hint = probe_local_tailscaled_url_hint(&effective.socket_path, &self.listen_addr).await;
        *self.local_hint_cache.write().await = Some((Instant::now(), hint.clone()));
        hint
    }
}

// ---------------------------------------------------------------------------
// Validation Helpers
// ---------------------------------------------------------------------------

fn normalize_provider_or_default(raw: &str) -> String {
    let p = raw.trim().to_ascii_lowercase();
    if p == "headscale" {
        "headscale".to_string()
    } else {
        "tailscale".to_string()
    }
}

fn normalize_tailnet_or_default(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        "-".to_string()
    } else {
        t.to_string()
    }
}

pub fn validate_provider(raw: &str) -> Result<String, AppError> {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "tailscale" | "headscale" => Ok(normalized),
        _ => Err(AppError::Validation(
            "provider must be 'tailscale' or 'headscale'".to_string(),
        )),
    }
}

pub fn validate_tailnet(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok("-".to_string());
    }
    if trimmed.len() > 128
        || trimmed.chars().any(|c| {
            matches!(c, '/' | '?' | '#' | '\\' | '%') || c.is_whitespace() || c.is_control()
        })
    {
        return Err(AppError::Validation(
            "tailnet contains invalid URL path characters".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

pub fn validate_auto_provision_role(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    match trimmed {
        "" | "member" => Ok(trimmed.to_string()),
        _ => Err(AppError::Validation(
            "auto_provision_role must be empty or 'member'".to_string(),
        )),
    }
}

pub fn validate_preferred_server_url(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let parsed = Url::parse(trimmed)
        .map_err(|e| AppError::Validation(format!("Invalid preferred_server_url: {e}")))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(AppError::Validation(
            "preferred_server_url must use http or https".to_string(),
        ));
    }
    if parsed.host_str().is_none() {
        return Err(AppError::Validation(
            "preferred_server_url must contain a host".to_string(),
        ));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(AppError::Validation(
            "preferred_server_url must not contain embedded credentials".to_string(),
        ));
    }
    Ok(trimmed.trim_end_matches('/').to_string())
}

pub fn validate_api_base_url(raw: &str) -> Result<Url, AppError> {
    let trimmed = raw.trim().trim_end_matches('/');
    let url =
        ssrf::validate_monitor_url(trimmed).map_err(|e| AppError::Validation(e.to_string()))?;
    let host = url
        .host_str()
        .ok_or_else(|| AppError::Validation("api_base_url has no host".to_string()))?
        .trim_start_matches('[')
        .trim_end_matches(']');
    if host.eq_ignore_ascii_case("localhost") {
        return Err(AppError::Validation(
            "SSRF guard: target 'localhost' is blocked (loopback not allowed)".to_string(),
        ));
    }
    ssrf::reject_literal_unsafe_target(trimmed).map_err(|e| AppError::Validation(e.to_string()))?;
    Ok(url)
}

// ---------------------------------------------------------------------------
// Upstream JSON Parsers (Tailscale v2 & Headscale v1)
// ---------------------------------------------------------------------------

fn null_to_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Deserialize)]
struct TailscaleDevicesEnvelope {
    #[serde(default, deserialize_with = "null_to_default")]
    devices: Vec<TailscaleApiDevice>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TailscaleApiDevice {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    node_id: Option<String>,
    #[serde(default, deserialize_with = "null_to_default")]
    name: String,
    #[serde(default, deserialize_with = "null_to_default")]
    hostname: String,
    #[serde(default)]
    os: Option<String>,
    #[serde(default, deserialize_with = "null_to_default")]
    addresses: Vec<String>,
    #[serde(default)]
    authorized: Option<bool>,
    #[serde(default)]
    connected_to_control: Option<bool>,
    #[serde(default)]
    last_seen: Option<String>,
    #[serde(default, deserialize_with = "null_to_default")]
    key_expiry_disabled: bool,
    #[serde(default)]
    expires: Option<String>,
    #[serde(default)]
    client_version: Option<String>,
    #[serde(default, deserialize_with = "null_to_default")]
    tags: Vec<String>,
}

pub fn parse_tailscale_devices_json(body: &str) -> Result<Vec<TailnetDeviceItem>, String> {
    let envelope: TailscaleDevicesEnvelope = serde_json::from_str(body)
        .map_err(|e| format!("Failed to parse Tailscale devices JSON: {e}"))?;
    let now = Utc::now();

    Ok(envelope
        .devices
        .into_iter()
        .map(|d| {
            let id = d.id.or(d.node_id).unwrap_or_else(|| d.name.clone());
            let clean_name = d.name.trim().trim_end_matches('.').to_string();
            let hostname = if d.hostname.trim().is_empty() {
                first_dns_label(&clean_name).to_string()
            } else {
                d.hostname.trim().to_string()
            };
            let online = d
                .connected_to_control
                .unwrap_or_else(|| is_recently_seen(d.last_seen.as_deref(), now));
            let expires = normalize_expiry(d.expires, d.key_expiry_disabled);
            TailnetDeviceItem {
                id,
                name: clean_name,
                hostname,
                os: d.os,
                addresses: d.addresses,
                authorized: d.authorized.unwrap_or(true),
                online,
                last_seen: normalize_go_timestamp(d.last_seen),
                key_expiry_disabled: d.key_expiry_disabled,
                expires,
                client_version: d.client_version,
                tags: d.tags,
                matched_server_id: None,
                matched_server_name: None,
            }
        })
        .collect())
}

#[derive(Deserialize)]
struct HeadscaleEnvelope {
    #[serde(default)]
    nodes: Option<Vec<HeadscaleNode>>,
    #[serde(default)]
    machines: Option<Vec<HeadscaleNode>>,
}

fn deserialize_string_or_int<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrInt {
        Str(String),
        Int(u64),
    }
    match StringOrInt::deserialize(deserializer)? {
        StringOrInt::Str(s) => Ok(s),
        StringOrInt::Int(n) => Ok(n.to_string()),
    }
}

#[derive(Deserialize)]
struct HeadscaleNode {
    #[serde(deserialize_with = "deserialize_string_or_int")]
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default, alias = "givenName")]
    given_name: Option<String>,
    #[serde(default, alias = "ipAddresses", deserialize_with = "null_to_default")]
    ip_addresses: Vec<String>,
    #[serde(default)]
    online: Option<bool>,
    #[serde(default, alias = "lastSeen")]
    last_seen: Option<String>,
    #[serde(default)]
    expiry: Option<String>,
    #[serde(default, deserialize_with = "null_to_default")]
    tags: Vec<String>,
    #[serde(default, alias = "forcedTags", deserialize_with = "null_to_default")]
    forced_tags: Vec<String>,
    #[serde(default, alias = "validTags", deserialize_with = "null_to_default")]
    valid_tags: Vec<String>,
}

pub fn parse_headscale_devices_json(body: &str) -> Result<Vec<TailnetDeviceItem>, String> {
    let envelope: HeadscaleEnvelope = serde_json::from_str(body)
        .map_err(|e| format!("Failed to parse Headscale nodes JSON: {e}"))?;
    let list = envelope.nodes.or(envelope.machines).unwrap_or_default();

    Ok(list
        .into_iter()
        .map(|node| {
            let given = node
                .given_name
                .as_deref()
                .map(|s| s.trim().trim_end_matches('.'))
                .filter(|s| !s.is_empty());
            let raw_name = node
                .name
                .as_deref()
                .map(|s| s.trim().trim_end_matches('.'))
                .filter(|s| !s.is_empty());

            let display_name = given.or(raw_name).unwrap_or(&node.id).to_string();
            let hostname = raw_name.or(given).unwrap_or(&node.id).to_string();

            let is_zero_expiry = node
                .expiry
                .as_deref()
                .is_none_or(|exp| exp.trim().is_empty() || exp.starts_with("0001-01-01"));
            let (key_expiry_disabled, expires) = if is_zero_expiry {
                (true, None)
            } else {
                (false, node.expiry)
            };

            let mut tags = Vec::new();
            for tag in node
                .tags
                .into_iter()
                .chain(node.forced_tags)
                .chain(node.valid_tags)
            {
                let trimmed = tag.trim();
                if !trimmed.is_empty() && !tags.iter().any(|existing| existing == trimmed) {
                    tags.push(trimmed.to_string());
                }
            }

            TailnetDeviceItem {
                id: node.id,
                name: display_name,
                hostname,
                os: None,
                addresses: node.ip_addresses,
                authorized: true,
                online: node.online.unwrap_or(false),
                last_seen: normalize_go_timestamp(node.last_seen),
                key_expiry_disabled,
                expires,
                client_version: None,
                tags,
                matched_server_id: None,
                matched_server_name: None,
            }
        })
        .collect())
}

fn normalize_go_timestamp(val: Option<String>) -> Option<String> {
    val.map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && !s.starts_with("0001-01-01"))
}

fn normalize_expiry(expires: Option<String>, disabled: bool) -> Option<String> {
    if disabled {
        return None;
    }
    normalize_go_timestamp(expires)
}

fn is_recently_seen(last_seen: Option<&str>, now: DateTime<Utc>) -> bool {
    let Some(raw) = last_seen else {
        return false;
    };
    let Ok(parsed) = DateTime::parse_from_rfc3339(raw) else {
        return false;
    };
    let seen_utc = parsed.with_timezone(&Utc);
    now.signed_duration_since(seen_utc) <= chrono::Duration::minutes(5)
}

pub fn is_key_expiring_soon(device: &TailnetDeviceItem, now: DateTime<Utc>) -> bool {
    if device.key_expiry_disabled {
        return false;
    }
    let Some(ref raw) = device.expires else {
        return false;
    };
    let Ok(parsed) = DateTime::parse_from_rfc3339(raw) else {
        return false;
    };
    parsed.with_timezone(&Utc) <= now + chrono::Duration::days(EXPIRING_SOON_DAYS)
}

// ---------------------------------------------------------------------------
// Device -> Server Auto-Linking
// ---------------------------------------------------------------------------

pub fn link_devices_to_servers(
    mut devices: Vec<TailnetDeviceItem>,
    servers: &[server::Model],
) -> Vec<TailnetDeviceItem> {
    for device in &mut devices {
        if let Some(matched) = find_matching_server(device, servers) {
            device.matched_server_id = Some(matched.id.clone());
            device.matched_server_name = Some(matched.name.clone());
        }
    }
    devices
}

fn find_matching_server<'a>(
    device: &TailnetDeviceItem,
    servers: &'a [server::Model],
) -> Option<&'a server::Model> {
    let device_ips: Vec<IpAddr> = device
        .addresses
        .iter()
        .filter_map(|a| a.trim().parse::<IpAddr>().ok())
        .collect();

    // Priority 1: IP address match against tailscale_ipv4, tailscale_ipv6, ipv4, or ipv6.
    if !device_ips.is_empty() {
        for srv in servers {
            let candidates = [
                srv.tailscale_ipv4.as_deref(),
                srv.tailscale_ipv6.as_deref(),
                srv.ipv4.as_deref(),
                srv.ipv6.as_deref(),
            ];
            for cand in candidates.into_iter().flatten() {
                if let Ok(srv_ip) = cand.trim().parse::<IpAddr>()
                    && device_ips.contains(&srv_ip)
                {
                    return Some(srv);
                }
            }
        }
    }

    let dev_dns = normalize_dns(&device.name);
    let dev_host = normalize_dns(&device.hostname);
    let dev_short = first_dns_label(&dev_dns);

    // Priority 2: Full MagicDNS match against server.tailscale_dns_name.
    if !dev_dns.is_empty() {
        for srv in servers {
            if let Some(ref srv_dns) = srv.tailscale_dns_name
                && normalize_dns(srv_dns) == dev_dns
            {
                return Some(srv);
            }
        }
    }

    // Priority 3: Hostname / short DNS label match against server.tailscale_dns_name or server.name.
    for srv in servers {
        let srv_name = normalize_dns(&srv.name);
        let srv_dns_short = srv
            .tailscale_dns_name
            .as_deref()
            .map(|d| first_dns_label(&normalize_dns(d)).to_string());

        let matches_host = (!dev_host.is_empty()
            && (srv_name == dev_host || srv_dns_short.as_deref() == Some(dev_host.as_str())))
            || (!dev_short.is_empty()
                && (srv_name == dev_short || srv_dns_short.as_deref() == Some(dev_short)));

        if matches_host {
            return Some(srv);
        }
    }

    None
}

fn normalize_dns(s: &str) -> String {
    s.trim().trim_end_matches('.').to_ascii_lowercase()
}

fn first_dns_label(s: &str) -> &str {
    s.split('.').next().unwrap_or(s)
}

// ---------------------------------------------------------------------------
// Local `tailscaled` Unix Socket Helpers (Status Hint & WhoIs Cross-Check)
// ---------------------------------------------------------------------------

const DEFAULT_TAILSCALED_SOCKETS: &[&str] = &[
    "/var/run/tailscale/tailscaled.sock",
    "/run/tailscale/tailscaled.sock",
    "/var/run/tailscaled.socket",
];

fn extract_listen_port(listen_addr: &str) -> u16 {
    listen_addr
        .rsplit_once(':')
        .and_then(|(_, p)| p.parse::<u16>().ok())
        .unwrap_or(9527)
}

#[cfg(unix)]
async fn localapi_get(socket_path: &str, path_and_query: &str) -> Option<(u16, String)> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let fut = async {
        let mut stream = tokio::net::UnixStream::connect(socket_path).await.ok()?;
        let req = format!(
            "GET {path_and_query} HTTP/1.1\r\nHost: local-tailscaled.sock\r\nConnection: close\r\n\r\n"
        );
        stream.write_all(req.as_bytes()).await.ok()?;

        let mut buf = Vec::with_capacity(8192);
        stream.read_to_end(&mut buf).await.ok()?;
        parse_raw_http_response(&buf)
    };

    tokio::time::timeout(Duration::from_millis(800), fut)
        .await
        .ok()
        .flatten()
}

#[cfg(not(unix))]
async fn localapi_get(_socket_path: &str, _path_and_query: &str) -> Option<(u16, String)> {
    None
}

fn parse_raw_http_response(raw: &[u8]) -> Option<(u16, String)> {
    let split_pos = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
    let header_str = std::str::from_utf8(&raw[..split_pos]).ok()?;
    let body_bytes = &raw[split_pos + 4..];

    let status_line = header_str.lines().next()?;
    let status_code: u16 = status_line.split_whitespace().nth(1)?.parse().ok()?;

    let is_chunked = header_str.lines().skip(1).any(|line| {
        line.split_once(':').is_some_and(|(k, v)| {
            k.trim().eq_ignore_ascii_case("transfer-encoding")
                && v.to_ascii_lowercase().contains("chunked")
        })
    });

    let decoded = if is_chunked {
        decode_chunked_body(body_bytes)?
    } else {
        body_bytes.to_vec()
    };

    Some((status_code, String::from_utf8_lossy(&decoded).into_owned()))
}

fn decode_chunked_body(mut input: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let line_end = input.windows(2).position(|w| w == b"\r\n")?;
        let size_str = std::str::from_utf8(&input[..line_end])
            .ok()?
            .split(';')
            .next()?
            .trim();
        let chunk_len = usize::from_str_radix(size_str, 16).ok()?;
        input = &input[line_end + 2..];
        if chunk_len == 0 {
            break;
        }
        if input.len() < chunk_len + 2 {
            return None;
        }
        out.extend_from_slice(&input[..chunk_len]);
        input = &input[chunk_len + 2..];
    }
    Some(out)
}

#[derive(Deserialize)]
struct LocalStatusSnippet {
    #[serde(default, rename = "BackendState", deserialize_with = "null_to_default")]
    backend_state: String,
    #[serde(default, rename = "TailscaleIPs", deserialize_with = "null_to_default")]
    tailscale_ips: Vec<String>,
    #[serde(default, rename = "CertDomains", deserialize_with = "null_to_default")]
    cert_domains: Vec<String>,
    #[serde(default, rename = "Self")]
    self_node: Option<LocalSelfNode>,
}

#[derive(Deserialize)]
struct LocalSelfNode {
    #[serde(default, rename = "DNSName", deserialize_with = "null_to_default")]
    dns_name: String,
    #[serde(default, rename = "TailscaleIPs", deserialize_with = "null_to_default")]
    tailscale_ips: Vec<String>,
}

fn parse_local_tailscaled_url_hint(body: &str, port: u16) -> Option<String> {
    let parsed = serde_json::from_str::<LocalStatusSnippet>(body).ok()?;
    if !parsed.backend_state.is_empty() && parsed.backend_state != "Running" {
        return None;
    }
    if let Some(dns) = parsed
        .self_node
        .as_ref()
        .map(|s| s.dns_name.trim().trim_end_matches('.'))
        .filter(|s| !s.is_empty())
    {
        let has_https_cert = parsed
            .cert_domains
            .iter()
            .any(|d| d.trim().trim_end_matches('.').eq_ignore_ascii_case(dns));
        if has_https_cert {
            return Some(format!("https://{dns}"));
        }
        return Some(format!("http://{dns}:{port}"));
    }
    let ip_candidates = if !parsed.tailscale_ips.is_empty() {
        &parsed.tailscale_ips
    } else if let Some(ref me) = parsed.self_node {
        &me.tailscale_ips
    } else {
        &parsed.tailscale_ips
    };
    if let Some(ip) = ip_candidates
        .iter()
        .filter_map(|s| s.trim().parse::<std::net::Ipv4Addr>().ok())
        .find(|v4| is_cgnat_ipv4(*v4))
    {
        return Some(format!("http://{ip}:{port}"));
    }
    None
}

pub async fn probe_local_tailscaled_url_hint(
    configured_socket: &str,
    listen_addr: &str,
) -> Option<String> {
    let candidates: Vec<&str> = if !configured_socket.trim().is_empty() {
        vec![configured_socket.trim()]
    } else {
        DEFAULT_TAILSCALED_SOCKETS.to_vec()
    };

    let port = extract_listen_port(listen_addr);
    for sock in candidates {
        if !std::path::Path::new(sock).exists() {
            continue;
        }
        let Some((200, body)) = localapi_get(sock, "/localapi/v0/status").await else {
            continue;
        };
        if let Some(url) = parse_local_tailscaled_url_hint(&body, port) {
            return Some(url);
        }
    }
    None
}

#[derive(Deserialize)]
struct LocalWhoIsSnippet {
    #[serde(default, rename = "UserProfile")]
    user_profile: Option<LocalUserProfile>,
}

#[derive(Deserialize)]
struct LocalUserProfile {
    #[serde(default, rename = "LoginName", deserialize_with = "null_to_default")]
    login_name: String,
}

/// Optional LocalAPI `/localapi/v0/whois?addr={peer}` cross-check when an
/// explicit `socket_path` is configured and reachable.
///
/// Returns:
/// - `Ok(true)` if no explicit socket is configured/reachable, or if `whois`
///   confirms `expected_login`.
/// - `Ok(false)` if `socket_path` is reachable and `tailscaled` rejects the
///   socket address or reports a different `LoginName`.
pub async fn verify_tailscale_whois_if_configured(
    socket_path: &str,
    peer: SocketAddr,
    expected_login: &str,
) -> bool {
    let sock = socket_path.trim();
    if sock.is_empty() || !std::path::Path::new(sock).exists() {
        return true;
    }
    let path = format!("/localapi/v0/whois?addr={peer}");
    let Some((status, body)) = localapi_get(sock, &path).await else {
        // Socket exists on filesystem but could not be connected (e.g. permission).
        return true;
    };
    if status != 200 {
        return false;
    }
    let Ok(whois) = serde_json::from_str::<LocalWhoIsSnippet>(&body) else {
        return false;
    };
    let Some(profile) = whois.user_profile else {
        return false;
    };
    let actual = profile.login_name.trim();
    !actual.is_empty() && actual.eq_ignore_ascii_case(expected_login.trim())
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::setup_test_db;
    use chrono::Duration as ChronoDuration;

    fn make_server_model(
        id: &str,
        name: &str,
        ipv4: Option<&str>,
        ts_ipv4: Option<&str>,
        ts_dns: Option<&str>,
    ) -> server::Model {
        let now = Utc::now();
        server::Model {
            id: id.to_string(),
            token_hash: None,
            token_prefix: None,
            name: name.to_string(),
            cpu_name: None,
            cpu_cores: None,
            cpu_arch: None,
            os: None,
            kernel_version: None,
            mem_total: None,
            swap_total: None,
            disk_total: None,
            ipv4: ipv4.map(str::to_string),
            ipv6: None,
            region: None,
            country_code: None,
            geo_manual: false,
            virtualization: None,
            agent_version: None,
            group_id: None,
            weight: 0,
            hidden: false,
            remark: None,
            public_remark: None,
            price: None,
            billing_cycle: None,
            currency: None,
            expired_at: None,
            renewal_state: None,
            traffic_limit: None,
            traffic_limit_type: None,
            billing_start_day: None,
            capabilities: 0,
            protocol_version: 1,
            features: "[]".to_string(),
            last_remote_addr: None,
            tailscale_ipv4: ts_ipv4.map(str::to_string),
            tailscale_ipv6: None,
            tailscale_dns_name: ts_dns.map(str::to_string),
            tailscale_tailnet: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn parses_tailscale_v2_devices_and_links_servers() {
        let json = r#"{
            "devices": [
                {
                    "id": "12345",
                    "name": "prod-tokyo.tail1234.ts.net.",
                    "hostname": "prod-tokyo",
                    "os": "linux",
                    "addresses": ["100.88.1.10", "fd7a:115c:a1e0::10"],
                    "authorized": true,
                    "connectedToControl": true,
                    "lastSeen": "2026-11-21T03:00:00Z",
                    "keyExpiryDisabled": false,
                    "expires": "2026-11-25T00:00:00Z",
                    "clientVersion": "1.76.1",
                    "tags": ["tag:prod"]
                },
                {
                    "nodeId": "n999",
                    "name": "db-box.tail1234.ts.net",
                    "hostname": "db-box",
                    "addresses": ["100.88.1.20"],
                    "connectedToControl": false,
                    "keyExpiryDisabled": true,
                    "expires": "2026-11-22T00:00:00Z"
                }
            ]
        }"#;

        let parsed = parse_tailscale_devices_json(json).expect("parse tailscale json");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].name, "prod-tokyo.tail1234.ts.net");
        assert!(parsed[0].online);
        assert!(!parsed[1].online);
        assert!(parsed[1].key_expiry_disabled);
        assert!(parsed[1].expires.is_none());

        let servers = vec![
            make_server_model("srv-1", "Tokyo Edge", None, Some("100.88.1.10"), None),
            make_server_model("srv-2", "db-box", None, None, None),
        ];
        let linked = link_devices_to_servers(parsed, &servers);
        assert_eq!(linked[0].matched_server_id.as_deref(), Some("srv-1"));
        assert_eq!(linked[0].matched_server_name.as_deref(), Some("Tokyo Edge"));
        assert_eq!(linked[1].matched_server_id.as_deref(), Some("srv-2"));
        assert_eq!(linked[1].matched_server_name.as_deref(), Some("db-box"));
    }

    #[test]
    fn parses_headscale_nodes_and_zero_expiry() {
        let json = r#"{
            "nodes": [
                {
                    "id": 1,
                    "name": "hs-node-1",
                    "givenName": "hs-node-1.ts.local",
                    "ipAddresses": ["100.64.0.1"],
                    "online": true,
                    "expiry": "0001-01-01T00:00:00Z",
                    "forcedTags": ["tag:hs"],
                    "validTags": ["tag:hs", "tag:extra"]
                }
            ]
        }"#;
        let parsed = parse_headscale_devices_json(json).expect("parse headscale json");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].id, "1");
        assert_eq!(parsed[0].name, "hs-node-1.ts.local");
        assert_eq!(parsed[0].hostname, "hs-node-1");
        assert!(parsed[0].key_expiry_disabled);
        assert!(parsed[0].expires.is_none());
        assert_eq!(parsed[0].tags, vec!["tag:hs", "tag:extra"]);
    }

    #[test]
    fn parses_headscale_v029_tags_and_go_null_slices() {
        // Real payload from Headscale v0.29.7 GET /api/v1/node after `headscale nodes tag -i 1 -t tag:hub`,
        // plus a second node exercising explicit Go `null` slices on ipAddresses/tags/forcedTags/validTags.
        let hs_json = r#"{
            "nodes": [
                {
                    "id": "1",
                    "name": "sb-ts-hub",
                    "givenName": "sb-ts-hub",
                    "ipAddresses": ["100.64.0.1", "fd7a:115c:a1e0::1"],
                    "online": true,
                    "lastSeen": "2026-11-21T05:30:00.635496004Z",
                    "expiry": null,
                    "tags": ["tag:hub"],
                    "forcedTags": ["tag:hub", "tag:forced"],
                    "validTags": null
                },
                {
                    "id": "2",
                    "name": "sb-ts-nulls",
                    "ipAddresses": null,
                    "online": false,
                    "lastSeen": "0001-01-01T00:00:00Z",
                    "expiry": null,
                    "tags": null,
                    "forcedTags": null,
                    "validTags": null
                }
            ]
        }"#;
        let hs_parsed = parse_headscale_devices_json(hs_json).expect("parse headscale v0.29 json");
        assert_eq!(hs_parsed.len(), 2);
        assert_eq!(hs_parsed[0].id, "1");
        assert_eq!(hs_parsed[0].name, "sb-ts-hub");
        assert_eq!(
            hs_parsed[0].addresses,
            vec!["100.64.0.1", "fd7a:115c:a1e0::1"]
        );
        assert!(hs_parsed[0].online);
        assert!(hs_parsed[0].key_expiry_disabled);
        assert!(hs_parsed[0].expires.is_none());
        assert_eq!(hs_parsed[0].tags, vec!["tag:hub", "tag:forced"]);

        assert_eq!(hs_parsed[1].id, "2");
        assert!(hs_parsed[1].addresses.is_empty());
        assert!(hs_parsed[1].tags.is_empty());
        assert!(hs_parsed[1].last_seen.is_none());

        // Also verify Tailscale v2 envelope/device handles Go `null` slices and fields cleanly.
        let ts_null_envelope = r#"{"devices": null}"#;
        assert!(
            parse_tailscale_devices_json(ts_null_envelope)
                .expect("null devices envelope")
                .is_empty()
        );

        let ts_null_fields = r#"{
            "devices": [
                {
                    "id": "n-null",
                    "name": "null-node.tail.ts.net.",
                    "hostname": null,
                    "addresses": null,
                    "authorized": null,
                    "keyExpiryDisabled": null,
                    "tags": null
                }
            ]
        }"#;
        let ts_parsed =
            parse_tailscale_devices_json(ts_null_fields).expect("null slice fields in device");
        assert_eq!(ts_parsed.len(), 1);
        assert_eq!(ts_parsed[0].hostname, "null-node");
        assert!(ts_parsed[0].addresses.is_empty());
        assert!(ts_parsed[0].authorized);
        assert!(!ts_parsed[0].key_expiry_disabled);
        assert!(ts_parsed[0].tags.is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn probe_local_tailscaled_url_hint_accepts_go_null_cert_domains_and_ips() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // Live payload captured from tailscaled 1.104.7 `/localapi/v0/status` (`"CertDomains": null`).
        let live_body = r#"{"BackendState":"Running","TailscaleIPs":["100.64.0.1","fd7a:115c:a1e0::1"],"CertDomains":null,"Self":{"DNSName":"sb-ts-hub.tailtest.internal."}}"#;
        assert_eq!(
            parse_local_tailscaled_url_hint(live_body, 9527).as_deref(),
            Some("http://sb-ts-hub.tailtest.internal:9527")
        );

        // Fallback when DNSName is absent/null and top-level TailscaleIPs is null, using Self.TailscaleIPs.
        let self_ip_fallback = r#"{"BackendState":"Running","TailscaleIPs":null,"CertDomains":null,"Self":{"DNSName":null,"TailscaleIPs":["100.64.0.1"]}}"#;
        assert_eq!(
            parse_local_tailscaled_url_hint(self_ip_fallback, 8080).as_deref(),
            Some("http://100.64.0.1:8080")
        );

        // End-to-end over a Unix domain socket via `probe_local_tailscaled_url_hint`.
        let sock_path = std::env::temp_dir().join(format!(
            "sb-ts-hint-{}.sock",
            &uuid::Uuid::new_v4().to_string()[..8]
        ));
        let _ = std::fs::remove_file(&sock_path);
        let listener =
            tokio::net::UnixListener::bind(&sock_path).expect("bind mock tailscaled status sock");

        let body_owned = live_body.to_string();
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 512];
                let _ = stream.read(&mut buf).await;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    body_owned.len(),
                    body_owned
                );
                let _ = stream.write_all(resp.as_bytes()).await;
            }
        });

        let sock_str = sock_path.to_string_lossy().to_string();
        let resolved = probe_local_tailscaled_url_hint(&sock_str, "0.0.0.0:9527").await;
        let _ = std::fs::remove_file(&sock_path);
        assert_eq!(
            resolved.as_deref(),
            Some("http://sb-ts-hub.tailtest.internal:9527")
        );
    }

    #[test]
    fn ssrf_validator_allows_lan_and_tailnet_but_blocks_loopback_and_metadata() {
        assert!(validate_api_base_url("https://api.tailscale.com").is_ok());
        assert!(validate_api_base_url("http://10.0.0.50:8080").is_ok());
        assert!(validate_api_base_url("http://100.64.0.10:8080").is_ok());
        assert!(validate_api_base_url("http://[fd7a:115c:a1e0::1]:8080").is_ok());

        assert!(validate_api_base_url("http://127.0.0.1:8080").is_err());
        assert!(validate_api_base_url("http://[::1]:8080").is_err());
        assert!(validate_api_base_url("http://localhost:8080").is_err());
        assert!(validate_api_base_url("http://169.254.169.254").is_err());
        assert!(validate_api_base_url("ftp://api.tailscale.com").is_err());
        assert!(validate_api_base_url("https://user:secret@api.tailscale.com").is_err());
    }

    #[test]
    fn expiring_key_detection_checks_14_day_window() {
        let now = Utc::now();
        let mut dev = TailnetDeviceItem {
            id: "1".into(),
            name: "n".into(),
            hostname: "n".into(),
            os: None,
            addresses: vec![],
            authorized: true,
            online: true,
            last_seen: None,
            key_expiry_disabled: false,
            expires: Some((now + ChronoDuration::days(5)).to_rfc3339()),
            client_version: None,
            tags: vec![],
            matched_server_id: None,
            matched_server_name: None,
        };
        assert!(is_key_expiring_soon(&dev, now));

        dev.key_expiry_disabled = true;
        assert!(!is_key_expiring_soon(&dev, now));

        dev.key_expiry_disabled = false;
        dev.expires = Some((now + ChronoDuration::days(30)).to_rfc3339());
        assert!(!is_key_expiring_soon(&dev, now));
    }

    #[test]
    fn debug_impls_redact_api_key() {
        let eff = EffectiveTailscaleConfig {
            enabled: true,
            provider: "tailscale".into(),
            api_base_url: "https://api.tailscale.com".into(),
            tailnet: "-".into(),
            api_key: "tskey-api-SUPERSECRET".into(),
            preferred_server_url: String::new(),
            socket_path: String::new(),
            auth_enabled: false,
            auto_provision_role: String::new(),
        };
        let formatted = format!("{eff:?}");
        assert!(!formatted.contains("SUPERSECRET"));
        assert!(formatted.contains("<redacted>"));
    }

    #[test]
    fn chunked_http_decoder_reassembles_chunks() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n7\r\n{\"ok\":t\r\n4\r\nrue}\r\n0\r\n\r\n";
        let (status, body) = parse_raw_http_response(raw).expect("decode chunked");
        assert_eq!(status, 200);
        assert_eq!(body, "{\"ok\":true}");
    }

    #[tokio::test]
    async fn update_config_preserves_clears_and_updates_api_key_without_leaking() {
        let (db, _tmp) = setup_test_db().await;
        let base = TailscaleConfig {
            api_key: "tskey-from-env".to_string(),
            ..TailscaleConfig::default()
        };
        let svc = TailscaleControlService::new(base, "0.0.0.0:9527".to_string());

        // Initially inherits env key.
        let initial = svc.get_admin_config(&db).await.unwrap();
        assert!(initial.has_api_key);

        // PUT with api_key: None preserves existing key.
        let (after_none, modified) = svc
            .update_config(
                &db,
                UpdateTailscaleConfigRequest {
                    enabled: true,
                    provider: "tailscale".into(),
                    api_base_url: "https://api.tailscale.com".into(),
                    tailnet: "example.com".into(),
                    api_key: None,
                    preferred_server_url: Some("https://bee.tail.ts.net".into()),
                    auth_enabled: Some(true),
                    auto_provision_role: Some("member".into()),
                },
            )
            .await
            .unwrap();
        assert!(!modified);
        assert!(after_none.has_api_key);
        assert_eq!(
            svc.effective_config(&db).await.unwrap().api_key,
            "tskey-from-env"
        );

        // PUT with Some("tskey-new") updates it.
        let (after_set, modified) = svc
            .update_config(
                &db,
                UpdateTailscaleConfigRequest {
                    enabled: true,
                    provider: "tailscale".into(),
                    api_base_url: "https://api.tailscale.com".into(),
                    tailnet: "example.com".into(),
                    api_key: Some("tskey-new".into()),
                    preferred_server_url: None,
                    auth_enabled: None,
                    auto_provision_role: None,
                },
            )
            .await
            .unwrap();
        assert!(modified);
        assert!(after_set.has_api_key);
        assert_eq!(
            svc.effective_config(&db).await.unwrap().api_key,
            "tskey-new"
        );

        // PUT with Some("") clears even over env default.
        let (after_clear, modified) = svc
            .update_config(
                &db,
                UpdateTailscaleConfigRequest {
                    enabled: false,
                    provider: "tailscale".into(),
                    api_base_url: "https://api.tailscale.com".into(),
                    tailnet: "-".into(),
                    api_key: Some(String::new()),
                    preferred_server_url: None,
                    auth_enabled: None,
                    auto_provision_role: None,
                },
            )
            .await
            .unwrap();
        assert!(modified);
        assert!(!after_clear.has_api_key);
        assert_eq!(svc.effective_config(&db).await.unwrap().api_key, "");

        // Rejects auto_provision_role = "admin".
        let bad_role = svc
            .update_config(
                &db,
                UpdateTailscaleConfigRequest {
                    enabled: false,
                    provider: "tailscale".into(),
                    api_base_url: "https://api.tailscale.com".into(),
                    tailnet: "-".into(),
                    api_key: None,
                    preferred_server_url: None,
                    auth_enabled: Some(true),
                    auto_provision_role: Some("admin".into()),
                },
            )
            .await;
        assert!(matches!(bad_role, Err(AppError::Validation(_))));
    }
}
