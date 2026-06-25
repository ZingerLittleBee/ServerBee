//! Public-status queries and DTOs.
//!
//! Defense-in-depth: every DTO defined here intentionally excludes the
//! IP-level identifiers and free-form leak fields listed in the design spec
//! (`docs/superpowers/specs/2026-05-26-status-page-refactor-design.md`,
//! §"Defense-in-depth: redaction at the API boundary"). Handlers in
//! `router::api::status` translate entity rows into these DTOs explicitly;
//! sensitive fields are simply absent from the DTO so they cannot leak via
//! any future refactor.

use std::collections::HashSet;

use chrono::{Duration, Utc};
use sea_orm::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::entity::{
    incident, incident_update, maintenance, server, server_group, status_page, unlock_service,
};
use crate::error::AppError;
use crate::service::agent_manager::aggregate_disk_io;
use crate::service::ip_quality::IpQualityService;
use crate::service::record::{QueryHistoryResult, RecordService};
use crate::service::uptime::{UptimeDailyEntry, UptimeService};

// ---------------------------------------------------------------------------
// DTOs
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

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicMetricsSummary {
    pub cpu: f64,
    pub mem_used: u64,
    pub mem_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub disk_used: u64,
    pub disk_total: u64,
    pub disk_read_bytes_per_sec: u64,
    pub disk_write_bytes_per_sec: u64,
    pub net_in_speed: u64,
    pub net_out_speed: u64,
    pub net_in_transfer: u64,
    pub net_out_transfer: u64,
    pub load_1: f64,
    pub load_5: f64,
    pub load_15: f64,
    pub tcp_conn: u32,
    pub udp_conn: u32,
    pub process_count: u32,
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
    // (Spec permits `hostname`; the plan opts to omit for additional defense-in-depth.)
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
    /// Absent until the agent has reported at least one snapshot.
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
// Network DTOs
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
// Time-series DTOs for `/api/status/servers/{id}/metrics`.
//
// The auth'd metrics endpoint returns raw `record::Model` rows, which carry
// no server-identity fields and are therefore safe to expose. However, that
// endpoint also accepts `interval=hourly` and returns the `record_hourly`
// shape, which has different fields. To keep the public surface simple we
// define a single normalized `PublicMetricsPoint` shape and convert both
// variants into it.
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
    pub from: chrono::DateTime<Utc>,
    pub to: chrono::DateTime<Utc>,
    #[serde(default = "default_interval")]
    pub interval: String,
}

fn default_interval() -> String {
    "auto".to_string()
}

/// Maximum window the public metrics endpoint serves at raw resolution.
///
/// The dashboard never requests a raw window wider than 24h (`1h`/`6h`/`24h`
/// use `raw`; `7d`/`30d` use `hourly`), so this preserves every legitimate
/// request while preventing an unauthenticated caller from forcing a full scan
/// of the high-cardinality raw table via a far-past `from`.
const MAX_PUBLIC_RAW_WINDOW_HOURS: i64 = 25;

/// Maximum window for any public metrics request (hourly resolution). Hourly
/// retention is ~90 days, so this is a generous upper bound that still rejects
/// absurd spans.
const MAX_PUBLIC_METRICS_WINDOW_DAYS: i64 = 120;

/// Clamp the public metrics range before it reaches the DB.
///
/// Rejects an inverted range and caps the span so an unauthenticated caller
/// cannot force an oversized scan/response. The raw cap is applied exactly when
/// `RecordService::query_history` would hit the raw table (mirrors its
/// resolution selection). Returns the possibly-adjusted `from`; `to` is left
/// unchanged so the response still ends at the requested instant.
fn clamp_public_metrics_from(
    from: chrono::DateTime<Utc>,
    to: chrono::DateTime<Utc>,
    interval: &str,
) -> Result<chrono::DateTime<Utc>, AppError> {
    if from > to {
        return Err(AppError::Validation(
            "`from` must not be after `to`".to_string(),
        ));
    }

    let uses_raw = match interval {
        "raw" => true,
        "hourly" => false,
        // "auto": query_history uses raw for windows <= 24h, hourly otherwise.
        _ => (to - from) <= Duration::hours(24),
    };

    let max_window = if uses_raw {
        Duration::hours(MAX_PUBLIC_RAW_WINDOW_HOURS)
    } else {
        Duration::days(MAX_PUBLIC_METRICS_WINDOW_DAYS)
    };

    if to - from > max_window {
        Ok(to - max_window)
    } else {
        Ok(from)
    }
}

// ---------------------------------------------------------------------------
// Scope guard
// ---------------------------------------------------------------------------

/// Resolved scope for the public surface: the loaded config plus the set of
/// server IDs that may appear in any public response.
///
/// Membership rule (intersection of all three):
///   1. `id` is listed in `status_page.server_ids_json`.
///   2. The `servers` row for `id` still exists.
///   3. `servers.hidden = false`.
pub struct PublicScope {
    pub config: status_page::Model,
    pub server_ids: Vec<String>,
}

impl PublicScope {
    pub fn contains(&self, id: &str) -> bool {
        self.server_ids.iter().any(|s| s == id)
    }
}

// ---------------------------------------------------------------------------
// Config / scope helpers
// ---------------------------------------------------------------------------

/// Load the singleton `status_page` row. The migration guarantees exactly one
/// row exists; this helper is the canonical reader. Returns `NotFound` if the
/// singleton invariant has somehow been violated.
pub async fn load_config(db: &DatabaseConnection) -> Result<status_page::Model, AppError> {
    status_page::Entity::find()
        .order_by_asc(status_page::Column::CreatedAt)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("status_page singleton row missing".into()))
}

/// Straight field mapping from the singleton entity to the public DTO.
pub fn to_public_config(model: &status_page::Model) -> PublicStatusConfig {
    PublicStatusConfig {
        enabled: model.enabled,
        title: model.title.clone(),
        description: model.description.clone(),
        default_layout: model.default_layout.clone(),
        show_server_detail: model.show_server_detail,
        show_network: model.show_network,
        show_ip_quality: model.show_ip_quality,
        show_incidents: model.show_incidents,
        show_maintenance: model.show_maintenance,
        uptime_yellow_threshold: model.uptime_yellow_threshold,
        uptime_red_threshold: model.uptime_red_threshold,
    }
}

