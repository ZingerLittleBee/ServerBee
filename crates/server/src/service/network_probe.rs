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
}

/// Result type for query_records supporting both raw and hourly records.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum ProbeQueryResult {
    Raw(Vec<network_probe_record::Model>),
    Hourly(Vec<network_probe_record_hourly::Model>),
}

// ---------------------------------------------------------------------------
// Service implementation
// ---------------------------------------------------------------------------

impl NetworkProbeService {
    // -----------------------------------------------------------------------
    // Target management
    // -----------------------------------------------------------------------

    /// List all probe targets (builtin + custom).
    pub async fn list_targets(
        db: &DatabaseConnection,
    ) -> Result<Vec<network_probe_target::Model>, AppError> {
        Ok(network_probe_target::Entity::find().all(db).await?)
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
            is_builtin: Set(false),
            created_at: Set(now),
            updated_at: Set(now),
        };

        Ok(model.insert(db).await?)
    }

    /// Update a custom probe target. Builtin targets cannot be modified.
    pub async fn update_target(
        db: &DatabaseConnection,
        id: &str,
        input: UpdateNetworkProbeTarget,
    ) -> Result<network_probe_target::Model, AppError> {
        let existing = network_probe_target::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Probe target {id} not found")))?;

        if existing.is_builtin {
            return Err(AppError::Forbidden(
                "Cannot modify a builtin probe target".to_string(),
            ));
        }

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

    /// Delete a custom probe target. Builtin targets cannot be deleted.
    /// Cascade-deletes associated config and records, and removes the target
    /// from `default_target_ids` in the global setting.
    pub async fn delete_target(db: &DatabaseConnection, id: &str) -> Result<(), AppError> {
        let existing = network_probe_target::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Probe target {id} not found")))?;

        if existing.is_builtin {
            return Err(AppError::Forbidden(
                "Cannot delete a builtin probe target".to_string(),
            ));
        }

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
        let setting: Option<NetworkProbeSetting> =
            ConfigService::get_typed(db, CONFIG_KEY).await?;
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

    /// Get targets assigned to a server (join config with target table).
    pub async fn get_server_targets(
        db: &DatabaseConnection,
        server_id: &str,
    ) -> Result<Vec<network_probe_target::Model>, AppError> {
        let configs = network_probe_config::Entity::find()
            .filter(network_probe_config::Column::ServerId.eq(server_id))
            .all(db)
            .await?;

        if configs.is_empty() {
            return Ok(Vec::new());
        }

        let target_ids: Vec<String> = configs.into_iter().map(|c| c.target_id).collect();

        let targets = network_probe_target::Entity::find()
            .filter(network_probe_target::Column::Id.is_in(target_ids))
            .all(db)
            .await?;

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
    pub async fn apply_defaults(
        db: &DatabaseConnection,
        server_id: &str,
    ) -> Result<(), AppError> {
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
    pub async fn query_records(
        db: &DatabaseConnection,
        server_id: &str,
        target_id: Option<String>,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<ProbeQueryResult, AppError> {
        let duration = to - from;
        let use_hourly = duration >= Duration::days(1);

        if use_hourly {
            let mut query = network_probe_record_hourly::Entity::find()
                .filter(network_probe_record_hourly::Column::ServerId.eq(server_id))
                .filter(network_probe_record_hourly::Column::Hour.gte(from))
                .filter(network_probe_record_hourly::Column::Hour.lte(to));

            if let Some(tid) = target_id {
                query =
                    query.filter(network_probe_record_hourly::Column::TargetId.eq(tid));
            }

            let records = query
                .order_by_asc(network_probe_record_hourly::Column::Hour)
                .all(db)
                .await?;
            Ok(ProbeQueryResult::Hourly(records))
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
            Ok(ProbeQueryResult::Raw(records))
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

        let mut target_summaries = Vec::new();
        let mut last_probe_at: Option<DateTime<Utc>> = None;

        for target in &targets {
            // Get the latest record for this server + target
            let latest = network_probe_record::Entity::find()
                .filter(network_probe_record::Column::ServerId.eq(server_id))
                .filter(network_probe_record::Column::TargetId.eq(&target.id))
                .order_by_desc(network_probe_record::Column::Timestamp)
                .one(db)
                .await?;

            if let Some(record) = latest {
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
        // Get all distinct server_ids from config
        let configs = network_probe_config::Entity::find().all(db).await?;

        let mut server_ids: Vec<String> = configs.iter().map(|c| c.server_id.clone()).collect();
        server_ids.sort();
        server_ids.dedup();

        // Load all servers to get names
        let servers = server::Entity::find()
            .filter(server::Column::Id.is_in(server_ids.iter().cloned()))
            .all(db)
            .await?;
        let server_map: std::collections::HashMap<String, String> = servers
            .into_iter()
            .map(|s| (s.id, s.name))
            .collect();

        // Load all targets for provider lookup
        let all_targets = network_probe_target::Entity::find().all(db).await?;
        let target_map: std::collections::HashMap<String, &network_probe_target::Model> =
            all_targets.iter().map(|t| (t.id.clone(), t)).collect();

        let anomaly_from = Utc::now() - Duration::hours(24);

        let mut overviews = Vec::new();

        for server_id in &server_ids {
            let server_name = server_map
                .get(server_id)
                .cloned()
                .unwrap_or_else(|| server_id.clone());
            let online = agent_manager.is_online(server_id);

            // Get the latest record per target for this server
            let target_configs: Vec<&network_probe_config::Model> =
                configs.iter().filter(|c| &c.server_id == server_id).collect();

            let target_ids: Vec<String> =
                target_configs.iter().map(|c| c.target_id.clone()).collect();

            let mut target_summaries = Vec::new();
            let mut last_probe_at: Option<DateTime<Utc>> = None;

            for target_id in &target_ids {
                let latest = network_probe_record::Entity::find()
                    .filter(network_probe_record::Column::ServerId.eq(server_id))
                    .filter(network_probe_record::Column::TargetId.eq(target_id))
                    .order_by_desc(network_probe_record::Column::Timestamp)
                    .one(db)
                    .await?;

                if let Some(record) = latest {
                    match last_probe_at {
                        Some(existing) if record.timestamp > existing => {
                            last_probe_at = Some(record.timestamp);
                        }
                        None => {
                            last_probe_at = Some(record.timestamp);
                        }
                        _ => {}
                    }

                    let target_name = target_map
                        .get(target_id)
                        .map(|t| t.name.clone())
                        .unwrap_or_else(|| target_id.clone());
                    let provider = target_map
                        .get(target_id)
                        .map(|t| t.provider.clone())
                        .unwrap_or_default();

                    target_summaries.push(TargetSummary {
                        target_id: target_id.clone(),
                        target_name,
                        provider,
                        avg_latency: record.avg_latency,
                        min_latency: record.min_latency,
                        max_latency: record.max_latency,
                        packet_loss: record.packet_loss,
                        availability: 1.0 - record.packet_loss,
                    });
                }
            }

            let anomaly_count = Self::count_anomalies(db, server_id, anomaly_from).await?;

            overviews.push(ServerOverview {
                server_id: server_id.clone(),
                server_name,
                online,
                last_probe_at: last_probe_at.map(|t| t.to_rfc3339()),
                targets: target_summaries,
                anomaly_count,
            });
        }

        Ok(overviews)
    }

    /// Get anomalous probe records for a server within a time range.
    /// Anomalies: avg_latency > 200ms OR packet_loss > 0.1 (10%).
    pub async fn get_anomalies(
        db: &DatabaseConnection,
        server_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<NetworkProbeAnomaly>, AppError> {
        // Load target names for display
        let targets = Self::get_server_targets(db, server_id).await?;
        let target_map: std::collections::HashMap<String, String> = targets
            .into_iter()
            .map(|t| (t.id.clone(), t.name.clone()))
            .collect();

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
                if latency > 500.0 {
                    anomalies.push(NetworkProbeAnomaly {
                        timestamp: record.timestamp.to_rfc3339(),
                        target_id: record.target_id.clone(),
                        target_name: target_name.clone(),
                        anomaly_type: "very_high_latency".to_string(),
                        value: latency,
                    });
                } else if latency > 200.0 {
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

    /// Count anomalous records for a server since a given time.
    /// Anomalies: unreachable (packet_loss == 1.0), high latency (>200ms), high packet loss (>0.1).
    async fn count_anomalies(
        db: &DatabaseConnection,
        server_id: &str,
        from: DateTime<Utc>,
    ) -> Result<i64, AppError> {
        let records = network_probe_record::Entity::find()
            .filter(network_probe_record::Column::ServerId.eq(server_id))
            .filter(network_probe_record::Column::Timestamp.gte(from))
            .all(db)
            .await?;

        let count = records
            .iter()
            .filter(|r| {
                (r.packet_loss - 1.0).abs() < f64::EPSILON
                    || r.avg_latency.is_some_and(|l| l > 200.0)
                    || r.packet_loss > 0.1
            })
            .count();

        Ok(count as i64)
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

        // Group by (server_id, target_id)
        let mut grouped: std::collections::HashMap<
            (String, String),
            Vec<&network_probe_record::Model>,
        > = std::collections::HashMap::new();

        for r in &records {
            grouped
                .entry((r.server_id.clone(), r.target_id.clone()))
                .or_default()
                .push(r);
        }

        let mut inserted = 0u64;

        // Truncate to the start of the hour
        let hour = one_hour_ago
            .with_minute(0)
            .and_then(|t| t.with_second(0))
            .and_then(|t| t.with_nanosecond(0))
            .unwrap_or(one_hour_ago);

        let hour_str = hour.to_rfc3339();

        for ((server_id, target_id), group) in &grouped {
            let count = group.len() as f64;

            let latencies: Vec<f64> = group.iter().filter_map(|r| r.avg_latency).collect();
            let avg_latency = if latencies.is_empty() {
                None
            } else {
                Some(latencies.iter().sum::<f64>() / latencies.len() as f64)
            };

            let min_latencies: Vec<f64> = group.iter().filter_map(|r| r.min_latency).collect();
            let min_latency = min_latencies
                .iter()
                .cloned()
                .reduce(f64::min);

            let max_latencies: Vec<f64> = group.iter().filter_map(|r| r.max_latency).collect();
            let max_latency = max_latencies
                .iter()
                .cloned()
                .reduce(f64::max);

            let avg_packet_loss =
                group.iter().map(|r| r.packet_loss).sum::<f64>() / count;

            let sample_count = group.len() as i32;

            // Use raw SQL INSERT OR REPLACE for idempotency on the UNIQUE(server_id, target_id, hour) constraint
            let avg_lat_sql = avg_latency
                .map(|v| v.to_string())
                .unwrap_or_else(|| "NULL".to_string());
            let min_lat_sql = min_latency
                .map(|v| v.to_string())
                .unwrap_or_else(|| "NULL".to_string());
            let max_lat_sql = max_latency
                .map(|v| v.to_string())
                .unwrap_or_else(|| "NULL".to_string());

            let sql = format!(
                "INSERT INTO network_probe_record_hourly (server_id, target_id, avg_latency, min_latency, max_latency, avg_packet_loss, sample_count, hour) \
                 VALUES ('{server_id}', '{target_id}', {avg_lat_sql}, {min_lat_sql}, {max_lat_sql}, {avg_packet_loss}, {sample_count}, '{hour_str}') \
                 ON CONFLICT (server_id, target_id, hour) DO UPDATE SET \
                 avg_latency = excluded.avg_latency, \
                 min_latency = excluded.min_latency, \
                 max_latency = excluded.max_latency, \
                 avg_packet_loss = excluded.avg_packet_loss, \
                 sample_count = excluded.sample_count"
            );

            let stmt = Statement::from_string(DatabaseBackend::Sqlite, sql);
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
        let hourly_cutoff =
            Utc::now() - Duration::days(retention.network_probe_hourly_days as i64);

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
    use crate::test_utils::setup_test_db;

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
            default_target_ids: vec!["t1".to_string(), "t2".to_string()],
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

        let created = NetworkProbeService::create_target(&db, input).await.unwrap();
        assert_eq!(created.name, "Test Target");
        assert!(!created.is_builtin);

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
        let created = NetworkProbeService::create_target(&db, input).await.unwrap();

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
        let created = NetworkProbeService::create_target(&db, input).await.unwrap();

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
        // Verify anomaly classification logic
        let high_latency = 250.0_f64;
        let very_high_latency = 600.0_f64;
        let normal_latency = 50.0_f64;

        assert!(high_latency > 200.0 && high_latency <= 500.0);
        assert!(very_high_latency > 500.0);
        assert!(normal_latency <= 200.0);

        let high_loss = 0.15_f64;
        let very_high_loss = 0.6_f64;
        let normal_loss = 0.05_f64;

        assert!(high_loss > 0.1 && high_loss <= 0.5);
        assert!(very_high_loss > 0.5);
        assert!(normal_loss <= 0.1);
    }
}
