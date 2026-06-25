use chrono::{DateTime, Utc};
use sea_orm::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::{ping_record, ping_task, server};
use crate::error::AppError;
use crate::service::agent_manager::AgentManager;
use serverbee_common::constants::{CAP_DEFAULT, has_capability, probe_type_to_cap};
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
        serverbee_common::ssrf::reject_literal_unsafe_target(&input.target)
            .map_err(|e| AppError::Validation(e.to_string()))?;

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
            serverbee_common::ssrf::reject_literal_unsafe_target(&target)
                .map_err(|e| AppError::Validation(e.to_string()))?;
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
    pub async fn sync_tasks_to_agent(
        db: &DatabaseConnection,
        agent_manager: &AgentManager,
        server_id: &str,
    ) {
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
            let msg = ServerMessage::PingTasksSync {
                tasks: task_configs,
            };
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
        let server_caps_map: std::collections::HashMap<String, u32> = match server::Entity::find()
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
    use serverbee_common::constants::{CAP_PING_HTTP, CAP_PING_ICMP};

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
        let created = PingService::create(&db, &agent_manager, input)
            .await
            .unwrap();

        let list = PingService::list(&db).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, created.id);
        assert_eq!(list[0].name, "Test HTTP Ping");
        assert_eq!(list[0].probe_type, "http");
    }

    #[tokio::test]
    async fn create_rejects_literal_metadata_target() {
        let (db, _tmp) = setup_test_db().await;
        let agent_manager = test_agent_manager();

        let mut input = sample_create_ping_task();
        input.probe_type = "http".to_string();
        input.target = "http://169.254.169.254/latest/meta-data/".to_string();

        let result = PingService::create(&db, &agent_manager, input).await;
        assert!(
            result.is_err(),
            "creating a ping task targeting cloud metadata must be rejected"
        );
        assert!(PingService::list(&db).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn update_rejects_literal_loopback_target() {
        let (db, _tmp) = setup_test_db().await;
        let agent_manager = test_agent_manager();

        let created = PingService::create(&db, &agent_manager, sample_create_ping_task())
            .await
            .unwrap();

        let result = PingService::update(
            &db,
            &agent_manager,
            &created.id,
            UpdatePingTask {
                name: None,
                probe_type: Some("tcp".to_string()),
                target: Some("127.0.0.1:22".to_string()),
                interval: None,
                server_ids: None,
                enabled: None,
            },
        )
        .await;
        assert!(
            result.is_err(),
            "updating a ping task to a loopback target must be rejected"
        );
    }

    #[tokio::test]
    async fn test_delete_ping_task() {
        let (db, _tmp) = setup_test_db().await;
        let agent_manager = test_agent_manager();

        let input = sample_create_ping_task();
        let created = PingService::create(&db, &agent_manager, input)
            .await
            .unwrap();

        PingService::delete(&db, &agent_manager, &created.id)
            .await
            .unwrap();

        let list = PingService::list(&db).await.unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn test_get_ping_task() {
        let (db, _tmp) = setup_test_db().await;
        let agent_manager = test_agent_manager();

        let input = sample_create_ping_task();
        let created = PingService::create(&db, &agent_manager, input)
            .await
            .unwrap();

        let fetched = PingService::get(&db, &created.id).await.unwrap();
        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.target, "https://example.com");
        assert_eq!(fetched.interval, 60);
        assert!(fetched.enabled);
    }

    // --- helpers --------------------------------------------------------

    /// Seed a minimal `servers` row with the given capabilities mirror.
    async fn insert_test_server(db: &DatabaseConnection, id: &str, capabilities: u32) {
        let now = Utc::now();
        server::ActiveModel {
            id: Set(id.to_string()),
            name: Set(format!("srv-{id}")),
            weight: Set(0),
            hidden: Set(false),
            capabilities: Set(capabilities as i32),
            protocol_version: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert test server should succeed");
    }

    /// Seed a single ping record and return the inserted row id.
    async fn insert_ping_record(
        db: &DatabaseConnection,
        task_id: &str,
        server_id: &str,
        latency: f64,
        success: bool,
        time: DateTime<Utc>,
    ) -> i64 {
        ping_record::ActiveModel {
            task_id: Set(task_id.to_string()),
            server_id: Set(server_id.to_string()),
            latency: Set(latency),
            success: Set(success),
            error: Set(if success { None } else { Some("timeout".to_string()) }),
            time: Set(time),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert ping record should succeed")
        .id
    }

    fn empty_update() -> UpdatePingTask {
        UpdatePingTask {
            name: None,
            probe_type: None,
            target: None,
            interval: None,
            server_ids: None,
            enabled: None,
        }
    }

    /// Register a connected agent with a real mpsc sender so `get_sender`
    /// returns `Some`. Returns the receiver so the channel stays open and we
    /// can inspect delivered `ServerMessage`s.
    fn connect_agent(
        agent_manager: &crate::service::agent_manager::AgentManager,
        server_id: &str,
    ) -> tokio::sync::mpsc::Receiver<ServerMessage> {
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 8080));
        agent_manager.add_connection(server_id.to_string(), format!("srv-{server_id}"), tx, addr);
        rx
    }

    // --- get error path -------------------------------------------------

    #[tokio::test]
    async fn get_missing_task_returns_not_found() {
        let (db, _tmp) = setup_test_db().await;
        let result = PingService::get(&db, "does-not-exist").await;
        assert!(matches!(result, Err(AppError::NotFound(_))));
    }

    // --- create validation branches ------------------------------------

    #[tokio::test]
    async fn create_rejects_invalid_probe_type() {
        let (db, _tmp) = setup_test_db().await;
        let agent_manager = test_agent_manager();

        let mut input = sample_create_ping_task();
        input.probe_type = "udp".to_string();

        let result = PingService::create(&db, &agent_manager, input).await;
        assert!(matches!(result, Err(AppError::Validation(_))));
        // Nothing should have been persisted.
        assert!(PingService::list(&db).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn create_persists_server_ids_json() {
        let (db, _tmp) = setup_test_db().await;
        let agent_manager = test_agent_manager();

        let mut input = sample_create_ping_task();
        input.probe_type = "icmp".to_string();
        input.target = "1.1.1.1".to_string();
        input.server_ids = vec!["s1".to_string(), "s2".to_string()];

        let created = PingService::create(&db, &agent_manager, input)
            .await
            .unwrap();
        let parsed: Vec<String> = serde_json::from_str(&created.server_ids_json).unwrap();
        assert_eq!(parsed, vec!["s1".to_string(), "s2".to_string()]);
    }

    // --- update branches ------------------------------------------------

    #[tokio::test]
    async fn update_missing_task_returns_not_found() {
        let (db, _tmp) = setup_test_db().await;
        let agent_manager = test_agent_manager();

        let result = PingService::update(&db, &agent_manager, "ghost", empty_update()).await;
        assert!(matches!(result, Err(AppError::NotFound(_))));
    }

    #[tokio::test]
    async fn update_applies_all_fields() {
        let (db, _tmp) = setup_test_db().await;
        let agent_manager = test_agent_manager();

        let created = PingService::create(&db, &agent_manager, sample_create_ping_task())
            .await
            .unwrap();

        let updated = PingService::update(
            &db,
            &agent_manager,
            &created.id,
            UpdatePingTask {
                name: Some("Renamed".to_string()),
                probe_type: Some("icmp".to_string()),
                target: Some("8.8.8.8".to_string()),
                interval: Some(120),
                server_ids: Some(vec!["a".to_string()]),
                enabled: Some(false),
            },
        )
        .await
        .unwrap();

        assert_eq!(updated.name, "Renamed");
        assert_eq!(updated.probe_type, "icmp");
        assert_eq!(updated.target, "8.8.8.8");
        assert_eq!(updated.interval, 120);
        assert!(!updated.enabled);
        let parsed: Vec<String> = serde_json::from_str(&updated.server_ids_json).unwrap();
        assert_eq!(parsed, vec!["a".to_string()]);
    }

    #[tokio::test]
    async fn update_with_no_fields_is_noop() {
        let (db, _tmp) = setup_test_db().await;
        let agent_manager = test_agent_manager();

        let created = PingService::create(&db, &agent_manager, sample_create_ping_task())
            .await
            .unwrap();

        let updated = PingService::update(&db, &agent_manager, &created.id, empty_update())
            .await
            .unwrap();

        // All original values must be preserved.
        assert_eq!(updated.name, created.name);
        assert_eq!(updated.probe_type, created.probe_type);
        assert_eq!(updated.target, created.target);
        assert_eq!(updated.interval, created.interval);
        assert_eq!(updated.enabled, created.enabled);
    }

    #[tokio::test]
    async fn update_rejects_invalid_probe_type() {
        let (db, _tmp) = setup_test_db().await;
        let agent_manager = test_agent_manager();

        let created = PingService::create(&db, &agent_manager, sample_create_ping_task())
            .await
            .unwrap();

        let result = PingService::update(
            &db,
            &agent_manager,
            &created.id,
            UpdatePingTask {
                probe_type: Some("ftp".to_string()),
                ..empty_update()
            },
        )
        .await;
        assert!(matches!(result, Err(AppError::Validation(_))));

        // Original probe_type must be unchanged.
        let reloaded = PingService::get(&db, &created.id).await.unwrap();
        assert_eq!(reloaded.probe_type, "http");
    }

    // --- delete branches ------------------------------------------------

    #[tokio::test]
    async fn delete_missing_task_returns_not_found() {
        let (db, _tmp) = setup_test_db().await;
        let agent_manager = test_agent_manager();

        let result = PingService::delete(&db, &agent_manager, "ghost").await;
        assert!(matches!(result, Err(AppError::NotFound(_))));
    }

    #[tokio::test]
    async fn delete_removes_associated_records() {
        let (db, _tmp) = setup_test_db().await;
        let agent_manager = test_agent_manager();

        let created = PingService::create(&db, &agent_manager, sample_create_ping_task())
            .await
            .unwrap();

        let t = Utc::now();
        insert_ping_record(&db, &created.id, "s1", 10.0, true, t).await;
        insert_ping_record(&db, &created.id, "s1", 12.0, true, t).await;
        // A record for an unrelated task must survive the delete.
        insert_ping_record(&db, "other-task", "s1", 99.0, true, t).await;

        PingService::delete(&db, &agent_manager, &created.id)
            .await
            .unwrap();

        let remaining = ping_record::Entity::find().all(&db).await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].task_id, "other-task");
    }

    // --- get_records branches -------------------------------------------

    #[tokio::test]
    async fn get_records_filters_by_time_range_and_orders_ascending() {
        let (db, _tmp) = setup_test_db().await;

        let base = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let t1 = base;
        let t2 = base + chrono::Duration::minutes(10);
        let t3 = base + chrono::Duration::minutes(20);
        let before = base - chrono::Duration::minutes(10);
        let after = base + chrono::Duration::minutes(30);

        // Insert out of order to verify ascending sort.
        insert_ping_record(&db, "task-a", "s1", 30.0, true, t3).await;
        insert_ping_record(&db, "task-a", "s1", 10.0, true, t1).await;
        insert_ping_record(&db, "task-a", "s1", 20.0, false, t2).await;
        // Out-of-range and other-task records that must be excluded.
        insert_ping_record(&db, "task-a", "s1", 1.0, true, before).await;
        insert_ping_record(&db, "task-a", "s1", 2.0, true, after).await;
        insert_ping_record(&db, "task-b", "s1", 3.0, true, t2).await;

        let records = PingService::get_records(&db, "task-a", t1, t3, None)
            .await
            .unwrap();
        assert_eq!(records.len(), 3, "only in-range task-a records expected");
        assert_eq!(records[0].latency, 10.0);
        assert_eq!(records[1].latency, 20.0);
        assert_eq!(records[2].latency, 30.0);
        assert!(!records[1].success);
        assert_eq!(records[1].error.as_deref(), Some("timeout"));
    }

    #[tokio::test]
    async fn get_records_filters_by_server_id() {
        let (db, _tmp) = setup_test_db().await;

        let t = Utc::now();
        let from = t - chrono::Duration::hours(1);
        let to = t + chrono::Duration::hours(1);

        insert_ping_record(&db, "task-a", "s1", 10.0, true, t).await;
        insert_ping_record(&db, "task-a", "s2", 20.0, true, t).await;
        insert_ping_record(&db, "task-a", "s1", 11.0, true, t).await;

        let s1_records = PingService::get_records(&db, "task-a", from, to, Some("s1"))
            .await
            .unwrap();
        assert_eq!(s1_records.len(), 2);
        assert!(s1_records.iter().all(|r| r.server_id == "s1"));

        let all_records = PingService::get_records(&db, "task-a", from, to, None)
            .await
            .unwrap();
        assert_eq!(all_records.len(), 3);
    }

    #[tokio::test]
    async fn get_records_empty_when_no_match() {
        let (db, _tmp) = setup_test_db().await;
        let t = Utc::now();
        let records = PingService::get_records(
            &db,
            "missing",
            t - chrono::Duration::hours(1),
            t + chrono::Duration::hours(1),
            None,
        )
        .await
        .unwrap();
        assert!(records.is_empty());
    }

    // --- sync_tasks_to_agent (single) -----------------------------------

    /// Drain all currently-buffered messages from the agent's receiver.
    fn drain_messages(rx: &mut tokio::sync::mpsc::Receiver<ServerMessage>) -> Vec<ServerMessage> {
        let mut out = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            out.push(msg);
        }
        out
    }

    fn ping_tasks_from(msgs: &[ServerMessage]) -> Option<Vec<PingTaskConfig>> {
        msgs.iter().find_map(|m| match m {
            ServerMessage::PingTasksSync { tasks } => Some(tasks.clone()),
            _ => None,
        })
    }

    #[tokio::test]
    async fn sync_to_agent_includes_capable_tasks_and_skips_uncapable() {
        let (db, _tmp) = setup_test_db().await;
        let agent_manager = test_agent_manager();

        // Server supports only ICMP ping (and nothing else relevant).
        insert_test_server(&db, "s1", CAP_PING_ICMP).await;

        // ICMP task targeting all agents (empty server_ids) -> should be sent.
        PingService::create(
            &db,
            &agent_manager,
            CreatePingTask {
                name: "icmp-all".to_string(),
                probe_type: "icmp".to_string(),
                target: "1.1.1.1".to_string(),
                interval: 30,
                server_ids: vec![],
                enabled: true,
            },
        )
        .await
        .unwrap();
        // HTTP task -> server lacks CAP_PING_HTTP, must be filtered out.
        PingService::create(
            &db,
            &agent_manager,
            CreatePingTask {
                name: "http-all".to_string(),
                probe_type: "http".to_string(),
                target: "https://example.com".to_string(),
                interval: 30,
                server_ids: vec![],
                enabled: true,
            },
        )
        .await
        .unwrap();

        let mut rx = connect_agent(&agent_manager, "s1");
        PingService::sync_tasks_to_agent(&db, &agent_manager, "s1").await;

        let msgs = drain_messages(&mut rx);
        let tasks = ping_tasks_from(&msgs).expect("a PingTasksSync must be delivered");
        assert_eq!(tasks.len(), 1, "only the ICMP task is capability-allowed");
        assert_eq!(tasks[0].probe_type, "icmp");
        assert_eq!(tasks[0].interval, 30);
    }

    #[tokio::test]
    async fn sync_to_agent_respects_server_id_targeting() {
        let (db, _tmp) = setup_test_db().await;
        let agent_manager = test_agent_manager();

        insert_test_server(&db, "s1", CAP_DEFAULT).await;

        // Task scoped to a different server -> not for s1.
        PingService::create(
            &db,
            &agent_manager,
            CreatePingTask {
                name: "scoped-other".to_string(),
                probe_type: "icmp".to_string(),
                target: "1.1.1.1".to_string(),
                interval: 60,
                server_ids: vec!["s2".to_string()],
                enabled: true,
            },
        )
        .await
        .unwrap();
        // Task explicitly scoped to s1 -> included.
        PingService::create(
            &db,
            &agent_manager,
            CreatePingTask {
                name: "scoped-s1".to_string(),
                probe_type: "tcp".to_string(),
                target: "1.1.1.1:53".to_string(),
                interval: 60,
                server_ids: vec!["s1".to_string()],
                enabled: true,
            },
        )
        .await
        .unwrap();

        let mut rx = connect_agent(&agent_manager, "s1");
        PingService::sync_tasks_to_agent(&db, &agent_manager, "s1").await;

        let msgs = drain_messages(&mut rx);
        let tasks = ping_tasks_from(&msgs).expect("a PingTasksSync must be delivered");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].probe_type, "tcp");
    }

    #[tokio::test]
    async fn sync_to_agent_excludes_disabled_tasks() {
        let (db, _tmp) = setup_test_db().await;
        let agent_manager = test_agent_manager();

        insert_test_server(&db, "s1", CAP_DEFAULT).await;

        PingService::create(
            &db,
            &agent_manager,
            CreatePingTask {
                name: "disabled".to_string(),
                probe_type: "icmp".to_string(),
                target: "1.1.1.1".to_string(),
                interval: 60,
                server_ids: vec![],
                enabled: false,
            },
        )
        .await
        .unwrap();

        let mut rx = connect_agent(&agent_manager, "s1");
        PingService::sync_tasks_to_agent(&db, &agent_manager, "s1").await;

        let msgs = drain_messages(&mut rx);
        // Sync still fires (telling the agent to stop probes) but with no tasks.
        let tasks = ping_tasks_from(&msgs).expect("a PingTasksSync must be delivered");
        assert!(tasks.is_empty());
    }

    #[tokio::test]
    async fn sync_to_agent_uses_cap_default_when_server_row_missing() {
        let (db, _tmp) = setup_test_db().await;
        let agent_manager = test_agent_manager();

        // No `servers` row for s1 -> falls back to CAP_DEFAULT (ICMP allowed).
        PingService::create(
            &db,
            &agent_manager,
            CreatePingTask {
                name: "icmp".to_string(),
                probe_type: "icmp".to_string(),
                target: "1.1.1.1".to_string(),
                interval: 60,
                server_ids: vec![],
                enabled: true,
            },
        )
        .await
        .unwrap();

        let mut rx = connect_agent(&agent_manager, "s1");
        PingService::sync_tasks_to_agent(&db, &agent_manager, "s1").await;

        let msgs = drain_messages(&mut rx);
        let tasks = ping_tasks_from(&msgs).expect("a PingTasksSync must be delivered");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].probe_type, "icmp");
    }

    #[tokio::test]
    async fn sync_to_agent_noop_when_agent_not_connected() {
        let (db, _tmp) = setup_test_db().await;
        let agent_manager = test_agent_manager();

        insert_test_server(&db, "s1", CAP_DEFAULT).await;
        PingService::create(&db, &agent_manager, sample_create_ping_task())
            .await
            .unwrap();

        // No connection registered -> get_sender returns None, nothing to assert
        // beyond "does not panic".
        PingService::sync_tasks_to_agent(&db, &agent_manager, "s1").await;
    }

    // --- sync_tasks_to_agents (plural, exercised via create/update/delete) -

    #[tokio::test]
    async fn create_syncs_per_agent_filtered_by_capability() {
        let (db, _tmp) = setup_test_db().await;
        let agent_manager = test_agent_manager();

        // s1 supports HTTP ping, s2 does not.
        insert_test_server(&db, "s1", CAP_PING_HTTP).await;
        insert_test_server(&db, "s2", CAP_PING_ICMP).await;
        let mut rx1 = connect_agent(&agent_manager, "s1");
        let mut rx2 = connect_agent(&agent_manager, "s2");

        // Creating the task triggers sync_tasks_to_agents over all connected agents.
        PingService::create(
            &db,
            &agent_manager,
            CreatePingTask {
                name: "http-all".to_string(),
                probe_type: "http".to_string(),
                target: "https://example.com".to_string(),
                interval: 45,
                server_ids: vec![],
                enabled: true,
            },
        )
        .await
        .unwrap();

        let tasks1 = ping_tasks_from(&drain_messages(&mut rx1)).expect("s1 gets a sync");
        assert_eq!(tasks1.len(), 1, "s1 has HTTP capability");
        assert_eq!(tasks1[0].interval, 45);

        let tasks2 = ping_tasks_from(&drain_messages(&mut rx2)).expect("s2 gets a sync");
        assert!(tasks2.is_empty(), "s2 lacks HTTP capability");
    }

    #[tokio::test]
    async fn create_syncs_explicit_server_ids_to_targeted_agent_only() {
        let (db, _tmp) = setup_test_db().await;
        let agent_manager = test_agent_manager();

        insert_test_server(&db, "s1", CAP_DEFAULT).await;
        insert_test_server(&db, "s2", CAP_DEFAULT).await;
        let mut rx1 = connect_agent(&agent_manager, "s1");
        let mut rx2 = connect_agent(&agent_manager, "s2");

        PingService::create(
            &db,
            &agent_manager,
            CreatePingTask {
                name: "scoped-s1".to_string(),
                probe_type: "icmp".to_string(),
                target: "1.1.1.1".to_string(),
                interval: 60,
                server_ids: vec!["s1".to_string()],
                enabled: true,
            },
        )
        .await
        .unwrap();

        let tasks1 = ping_tasks_from(&drain_messages(&mut rx1)).expect("s1 gets a sync");
        assert_eq!(tasks1.len(), 1);

        let tasks2 = ping_tasks_from(&drain_messages(&mut rx2)).expect("s2 gets a sync");
        assert!(tasks2.is_empty(), "task is scoped to s1 only");
    }
}