/// Resolve the in-scope server IDs by intersecting the admin-selected list
/// with the live, non-hidden `servers` rows.
pub async fn resolve_scope(db: &DatabaseConnection) -> Result<PublicScope, AppError> {
    let config = load_config(db).await?;
    let selected: Vec<String> = if config.server_ids_json.trim().is_empty() {
        Vec::new()
    } else {
        serde_json::from_str(&config.server_ids_json)
            .map_err(|e| AppError::Internal(format!("invalid server_ids_json: {e}")))?
    };

    let live_ids: HashSet<String> = server::Entity::find()
        .filter(server::Column::Hidden.eq(false))
        .all(db)
        .await?
        .into_iter()
        .map(|s| s.id)
        .collect();

    let server_ids = selected
        .into_iter()
        .filter(|id| live_ids.contains(id))
        .collect();

    Ok(PublicScope { config, server_ids })
}

// ---------------------------------------------------------------------------
// Internal mapping helpers
// ---------------------------------------------------------------------------

fn uptime_percent(daily: &[UptimeDailyEntry]) -> Option<f64> {
    let mut total = 0i64;
    let mut online = 0i64;
    for entry in daily {
        total += entry.total_minutes as i64;
        online += entry.online_minutes as i64;
    }
    if total == 0 {
        None
    } else {
        Some((online as f64 / total as f64) * 100.0)
    }
}

fn build_summary(
    server: &server::Model,
    online: bool,
    in_maintenance: bool,
    group_name: Option<String>,
    metrics: Option<PublicMetricsSummary>,
    uptime_daily: Vec<UptimeDailyEntry>,
) -> PublicServerSummary {
    let uptime_percent = uptime_percent(&uptime_daily);
    PublicServerSummary {
        id: server.id.clone(),
        name: server.name.clone(),
        group_name,
        region: server.region.clone(),
        country_code: server.country_code.clone(),
        online,
        in_maintenance,
        public_remark: server.public_remark.clone(),
        os: server.os.clone(),
        metrics,
        uptime_percent,
        uptime_daily,
    }
}

fn report_to_metrics(
    report: &serverbee_common::types::SystemReport,
    mem_total: i64,
    swap_total: i64,
    disk_total: i64,
) -> PublicMetricsSummary {
    let (disk_read_bytes_per_sec, disk_write_bytes_per_sec) = aggregate_disk_io(report);
    PublicMetricsSummary {
        cpu: report.cpu,
        mem_used: report.mem_used.max(0) as u64,
        mem_total: mem_total.max(0) as u64,
        swap_used: report.swap_used.max(0) as u64,
        swap_total: swap_total.max(0) as u64,
        disk_used: report.disk_used.max(0) as u64,
        disk_total: disk_total.max(0) as u64,
        disk_read_bytes_per_sec,
        disk_write_bytes_per_sec,
        net_in_speed: report.net_in_speed.max(0) as u64,
        net_out_speed: report.net_out_speed.max(0) as u64,
        net_in_transfer: report.net_in_transfer.max(0) as u64,
        net_out_transfer: report.net_out_transfer.max(0) as u64,
        load_1: report.load1,
        load_5: report.load5,
        load_15: report.load15,
        tcp_conn: report.tcp_conn.max(0) as u32,
        udp_conn: report.udp_conn.max(0) as u32,
        process_count: report.process_count.max(0) as u32,
        uptime: report.uptime,
    }
}

fn snapshot_to_public(
    snap: &serverbee_common::protocol::IpQualitySnapshotData,
) -> PublicIpQualitySnapshot {
    PublicIpQualitySnapshot {
        country: snap.country.clone(),
        ip_type: snap.ip_type.clone(),
        risk_score: snap.risk_score,
        risk_level: snap.risk_level.clone(),
        checked_at: snap.checked_at.to_rfc3339(),
    }
}

fn unlock_to_public(r: &crate::service::ip_quality::UnlockResultDto) -> PublicUnlockResult {
    PublicUnlockResult {
        service_id: r.service_id.clone(),
        status: r.status.clone(),
        region: r.region.clone(),
        latency_ms: r.latency_ms,
        checked_at: r.checked_at.clone(),
    }
}

// ---------------------------------------------------------------------------
// Per-endpoint queries
// ---------------------------------------------------------------------------

/// List all in-scope servers with their summary metrics + uptime band.
pub async fn list_servers(
    db: &DatabaseConnection,
    agent_manager: &crate::service::agent_manager::AgentManager,
) -> Result<Vec<PublicServerSummary>, AppError> {
    let scope = resolve_scope(db).await?;
    if scope.server_ids.is_empty() {
        return Ok(Vec::new());
    }

    // Load the actual `server` rows for in-scope ids in one query, then
    // preserve the admin-configured ordering.
    let mut servers: std::collections::HashMap<String, server::Model> = server::Entity::find()
        .filter(server::Column::Id.is_in(scope.server_ids.iter().cloned()))
        .all(db)
        .await?
        .into_iter()
        .map(|s| (s.id.clone(), s))
        .collect();

    // Build group name lookup
    let group_ids: HashSet<String> = servers
        .values()
        .filter_map(|s| s.group_id.clone())
        .collect();
    let group_lookup: std::collections::HashMap<String, String> = if group_ids.is_empty() {
        std::collections::HashMap::new()
    } else {
        server_group::Entity::find()
            .filter(server_group::Column::Id.is_in(group_ids.iter().cloned()))
            .all(db)
            .await?
            .into_iter()
            .map(|g| (g.id, g.name))
            .collect()
    };

    let mut out = Vec::with_capacity(scope.server_ids.len());
    for id in &scope.server_ids {
        let Some(srv) = servers.remove(id) else {
            continue;
        };
        let online = agent_manager.is_online(&srv.id);
        let in_maintenance =
            crate::service::maintenance::MaintenanceService::is_in_maintenance(db, &srv.id)
                .await
                .unwrap_or(false);
        let group_name = srv
            .group_id
            .as_deref()
            .and_then(|g| group_lookup.get(g).cloned());

        let metrics = agent_manager.get_latest_report(&srv.id).map(|r| {
            report_to_metrics(
                &r,
                srv.mem_total.unwrap_or(0),
                srv.swap_total.unwrap_or(0),
                srv.disk_total.unwrap_or(0),
            )
        });

        // 90-day uptime band (canonical for the public surface).
        let uptime_daily = UptimeService::get_daily_filled(db, &srv.id, 90)
            .await
            .unwrap_or_default();

        out.push(build_summary(
            &srv,
            online,
            in_maintenance,
            group_name,
            metrics,
            uptime_daily,
        ));
    }

    Ok(out)
}

