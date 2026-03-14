use chrono::{DateTime, Utc};
use sea_orm::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::{ping_record, ping_task, server};
use crate::error::AppError;
use crate::service::agent_manager::AgentManager;
use serverbee_common::constants::{has_capability, probe_type_to_cap, CAP_DEFAULT};
use serverbee_common::protocol::ServerMessage;
use serverbee_common::types::PingTaskConfig;

pub struct PingService;

#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreatePingTask {
    pub name: String,
    pub probe_type: String,
    pub target: String,
    #[serde(default = "default_interval")]
    pub interval: i32,
    #[serde(default)]
    pub server_ids: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_interval() -> i32 {
    60
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdatePingTask {
    pub name: Option<String>,
    pub probe_type: Option<String>,
    pub target: Option<String>,
    pub interval: Option<i32>,
    pub server_ids: Option<Vec<String>>,
    pub enabled: Option<bool>,
}

impl PingService {
    pub async fn list(db: &DatabaseConnection) -> Result<Vec<ping_task::Model>, AppError> {
        Ok(ping_task::Entity::find().all(db).await?)
    }

    pub async fn get(db: &DatabaseConnection, id: &str) -> Result<ping_task::Model, AppError> {
        ping_task::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Ping task {id} not found")))
    }

    pub async fn create(
        db: &DatabaseConnection,
        agent_manager: &AgentManager,
        input: CreatePingTask,
    ) -> Result<ping_task::Model, AppError> {
        if !["icmp", "tcp", "http"].contains(&input.probe_type.as_str()) {
            return Err(AppError::Validation(
                "probe_type must be icmp, tcp, or http".to_string(),
            ));
        }

        let server_ids_json = serde_json::to_string(&input.server_ids)
            .map_err(|e| AppError::Validation(format!("Invalid server_ids: {e}")))?;

        let model = ping_task::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            name: Set(input.name),
            probe_type: Set(input.probe_type),
            target: Set(input.target),
            interval: Set(input.interval),
            server_ids_json: Set(server_ids_json),
            enabled: Set(input.enabled),
            created_at: Set(Utc::now()),
        };
        let created = model.insert(db).await?;

        // Sync tasks to affected agents
        Self::sync_tasks_to_agents(db, agent_manager).await;

        Ok(created)
    }

    pub async fn update(
        db: &DatabaseConnection,
        agent_manager: &AgentManager,
        id: &str,
        input: UpdatePingTask,
    ) -> Result<ping_task::Model, AppError> {
        let existing = Self::get(db, id).await?;
        let mut model: ping_task::ActiveModel = existing.into();

        if let Some(name) = input.name {
            model.name = Set(name);
        }
        if let Some(probe_type) = input.probe_type {
            if !["icmp", "tcp", "http"].contains(&probe_type.as_str()) {
                return Err(AppError::Validation(
                    "probe_type must be icmp, tcp, or http".to_string(),
                ));
            }
            model.probe_type = Set(probe_type);
        }
        if let Some(target) = input.target {
            model.target = Set(target);
        }
        if let Some(interval) = input.interval {
            model.interval = Set(interval);
        }
        if let Some(server_ids) = input.server_ids {
            let json = serde_json::to_string(&server_ids)
                .map_err(|e| AppError::Validation(format!("Invalid server_ids: {e}")))?;
            model.server_ids_json = Set(json);
        }
        if let Some(enabled) = input.enabled {
            model.enabled = Set(enabled);
        }

        let updated = model.update(db).await?;

        Self::sync_tasks_to_agents(db, agent_manager).await;

        Ok(updated)
    }

    pub async fn delete(
        db: &DatabaseConnection,
        agent_manager: &AgentManager,
        id: &str,
    ) -> Result<(), AppError> {
        let result = ping_task::Entity::delete_by_id(id).exec(db).await?;
        if result.rows_affected == 0 {
            return Err(AppError::NotFound(format!("Ping task {id} not found")));
        }
        // Clean up records
        ping_record::Entity::delete_many()
            .filter(ping_record::Column::TaskId.eq(id))
            .exec(db)
            .await?;

        Self::sync_tasks_to_agents(db, agent_manager).await;

        Ok(())
    }

    pub async fn get_records(
        db: &DatabaseConnection,
        task_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        server_id: Option<&str>,
    ) -> Result<Vec<ping_record::Model>, AppError> {
        let mut query = ping_record::Entity::find()
            .filter(ping_record::Column::TaskId.eq(task_id))
            .filter(ping_record::Column::Time.gte(from))
            .filter(ping_record::Column::Time.lte(to));

        if let Some(sid) = server_id {
            query = query.filter(ping_record::Column::ServerId.eq(sid));
        }

        Ok(query
            .order_by_asc(ping_record::Column::Time)
            .all(db)
            .await?)
    }

    /// Send current ping tasks to a specific agent (e.g., on new connection).
    pub async fn sync_tasks_to_agent(db: &DatabaseConnection, agent_manager: &AgentManager, server_id: &str) {
        let tasks = match ping_task::Entity::find()
            .filter(ping_task::Column::Enabled.eq(true))
            .all(db)
            .await
        {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("Failed to load ping tasks for agent sync: {e}");
                return;
            }
        };

        // Fetch server capabilities
        let server_caps = server::Entity::find_by_id(server_id)
            .one(db)
            .await
            .ok()
            .flatten()
            .map(|s| s.capabilities as u32)
            .unwrap_or(CAP_DEFAULT);

        let mut task_configs: Vec<PingTaskConfig> = Vec::new();
        for task in &tasks {
            let server_ids: Vec<String> =
                serde_json::from_str(&task.server_ids_json).unwrap_or_default();
            // Include task if server_ids is empty (all agents) or contains this server
            if server_ids.is_empty() || server_ids.contains(&server_id.to_string()) {
                // Filter by capability
                if probe_type_to_cap(&task.probe_type)
                    .map(|cap| has_capability(server_caps, cap))
                    .unwrap_or(false)
                {
                    task_configs.push(PingTaskConfig {
                        task_id: task.id.clone(),
                        probe_type: task.probe_type.clone(),
                        target: task.target.clone(),
                        interval: task.interval as u32,
                    });
                }
            }
        }

        // Always send PingTasksSync (even if empty — tells Agent to stop all probes)
        if let Some(tx) = agent_manager.get_sender(server_id) {
            let msg = ServerMessage::PingTasksSync { tasks: task_configs };
            let _ = tx.send(msg).await;
        }
    }

    /// Sync all enabled ping tasks to all connected agents.
    async fn sync_tasks_to_agents(db: &DatabaseConnection, agent_manager: &AgentManager) {
        let tasks = match ping_task::Entity::find()
            .filter(ping_task::Column::Enabled.eq(true))
            .all(db)
            .await
        {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("Failed to load ping tasks for sync: {e}");
                return;
            }
        };

        // Fetch capabilities for all connected agents
        let connected_ids = agent_manager.connected_server_ids();
        let server_caps_map: std::collections::HashMap<String, u32> =
            match server::Entity::find()
                .filter(server::Column::Id.is_in(connected_ids.iter().cloned()))
                .all(db)
                .await
            {
                Ok(servers) => servers
                    .into_iter()
                    .map(|s| {
                        let caps = s.capabilities as u32;
                        (s.id, caps)
                    })
                    .collect(),
                Err(e) => {
                    tracing::error!("Failed to load server caps for ping sync: {e}");
                    return;
                }
            };

        // Build per-agent task lists filtered by capability
        let mut agent_tasks: std::collections::HashMap<String, Vec<PingTaskConfig>> =
            std::collections::HashMap::new();

        // Ensure every connected agent gets an entry (even if empty)
        for sid in &connected_ids {
            agent_tasks.entry(sid.clone()).or_default();
        }

        for task in &tasks {
            let server_ids: Vec<String> =
                serde_json::from_str(&task.server_ids_json).unwrap_or_default();
            let config = PingTaskConfig {
                task_id: task.id.clone(),
                probe_type: task.probe_type.clone(),
                target: task.target.clone(),
                interval: task.interval as u32,
            };

            let target_ids: Vec<String> = if server_ids.is_empty() {
                connected_ids.clone()
            } else {
                server_ids
            };

            for sid in target_ids {
                let caps = server_caps_map.get(&sid).copied().unwrap_or(CAP_DEFAULT);
                if probe_type_to_cap(&task.probe_type)
                    .map(|cap| has_capability(caps, cap))
                    .unwrap_or(false)
                {
                    agent_tasks.entry(sid).or_default().push(config.clone());
                }
            }
        }

        for (server_id, task_configs) in agent_tasks {
            if let Some(tx) = agent_manager.get_sender(&server_id) {
                let msg = ServerMessage::PingTasksSync {
                    tasks: task_configs,
                };
                let _ = tx.send(msg).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::setup_test_db;

    fn test_agent_manager() -> crate::service::agent_manager::AgentManager {
        let (tx, _) = tokio::sync::broadcast::channel(16);
        crate::service::agent_manager::AgentManager::new(tx)
    }

    fn sample_create_ping_task() -> CreatePingTask {
        CreatePingTask {
            name: "Test HTTP Ping".to_string(),
            probe_type: "http".to_string(),
            target: "https://example.com".to_string(),
            interval: 60,
            server_ids: vec![],
            enabled: true,
        }
    }

    #[tokio::test]
    async fn test_create_and_list_ping_task() {
        let (db, _tmp) = setup_test_db().await;
        let agent_manager = test_agent_manager();

        let input = sample_create_ping_task();
        let created = PingService::create(&db, &agent_manager, input).await.unwrap();

        let list = PingService::list(&db).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, created.id);
        assert_eq!(list[0].name, "Test HTTP Ping");
        assert_eq!(list[0].probe_type, "http");
    }

    #[tokio::test]
    async fn test_delete_ping_task() {
        let (db, _tmp) = setup_test_db().await;
        let agent_manager = test_agent_manager();

        let input = sample_create_ping_task();
        let created = PingService::create(&db, &agent_manager, input).await.unwrap();

        PingService::delete(&db, &agent_manager, &created.id).await.unwrap();

        let list = PingService::list(&db).await.unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn test_get_ping_task() {
        let (db, _tmp) = setup_test_db().await;
        let agent_manager = test_agent_manager();

        let input = sample_create_ping_task();
        let created = PingService::create(&db, &agent_manager, input).await.unwrap();

        let fetched = PingService::get(&db, &created.id).await.unwrap();
        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.target, "https://example.com");
        assert_eq!(fetched.interval, 60);
        assert!(fetched.enabled);
    }
}
