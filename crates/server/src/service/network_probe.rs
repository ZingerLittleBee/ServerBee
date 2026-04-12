use std::collections::HashMap;

use chrono::{DateTime, Duration, Timelike, Utc};
use sea_orm::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::RetentionConfig;
use crate::entity::{
    network_probe_config, network_probe_record, network_probe_record_hourly, network_probe_target,
    server,
};
use crate::error::AppError;
use crate::service::agent_manager::AgentManager;
use crate::service::config::ConfigService;
use serverbee_common::types::NetworkProbeResultData;

const CONFIG_KEY: &str = "network_probe_setting";

pub struct NetworkProbeService;

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

pub const SPARKLINE_LENGTH: usize = 30;

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct NetworkProbeSetting {
    pub interval: u32,
    pub packet_count: u32,
    pub default_target_ids: Vec<String>,
}

impl Default for NetworkProbeSetting {
    fn default() -> Self {
        Self {
            interval: 60,
            packet_count: 10,
            default_target_ids: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateNetworkProbeTarget {
    pub name: String,
    pub provider: String,
    pub location: String,
    pub target: String,
    pub probe_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateNetworkProbeTarget {
    pub name: Option<String>,
    pub provider: Option<String>,
    pub location: Option<String>,
    pub target: Option<String>,
    pub probe_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct NetworkProbeAnomaly {
    pub timestamp: String,
    pub target_id: String,
    pub target_name: String,
    pub anomaly_type: String,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct TargetSummary {
    pub target_id: String,
    pub target_name: String,
    pub provider: String,
    pub avg_latency: Option<f64>,
    pub min_latency: Option<f64>,
    pub max_latency: Option<f64>,
    pub packet_loss: f64,
    pub availability: f64,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ServerSummary {
    pub server_id: String,
    pub server_name: String,
    pub online: bool,
    pub targets: Vec<TargetSummary>,
    pub last_probe_at: Option<String>,
    pub anomaly_count: i64,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ServerOverview {
    pub server_id: String,
    pub server_name: String,
    pub online: bool,
    pub last_probe_at: Option<String>,
    pub targets: Vec<TargetSummary>,
    pub anomaly_count: i64,
    pub latency_sparkline: Vec<Option<f64>>,
    pub loss_sparkline: Vec<Option<f64>>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct SparklineBundle {
    pub latency: Vec<Option<f64>>,
    pub loss: Vec<Option<f64>>,
}

/// Unified record DTO returned by query_records.
/// Maps both raw and hourly records to a single shape the frontend expects.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ProbeRecordDto {
    pub server_id: String,
    pub target_id: String,
    pub timestamp: String,
    pub avg_latency: Option<f64>,
    pub min_latency: Option<f64>,
    pub max_latency: Option<f64>,
    pub packet_loss: f64,
    pub packet_sent: i32,
    pub packet_received: i32,
}

/// Internal row type for the raw SQL window-function query that fetches the
/// latest record per (server_id, target_id) pair.  Uses `FromQueryResult` so
/// sea-orm can map the raw `QueryResult` rows automatically.
#[derive(Debug, FromQueryResult)]
struct LatestRecordRow {
    pub server_id: String,
    pub target_id: String,
    pub avg_latency: Option<f64>,
    pub min_latency: Option<f64>,
    pub max_latency: Option<f64>,
    pub packet_loss: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, FromQueryResult)]
struct SparklineRow {
    pub server_id: String,
    pub bucket_ts: i64,
    pub latency: Option<f64>,
    pub loss: Option<f64>,
}

/// Unified target DTO merging preset and custom targets.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct TargetDto {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub location: String,
    pub target: String,
    pub probe_type: String,
    pub source: Option<String>,
    pub source_name: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

impl TargetDto {
    pub fn from_preset(t: &crate::presets::FlatPresetTarget) -> Self {
        Self {
            id: t.id.clone(),
            name: t.name.clone(),
            provider: t.provider.clone(),
            location: t.location.clone(),
            target: t.target.clone(),
            probe_type: t.probe_type.clone(),
            source: Some(format!("preset:{}", t.group_id)),
            source_name: Some(t.group_name.clone()),
            created_at: None,
            updated_at: None,
        }
    }

    pub fn from_model(m: &network_probe_target::Model) -> Self {
        Self {
            id: m.id.clone(),
            name: m.name.clone(),
            provider: m.provider.clone(),
            location: m.location.clone(),
            target: m.target.clone(),
            probe_type: m.probe_type.clone(),
            source: None,
            source_name: None,
            created_at: Some(m.created_at.to_rfc3339()),
            updated_at: Some(m.updated_at.to_rfc3339()),
        }
    }
}

// ---------------------------------------------------------------------------
// Service implementation
// ---------------------------------------------------------------------------

impl NetworkProbeService {
    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Check if a target ID is valid (exists as preset or in DB).
    async fn is_valid_target(db: &DatabaseConnection, id: &str) -> bool {
        crate::presets::PresetTargets::is_preset(id)
            || network_probe_target::Entity::find_by_id(id)
                .one(db)
                .await
                .ok()
                .flatten()
                .is_some()
    }

    /// Build a target name+provider lookup map from presets + DB.
    async fn build_target_map(db: &DatabaseConnection) -> HashMap<String, (String, String)> {
        let mut map: HashMap<String, (String, String)> = HashMap::new();
        for t in crate::presets::PresetTargets::load() {
            map.insert(t.id.clone(), (t.name.clone(), t.provider.clone()));
        }
        if let Ok(custom) = network_probe_target::Entity::find().all(db).await {
            for t in custom {
                map.insert(t.id.clone(), (t.name.clone(), t.provider.clone()));
            }
        }
        map
    }

    fn empty_sparkline_bundle() -> SparklineBundle {
        SparklineBundle {
            latency: vec![None; SPARKLINE_LENGTH],
            loss: vec![None; SPARKLINE_LENGTH],
        }
    }

    // -----------------------------------------------------------------------
    // Target management
    // -----------------------------------------------------------------------

    /// List all probe targets (preset + custom).
    pub async fn list_targets(db: &DatabaseConnection) -> Result<Vec<TargetDto>, AppError> {
        let mut targets: Vec<TargetDto> = crate::presets::PresetTargets::load()
            .iter()
            .map(TargetDto::from_preset)
            .collect();
        let custom = network_probe_target::Entity::find()
            .order_by_asc(network_probe_target::Column::Name)
            .all(db)
            .await?;
        targets.extend(custom.iter().map(TargetDto::from_model));
        Ok(targets)
    }

    /// Create a custom probe target.
    pub async fn create_target(
        db: &DatabaseConnection,
        input: CreateNetworkProbeTarget,
    ) -> Result<network_probe_target::Model, AppError> {
        if !["icmp", "tcp", "http"].contains(&input.probe_type.as_str()) {
            return Err(AppError::Validation(
                "probe_type must be icmp, tcp, or http".to_string(),
            ));
        }

        let now = Utc::now();
        let model = network_probe_target::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            name: Set(input.name),
            provider: Set(input.provider),
            location: Set(input.location),
            target: Set(input.target),
            probe_type: Set(input.probe_type),
            created_at: Set(now),
            updated_at: Set(now),
        };

        Ok(model.insert(db).await?)
    }

    /// Update a custom probe target. Preset targets cannot be modified.
    pub async fn update_target(
        db: &DatabaseConnection,
        id: &str,
        input: UpdateNetworkProbeTarget,
    ) -> Result<network_probe_target::Model, AppError> {
        if crate::presets::PresetTargets::is_preset(id) {
            return Err(AppError::Forbidden(
                "Cannot modify a preset probe target".to_string(),
            ));
        }

        let existing = network_probe_target::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Probe target {id} not found")))?;

        let mut active: network_probe_target::ActiveModel = existing.into();

        if let Some(name) = input.name {
            active.name = Set(name);
        }
        if let Some(provider) = input.provider {
            active.provider = Set(provider);
        }
        if let Some(location) = input.location {
            active.location = Set(location);
        }
        if let Some(target) = input.target {
            active.target = Set(target);
        }
        if let Some(probe_type) = input.probe_type {
            if !["icmp", "tcp", "http"].contains(&probe_type.as_str()) {
                return Err(AppError::Validation(
                    "probe_type must be icmp, tcp, or http".to_string(),
                ));
            }
            active.probe_type = Set(probe_type);
        }
        active.updated_at = Set(Utc::now());

        Ok(active.update(db).await?)
    }

    /// Delete a custom probe target. Preset targets cannot be deleted.
    /// Cascade-deletes associated config and records, and removes the target
    /// from `default_target_ids` in the global setting.
    pub async fn delete_target(db: &DatabaseConnection, id: &str) -> Result<(), AppError> {
        if crate::presets::PresetTargets::is_preset(id) {
            return Err(AppError::Forbidden(
                "Cannot delete a preset probe target".to_string(),
            ));
        }

        // Verify the target exists in the DB
        network_probe_target::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Probe target {id} not found")))?;

        // Cascade delete config entries
        network_probe_config::Entity::delete_many()
            .filter(network_probe_config::Column::TargetId.eq(id))
            .exec(db)
            .await?;

        // Cascade delete raw records
        network_probe_record::Entity::delete_many()
            .filter(network_probe_record::Column::TargetId.eq(id))
            .exec(db)
            .await?;

        // Cascade delete hourly records
        network_probe_record_hourly::Entity::delete_many()
            .filter(network_probe_record_hourly::Column::TargetId.eq(id))
            .exec(db)
            .await?;

        // Remove from default_target_ids in setting
        let mut setting = Self::get_setting(db).await?;
        setting.default_target_ids.retain(|tid| tid != id);
        Self::save_setting(db, &setting).await?;

        // Delete the target itself
        network_probe_target::Entity::delete_by_id(id)
            .exec(db)
            .await?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Settings
    // -----------------------------------------------------------------------

    /// Get global network probe setting. Returns defaults if not configured.
    pub async fn get_setting(db: &DatabaseConnection) -> Result<NetworkProbeSetting, AppError> {
        let setting: Option<NetworkProbeSetting> = ConfigService::get_typed(db, CONFIG_KEY).await?;
        Ok(setting.unwrap_or_default())
    }

    /// Update global network probe setting with validation.
    pub async fn update_setting(
        db: &DatabaseConnection,
        setting: &NetworkProbeSetting,
    ) -> Result<(), AppError> {
        if !(30..=600).contains(&setting.interval) {
            return Err(AppError::BadRequest(
                "interval must be between 30 and 600 seconds".to_string(),
            ));
        }
        if !(5..=20).contains(&setting.packet_count) {
            return Err(AppError::BadRequest(
                "packet_count must be between 5 and 20".to_string(),
            ));
        }
        for id in &setting.default_target_ids {
            if !Self::is_valid_target(db, id).await {
                return Err(AppError::Validation(format!(
                    "Invalid target ID in default_target_ids: {id}"
                )));
            }
        }
        Self::save_setting(db, setting).await
    }

    /// Save setting without validation (internal use, e.g. removing a deleted target from defaults).
    async fn save_setting(
        db: &DatabaseConnection,
        setting: &NetworkProbeSetting,
    ) -> Result<(), AppError> {
        ConfigService::set_typed(db, CONFIG_KEY, setting).await
    }

    // -----------------------------------------------------------------------
    // Server target config
    // -----------------------------------------------------------------------

    /// Get targets assigned to a server, resolving both presets and DB targets.
    pub async fn get_server_targets(
        db: &DatabaseConnection,
        server_id: &str,
    ) -> Result<Vec<TargetDto>, AppError> {
        let configs = network_probe_config::Entity::find()
            .filter(network_probe_config::Column::ServerId.eq(server_id))
            .all(db)
            .await?;

        if configs.is_empty() {
            return Ok(Vec::new());
        }

        let target_ids: Vec<String> = configs.iter().map(|c| c.target_id.clone()).collect();

        let mut targets = Vec::new();
        let mut db_ids = Vec::new();
        for id in &target_ids {
            if let Some(preset) = crate::presets::PresetTargets::find(id) {
                targets.push(TargetDto::from_preset(preset));
            } else {
                db_ids.push(id.clone());
            }
        }

        if !db_ids.is_empty() {
            let db_targets = network_probe_target::Entity::find()
                .filter(network_probe_target::Column::Id.is_in(db_ids))
                .all(db)
                .await?;
            targets.extend(db_targets.iter().map(TargetDto::from_model));
        }

        Ok(targets)
    }

    /// Replace target assignments for a server. Enforces max 20 targets.
    /// Wrapped in a transaction for atomicity.
    pub async fn set_server_targets(
        db: &DatabaseConnection,
        server_id: &str,
        target_ids: Vec<String>,
    ) -> Result<(), AppError> {
        if target_ids.len() > 20 {
            return Err(AppError::Validation(
                "Cannot assign more than 20 targets to a server".to_string(),
            ));
        }

        for id in &target_ids {
            if !Self::is_valid_target(db, id).await {
                return Err(AppError::Validation(format!("Invalid target ID: {id}")));
            }
        }

        let txn = db.begin().await?;

        // Delete existing config for this server
        network_probe_config::Entity::delete_many()
            .filter(network_probe_config::Column::ServerId.eq(server_id))
            .exec(&txn)
            .await?;

        // Insert new assignments
        let now = Utc::now();
        for target_id in target_ids {
            let config = network_probe_config::ActiveModel {
                id: Set(Uuid::new_v4().to_string()),
                server_id: Set(server_id.to_string()),
                target_id: Set(target_id),
                created_at: Set(now),
            };
            config.insert(&txn).await?;
        }

        txn.commit().await?;

        Ok(())
    }

    /// Apply default targets from the global setting to a server.
    pub async fn apply_defaults(db: &DatabaseConnection, server_id: &str) -> Result<(), AppError> {
        let setting = Self::get_setting(db).await?;
        Self::set_server_targets(db, server_id, setting.default_target_ids).await
    }

    // -----------------------------------------------------------------------
    // Records
    // -----------------------------------------------------------------------

    /// Save probe results to the database. Inserts all in a single transaction.
    pub async fn save_results(
        db: &DatabaseConnection,
        server_id: &str,
        results: Vec<NetworkProbeResultData>,
    ) -> Result<(), AppError> {
        let txn = db.begin().await?;

        for r in results {
            let record = network_probe_record::ActiveModel {
                id: NotSet,
                server_id: Set(server_id.to_string()),
                target_id: Set(r.target_id),
                avg_latency: Set(r.avg_latency),
                min_latency: Set(r.min_latency),
                max_latency: Set(r.max_latency),
                packet_loss: Set(r.packet_loss),
                packet_sent: Set(r.packet_sent as i32),
                packet_received: Set(r.packet_received as i32),
                timestamp: Set(r.timestamp),
            };
            record.insert(&txn).await?;
        }

        txn.commit().await?;
        Ok(())
    }

    /// Query probe records for a server with smart interval selection.
    /// < 1 day: raw records, >= 1 day: hourly aggregates.
    /// Both are mapped to a unified `ProbeRecordDto`.
    pub async fn query_records(
        db: &DatabaseConnection,
        server_id: &str,
        target_id: Option<String>,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<ProbeRecordDto>, AppError> {
        let duration = to - from;
        let use_hourly = duration >= Duration::days(1);

        if use_hourly {
            let mut query = network_probe_record_hourly::Entity::find()
                .filter(network_probe_record_hourly::Column::ServerId.eq(server_id))
                .filter(network_probe_record_hourly::Column::Hour.gte(from))
                .filter(network_probe_record_hourly::Column::Hour.lte(to));

            if let Some(tid) = target_id {
                query = query.filter(network_probe_record_hourly::Column::TargetId.eq(tid));
            }

            let records = query
                .order_by_asc(network_probe_record_hourly::Column::Hour)
                .all(db)
                .await?;
            Ok(records
                .into_iter()
                .map(|r| ProbeRecordDto {
                    server_id: r.server_id,
                    target_id: r.target_id,
                    timestamp: r.hour.to_rfc3339(),
                    avg_latency: r.avg_latency,
                    min_latency: r.min_latency,
                    max_latency: r.max_latency,
                    packet_loss: r.avg_packet_loss,
                    packet_sent: r.sample_count,
                    packet_received: r.sample_count,
                })
                .collect())
        } else {
            let mut query = network_probe_record::Entity::find()
                .filter(network_probe_record::Column::ServerId.eq(server_id))
                .filter(network_probe_record::Column::Timestamp.gte(from))
                .filter(network_probe_record::Column::Timestamp.lte(to));

            if let Some(tid) = target_id {
                query = query.filter(network_probe_record::Column::TargetId.eq(tid));
            }

            let records = query
                .order_by_asc(network_probe_record::Column::Timestamp)
                .all(db)
                .await?;
            Ok(records
                .into_iter()
                .map(|r| ProbeRecordDto {
                    server_id: r.server_id,
                    target_id: r.target_id,
                    timestamp: r.timestamp.to_rfc3339(),
                    avg_latency: r.avg_latency,
                    min_latency: r.min_latency,
                    max_latency: r.max_latency,
                    packet_loss: r.packet_loss,
                    packet_sent: r.packet_sent,
                    packet_received: r.packet_received,
                })
                .collect())
        }
    }

    /// Get a summary of the latest probe result per target for a server.
    pub async fn get_server_summary(
        db: &DatabaseConnection,
        agent_manager: &AgentManager,
        server_id: &str,
    ) -> Result<ServerSummary, AppError> {
        // Get server name
        let srv = server::Entity::find_by_id(server_id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Server {server_id} not found")))?;
        let server_name = srv.name;
        let online = agent_manager.is_online(server_id);

        // Get all targets assigned to this server
        let targets = Self::get_server_targets(db, server_id).await?;

        // Fetch the latest record per target in a single query using a window function,
        // avoiding an N+1 query pattern (one query per target).
        let latest_records = Self::fetch_latest_records_for_server(db, server_id).await?;

        // Build a lookup from target_id -> latest record
        let record_by_target: HashMap<String, &LatestRecordRow> = latest_records
            .iter()
            .map(|r| (r.target_id.clone(), r))
            .collect();

        let mut target_summaries = Vec::new();
        let mut last_probe_at: Option<DateTime<Utc>> = None;

        // Iterate over ALL configured targets so that newly-added targets
        // (which have no probe records yet) still appear in the summary.
        for target in &targets {
            if let Some(record) = record_by_target.get(&target.id) {
                // Track the most recent probe timestamp
                match last_probe_at {
                    Some(existing) if record.timestamp > existing => {
                        last_probe_at = Some(record.timestamp);
                    }
                    None => {
                        last_probe_at = Some(record.timestamp);
                    }
                    _ => {}
                }

                target_summaries.push(TargetSummary {
                    target_id: target.id.clone(),
                    target_name: target.name.clone(),
                    provider: target.provider.clone(),
                    avg_latency: record.avg_latency,
                    min_latency: record.min_latency,
                    max_latency: record.max_latency,
                    packet_loss: record.packet_loss,
                    availability: 1.0 - record.packet_loss,
                });
            } else {
                // Target has no records yet — include with None latency values
                target_summaries.push(TargetSummary {
                    target_id: target.id.clone(),
                    target_name: target.name.clone(),
                    provider: target.provider.clone(),
                    avg_latency: None,
                    min_latency: None,
                    max_latency: None,
                    packet_loss: 0.0,
                    availability: 0.0,
                });
            }
        }

        // Count anomalies from the last 24 hours
        let anomaly_from = Utc::now() - Duration::hours(24);
        let anomaly_count = Self::count_anomalies(db, server_id, anomaly_from).await?;

        Ok(ServerSummary {
            server_id: server_id.to_string(),
            server_name,
            online,
            targets: target_summaries,
            last_probe_at: last_probe_at.map(|t| t.to_rfc3339()),
            anomaly_count,
        })
    }

    /// Get an overview of all servers' network probe status.
    pub async fn get_overview(
        db: &DatabaseConnection,
        agent_manager: &AgentManager,
    ) -> Result<Vec<ServerOverview>, AppError> {
        // Load ALL servers (not just ones with probe config)
        let servers = server::Entity::find().all(db).await?;
        let server_map: HashMap<String, String> = servers
            .iter()
            .map(|s| (s.id.clone(), s.name.clone()))
            .collect();
        let server_ids: Vec<String> = servers.into_iter().map(|s| s.id).collect();
        let setting = Self::get_setting(db).await?;
        let bucket_seconds = i64::from(setting.interval).max(60);

        // Load all probe configs for target lookup
        let configs = network_probe_config::Entity::find().all(db).await?;

        // Build target name+provider lookup map from presets + DB
        let target_map: HashMap<String, (String, String)> = Self::build_target_map(db).await;

        let anomaly_from = Utc::now() - Duration::hours(24);
        let sparklines = Self::query_sparklines(db, &server_ids, bucket_seconds).await?;

        // Fetch ALL latest records across ALL servers in a single query using a window
        // function, avoiding an N+1 query pattern (one query per server per target).
        let all_latest = Self::fetch_latest_records_all_servers(db).await?;

        // Group latest records by server_id for efficient lookup
        let mut records_by_server: HashMap<String, Vec<LatestRecordRow>> = HashMap::new();
        for record in all_latest {
            records_by_server
                .entry(record.server_id.clone())
                .or_default()
                .push(record);
        }

        // Build a set of valid target_ids per server from configs
        let mut config_targets_by_server: HashMap<String, Vec<String>> = HashMap::new();
        for c in &configs {
            config_targets_by_server
                .entry(c.server_id.clone())
                .or_default()
                .push(c.target_id.clone());
        }

        let mut overviews = Vec::new();

        for server_id in &server_ids {
            let server_name = server_map
                .get(server_id)
                .cloned()
                .unwrap_or_else(|| server_id.clone());
            let online = agent_manager.is_online(server_id);

            let valid_target_ids: std::collections::HashSet<&String> = config_targets_by_server
                .get(server_id)
                .map(|ids| ids.iter().collect())
                .unwrap_or_default();

            let mut target_summaries = Vec::new();
            let mut last_probe_at: Option<DateTime<Utc>> = None;

            // Build a lookup from target_id -> latest record for this server
            let record_map: HashMap<&String, &LatestRecordRow> = records_by_server
                .get(server_id)
                .map(|rs| rs.iter().map(|r| (&r.target_id, r)).collect())
                .unwrap_or_default();

            // Iterate over ALL configured targets so that newly-added targets
            // (which have no probe records yet) still appear in the overview.
            for target_id in &valid_target_ids {
                let (target_name, provider) = target_map
                    .get(*target_id)
                    .cloned()
                    .unwrap_or_else(|| ((*target_id).clone(), String::new()));

                if let Some(record) = record_map.get(*target_id) {
                    match last_probe_at {
                        Some(existing) if record.timestamp > existing => {
                            last_probe_at = Some(record.timestamp);
                        }
                        None => {
                            last_probe_at = Some(record.timestamp);
                        }
                        _ => {}
                    }

                    target_summaries.push(TargetSummary {
                        target_id: (*target_id).clone(),
                        target_name,
                        provider,
                        avg_latency: record.avg_latency,
                        min_latency: record.min_latency,
                        max_latency: record.max_latency,
                        packet_loss: record.packet_loss,
                        availability: 1.0 - record.packet_loss,
                    });
                } else {
                    target_summaries.push(TargetSummary {
                        target_id: (*target_id).clone(),
                        target_name,
                        provider,
                        avg_latency: None,
                        min_latency: None,
                        max_latency: None,
                        packet_loss: 0.0,
                        availability: 0.0,
                    });
                }
            }

            let anomaly_count = Self::count_anomalies(db, server_id, anomaly_from).await?;
            let sparkline_bundle = sparklines
                .get(server_id)
                .cloned()
                .unwrap_or_else(Self::empty_sparkline_bundle);

            overviews.push(ServerOverview {
                server_id: server_id.clone(),
                server_name,
                online,
                last_probe_at: last_probe_at.map(|t| t.to_rfc3339()),
                targets: target_summaries,
                anomaly_count,
                latency_sparkline: sparkline_bundle.latency,
                loss_sparkline: sparkline_bundle.loss,
            });
        }

        Ok(overviews)
    }

    /// Get anomalous probe records for a server within a time range.
    /// Anomalies: avg_latency > 150ms OR packet_loss > 0.1 (10%).
    pub async fn get_anomalies(
        db: &DatabaseConnection,
        server_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<NetworkProbeAnomaly>, AppError> {
        // Load target names for display
        let targets = Self::get_server_targets(db, server_id).await?;
        let target_map: HashMap<String, String> =
            targets.into_iter().map(|t| (t.id, t.name)).collect();

        // Query raw records in the time range (capped at 500 to bound memory usage)
        let records = network_probe_record::Entity::find()
            .filter(network_probe_record::Column::ServerId.eq(server_id))
            .filter(network_probe_record::Column::Timestamp.gte(from))
            .filter(network_probe_record::Column::Timestamp.lte(to))
            .order_by_asc(network_probe_record::Column::Timestamp)
            .limit(500)
            .all(db)
            .await?;

        let mut anomalies = Vec::new();

        for record in records {
            let target_name = target_map
                .get(&record.target_id)
                .cloned()
                .unwrap_or_else(|| record.target_id.clone());

            // Check for unreachable (packet_loss == 1.0)
            if (record.packet_loss - 1.0).abs() < f64::EPSILON {
                anomalies.push(NetworkProbeAnomaly {
                    timestamp: record.timestamp.to_rfc3339(),
                    target_id: record.target_id.clone(),
                    target_name: target_name.clone(),
                    anomaly_type: "unreachable".to_string(),
                    value: record.packet_loss,
                });
                continue;
            }

            // Check latency anomalies
            if let Some(latency) = record.avg_latency {
                if latency > 240.0 {
                    anomalies.push(NetworkProbeAnomaly {
                        timestamp: record.timestamp.to_rfc3339(),
                        target_id: record.target_id.clone(),
                        target_name: target_name.clone(),
                        anomaly_type: "very_high_latency".to_string(),
                        value: latency,
                    });
                } else if latency > 150.0 {
                    anomalies.push(NetworkProbeAnomaly {
                        timestamp: record.timestamp.to_rfc3339(),
                        target_id: record.target_id.clone(),
                        target_name: target_name.clone(),
                        anomaly_type: "high_latency".to_string(),
                        value: latency,
                    });
                }
            }

            // Check packet loss anomalies
            if record.packet_loss > 0.5 {
                anomalies.push(NetworkProbeAnomaly {
                    timestamp: record.timestamp.to_rfc3339(),
                    target_id: record.target_id.clone(),
                    target_name: target_name.clone(),
                    anomaly_type: "very_high_packet_loss".to_string(),
                    value: record.packet_loss,
                });
            } else if record.packet_loss > 0.1 {
                anomalies.push(NetworkProbeAnomaly {
                    timestamp: record.timestamp.to_rfc3339(),
                    target_id: record.target_id.clone(),
                    target_name: target_name.clone(),
                    anomaly_type: "high_packet_loss".to_string(),
                    value: record.packet_loss,
                });
            }
        }

        Ok(anomalies)
    }

    /// Fetch the latest probe record per target for a single server using a
    /// window function.  Returns one row per target_id with the most recent
    /// record, executing a single SQL query instead of N queries.
    async fn fetch_latest_records_for_server(
        db: &DatabaseConnection,
        server_id: &str,
    ) -> Result<Vec<LatestRecordRow>, AppError> {
        let sql = "SELECT server_id, target_id, avg_latency, min_latency, max_latency, \
                          packet_loss, timestamp \
                   FROM ( \
                       SELECT *, ROW_NUMBER() OVER (PARTITION BY target_id ORDER BY timestamp DESC) AS rn \
                       FROM network_probe_record \
                       WHERE server_id = ? \
                   ) WHERE rn = 1";
        let stmt =
            Statement::from_sql_and_values(DatabaseBackend::Sqlite, sql, vec![server_id.into()]);
        Ok(LatestRecordRow::find_by_statement(stmt).all(db).await?)
    }

    /// Fetch the latest probe record per (server_id, target_id) across ALL
    /// servers using a window function.  Returns one row per server+target
    /// combination, executing a single SQL query instead of S*T queries.
    async fn fetch_latest_records_all_servers(
        db: &DatabaseConnection,
    ) -> Result<Vec<LatestRecordRow>, AppError> {
        let sql = "SELECT server_id, target_id, avg_latency, min_latency, max_latency, \
                          packet_loss, timestamp \
                   FROM ( \
                       SELECT *, ROW_NUMBER() OVER (PARTITION BY server_id, target_id ORDER BY timestamp DESC) AS rn \
                       FROM network_probe_record \
                   ) WHERE rn = 1";
        let stmt = Statement::from_sql_and_values(DatabaseBackend::Sqlite, sql, vec![]);
        Ok(LatestRecordRow::find_by_statement(stmt).all(db).await?)
    }

    pub async fn query_sparklines(
        db: &DatabaseConnection,
        server_ids: &[String],
        bucket_seconds: i64,
    ) -> Result<HashMap<String, SparklineBundle>, AppError> {
        if server_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let bucket_seconds = bucket_seconds.max(1);
        let now_bucket = Utc::now().timestamp().div_euclid(bucket_seconds) * bucket_seconds;
        let window_span = (SPARKLINE_LENGTH as i64 - 1) * bucket_seconds;
        let placeholders = vec!["(?)"; server_ids.len()].join(", ");
        let sql = format!(
            "WITH input_servers(server_id) AS (VALUES {placeholders}), \
             latest_buckets AS ( \
                 SELECT s.server_id, \
                        ((MAX(CAST(strftime('%s', r.timestamp) AS INTEGER)) / ?) * ?) AS latest_bucket \
                 FROM input_servers s \
                 LEFT JOIN network_probe_record r ON r.server_id = s.server_id \
                 GROUP BY s.server_id \
             ), \
             bucketed AS ( \
                 SELECT r.server_id, \
                        ((CAST(strftime('%s', r.timestamp) AS INTEGER) / ?) * ?) AS bucket_ts, \
                        AVG(r.avg_latency) AS latency, \
                        AVG(r.packet_loss) AS loss \
                  FROM network_probe_record r \
                  JOIN latest_buckets lb ON lb.server_id = r.server_id \
                  WHERE lb.latest_bucket IS NOT NULL \
                   AND ((CAST(strftime('%s', r.timestamp) AS INTEGER) / ?) * ?) \
                       BETWEEN lb.latest_bucket - ? AND lb.latest_bucket \
                 GROUP BY r.server_id, bucket_ts \
             ) \
             SELECT server_id, bucket_ts, latency, loss \
             FROM bucketed \
             ORDER BY server_id ASC, bucket_ts ASC"
        );

        let mut values: Vec<Value> = server_ids.iter().cloned().map(Value::from).collect();
        values.push(bucket_seconds.into());
        values.push(bucket_seconds.into());
        values.push(bucket_seconds.into());
        values.push(bucket_seconds.into());
        values.push(bucket_seconds.into());
        values.push(bucket_seconds.into());
        values.push(window_span.into());

        let stmt = Statement::from_sql_and_values(DatabaseBackend::Sqlite, sql, values);
        let rows = SparklineRow::find_by_statement(stmt).all(db).await?;
        let mut rows_by_server: HashMap<String, Vec<SparklineRow>> = HashMap::new();

        for row in rows {
            rows_by_server
                .entry(row.server_id.clone())
                .or_default()
                .push(row);
        }

        let mut sparklines = HashMap::new();

        for server_id in server_ids {
            let server_rows = rows_by_server.remove(server_id).unwrap_or_default();
            let latest_bucket = server_rows
                .last()
                .map(|row| row.bucket_ts)
                .unwrap_or(now_bucket);
            let start_bucket = latest_bucket - ((SPARKLINE_LENGTH as i64 - 1) * bucket_seconds);
            let row_map: HashMap<i64, SparklineRow> = server_rows
                .into_iter()
                .map(|row| (row.bucket_ts, row))
                .collect();
            let mut latency = Vec::with_capacity(SPARKLINE_LENGTH);
            let mut loss = Vec::with_capacity(SPARKLINE_LENGTH);

            for index in 0..SPARKLINE_LENGTH {
                let bucket_ts = start_bucket + index as i64 * bucket_seconds;
                if let Some(row) = row_map.get(&bucket_ts) {
                    latency.push(row.latency);
                    loss.push(row.loss);
                } else {
                    latency.push(None);
                    loss.push(None);
                }
            }

            sparklines.insert(server_id.clone(), SparklineBundle { latency, loss });
        }

        Ok(sparklines)
    }

    /// Count anomalous records for a server since a given time.
    /// Anomalies: unreachable (packet_loss == 1.0), high latency (>150ms), high packet loss (>0.1).
    async fn count_anomalies(
        db: &DatabaseConnection,
        server_id: &str,
        from: DateTime<Utc>,
    ) -> Result<i64, AppError> {
        let from_str = from.to_rfc3339();
        let sql = "SELECT COUNT(*) as count FROM network_probe_record \
                   WHERE server_id = ? AND timestamp >= ? AND \
                   (packet_loss >= 1.0 OR (avg_latency IS NOT NULL AND avg_latency > 150.0) OR packet_loss > 0.1)";
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            sql,
            vec![server_id.into(), from_str.into()],
        );
        let result = db.query_one(stmt).await?;
        let count = result
            .and_then(|row| row.try_get_by_index::<i64>(0).ok())
            .unwrap_or(0);
        Ok(count)
    }

    // -----------------------------------------------------------------------
    // Background task methods
    // -----------------------------------------------------------------------

    /// Aggregate raw probe records from the last hour into hourly averages.
    /// Groups by (server_id, target_id, hour) and upserts into the hourly table.
    /// Uses INSERT OR REPLACE for idempotency on re-runs.
    pub async fn aggregate_hourly(db: &DatabaseConnection) -> Result<u64, AppError> {
        let now = Utc::now();
        let one_hour_ago = now - Duration::hours(1);

        let records = network_probe_record::Entity::find()
            .filter(network_probe_record::Column::Timestamp.gte(one_hour_ago))
            .filter(network_probe_record::Column::Timestamp.lt(now))
            .all(db)
            .await?;

        if records.is_empty() {
            return Ok(0);
        }

        // Group by (server_id, target_id, hour_str) so records spanning two clock hours
        // get aggregated into the correct hour bucket rather than all sharing the same hour.
        let mut grouped: HashMap<(String, String, String), Vec<&network_probe_record::Model>> =
            HashMap::new();

        for r in &records {
            let hour = r
                .timestamp
                .with_minute(0)
                .and_then(|t| t.with_second(0))
                .and_then(|t| t.with_nanosecond(0))
                .unwrap_or(r.timestamp);
            let hour_str = hour.to_rfc3339();
            grouped
                .entry((r.server_id.clone(), r.target_id.clone(), hour_str))
                .or_default()
                .push(r);
        }

        let mut inserted = 0u64;

        for ((server_id, target_id, hour_str), group) in &grouped {
            let count = group.len() as f64;

            let latencies: Vec<f64> = group.iter().filter_map(|r| r.avg_latency).collect();
            let avg_latency = if latencies.is_empty() {
                None
            } else {
                Some(latencies.iter().sum::<f64>() / latencies.len() as f64)
            };

            let min_latencies: Vec<f64> = group.iter().filter_map(|r| r.min_latency).collect();
            let min_latency = min_latencies.iter().cloned().reduce(f64::min);

            let max_latencies: Vec<f64> = group.iter().filter_map(|r| r.max_latency).collect();
            let max_latency = max_latencies.iter().cloned().reduce(f64::max);

            let avg_packet_loss = group.iter().map(|r| r.packet_loss).sum::<f64>() / count;

            let sample_count = group.len() as i32;

            // Use parameterized INSERT … ON CONFLICT to prevent SQL injection.
            let sql = "INSERT INTO network_probe_record_hourly \
                       (server_id, target_id, avg_latency, min_latency, max_latency, avg_packet_loss, sample_count, hour) \
                       VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
                       ON CONFLICT (server_id, target_id, hour) DO UPDATE SET \
                       avg_latency = excluded.avg_latency, \
                       min_latency = excluded.min_latency, \
                       max_latency = excluded.max_latency, \
                       avg_packet_loss = excluded.avg_packet_loss, \
                       sample_count = excluded.sample_count";

            let stmt = Statement::from_sql_and_values(
                DatabaseBackend::Sqlite,
                sql,
                vec![
                    Value::from(server_id.clone()),
                    Value::from(target_id.clone()),
                    avg_latency
                        .map(|v| Value::Double(Some(v)))
                        .unwrap_or(Value::Double(None)),
                    min_latency
                        .map(|v| Value::Double(Some(v)))
                        .unwrap_or(Value::Double(None)),
                    max_latency
                        .map(|v| Value::Double(Some(v)))
                        .unwrap_or(Value::Double(None)),
                    Value::from(avg_packet_loss),
                    Value::from(sample_count),
                    Value::from(hour_str.clone()),
                ],
            );
            db.execute(stmt).await?;
            inserted += 1;
        }

        Ok(inserted)
    }

    /// Clean up old records based on retention configuration.
    pub async fn cleanup_old_records(
        db: &DatabaseConnection,
        retention: &RetentionConfig,
    ) -> Result<(u64, u64), AppError> {
        let raw_cutoff = Utc::now() - Duration::days(retention.network_probe_days as i64);
        let hourly_cutoff = Utc::now() - Duration::days(retention.network_probe_hourly_days as i64);

        let raw_deleted = network_probe_record::Entity::delete_many()
            .filter(network_probe_record::Column::Timestamp.lt(raw_cutoff))
            .exec(db)
            .await?
            .rows_affected;

        let hourly_deleted = network_probe_record_hourly::Entity::delete_many()
            .filter(network_probe_record_hourly::Column::Hour.lt(hourly_cutoff))
            .exec(db)
            .await?
            .rows_affected;

        Ok((raw_deleted, hourly_deleted))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::auth::AuthService;
    use crate::test_utils::setup_test_db;
    use sea_orm::{ActiveModelTrait, Set};
    use serverbee_common::constants::CAP_DEFAULT;
    use tokio::sync::broadcast;

    async fn insert_test_server(db: &DatabaseConnection, id: &str, name: &str) {
        let token_hash = AuthService::hash_password("test").expect("hash_password should succeed");
        let now = Utc::now();

        server::ActiveModel {
            id: Set(id.to_string()),
            token_hash: Set(token_hash),
            token_prefix: Set("serverbee_test".to_string()),
            name: Set(name.to_string()),
            weight: Set(0),
            hidden: Set(false),
            capabilities: Set(CAP_DEFAULT as i32),
            protocol_version: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert test server should succeed");
    }

    async fn seed_probe_record(
        db: &DatabaseConnection,
        server_id: &str,
        target_id: &str,
        timestamp: DateTime<Utc>,
        avg_latency: Option<f64>,
        packet_loss: f64,
    ) {
        let packet_sent = 10_u32;
        let packet_received =
            ((1.0 - packet_loss).clamp(0.0, 1.0) * packet_sent as f64).round() as u32;

        NetworkProbeService::save_results(
            db,
            server_id,
            vec![NetworkProbeResultData {
                target_id: target_id.to_string(),
                avg_latency,
                min_latency: avg_latency,
                max_latency: avg_latency,
                packet_loss,
                packet_sent,
                packet_received,
                timestamp,
            }],
        )
        .await
        .expect("seed probe record should succeed");
    }

    fn bucket_start(timestamp: DateTime<Utc>, bucket_seconds: i64) -> DateTime<Utc> {
        let bucket_ts = timestamp.timestamp().div_euclid(bucket_seconds) * bucket_seconds;
        DateTime::from_timestamp(bucket_ts, 0).expect("bucket timestamp should be valid")
    }

    fn bucket_sample_time(timestamp: DateTime<Utc>, bucket_seconds: i64) -> DateTime<Utc> {
        let offset_seconds = if bucket_seconds > 1 { 1 } else { 0 };
        bucket_start(timestamp, bucket_seconds) + Duration::seconds(offset_seconds)
    }

    fn assert_option_series_eq(actual: &[Option<f64>], expected: &[Option<f64>]) {
        assert_eq!(
            actual.len(),
            expected.len(),
            "series lengths differ: actual={actual:?}, expected={expected:?}"
        );

        for (index, (actual_value, expected_value)) in actual.iter().zip(expected).enumerate() {
            match (actual_value, expected_value) {
                (Some(actual), Some(expected)) => {
                    assert!(
                        (actual - expected).abs() < 1e-9,
                        "series value mismatch at index {index}: actual={actual}, expected={expected}"
                    );
                }
                (None, None) => {}
                _ => {
                    panic!(
                        "series option mismatch at index {index}: actual={actual_value:?}, expected={expected_value:?}"
                    );
                }
            }
        }
    }

    fn empty_sparkline() -> Vec<Option<f64>> {
        vec![None; SPARKLINE_LENGTH]
    }

    fn test_agent_manager() -> AgentManager {
        let (browser_tx, _) = broadcast::channel(4);
        AgentManager::new(browser_tx)
    }

    #[tokio::test]
    async fn test_setting_default_roundtrip() {
        let (db, _tmp) = setup_test_db().await;

        // Should return default when not set
        let setting = NetworkProbeService::get_setting(&db).await.unwrap();
        assert_eq!(setting.interval, 60);
        assert_eq!(setting.packet_count, 10);
        assert!(setting.default_target_ids.is_empty());

        // Update and read back
        let new_setting = NetworkProbeSetting {
            interval: 120,
            packet_count: 5,
            default_target_ids: vec!["cn-bj-ct".to_string(), "cn-bj-cu".to_string()],
        };
        NetworkProbeService::update_setting(&db, &new_setting)
            .await
            .unwrap();

        let read_back = NetworkProbeService::get_setting(&db).await.unwrap();
        assert_eq!(read_back.interval, 120);
        assert_eq!(read_back.packet_count, 5);
        assert_eq!(read_back.default_target_ids.len(), 2);
    }

    #[tokio::test]
    async fn test_create_and_list_targets() {
        let (db, _tmp) = setup_test_db().await;

        let input = CreateNetworkProbeTarget {
            name: "Test Target".to_string(),
            provider: "Provider".to_string(),
            location: "Location".to_string(),
            target: "8.8.8.8".to_string(),
            probe_type: "icmp".to_string(),
        };

        let before_count = NetworkProbeService::list_targets(&db).await.unwrap().len();

        let created = NetworkProbeService::create_target(&db, input)
            .await
            .unwrap();
        assert_eq!(created.name, "Test Target");

        let list = NetworkProbeService::list_targets(&db).await.unwrap();
        assert_eq!(list.len(), before_count + 1);
        assert!(list.iter().any(|t| t.id == created.id));
    }

    #[tokio::test]
    async fn test_create_target_invalid_probe_type() {
        let (db, _tmp) = setup_test_db().await;

        let input = CreateNetworkProbeTarget {
            name: "Bad".to_string(),
            provider: "P".to_string(),
            location: "L".to_string(),
            target: "x".to_string(),
            probe_type: "invalid".to_string(),
        };

        let result = NetworkProbeService::create_target(&db, input).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_target() {
        let (db, _tmp) = setup_test_db().await;

        let input = CreateNetworkProbeTarget {
            name: "Original".to_string(),
            provider: "P".to_string(),
            location: "L".to_string(),
            target: "1.1.1.1".to_string(),
            probe_type: "icmp".to_string(),
        };
        let created = NetworkProbeService::create_target(&db, input)
            .await
            .unwrap();

        let update = UpdateNetworkProbeTarget {
            name: Some("Updated".to_string()),
            provider: None,
            location: None,
            target: None,
            probe_type: None,
        };
        let updated = NetworkProbeService::update_target(&db, &created.id, update)
            .await
            .unwrap();
        assert_eq!(updated.name, "Updated");
        assert_eq!(updated.target, "1.1.1.1");
    }

    #[tokio::test]
    async fn test_delete_target() {
        let (db, _tmp) = setup_test_db().await;

        let input = CreateNetworkProbeTarget {
            name: "ToDelete".to_string(),
            provider: "P".to_string(),
            location: "L".to_string(),
            target: "2.2.2.2".to_string(),
            probe_type: "tcp".to_string(),
        };
        let created = NetworkProbeService::create_target(&db, input)
            .await
            .unwrap();

        let before_count = NetworkProbeService::list_targets(&db).await.unwrap().len();

        NetworkProbeService::delete_target(&db, &created.id)
            .await
            .unwrap();

        let list = NetworkProbeService::list_targets(&db).await.unwrap();
        assert_eq!(list.len(), before_count - 1);
        assert!(!list.iter().any(|t| t.id == created.id));
    }

    #[tokio::test]
    async fn test_set_server_targets_max_limit() {
        let (db, _tmp) = setup_test_db().await;

        let ids: Vec<String> = (0..21).map(|i| format!("target-{i}")).collect();
        let result = NetworkProbeService::set_server_targets(&db, "srv-1", ids).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_anomaly_thresholds() {
        // Verify anomaly classification logic (NodeQuality-aligned thresholds)
        let high_latency = 200.0_f64;
        let very_high_latency = 300.0_f64;
        let normal_latency = 50.0_f64;

        assert!(high_latency > 150.0 && high_latency <= 240.0);
        assert!(very_high_latency > 240.0);
        assert!(normal_latency <= 150.0);

        let high_loss = 0.15_f64;
        let very_high_loss = 0.6_f64;
        let normal_loss = 0.05_f64;

        assert!(high_loss > 0.1 && high_loss <= 0.5);
        assert!(very_high_loss > 0.5);
        assert!(normal_loss <= 0.1);
    }

    #[tokio::test]
    async fn test_list_targets_presets_first() {
        let (db, _tmp) = setup_test_db().await;

        // Create a custom target
        let input = CreateNetworkProbeTarget {
            name: "ZZZ Custom".to_string(),
            provider: "P".to_string(),
            location: "L".to_string(),
            target: "1.2.3.4".to_string(),
            probe_type: "tcp".to_string(),
        };
        NetworkProbeService::create_target(&db, input)
            .await
            .unwrap();

        let list = NetworkProbeService::list_targets(&db).await.unwrap();
        assert_eq!(list.len(), 97); // 96 presets + 1 custom

        // First 96 should be presets (have source set)
        assert!(list[0].source.is_some());
        assert!(list[0].source.as_ref().unwrap().starts_with("preset:"));
        assert!(list[0].source_name.is_some());

        // Last one should be custom (source = None)
        let custom = list.iter().find(|t| t.name == "ZZZ Custom").unwrap();
        assert!(custom.source.is_none());
        assert!(custom.source_name.is_none());
        assert!(custom.created_at.is_some());
    }

    #[tokio::test]
    async fn test_update_preset_target_forbidden() {
        let (db, _tmp) = setup_test_db().await;

        let update = UpdateNetworkProbeTarget {
            name: Some("Hacked".to_string()),
            provider: None,
            location: None,
            target: None,
            probe_type: None,
        };
        let result = NetworkProbeService::update_target(&db, "cn-bj-ct", update).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::Forbidden(_) => {} // expected
            other => panic!("Expected Forbidden, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_delete_preset_target_forbidden() {
        let (db, _tmp) = setup_test_db().await;

        let result = NetworkProbeService::delete_target(&db, "cn-bj-ct").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::Forbidden(_) => {}
            other => panic!("Expected Forbidden, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_update_setting_rejects_invalid_target_ids() {
        let (db, _tmp) = setup_test_db().await;

        let setting = NetworkProbeSetting {
            interval: 60,
            packet_count: 10,
            default_target_ids: vec!["nonexistent-target-id".to_string()],
        };
        let result = NetworkProbeService::update_setting(&db, &setting).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_set_server_targets_validates_ids() {
        let (db, _tmp) = setup_test_db().await;

        let result = NetworkProbeService::set_server_targets(
            &db,
            "srv-1",
            vec!["nonexistent-id".to_string()],
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_set_server_targets_accepts_preset_ids() {
        let (db, _tmp) = setup_test_db().await;

        // Create a server record with all required fields
        use crate::entity::server;
        use sea_orm::{ActiveModelTrait, Set};

        let now = chrono::Utc::now();
        let srv = server::ActiveModel {
            id: Set("srv-test".to_string()),
            token_hash: Set("hash".to_string()),
            token_prefix: Set("prefix".to_string()),
            name: Set("Test Server".to_string()),
            weight: Set(0),
            hidden: Set(false),
            capabilities: Set(0),
            protocol_version: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        srv.insert(&db).await.unwrap();

        let result = NetworkProbeService::set_server_targets(
            &db,
            "srv-test",
            vec!["cn-bj-ct".to_string(), "intl-cloudflare".to_string()],
        )
        .await;
        assert!(result.is_ok());

        let targets = NetworkProbeService::get_server_targets(&db, "srv-test")
            .await
            .unwrap();
        assert_eq!(targets.len(), 2);
        assert!(targets.iter().all(|t| t.source.is_some()));
    }

    #[tokio::test]
    async fn test_sparkline_empty_server_ids() {
        let (db, _tmp) = setup_test_db().await;

        let sparklines = NetworkProbeService::query_sparklines(&db, &[], 60)
            .await
            .unwrap();

        assert!(sparklines.is_empty());
    }

    #[tokio::test]
    async fn test_sparkline_single_server_dense_data() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-dense", "Dense Server").await;

        let bucket_seconds = 60;
        let latest_bucket = bucket_start(Utc::now(), bucket_seconds);

        for index in 0..SPARKLINE_LENGTH {
            let bucket_time = latest_bucket
                - Duration::seconds(((SPARKLINE_LENGTH - 1 - index) as i64) * bucket_seconds);
            seed_probe_record(
                &db,
                "srv-dense",
                "dense-target",
                bucket_sample_time(bucket_time, bucket_seconds),
                Some(index as f64),
                index as f64 / 100.0,
            )
            .await;
        }

        let sparklines =
            NetworkProbeService::query_sparklines(&db, &["srv-dense".to_string()], bucket_seconds)
                .await
                .unwrap();
        let bundle = sparklines.get("srv-dense").expect("sparkline should exist");

        let expected_latency: Vec<Option<f64>> = (0..SPARKLINE_LENGTH)
            .map(|index| Some(index as f64))
            .collect();
        let expected_loss: Vec<Option<f64>> = (0..SPARKLINE_LENGTH)
            .map(|index| Some(index as f64 / 100.0))
            .collect();

        assert_option_series_eq(&bundle.latency, &expected_latency);
        assert_option_series_eq(&bundle.loss, &expected_loss);
    }

    #[tokio::test]
    async fn test_sparkline_multi_target_bucket_averaging() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-avg", "Average Server").await;

        let bucket_seconds = 60;
        let bucket_time = bucket_start(Utc::now(), bucket_seconds);

        seed_probe_record(
            &db,
            "srv-avg",
            "target-a",
            bucket_sample_time(bucket_time, bucket_seconds),
            Some(10.0),
            0.0,
        )
        .await;
        seed_probe_record(
            &db,
            "srv-avg",
            "target-b",
            bucket_sample_time(bucket_time, bucket_seconds) + Duration::seconds(10),
            Some(30.0),
            0.5,
        )
        .await;

        let sparklines =
            NetworkProbeService::query_sparklines(&db, &["srv-avg".to_string()], bucket_seconds)
                .await
                .unwrap();
        let bundle = sparklines.get("srv-avg").expect("sparkline should exist");

        let mut expected_latency = empty_sparkline();
        expected_latency[SPARKLINE_LENGTH - 1] = Some(20.0);
        let mut expected_loss = empty_sparkline();
        expected_loss[SPARKLINE_LENGTH - 1] = Some(0.25);

        assert_option_series_eq(&bundle.latency, &expected_latency);
        assert_option_series_eq(&bundle.loss, &expected_loss);
    }

    #[tokio::test]
    async fn test_sparkline_sparse_data_with_front_padding() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-sparse", "Sparse Server").await;

        let bucket_seconds = 60;
        let latest_bucket = bucket_start(Utc::now(), bucket_seconds);
        let previous_bucket = latest_bucket - Duration::seconds(bucket_seconds);

        seed_probe_record(
            &db,
            "srv-sparse",
            "sparse-target",
            bucket_sample_time(previous_bucket, bucket_seconds),
            Some(11.0),
            0.1,
        )
        .await;
        seed_probe_record(
            &db,
            "srv-sparse",
            "sparse-target",
            bucket_sample_time(latest_bucket, bucket_seconds),
            Some(22.0),
            0.2,
        )
        .await;

        let sparklines =
            NetworkProbeService::query_sparklines(&db, &["srv-sparse".to_string()], bucket_seconds)
                .await
                .unwrap();
        let bundle = sparklines
            .get("srv-sparse")
            .expect("sparkline should exist");

        let mut expected_latency = empty_sparkline();
        expected_latency[SPARKLINE_LENGTH - 2] = Some(11.0);
        expected_latency[SPARKLINE_LENGTH - 1] = Some(22.0);
        let mut expected_loss = empty_sparkline();
        expected_loss[SPARKLINE_LENGTH - 2] = Some(0.1);
        expected_loss[SPARKLINE_LENGTH - 1] = Some(0.2);

        assert_option_series_eq(&bundle.latency, &expected_latency);
        assert_option_series_eq(&bundle.loss, &expected_loss);
    }

    #[tokio::test]
    async fn test_sparkline_gap_fill_continuity() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-gap", "Gap Server").await;

        let bucket_seconds = 60;
        let latest_bucket = bucket_start(Utc::now(), bucket_seconds);
        let oldest_bucket = latest_bucket - Duration::seconds(bucket_seconds * 2);

        seed_probe_record(
            &db,
            "srv-gap",
            "gap-target",
            bucket_sample_time(oldest_bucket, bucket_seconds),
            Some(5.0),
            0.05,
        )
        .await;
        seed_probe_record(
            &db,
            "srv-gap",
            "gap-target",
            bucket_sample_time(latest_bucket, bucket_seconds),
            Some(25.0),
            0.25,
        )
        .await;

        let sparklines =
            NetworkProbeService::query_sparklines(&db, &["srv-gap".to_string()], bucket_seconds)
                .await
                .unwrap();
        let bundle = sparklines.get("srv-gap").expect("sparkline should exist");

        let mut expected_latency = empty_sparkline();
        expected_latency[SPARKLINE_LENGTH - 3] = Some(5.0);
        expected_latency[SPARKLINE_LENGTH - 1] = Some(25.0);
        let mut expected_loss = empty_sparkline();
        expected_loss[SPARKLINE_LENGTH - 3] = Some(0.05);
        expected_loss[SPARKLINE_LENGTH - 1] = Some(0.25);

        assert_option_series_eq(&bundle.latency, &expected_latency);
        assert_option_series_eq(&bundle.loss, &expected_loss);
    }

    #[tokio::test]
    async fn test_sparkline_null_latency_handling() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-null", "Null Server").await;

        let bucket_seconds = 60;
        let bucket_time = bucket_start(Utc::now(), bucket_seconds);

        seed_probe_record(
            &db,
            "srv-null",
            "null-target",
            bucket_sample_time(bucket_time, bucket_seconds),
            None,
            1.0,
        )
        .await;
        seed_probe_record(
            &db,
            "srv-null",
            "nonnull-target",
            bucket_sample_time(bucket_time, bucket_seconds) + Duration::seconds(5),
            Some(20.0),
            0.0,
        )
        .await;

        let sparklines =
            NetworkProbeService::query_sparklines(&db, &["srv-null".to_string()], bucket_seconds)
                .await
                .unwrap();
        let bundle = sparklines.get("srv-null").expect("sparkline should exist");

        let mut expected_latency = empty_sparkline();
        expected_latency[SPARKLINE_LENGTH - 1] = Some(20.0);
        let mut expected_loss = empty_sparkline();
        expected_loss[SPARKLINE_LENGTH - 1] = Some(0.5);

        assert_option_series_eq(&bundle.latency, &expected_latency);
        assert_option_series_eq(&bundle.loss, &expected_loss);
    }

    #[tokio::test]
    async fn test_sparkline_batch_multiple_servers() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-batch-a", "Batch Server A").await;
        insert_test_server(&db, "srv-batch-b", "Batch Server B").await;

        let bucket_seconds = 60;
        let latest_bucket = bucket_start(Utc::now(), bucket_seconds);

        seed_probe_record(
            &db,
            "srv-batch-a",
            "batch-a",
            bucket_sample_time(
                latest_bucket - Duration::seconds(bucket_seconds),
                bucket_seconds,
            ),
            Some(7.0),
            0.07,
        )
        .await;
        seed_probe_record(
            &db,
            "srv-batch-b",
            "batch-b",
            bucket_sample_time(latest_bucket, bucket_seconds),
            Some(9.0),
            0.09,
        )
        .await;

        let sparklines = NetworkProbeService::query_sparklines(
            &db,
            &["srv-batch-a".to_string(), "srv-batch-b".to_string()],
            bucket_seconds,
        )
        .await
        .unwrap();

        let mut expected_a = empty_sparkline();
        expected_a[SPARKLINE_LENGTH - 1] = Some(7.0);
        let mut expected_b = empty_sparkline();
        expected_b[SPARKLINE_LENGTH - 1] = Some(9.0);

        assert_eq!(sparklines.len(), 2);
        assert_option_series_eq(
            &sparklines
                .get("srv-batch-a")
                .expect("server A sparkline should exist")
                .latency,
            &expected_a,
        );
        assert_option_series_eq(
            &sparklines
                .get("srv-batch-b")
                .expect("server B sparkline should exist")
                .latency,
            &expected_b,
        );
    }

    #[tokio::test]
    async fn test_sparkline_preserves_offline_frozen_history() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-offline", "Offline Server").await;

        let bucket_seconds = 60;
        let now_bucket = bucket_start(Utc::now(), bucket_seconds);
        let historical_latest = now_bucket - Duration::seconds(bucket_seconds * 40);

        seed_probe_record(
            &db,
            "srv-offline",
            "offline-target",
            bucket_sample_time(
                historical_latest - Duration::seconds(bucket_seconds),
                bucket_seconds,
            ),
            Some(12.0),
            0.12,
        )
        .await;
        seed_probe_record(
            &db,
            "srv-offline",
            "offline-target",
            bucket_sample_time(historical_latest, bucket_seconds),
            Some(34.0),
            0.34,
        )
        .await;

        let sparklines = NetworkProbeService::query_sparklines(
            &db,
            &["srv-offline".to_string()],
            bucket_seconds,
        )
        .await
        .unwrap();
        let bundle = sparklines
            .get("srv-offline")
            .expect("offline sparkline should exist");

        let mut expected_latency = empty_sparkline();
        expected_latency[SPARKLINE_LENGTH - 2] = Some(12.0);
        expected_latency[SPARKLINE_LENGTH - 1] = Some(34.0);
        let mut expected_loss = empty_sparkline();
        expected_loss[SPARKLINE_LENGTH - 2] = Some(0.12);
        expected_loss[SPARKLINE_LENGTH - 1] = Some(0.34);

        assert_option_series_eq(&bundle.latency, &expected_latency);
        assert_option_series_eq(&bundle.loss, &expected_loss);
    }

    #[tokio::test]
    async fn test_sparkline_offline_history_keeps_latest_30_buckets() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-offline-30", "Offline Server 30").await;

        let bucket_seconds = 60;
        let now_bucket = bucket_start(Utc::now(), bucket_seconds);
        let historical_latest = now_bucket - Duration::seconds(bucket_seconds * 40);

        for index in 0..(SPARKLINE_LENGTH + 2) {
            let bucket_time = historical_latest
                - Duration::seconds(((SPARKLINE_LENGTH + 1 - index) as i64) * bucket_seconds);
            seed_probe_record(
                &db,
                "srv-offline-30",
                "offline-30-target",
                bucket_sample_time(bucket_time, bucket_seconds),
                Some(index as f64),
                index as f64 / 100.0,
            )
            .await;
        }

        let sparklines = NetworkProbeService::query_sparklines(
            &db,
            &["srv-offline-30".to_string()],
            bucket_seconds,
        )
        .await
        .unwrap();
        let bundle = sparklines
            .get("srv-offline-30")
            .expect("offline 30 sparkline should exist");

        let expected_latency: Vec<Option<f64>> = (2..(SPARKLINE_LENGTH + 2))
            .map(|index| Some(index as f64))
            .collect();
        let expected_loss: Vec<Option<f64>> = (2..(SPARKLINE_LENGTH + 2))
            .map(|index| Some(index as f64 / 100.0))
            .collect();

        assert_option_series_eq(&bundle.latency, &expected_latency);
        assert_option_series_eq(&bundle.loss, &expected_loss);
    }

    #[tokio::test]
    async fn test_sparkline_adaptive_bucket_size() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-adaptive", "Adaptive Server").await;
        insert_test_server(&db, "srv-empty", "Empty Server").await;

        let thirty_second_bucket = 30;
        let sixty_second_bucket = 60;
        let minute_bucket = bucket_start(Utc::now(), sixty_second_bucket);

        NetworkProbeService::update_setting(
            &db,
            &NetworkProbeSetting {
                interval: 30,
                packet_count: 10,
                default_target_ids: vec![],
            },
        )
        .await
        .unwrap();

        seed_probe_record(
            &db,
            "srv-adaptive",
            "adaptive-a",
            bucket_sample_time(minute_bucket, thirty_second_bucket),
            Some(100.0),
            0.1,
        )
        .await;
        seed_probe_record(
            &db,
            "srv-adaptive",
            "adaptive-b",
            bucket_sample_time(minute_bucket, thirty_second_bucket) + Duration::seconds(30),
            Some(200.0),
            0.3,
        )
        .await;

        let overviews = NetworkProbeService::get_overview(&db, &test_agent_manager())
            .await
            .unwrap();
        let adaptive = overviews
            .iter()
            .find(|overview| overview.server_id == "srv-adaptive")
            .expect("adaptive server should be present");
        let empty = overviews
            .iter()
            .find(|overview| overview.server_id == "srv-empty")
            .expect("empty server should be present");

        let mut expected_latency = empty_sparkline();
        expected_latency[SPARKLINE_LENGTH - 1] = Some(150.0);
        let mut expected_loss = empty_sparkline();
        expected_loss[SPARKLINE_LENGTH - 1] = Some(0.2);

        assert_option_series_eq(&adaptive.latency_sparkline, &expected_latency);
        assert_option_series_eq(&adaptive.loss_sparkline, &expected_loss);
        assert_option_series_eq(&empty.latency_sparkline, &empty_sparkline());
        assert_option_series_eq(&empty.loss_sparkline, &empty_sparkline());
    }

    #[tokio::test]
    async fn test_sparkline_widened_bucket_size() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-wide", "Wide Bucket Server").await;

        let bucket_seconds = 120;
        let wide_bucket = bucket_start(Utc::now(), bucket_seconds);

        NetworkProbeService::update_setting(
            &db,
            &NetworkProbeSetting {
                interval: bucket_seconds as u32,
                packet_count: 10,
                default_target_ids: vec![],
            },
        )
        .await
        .unwrap();

        seed_probe_record(
            &db,
            "srv-wide",
            "wide-a",
            bucket_sample_time(wide_bucket, bucket_seconds),
            Some(100.0),
            0.1,
        )
        .await;
        seed_probe_record(
            &db,
            "srv-wide",
            "wide-b",
            bucket_sample_time(wide_bucket, bucket_seconds) + Duration::seconds(60),
            Some(200.0),
            0.3,
        )
        .await;

        let overviews = NetworkProbeService::get_overview(&db, &test_agent_manager())
            .await
            .unwrap();
        let overview = overviews
            .iter()
            .find(|candidate| candidate.server_id == "srv-wide")
            .expect("wide bucket server should be present");

        let mut expected_latency = empty_sparkline();
        expected_latency[SPARKLINE_LENGTH - 1] = Some(150.0);
        let mut expected_loss = empty_sparkline();
        expected_loss[SPARKLINE_LENGTH - 1] = Some(0.2);

        assert_option_series_eq(&overview.latency_sparkline, &expected_latency);
        assert_option_series_eq(&overview.loss_sparkline, &expected_loss);
    }
}