/// Fetch a single in-scope server's full detail. Out-of-scope IDs return
/// `NotFound` — never `Forbidden` — to avoid confirming the existence of
/// hidden or unselected servers (see spec §"Public server scope").
pub async fn get_server_detail(
    db: &DatabaseConnection,
    agent_manager: &crate::service::agent_manager::AgentManager,
    id: &str,
) -> Result<PublicServerDetail, AppError> {
    let scope = resolve_scope(db).await?;
    if !scope.contains(id) {
        return Err(AppError::NotFound("server".into()));
    }

    let srv = server::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("server".into()))?;

    let online = agent_manager.is_online(&srv.id);
    let in_maintenance =
        crate::service::maintenance::MaintenanceService::is_in_maintenance(db, &srv.id)
            .await
            .unwrap_or(false);

    let group_name = if let Some(group_id) = &srv.group_id {
        server_group::Entity::find_by_id(group_id)
            .one(db)
            .await?
            .map(|g| g.name)
    } else {
        None
    };

    let latest = agent_manager.get_latest_report(&srv.id);

    let metrics = latest.as_ref().map(|r| {
        report_to_metrics(
            r,
            srv.mem_total.unwrap_or(0),
            srv.swap_total.unwrap_or(0),
            srv.disk_total.unwrap_or(0),
        )
    });

    let uptime_daily = UptimeService::get_daily_filled(db, &srv.id, 90)
        .await
        .unwrap_or_default();

    let summary = build_summary(
        &srv,
        online,
        in_maintenance,
        group_name,
        metrics,
        uptime_daily,
    );

    Ok(PublicServerDetail {
        summary,
        cpu_name: srv.cpu_name.clone(),
        cpu_cores: srv.cpu_cores.map(|v| v.max(0) as u32),
        cpu_arch: srv.cpu_arch.clone(),
        kernel_version: srv.kernel_version.clone(),
        agent_version: srv.agent_version.clone(),
        mem_total: srv.mem_total.map(|v| v.max(0) as u64),
        disk_total: srv.disk_total.map(|v| v.max(0) as u64),
        process_count: latest.as_ref().map(|r| r.process_count.max(0) as u32),
        tcp_conn: latest.as_ref().map(|r| r.tcp_conn.max(0) as u32),
        udp_conn: latest.as_ref().map(|r| r.udp_conn.max(0) as u32),
    })
}

/// Per-server time-series metrics. Reuses `RecordService::query_history` and
/// normalizes both raw and hourly variants into `PublicMetricsPoint`.
pub async fn get_server_metrics(
    db: &DatabaseConnection,
    id: &str,
    range: PublicMetricsRangeQuery,
) -> Result<Vec<PublicMetricsPoint>, AppError> {
    let scope = resolve_scope(db).await?;
    if !scope.contains(id) {
        return Err(AppError::NotFound("server".into()));
    }

    let from = clamp_public_metrics_from(range.from, range.to, &range.interval)?;
    let result = RecordService::query_history(db, id, from, range.to, &range.interval).await?;

    let points = match result {
        QueryHistoryResult::Raw(records) => records
            .into_iter()
            .map(|r| PublicMetricsPoint {
                time: r.time.to_rfc3339(),
                cpu: r.cpu,
                mem_used: r.mem_used,
                disk_used: r.disk_used,
                net_in_speed: r.net_in_speed,
                net_out_speed: r.net_out_speed,
                net_in_transfer: r.net_in_transfer,
                net_out_transfer: r.net_out_transfer,
                load1: r.load1,
                load5: r.load5,
                load15: r.load15,
                tcp_conn: r.tcp_conn,
                udp_conn: r.udp_conn,
                process_count: r.process_count,
                temperature: r.temperature,
                gpu_usage: r.gpu_usage,
            })
            .collect(),
        QueryHistoryResult::Hourly(records) => records
            .into_iter()
            .map(|r| PublicMetricsPoint {
                time: r.time.to_rfc3339(),
                cpu: r.cpu,
                mem_used: r.mem_used,
                disk_used: r.disk_used,
                net_in_speed: r.net_in_speed,
                net_out_speed: r.net_out_speed,
                net_in_transfer: r.net_in_transfer,
                net_out_transfer: r.net_out_transfer,
                load1: r.load1,
                load5: r.load5,
                load15: r.load15,
                tcp_conn: r.tcp_conn,
                udp_conn: r.udp_conn,
                process_count: r.process_count,
                temperature: r.temperature,
                gpu_usage: r.gpu_usage,
            })
            .collect(),
    };

    Ok(points)
}

/// 90-day uptime band for a scoped server.
pub async fn get_server_uptime_daily(
    db: &DatabaseConnection,
    id: &str,
) -> Result<Vec<UptimeDailyEntry>, AppError> {
    let scope = resolve_scope(db).await?;
    if !scope.contains(id) {
        return Err(AppError::NotFound("server".into()));
    }
    UptimeService::get_daily_filled(db, id, 90).await
}

/// Network overview: per-server probe averages scoped to the public set.
pub async fn network_overview(
    db: &DatabaseConnection,
    agent_manager: &crate::service::agent_manager::AgentManager,
    config: &crate::config::NetworkProbeConfig,
) -> Result<PublicNetworkOverview, AppError> {
    let scope = resolve_scope(db).await?;
    let scope_set: HashSet<String> = scope.server_ids.iter().cloned().collect();

    let all =
        crate::service::network_probe::NetworkProbeService::get_overview(db, agent_manager, config)
            .await?;
    let servers = all
        .into_iter()
        .filter(|s| scope_set.contains(&s.server_id))
        .collect();
    Ok(PublicNetworkOverview { servers })
}

/// Per-server network detail (summary + recent anomalies). 24h window for
/// anomalies matches the existing auth'd UI's default span.
pub async fn network_server_detail(
    db: &DatabaseConnection,
    agent_manager: &crate::service::agent_manager::AgentManager,
    config: &crate::config::NetworkProbeConfig,
    id: &str,
) -> Result<PublicNetworkServerDetail, AppError> {
    let scope = resolve_scope(db).await?;
    if !scope.contains(id) {
        return Err(AppError::NotFound("server".into()));
    }

    let summary = crate::service::network_probe::NetworkProbeService::get_server_summary(
        db,
        agent_manager,
        id,
        config,
    )
    .await?;

    let to = Utc::now();
    let from = to - Duration::hours(24);
    let anomalies =
        crate::service::network_probe::NetworkProbeService::get_anomalies(db, id, from, to, config)
            .await
            .unwrap_or_default();

    Ok(PublicNetworkServerDetail { summary, anomalies })
}

