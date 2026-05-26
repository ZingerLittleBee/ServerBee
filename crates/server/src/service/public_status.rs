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
    disk_total: i64,
) -> PublicMetricsSummary {
    PublicMetricsSummary {
        cpu: report.cpu,
        mem_used: report.mem_used.max(0) as u64,
        mem_total: mem_total.max(0) as u64,
        disk_used: report.disk_used.max(0) as u64,
        disk_total: disk_total.max(0) as u64,
        net_in_speed: report.net_in_speed.max(0) as u64,
        net_out_speed: report.net_out_speed.max(0) as u64,
        load_1: report.load1,
        load_5: report.load5,
        load_15: report.load15,
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

        let metrics = if online {
            agent_manager.get_latest_report(&srv.id).map(|r| {
                report_to_metrics(&r, srv.mem_total.unwrap_or(0), srv.disk_total.unwrap_or(0))
            })
        } else {
            None
        };

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

    let latest = if online {
        agent_manager.get_latest_report(&srv.id)
    } else {
        None
    };

    let metrics = latest
        .as_ref()
        .map(|r| report_to_metrics(r, srv.mem_total.unwrap_or(0), srv.disk_total.unwrap_or(0)));

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

    let result =
        RecordService::query_history(db, id, range.from, range.to, &range.interval).await?;

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
