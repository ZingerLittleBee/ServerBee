//! Public-status DTOs.
//!
//! Defense-in-depth: every DTO defined here intentionally excludes the
//! IP-level identifiers and free-form leak fields listed in the design spec
//! (`docs/superpowers/specs/2026-05-26-status-page-refactor-design.md`,
//! §"Defense-in-depth: redaction at the API boundary"). Handlers in
//! `router::api::status` translate entity rows into these DTOs explicitly;
//! sensitive fields are simply absent from the DTO so they cannot leak via
//! any future refactor.
//!
//! Service queries that materialize these DTOs live in subsequent commits.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::service::uptime::UptimeDailyEntry;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicStatusConfig {
    pub enabled: bool,
    pub title: String,
    pub description: Option<String>,
    /// "list" | "grid"
    pub default_layout: String,
    pub show_server_detail: bool,
    pub show_network: bool,
    pub show_ip_quality: bool,
    pub show_incidents: bool,
    pub show_maintenance: bool,
    pub uptime_yellow_threshold: f64,
    pub uptime_red_threshold: f64,
}

// ---------------------------------------------------------------------------
// Server summary / detail
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicMetricsSummary {
    pub cpu: f64,
    pub mem_used: u64,
    pub mem_total: u64,
    pub disk_used: u64,
    pub disk_total: u64,
    pub net_in_speed: u64,
    pub net_out_speed: u64,
    pub load_1: f64,
    pub load_5: f64,
    pub load_15: f64,
    pub uptime: u64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicServerSummary {
    pub id: String,
    pub name: String,
    pub group_name: Option<String>,
    pub region: Option<String>,
    pub country_code: Option<String>,
    pub online: bool,
    pub in_maintenance: bool,
    pub public_remark: Option<String>,
    pub os: Option<String>,
    pub metrics: Option<PublicMetricsSummary>,
    pub uptime_percent: Option<f64>,
    pub uptime_daily: Vec<UptimeDailyEntry>,
    // No ipv4/ipv6/hostname/interfaces/public_ip — by design absent.
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicServerDetail {
    #[serde(flatten)]
    pub summary: PublicServerSummary,
    pub cpu_name: Option<String>,
    pub cpu_cores: Option<u32>,
    pub cpu_arch: Option<String>,
    pub kernel_version: Option<String>,
    pub agent_version: Option<String>,
    pub mem_total: Option<u64>,
    pub disk_total: Option<u64>,
    pub process_count: Option<u32>,
    pub tcp_conn: Option<u32>,
    pub udp_conn: Option<u32>,
}

// ---------------------------------------------------------------------------
// IP-quality
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicIpQualitySnapshot {
    pub country: Option<String>,
    pub ip_type: String,
    pub risk_score: Option<i32>,
    pub risk_level: String,
    pub checked_at: String,
    // Explicitly NOT included: ip, asn, as_org, region, city,
    // is_proxy, is_vpn, is_hosting, is_tor, is_abuser, is_mobile,
    // asn_abuser_score, abuse_email.
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicUnlockResult {
    pub service_id: String,
    pub status: String,
    /// Service-specific unlock region (e.g. "US-NY" for Netflix); distinct
    /// from the egress IP's geographic region, which is stripped.
    pub region: Option<String>,
    pub latency_ms: Option<i32>,
    pub checked_at: String,
    // Explicitly NOT included: `detail` (free-form, may leak).
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicIpQualityEntry {
    pub server_id: String,
    pub ip_quality: Option<PublicIpQualitySnapshot>,
    pub unlock_results: Vec<PublicUnlockResult>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicIpQualityServiceMeta {
    pub id: String,
    pub key: String,
    pub name: String,
    pub category: String,
    pub popularity: i32,
    pub is_builtin: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicIpQualityOverview {
    pub entries: Vec<PublicIpQualityEntry>,
    pub services: Vec<PublicIpQualityServiceMeta>,
}

// ---------------------------------------------------------------------------
// Incidents / maintenance
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicIncidentUpdate {
    pub id: String,
    pub status: String,
    pub message: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicIncident {
    pub id: String,
    pub title: String,
    pub severity: String,
    pub status: String,
    pub created_at: String,
    pub resolved_at: Option<String>,
    pub updates: Vec<PublicIncidentUpdate>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicMaintenance {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub start_at: String,
    pub end_at: String,
}

// ---------------------------------------------------------------------------
// Network
//
// The auth'd `NetworkProbeService::{ServerSummary, ServerOverview,
// TargetSummary, NetworkProbeAnomaly}` types are already IP-free at the
// server level — they only carry `server_id` / `server_name` — so we expose
// thin public wrappers without redefining each field. Network-probe `target`
// IPs and traceroute hop IPs are retained per spec (admin-configured probe
// topology is considered public information).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicNetworkOverview {
    pub servers: Vec<crate::service::network_probe::ServerOverview>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicNetworkServerDetail {
    pub summary: crate::service::network_probe::ServerSummary,
    pub anomalies: Vec<crate::service::network_probe::NetworkProbeAnomaly>,
}

// ---------------------------------------------------------------------------
// Time-series metrics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicMetricsPoint {
    pub time: String,
    pub cpu: f64,
    pub mem_used: i64,
    pub disk_used: i64,
    pub net_in_speed: i64,
    pub net_out_speed: i64,
    pub net_in_transfer: i64,
    pub net_out_transfer: i64,
    pub load1: f64,
    pub load5: f64,
    pub load15: f64,
    pub tcp_conn: i32,
    pub udp_conn: i32,
    pub process_count: i32,
    pub temperature: Option<f64>,
    pub gpu_usage: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PublicMetricsRangeQuery {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    #[serde(default = "default_interval")]
    pub interval: String,
}

fn default_interval() -> String {
    "auto".to_string()
}