/// IP-quality overview, redacted per spec.
pub async fn ip_quality_overview(
    db: &DatabaseConnection,
) -> Result<PublicIpQualityOverview, AppError> {
    let scope = resolve_scope(db).await?;
    let raw = IpQualityService::get_summaries(db, &scope.server_ids).await?;

    let entries = raw
        .into_iter()
        .map(|d| PublicIpQualityEntry {
            server_id: d.server_id,
            ip_quality: d.ip_quality.as_ref().map(snapshot_to_public),
            unlock_results: d.unlock_results.iter().map(unlock_to_public).collect(),
        })
        .collect();

    // Enabled service catalog so the SPA can render the matrix headers even
    // for servers that don't yet have results for some services.
    let services = unlock_service::Entity::find()
        .filter(unlock_service::Column::Enabled.eq(true))
        .order_by_asc(unlock_service::Column::Category)
        .order_by_desc(unlock_service::Column::Popularity)
        .all(db)
        .await?
        .into_iter()
        .map(|s| PublicIpQualityServiceMeta {
            id: s.id,
            key: s.key,
            name: s.name,
            category: s.category,
            popularity: s.popularity,
            is_builtin: s.is_builtin,
        })
        .collect();

    Ok(PublicIpQualityOverview { entries, services })
}

/// Active and recent incidents. Active = any non-resolved row. Recent =
/// resolved within the last 7 days.
pub async fn list_incidents(
    db: &DatabaseConnection,
) -> Result<(Vec<PublicIncident>, Vec<PublicIncident>), AppError> {
    // Filter at the DB layer: only rows the admin has explicitly marked public.
    let public_rows = incident::Entity::find()
        .filter(incident::Column::IsPublic.eq(true))
        .order_by_desc(incident::Column::CreatedAt)
        .all(db)
        .await?;

    if public_rows.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    // Pull all updates for these incidents in one query, then bucket by id.
    let ids: Vec<String> = public_rows.iter().map(|r| r.id.clone()).collect();
    let all_updates = incident_update::Entity::find()
        .filter(incident_update::Column::IncidentId.is_in(ids.iter().cloned()))
        .order_by_asc(incident_update::Column::CreatedAt)
        .all(db)
        .await?;

    let mut updates_by_incident: std::collections::HashMap<String, Vec<PublicIncidentUpdate>> =
        std::collections::HashMap::new();
    for u in all_updates {
        updates_by_incident
            .entry(u.incident_id.clone())
            .or_default()
            .push(PublicIncidentUpdate {
                id: u.id,
                status: u.status,
                message: u.message,
                created_at: u.created_at.to_rfc3339(),
            });
    }

    let now = Utc::now();
    let recent_cutoff = now - Duration::days(7);
    let mut active = Vec::new();
    let mut recent = Vec::new();

    for r in public_rows {
        let updates = updates_by_incident.remove(&r.id).unwrap_or_default();
        let dto = PublicIncident {
            id: r.id,
            title: r.title,
            severity: r.severity,
            status: r.status.clone(),
            created_at: r.created_at.to_rfc3339(),
            resolved_at: r.resolved_at.map(|t| t.to_rfc3339()),
            updates,
        };

        if dto.status != "resolved" {
            active.push(dto);
        } else if let Some(ts) = r.resolved_at
            && ts >= recent_cutoff
        {
            recent.push(dto);
        }
    }

    Ok((active, recent))
}

/// Currently-active or upcoming maintenance windows that the admin has
/// marked public.
pub async fn list_maintenances(
    db: &DatabaseConnection,
) -> Result<Vec<PublicMaintenance>, AppError> {
    let now = Utc::now();
    let rows = maintenance::Entity::find()
        .filter(maintenance::Column::IsPublic.eq(true))
        .filter(maintenance::Column::Active.eq(true))
        .filter(maintenance::Column::EndAt.gte(now))
        .order_by_asc(maintenance::Column::StartAt)
        .all(db)
        .await?;

    Ok(rows
        .into_iter()
        .map(|r| PublicMaintenance {
            id: r.id,
            title: r.title,
            description: r.description,
            start_at: r.start_at.to_rfc3339(),
            end_at: r.end_at.to_rfc3339(),
        })
        .collect())
}

#[cfg(test)]
mod metrics_range_tests {
    use super::*;

    fn t(offset_hours: i64) -> chrono::DateTime<Utc> {
        // Fixed reference instant so the test is deterministic.
        let base = chrono::DateTime::parse_from_rfc3339("2026-06-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        base + Duration::hours(offset_hours)
    }

    #[test]
    fn inverted_range_is_rejected() {
        let err = clamp_public_metrics_from(t(10), t(0), "auto");
        assert!(matches!(err, Err(AppError::Validation(_))));
    }

    #[test]
    fn legitimate_raw_window_is_unchanged() {
        // 24h raw is the widest raw window the dashboard requests.
        let from = t(0);
        let to = t(24);
        assert_eq!(clamp_public_metrics_from(from, to, "raw").unwrap(), from);
    }

    #[test]
    fn far_past_raw_request_is_clamped_to_raw_window() {
        // Abuse case: 30-day span at raw resolution.
        let from = t(0);
        let to = t(24 * 30);
        let clamped = clamp_public_metrics_from(from, to, "raw").unwrap();
        assert_eq!(clamped, to - Duration::hours(MAX_PUBLIC_RAW_WINDOW_HOURS));
        assert!(clamped > from, "from must be pulled forward");
    }

    #[test]
    fn auto_wide_span_uses_hourly_window() {
        // "auto" with a >24h span resolves to hourly, so the wider cap applies.
        let from = t(0);
        let to = t(24 * 200); // 200 days
        let clamped = clamp_public_metrics_from(from, to, "auto").unwrap();
        assert_eq!(clamped, to - Duration::days(MAX_PUBLIC_METRICS_WINDOW_DAYS));
    }

    #[test]
    fn legitimate_hourly_window_is_unchanged() {
        // 30d hourly is the widest hourly window the dashboard requests.
        let from = t(0);
        let to = t(24 * 30);
        assert_eq!(
            clamp_public_metrics_from(from, to, "hourly").unwrap(),
            from
        );
    }

    #[test]
    fn auto_narrow_span_uses_raw_window() {
        // "auto" with a <=24h span resolves to raw; a 25h+ request is still
        // capped by the raw window even though "auto" was requested.
        let from = t(0);
        let to = t(20); // 20h <= 24h => raw, within cap, unchanged
        assert_eq!(clamp_public_metrics_from(from, to, "auto").unwrap(), from);
    }

    #[test]
    fn equal_from_to_is_accepted() {
        // from == to is not "after", so it must pass validation unchanged.
        let from = t(5);
        assert_eq!(clamp_public_metrics_from(from, from, "raw").unwrap(), from);
    }

    #[test]
    fn unknown_interval_falls_back_to_auto_window() {
        // An unrecognized interval string falls into the "auto" arm. A >24h
        // span therefore resolves to hourly and the wide cap applies.
        let from = t(0);
        let to = t(24 * 200);
        let clamped = clamp_public_metrics_from(from, to, "weekly").unwrap();
        assert_eq!(clamped, to - Duration::days(MAX_PUBLIC_METRICS_WINDOW_DAYS));
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use crate::entity::{incident, incident_update, maintenance, server, server_group, unlock_service};
    use crate::service::agent_manager::AgentManager;
    use crate::test_utils::setup_test_db;
    use chrono::Utc;
    use sea_orm::Set;
    use serverbee_common::protocol::BrowserMessage;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use tokio::sync::broadcast;

    fn make_manager() -> AgentManager {
        let (tx, _rx) = broadcast::channel::<BrowserMessage>(16);
        AgentManager::new(tx)
    }

    fn sock() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080)
    }

    /// Overwrite the migration-seeded singleton `status_page` row so tests can
    /// drive the config / scope branches deterministically.
    async fn set_config(
        db: &DatabaseConnection,
        enabled: bool,
        server_ids_json: &str,
        description: Option<&str>,
    ) {
        let model = load_config(db).await.expect("singleton present");
        let mut active: status_page::ActiveModel = model.into();
        active.enabled = Set(enabled);
        active.server_ids_json = Set(server_ids_json.to_string());
        active.description = Set(description.map(|s| s.to_string()));
        active.title = Set("My Status".to_string());
        active.default_layout = Set("grid".to_string());
        active.show_server_detail = Set(true);
        active.show_network = Set(false);
        active.show_ip_quality = Set(true);
        active.show_incidents = Set(true);
        active.show_maintenance = Set(true);
        active.uptime_yellow_threshold = Set(99.0);
        active.uptime_red_threshold = Set(95.0);
        active.update(db).await.expect("update config");
    }

    async fn seed_server(
        db: &DatabaseConnection,
        id: &str,
        name: &str,
        hidden: bool,
        group_id: Option<&str>,
    ) {
        let now = Utc::now();
        server::ActiveModel {
            id: Set(id.to_string()),
            name: Set(name.to_string()),
            weight: Set(0),
            hidden: Set(hidden),
            capabilities: Set(0),
            protocol_version: Set(1),
            group_id: Set(group_id.map(|g| g.to_string())),
            region: Set(Some("US".to_string())),
            country_code: Set(Some("US".to_string())),
            os: Set(Some("linux".to_string())),
            public_remark: Set(Some("public note".to_string())),
            // IP fields are populated to prove the public DTOs never surface them.
            ipv4: Set(Some("203.0.113.7".to_string())),
            ipv6: Set(Some("2001:db8::1".to_string())),
            remark: Set(Some("private internal note".to_string())),
            cpu_name: Set(Some("TestCPU".to_string())),
            cpu_cores: Set(Some(4)),
            cpu_arch: Set(Some("x86_64".to_string())),
            kernel_version: Set(Some("6.1.0".to_string())),
            agent_version: Set(Some("1.2.3".to_string())),
            mem_total: Set(Some(8_000)),
            swap_total: Set(Some(2_000)),
            disk_total: Set(Some(100_000)),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert server");
    }

    fn sample_report() -> serverbee_common::types::SystemReport {
        serverbee_common::types::SystemReport {
            cpu: 42.5,
            mem_used: 4_000,
            swap_used: 500,
            disk_used: 50_000,
            net_in_speed: 100,
            net_out_speed: 200,
            net_in_transfer: 1_000,
            net_out_transfer: 2_000,
            load1: 0.5,
            load5: 0.4,
            load15: 0.3,
            tcp_conn: 12,
            udp_conn: 3,
            process_count: 88,
            uptime: 3_600,
            ..Default::default()
        }
    }

    // ---------------------------------------------------------------------
    // Config / scope
    // ---------------------------------------------------------------------

    #[tokio::test]
    async fn load_config_returns_seeded_singleton() {
        let (db, _tmp) = setup_test_db().await;
        let cfg = load_config(&db).await.expect("config loads");
        // Migration seeds exactly one row with a non-empty id.
        assert!(!cfg.id.is_empty());
    }

    #[tokio::test]
    async fn to_public_config_maps_all_fields() {
        let (db, _tmp) = setup_test_db().await;
        set_config(&db, true, "[]", Some("hello")).await;
        let model = load_config(&db).await.unwrap();
        let dto = to_public_config(&model);
        assert!(dto.enabled);
        assert_eq!(dto.title, "My Status");
        assert_eq!(dto.description.as_deref(), Some("hello"));
        assert_eq!(dto.default_layout, "grid");
        assert!(dto.show_server_detail);
        assert!(!dto.show_network);
        assert!(dto.show_ip_quality);
        assert!(dto.show_incidents);
        assert!(dto.show_maintenance);
        assert_eq!(dto.uptime_yellow_threshold, 99.0);
        assert_eq!(dto.uptime_red_threshold, 95.0);
    }

    #[tokio::test]
    async fn to_public_config_maps_none_description() {
        let (db, _tmp) = setup_test_db().await;
        set_config(&db, false, "[]", None).await;
        let model = load_config(&db).await.unwrap();
        let dto = to_public_config(&model);
        assert!(!dto.enabled);
        assert!(dto.description.is_none());
    }

    #[tokio::test]
    async fn resolve_scope_empty_when_no_ids_selected() {
        let (db, _tmp) = setup_test_db().await;
        set_config(&db, true, "  ", None).await; // whitespace-only => empty
        let scope = resolve_scope(&db).await.expect("scope resolves");
        assert!(scope.server_ids.is_empty());
        assert!(!scope.contains("anything"));
    }

    #[tokio::test]
    async fn resolve_scope_intersects_live_non_hidden_servers() {
        let (db, _tmp) = setup_test_db().await;
        seed_server(&db, "live", "Live", false, None).await;
        seed_server(&db, "hidden", "Hidden", true, None).await;
        // "missing" is selected but has no servers row.
        set_config(&db, true, r#"["live","hidden","missing"]"#, None).await;

        let scope = resolve_scope(&db).await.expect("scope resolves");
        assert_eq!(scope.server_ids, vec!["live".to_string()]);
        assert!(scope.contains("live"));
        assert!(!scope.contains("hidden"), "hidden server is excluded");
        assert!(!scope.contains("missing"), "missing server is excluded");
    }

    #[tokio::test]
    async fn resolve_scope_preserves_admin_ordering() {
        let (db, _tmp) = setup_test_db().await;
        seed_server(&db, "b", "B", false, None).await;
        seed_server(&db, "a", "A", false, None).await;
        set_config(&db, true, r#"["b","a"]"#, None).await;
        let scope = resolve_scope(&db).await.unwrap();
        assert_eq!(scope.server_ids, vec!["b".to_string(), "a".to_string()]);
    }

    #[tokio::test]
    async fn resolve_scope_rejects_invalid_json() {
        let (db, _tmp) = setup_test_db().await;
        set_config(&db, true, "{not valid json", None).await;
        let err = resolve_scope(&db).await;
        assert!(matches!(err, Err(AppError::Internal(_))));
    }

    #[test]
    fn public_scope_contains_matches_exact_id() {
        let now = Utc::now();
        let cfg = status_page::Model {
            id: "sp".into(),
            title: "t".into(),
            description: None,
            server_ids_json: "[]".into(),
            group_by_server_group: false,
            enabled: true,
            uptime_yellow_threshold: 99.0,
            uptime_red_threshold: 95.0,
            show_ip_quality: false,
            default_layout: "grid".into(),
            show_server_detail: true,
            show_network: false,
            show_incidents: true,
            show_maintenance: true,
            created_at: now,
            updated_at: now,
        };
        let scope = PublicScope {
            config: cfg,
            server_ids: vec!["x".into(), "y".into()],
        };
        assert!(scope.contains("x"));
        assert!(scope.contains("y"));
        assert!(!scope.contains("z"));
    }

    // ---------------------------------------------------------------------
    // uptime_percent (pure helper)
    // ---------------------------------------------------------------------

    #[test]
    fn uptime_percent_none_when_total_zero() {
        assert!(uptime_percent(&[]).is_none());
        let zero = vec![UptimeDailyEntry {
            date: chrono::NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
            total_minutes: 0,
            online_minutes: 0,
            downtime_incidents: 0,
        }];
        assert!(uptime_percent(&zero).is_none());
    }

    #[test]
    fn uptime_percent_computes_ratio() {
        let daily = vec![
            UptimeDailyEntry {
                date: chrono::NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
                total_minutes: 100,
                online_minutes: 90,
                downtime_incidents: 1,
            },
            UptimeDailyEntry {
                date: chrono::NaiveDate::from_ymd_opt(2026, 6, 2).unwrap(),
                total_minutes: 100,
                online_minutes: 100,
                downtime_incidents: 0,
            },
        ];
        let pct = uptime_percent(&daily).unwrap();
        assert!((pct - 95.0).abs() < 1e-9, "got {pct}");
    }

    // ---------------------------------------------------------------------
    // list_servers
    // ---------------------------------------------------------------------

    #[tokio::test]
    async fn list_servers_empty_scope_returns_empty() {
        let (db, _tmp) = setup_test_db().await;
        set_config(&db, true, "[]", None).await;
        let mgr = make_manager();
        let out = list_servers(&db, &mgr).await.expect("list ok");
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn list_servers_offline_has_no_metrics() {
        let (db, _tmp) = setup_test_db().await;
        seed_server(&db, "s1", "Server One", false, None).await;
        set_config(&db, true, r#"["s1"]"#, None).await;
        let mgr = make_manager(); // no connection => offline, no report
        let out = list_servers(&db, &mgr).await.unwrap();
        assert_eq!(out.len(), 1);
        let s = &out[0];
        assert_eq!(s.id, "s1");
        assert_eq!(s.name, "Server One");
        assert!(!s.online);
        assert!(!s.in_maintenance);
        assert!(s.metrics.is_none(), "no report => no metrics");
        assert_eq!(s.public_remark.as_deref(), Some("public note"));
        assert!(s.group_name.is_none());
    }

    #[tokio::test]
    async fn list_servers_online_with_metrics_and_group() {
        let (db, _tmp) = setup_test_db().await;
        // Seed a group and a server in it.
        server_group::ActiveModel {
            id: Set("g1".into()),
            name: Set("Production".into()),
            weight: Set(0),
            created_at: Set(Utc::now()),
        }
        .insert(&db)
        .await
        .unwrap();
        seed_server(&db, "s1", "Server One", false, Some("g1")).await;
        set_config(&db, true, r#"["s1"]"#, None).await;

        let mgr = make_manager();
        let (tx, _rx) = tokio::sync::mpsc::channel(8);
        mgr.add_connection("s1".into(), "Server One".into(), tx, sock());
        mgr.update_report("s1", sample_report());

        let out = list_servers(&db, &mgr).await.unwrap();
        assert_eq!(out.len(), 1);
        let s = &out[0];
        assert!(s.online);
        assert_eq!(s.group_name.as_deref(), Some("Production"));
        let m = s.metrics.as_ref().expect("online server has metrics");
        assert_eq!(m.cpu, 42.5);
        assert_eq!(m.mem_used, 4_000);
        assert_eq!(m.mem_total, 8_000);
        assert_eq!(m.disk_total, 100_000);
        assert_eq!(m.tcp_conn, 12);
        assert_eq!(m.process_count, 88);

        // Redaction: serialize the summary and prove no IP/identity leak fields.
        let json = serde_json::to_value(s).unwrap();
        for leaked in ["ipv4", "ipv6", "hostname", "remark", "last_remote_addr", "fingerprint"] {
            assert!(
                json.get(leaked).is_none(),
                "public summary must not expose `{leaked}`"
            );
        }
    }

    #[tokio::test]
    async fn list_servers_reflects_active_maintenance() {
        let (db, _tmp) = setup_test_db().await;
        seed_server(&db, "s1", "Server One", false, None).await;
        set_config(&db, true, r#"["s1"]"#, None).await;

        let now = Utc::now();
        maintenance::ActiveModel {
            id: Set("m1".into()),
            title: Set("DB upgrade".into()),
            description: Set(None),
            start_at: Set(now - Duration::hours(1)),
            end_at: Set(now + Duration::hours(1)),
            // No server filter => applies to all servers.
            server_ids_json: Set(None),
            is_public: Set(true),
            active: Set(true),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&db)
        .await
        .unwrap();

        let mgr = make_manager();
        let out = list_servers(&db, &mgr).await.unwrap();
        assert_eq!(out.len(), 1);
        assert!(out[0].in_maintenance, "server should be in maintenance");
    }

    // ---------------------------------------------------------------------
    // get_server_detail
    // ---------------------------------------------------------------------

    #[tokio::test]
    async fn get_server_detail_out_of_scope_is_not_found() {
        let (db, _tmp) = setup_test_db().await;
        seed_server(&db, "s1", "Server One", false, None).await;
        set_config(&db, true, "[]", None).await; // s1 not selected
        let mgr = make_manager();
        let err = get_server_detail(&db, &mgr, "s1").await;
        assert!(matches!(err, Err(AppError::NotFound(_))));
    }

    #[tokio::test]
    async fn get_server_detail_offline_omits_runtime_fields() {
        let (db, _tmp) = setup_test_db().await;
        seed_server(&db, "s1", "Server One", false, None).await;
        set_config(&db, true, r#"["s1"]"#, None).await;
        let mgr = make_manager(); // offline, no report

        let detail = get_server_detail(&db, &mgr, "s1").await.unwrap();
        assert_eq!(detail.summary.id, "s1");
        assert!(!detail.summary.online);
        // Static hardware fields come from the row.
        assert_eq!(detail.cpu_name.as_deref(), Some("TestCPU"));
        assert_eq!(detail.cpu_cores, Some(4));
        assert_eq!(detail.cpu_arch.as_deref(), Some("x86_64"));
        assert_eq!(detail.kernel_version.as_deref(), Some("6.1.0"));
        assert_eq!(detail.agent_version.as_deref(), Some("1.2.3"));
        assert_eq!(detail.mem_total, Some(8_000));
        assert_eq!(detail.disk_total, Some(100_000));
        // Runtime fields are absent without a report.
        assert!(detail.process_count.is_none());
        assert!(detail.tcp_conn.is_none());
        assert!(detail.udp_conn.is_none());
        assert!(detail.summary.metrics.is_none());
    }

    #[tokio::test]
    async fn get_server_detail_online_includes_runtime_fields_and_group() {
        let (db, _tmp) = setup_test_db().await;
        server_group::ActiveModel {
            id: Set("g1".into()),
            name: Set("Edge".into()),
            weight: Set(0),
            created_at: Set(Utc::now()),
        }
        .insert(&db)
        .await
        .unwrap();
        seed_server(&db, "s1", "Server One", false, Some("g1")).await;
        set_config(&db, true, r#"["s1"]"#, None).await;

        let mgr = make_manager();
        let (tx, _rx) = tokio::sync::mpsc::channel(8);
        mgr.add_connection("s1".into(), "Server One".into(), tx, sock());
        mgr.update_report("s1", sample_report());

        let detail = get_server_detail(&db, &mgr, "s1").await.unwrap();
        assert!(detail.summary.online);
        assert_eq!(detail.summary.group_name.as_deref(), Some("Edge"));
        assert_eq!(detail.process_count, Some(88));
        assert_eq!(detail.tcp_conn, Some(12));
        assert_eq!(detail.udp_conn, Some(3));
        assert!(detail.summary.metrics.is_some());

        // Redaction at the detail level too.
        let json = serde_json::to_value(&detail).unwrap();
        for leaked in ["ipv4", "ipv6", "remark", "fingerprint", "last_remote_addr"] {
            assert!(json.get(leaked).is_none(), "detail leaks `{leaked}`");
        }
    }

    // ---------------------------------------------------------------------
    // get_server_metrics
    // ---------------------------------------------------------------------

    #[tokio::test]
    async fn get_server_metrics_out_of_scope_is_not_found() {
        let (db, _tmp) = setup_test_db().await;
        seed_server(&db, "s1", "Server One", false, None).await;
        set_config(&db, true, "[]", None).await;
        let range = PublicMetricsRangeQuery {
            from: Utc::now() - Duration::hours(1),
            to: Utc::now(),
            interval: "raw".into(),
        };
        let err = get_server_metrics(&db, "s1", range).await;
        assert!(matches!(err, Err(AppError::NotFound(_))));
    }

    #[tokio::test]
    async fn get_server_metrics_inverted_range_is_validation_error() {
        let (db, _tmp) = setup_test_db().await;
        seed_server(&db, "s1", "Server One", false, None).await;
        set_config(&db, true, r#"["s1"]"#, None).await;
        let now = Utc::now();
        let range = PublicMetricsRangeQuery {
            from: now, // from after to
            to: now - Duration::hours(1),
            interval: "raw".into(),
        };
        let err = get_server_metrics(&db, "s1", range).await;
        assert!(matches!(err, Err(AppError::Validation(_))));
    }

    #[tokio::test]
    async fn get_server_metrics_in_scope_no_records_returns_empty() {
        let (db, _tmp) = setup_test_db().await;
        seed_server(&db, "s1", "Server One", false, None).await;
        set_config(&db, true, r#"["s1"]"#, None).await;
        let range = PublicMetricsRangeQuery {
            from: Utc::now() - Duration::hours(2),
            to: Utc::now(),
            interval: "raw".into(),
        };
        let points = get_server_metrics(&db, "s1", range).await.unwrap();
        assert!(points.is_empty(), "no seeded records => empty series");
    }

    // ---------------------------------------------------------------------
    // get_server_uptime_daily
    // ---------------------------------------------------------------------

    #[tokio::test]
    async fn get_server_uptime_daily_out_of_scope_is_not_found() {
        let (db, _tmp) = setup_test_db().await;
        seed_server(&db, "s1", "Server One", false, None).await;
        set_config(&db, true, "[]", None).await;
        let err = get_server_uptime_daily(&db, "s1").await;
        assert!(matches!(err, Err(AppError::NotFound(_))));
    }

    #[tokio::test]
    async fn get_server_uptime_daily_in_scope_returns_filled_band() {
        let (db, _tmp) = setup_test_db().await;
        seed_server(&db, "s1", "Server One", false, None).await;
        set_config(&db, true, r#"["s1"]"#, None).await;
        // get_daily_filled returns a 90-entry band even without rows.
        let band = get_server_uptime_daily(&db, "s1").await.unwrap();
        assert_eq!(band.len(), 90);
    }

    // ---------------------------------------------------------------------
    // ip_quality_overview (redaction)
    // ---------------------------------------------------------------------

    #[tokio::test]
    async fn ip_quality_overview_redacts_and_lists_services() {
        let (db, _tmp) = setup_test_db().await;
        seed_server(&db, "s1", "Server One", false, None).await;
        set_config(&db, true, r#"["s1"]"#, None).await;

        // Migrations seed the built-in unlock-service catalog; clear it so this
        // test controls exactly which services exist.
        unlock_service::Entity::delete_many()
            .exec(&db)
            .await
            .unwrap();

        // Enabled service appears in the catalog; disabled is filtered out.
        let now = Utc::now();
        unlock_service::ActiveModel {
            id: Set("svc-on".into()),
            key: Set("netflix".into()),
            name: Set("Netflix".into()),
            category: Set("streaming".into()),
            popularity: Set(100),
            is_builtin: Set(true),
            enabled: Set(true),
            detector: Set(None),
            request: Set(None),
            rules: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&db)
        .await
        .unwrap();
        unlock_service::ActiveModel {
            id: Set("svc-off".into()),
            key: Set("disney".into()),
            name: Set("Disney".into()),
            category: Set("streaming".into()),
            popularity: Set(50),
            is_builtin: Set(false),
            enabled: Set(false),
            detector: Set(None),
            request: Set(None),
            rules: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&db)
        .await
        .unwrap();

        let overview = ip_quality_overview(&db).await.unwrap();
        // One entry per in-scope server; no snapshot/results seeded.
        assert_eq!(overview.entries.len(), 1);
        let e = &overview.entries[0];
        assert_eq!(e.server_id, "s1");
        assert!(e.ip_quality.is_none(), "no snapshot => None (redacted shape)");
        assert!(e.unlock_results.is_empty());

        // Service catalog excludes the disabled service.
        assert_eq!(overview.services.len(), 1);
        assert_eq!(overview.services[0].id, "svc-on");
        assert_eq!(overview.services[0].key, "netflix");
        assert!(overview.services[0].is_builtin);
    }

    #[tokio::test]
    async fn ip_quality_overview_empty_scope_has_no_entries() {
        let (db, _tmp) = setup_test_db().await;
        set_config(&db, true, "[]", None).await;
        let overview = ip_quality_overview(&db).await.unwrap();
        assert!(overview.entries.is_empty());
    }

    // ---------------------------------------------------------------------
    // list_incidents
    // ---------------------------------------------------------------------

    async fn insert_incident(
        db: &DatabaseConnection,
        id: &str,
        status: &str,
        is_public: bool,
        resolved_at: Option<chrono::DateTime<Utc>>,
    ) {
        let now = Utc::now();
        incident::ActiveModel {
            id: Set(id.into()),
            title: Set(format!("Incident {id}")),
            status: Set(status.into()),
            severity: Set("major".into()),
            server_ids_json: Set(None),
            is_public: Set(is_public),
            created_at: Set(now),
            updated_at: Set(now),
            resolved_at: Set(resolved_at),
        }
        .insert(db)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn list_incidents_empty_when_none_public() {
        let (db, _tmp) = setup_test_db().await;
        // Private incident must not surface.
        insert_incident(&db, "i1", "investigating", false, None).await;
        let (active, recent) = list_incidents(&db).await.unwrap();
        assert!(active.is_empty());
        assert!(recent.is_empty());
    }

    #[tokio::test]
    async fn list_incidents_buckets_active_recent_and_drops_old() {
        let (db, _tmp) = setup_test_db().await;
        let now = Utc::now();
        // Active: non-resolved.
        insert_incident(&db, "active1", "investigating", true, None).await;
        // Recent: resolved within 7 days.
        insert_incident(
            &db,
            "recent1",
            "resolved",
            true,
            Some(now - Duration::days(2)),
        )
        .await;
        // Old: resolved more than 7 days ago => dropped from both buckets.
        insert_incident(
            &db,
            "old1",
            "resolved",
            true,
            Some(now - Duration::days(30)),
        )
        .await;

        // Seed an update for the active incident; it must be attached.
        incident_update::ActiveModel {
            id: Set("u1".into()),
            incident_id: Set("active1".into()),
            status: Set("investigating".into()),
            message: Set("Looking into it".into()),
            created_at: Set(now),
        }
        .insert(&db)
        .await
        .unwrap();

        let (active, recent) = list_incidents(&db).await.unwrap();
        assert_eq!(active.len(), 1, "one active incident");
        assert_eq!(active[0].id, "active1");
        assert_eq!(active[0].updates.len(), 1);
        assert_eq!(active[0].updates[0].message, "Looking into it");

        assert_eq!(recent.len(), 1, "one recent resolved incident");
        assert_eq!(recent[0].id, "recent1");
        assert!(recent[0].resolved_at.is_some());
    }

    // ---------------------------------------------------------------------
    // list_maintenances
    // ---------------------------------------------------------------------

    async fn insert_maintenance(
        db: &DatabaseConnection,
        id: &str,
        is_public: bool,
        active: bool,
        end_offset: Duration,
    ) {
        let now = Utc::now();
        maintenance::ActiveModel {
            id: Set(id.into()),
            title: Set(format!("Maint {id}")),
            description: Set(Some("desc".into())),
            start_at: Set(now - Duration::hours(1)),
            end_at: Set(now + end_offset),
            server_ids_json: Set(None),
            is_public: Set(is_public),
            active: Set(active),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(db)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn list_maintenances_filters_non_public_inactive_and_past() {
        let (db, _tmp) = setup_test_db().await;
        // Visible: public + active + ends in the future.
        insert_maintenance(&db, "ok", true, true, Duration::hours(2)).await;
        // Excluded: not public.
        insert_maintenance(&db, "private", false, true, Duration::hours(2)).await;
        // Excluded: inactive.
        insert_maintenance(&db, "inactive", true, false, Duration::hours(2)).await;
        // Excluded: already ended.
        insert_maintenance(&db, "past", true, true, -Duration::hours(2)).await;

        let out = list_maintenances(&db).await.unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "ok");
        assert_eq!(out[0].title, "Maint ok");
        assert_eq!(out[0].description.as_deref(), Some("desc"));
    }

    #[tokio::test]
    async fn list_maintenances_empty_when_none() {
        let (db, _tmp) = setup_test_db().await;
        let out = list_maintenances(&db).await.unwrap();
        assert!(out.is_empty());
    }
}
