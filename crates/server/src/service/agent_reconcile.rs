//! Reconciles the server-owned desired state that agents execute.
//!
//! Routers and WebSocket handlers own transport concerns. This module owns the
//! fetch -> map -> send lifecycle for every full-state agent configuration,
//! including provider-specific capability and protocol policy. Firewall
//! mutations deliberately remain incremental;
//! the firewall provider here is the authoritative reset + sync path used on
//! connection and for explicit full repairs.

use std::collections::HashMap;
use std::sync::Arc;

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serverbee_common::constants::{
    CAP_DEFAULT, CAP_FIREWALL_BLOCK, has_capability, probe_type_to_cap,
};
use serverbee_common::firewall::FIREWALL_MIN_PROTOCOL;
use serverbee_common::protocol::{ServerMessage, UnlockServiceDef};
use serverbee_common::types::{NetworkProbeTarget, PingTaskConfig};
use tokio::sync::Mutex;

use crate::entity::{ping_task, server};
use crate::error::AppError;
use crate::service::agent_manager::AgentManager;
use crate::service::firewall::FirewallService;
use crate::service::ip_quality::IpQualityService;
use crate::service::network_probe::{NetworkProbeService, NetworkProbeSetting, TargetDto};

/// A full-state configuration owned by the server and executed by an agent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentDesiredStateDomain {
    PingTasks,
    NetworkProbes,
    IpQuality,
    Firewall,
}

/// Serializes each provider's read-and-send sequence.
///
/// A mutation commits before it requests reconciliation. Holding the provider
/// lock across both the database read and the sends means two concurrent
/// mutations can send the same newest snapshot, but can never send an older
/// snapshot after a newer one.
#[derive(Default)]
struct ProviderLocks {
    ping_tasks: Mutex<()>,
    network_probes: Mutex<()>,
    ip_quality: Mutex<()>,
    firewall: Mutex<()>,
}

impl ProviderLocks {
    fn for_domain(&self, domain: AgentDesiredStateDomain) -> &Mutex<()> {
        match domain {
            AgentDesiredStateDomain::PingTasks => &self.ping_tasks,
            AgentDesiredStateDomain::NetworkProbes => &self.network_probes,
            AgentDesiredStateDomain::IpQuality => &self.ip_quality,
            AgentDesiredStateDomain::Firewall => &self.firewall,
        }
    }
}

/// Owns projection of persisted desired state into agent protocol messages.
#[derive(Clone)]
pub struct AgentDesiredStateReconciler {
    db: DatabaseConnection,
    agent_manager: Arc<AgentManager>,
    firewall: Arc<FirewallService>,
    locks: Arc<ProviderLocks>,
}

impl AgentDesiredStateReconciler {
    pub fn new(
        db: DatabaseConnection,
        agent_manager: Arc<AgentManager>,
        firewall: Arc<FirewallService>,
    ) -> Self {
        Self {
            db,
            agent_manager,
            firewall,
            locks: Arc::new(ProviderLocks::default()),
        }
    }

    /// Reconcile every desired-state domain after a new connection reports its
    /// live capabilities and protocol version. All domains are attempted even
    /// if one provider fails.
    pub async fn reconcile_connection(&self, server_id: &str) -> Result<(), AppError> {
        let mut first_error = None;
        for domain in [
            AgentDesiredStateDomain::PingTasks,
            AgentDesiredStateDomain::NetworkProbes,
            AgentDesiredStateDomain::IpQuality,
            AgentDesiredStateDomain::Firewall,
        ] {
            if let Err(error) = self.reconcile_agent(server_id, domain).await {
                tracing::warn!(
                    server_id,
                    ?domain,
                    error = %error,
                    "failed to reconcile agent desired state"
                );
                first_error.get_or_insert(error);
            }
        }

        first_error.map_or(Ok(()), Err)
    }

    /// Reconcile one provider for one agent.
    pub async fn reconcile_agent(
        &self,
        server_id: &str,
        domain: AgentDesiredStateDomain,
    ) -> Result<(), AppError> {
        let _guard = self.locks.for_domain(domain).lock().await;
        match domain {
            AgentDesiredStateDomain::PingTasks => {
                let tasks = self.load_ping_tasks().await?;
                let capabilities = self.capabilities_for_agent(server_id).await?;
                self.send_ping_tasks(server_id, &tasks, capabilities).await;
                Ok(())
            }
            AgentDesiredStateDomain::NetworkProbes => {
                let setting = NetworkProbeService::get_setting(&self.db).await?;
                self.send_network_probes(server_id, &setting).await
            }
            AgentDesiredStateDomain::IpQuality => self.send_ip_quality(server_id).await,
            AgentDesiredStateDomain::Firewall => self.send_firewall(server_id).await,
        }
    }

    /// Reconcile one provider for every agent that is online at the start of
    /// this call. Provider-global state is fetched once and reused for the
    /// complete fan-out.
    pub async fn reconcile_connected(
        &self,
        domain: AgentDesiredStateDomain,
    ) -> Result<(), AppError> {
        let _guard = self.locks.for_domain(domain).lock().await;
        match domain {
            AgentDesiredStateDomain::PingTasks => self.reconcile_connected_ping_tasks().await,
            AgentDesiredStateDomain::NetworkProbes => {
                self.reconcile_connected_network_probes().await
            }
            AgentDesiredStateDomain::IpQuality => self.reconcile_connected_ip_quality().await,
            AgentDesiredStateDomain::Firewall => self.reconcile_connected_firewall().await,
        }
    }

    /// [`Self::reconcile_connected`], demoted to a warning. Mutation handlers
    /// must not fail their HTTP request over a push failure — the agent's
    /// state converges on its next connection reconcile.
    pub async fn reconcile_connected_or_warn(&self, domain: AgentDesiredStateDomain) {
        if let Err(error) = self.reconcile_connected(domain).await {
            tracing::warn!(
                ?domain,
                error = %error,
                "failed to reconcile connected agents' desired state"
            );
        }
    }

    /// [`Self::reconcile_agent`], demoted to a warning under the same
    /// contract as [`Self::reconcile_connected_or_warn`].
    pub async fn reconcile_agent_or_warn(&self, server_id: &str, domain: AgentDesiredStateDomain) {
        if let Err(error) = self.reconcile_agent(server_id, domain).await {
            tracing::warn!(
                server_id,
                ?domain,
                error = %error,
                "failed to reconcile agent desired state"
            );
        }
    }

    async fn reconcile_connected_ping_tasks(&self) -> Result<(), AppError> {
        let server_ids = self.agent_manager.connected_server_ids();
        let tasks = self.load_ping_tasks().await?;
        let capabilities = self.capabilities_for_agents(&server_ids).await?;

        for server_id in server_ids {
            let server_capabilities = capabilities.get(&server_id).copied().unwrap_or(CAP_DEFAULT);
            self.send_ping_tasks(&server_id, &tasks, server_capabilities)
                .await;
        }
        Ok(())
    }

    async fn reconcile_connected_network_probes(&self) -> Result<(), AppError> {
        let server_ids = self.agent_manager.connected_server_ids();
        let setting = NetworkProbeService::get_setting(&self.db).await?;
        let mut first_error = None;

        for server_id in server_ids {
            if let Err(error) = self.send_network_probes(&server_id, &setting).await {
                tracing::warn!(
                    server_id,
                    error = %error,
                    "failed to reconcile network probe desired state"
                );
                first_error.get_or_insert(error);
            }
        }

        first_error.map_or(Ok(()), Err)
    }

    async fn reconcile_connected_ip_quality(&self) -> Result<(), AppError> {
        let server_ids = self.agent_manager.connected_server_ids();
        let (services, interval_hours) = self.load_ip_quality().await?;

        for server_id in server_ids {
            self.send_if_online(
                &server_id,
                ServerMessage::IpQualitySync {
                    services: services.clone(),
                    interval_hours,
                },
            )
            .await;
        }
        Ok(())
    }

    async fn reconcile_connected_firewall(&self) -> Result<(), AppError> {
        let server_ids = self.agent_manager.connected_server_ids();
        let mut first_error = None;

        for server_id in server_ids {
            if let Err(error) = self.send_firewall(&server_id).await {
                tracing::warn!(
                    server_id,
                    error = %error,
                    "failed to reconcile firewall desired state"
                );
                first_error.get_or_insert(error);
            }
        }

        first_error.map_or(Ok(()), Err)
    }

    async fn load_ping_tasks(&self) -> Result<Vec<ping_task::Model>, AppError> {
        Ok(ping_task::Entity::find()
            .filter(ping_task::Column::Enabled.eq(true))
            .all(&self.db)
            .await?)
    }

    async fn capabilities_for_agent(&self, server_id: &str) -> Result<u32, AppError> {
        if let Some(capabilities) = self.agent_manager.get_effective_capabilities(server_id) {
            return Ok(capabilities);
        }

        let mirrored = server::Entity::find_by_id(server_id).one(&self.db).await?;
        Ok(mirrored
            .and_then(|model| u32::try_from(model.capabilities).ok())
            .unwrap_or(CAP_DEFAULT))
    }

    async fn capabilities_for_agents(
        &self,
        server_ids: &[String],
    ) -> Result<HashMap<String, u32>, AppError> {
        if server_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let mirrored: HashMap<String, u32> = server::Entity::find()
            .filter(server::Column::Id.is_in(server_ids.iter().cloned()))
            .all(&self.db)
            .await?
            .into_iter()
            .filter_map(|model| {
                u32::try_from(model.capabilities)
                    .ok()
                    .map(|capabilities| (model.id, capabilities))
            })
            .collect();

        Ok(server_ids
            .iter()
            .map(|server_id| {
                let capabilities = self
                    .agent_manager
                    .get_effective_capabilities(server_id)
                    .or_else(|| mirrored.get(server_id).copied())
                    .unwrap_or(CAP_DEFAULT);
                (server_id.clone(), capabilities)
            })
            .collect())
    }

    async fn agent_has_capability(
        &self,
        server_id: &str,
        capability: u32,
    ) -> Result<bool, AppError> {
        Ok(has_capability(
            self.capabilities_for_agent(server_id).await?,
            capability,
        ))
    }

    async fn send_ping_tasks(
        &self,
        server_id: &str,
        tasks: &[ping_task::Model],
        capabilities: u32,
    ) {
        let tasks = ping_configs_for_agent(tasks, server_id, capabilities);
        self.send_if_online(server_id, ServerMessage::PingTasksSync { tasks })
            .await;
    }

    async fn send_network_probes(
        &self,
        server_id: &str,
        setting: &NetworkProbeSetting,
    ) -> Result<(), AppError> {
        let targets = NetworkProbeService::get_server_targets(&self.db, server_id).await?;
        let targets = network_targets_for_agent(targets);

        self.send_if_online(
            server_id,
            ServerMessage::NetworkProbeSync {
                targets,
                interval: setting.interval,
                packet_count: setting.packet_count,
            },
        )
        .await;
        Ok(())
    }

    /// Load the enabled unlock-service catalog and check interval that every
    /// `IpQualitySync` frame carries.
    async fn load_ip_quality(&self) -> Result<(Vec<UnlockServiceDef>, u32), AppError> {
        let services = IpQualityService::enabled_service_defs(&self.db).await?;
        let interval_hours = ip_quality_interval_hours(
            IpQualityService::get_setting(&self.db)
                .await?
                .check_interval_hours,
        )?;
        Ok((services, interval_hours))
    }

    async fn send_ip_quality(&self, server_id: &str) -> Result<(), AppError> {
        let (services, interval_hours) = self.load_ip_quality().await?;
        self.send_if_online(
            server_id,
            ServerMessage::IpQualitySync {
                services,
                interval_hours,
            },
        )
        .await;
        Ok(())
    }

    async fn send_firewall(&self, server_id: &str) -> Result<(), AppError> {
        let protocol_version = self
            .agent_manager
            .get_protocol_version(server_id)
            .unwrap_or_default();
        if protocol_version < FIREWALL_MIN_PROTOCOL {
            return Ok(());
        }

        // Reset is intentionally sent even when the capability is disabled.
        // It removes ServerBee's stale nftables state after capability
        // revocation; the agent protocol defines reset as an ungated cleanup.
        self.firewall
            .push_reset_to(server_id, &self.agent_manager)
            .await;
        if self
            .agent_has_capability(server_id, CAP_FIREWALL_BLOCK)
            .await?
        {
            self.firewall
                .push_sync_to(server_id, &self.agent_manager)
                .await?;
        }
        Ok(())
    }

    async fn send_if_online(&self, server_id: &str, message: ServerMessage) {
        let Some(sender) = self.agent_manager.get_sender(server_id) else {
            return;
        };
        if sender.send(message).await.is_err() {
            tracing::debug!(
                server_id,
                "agent disconnected during desired-state reconcile"
            );
        }
    }
}

fn ping_configs_for_agent(
    tasks: &[ping_task::Model],
    server_id: &str,
    capabilities: u32,
) -> Vec<PingTaskConfig> {
    let mut configs = Vec::new();

    for task in tasks {
        let Ok(server_ids) = serde_json::from_str::<Vec<String>>(&task.server_ids_json) else {
            tracing::warn!(
                task_id = task.id,
                "ignoring ping task with an invalid persisted server assignment"
            );
            continue;
        };
        if !server_ids.is_empty() && !server_ids.iter().any(|id| id == server_id) {
            continue;
        }

        let Some(capability) = probe_type_to_cap(&task.probe_type) else {
            tracing::warn!(
                task_id = task.id,
                probe_type = task.probe_type,
                "ignoring ping task with unknown probe type"
            );
            continue;
        };
        if !has_capability(capabilities, capability) {
            continue;
        }

        let Ok(interval) = u32::try_from(task.interval) else {
            tracing::warn!(
                task_id = task.id,
                interval = task.interval,
                "ignoring ping task with a negative interval"
            );
            continue;
        };
        configs.push(PingTaskConfig {
            task_id: task.id.clone(),
            probe_type: task.probe_type.clone(),
            target: task.target.clone(),
            interval,
        });
    }

    configs
}

fn network_targets_for_agent(targets: Vec<TargetDto>) -> Vec<NetworkProbeTarget> {
    targets
        .into_iter()
        .map(|target| NetworkProbeTarget {
            target_id: target.id,
            name: target.name,
            target: target.target,
            probe_type: target.probe_type,
        })
        .collect()
}

fn ip_quality_interval_hours(value: i32) -> Result<u32, AppError> {
    u32::try_from(value).map_err(|_| {
        tracing::error!(
            interval_hours = value,
            "invalid persisted IP quality check interval"
        );
        AppError::Internal("Invalid persisted IP quality setting".to_string())
    })
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use chrono::Utc;
    use sea_orm::{ActiveModelTrait, Set};
    use serverbee_common::constants::{
        CAP_FIREWALL_BLOCK, CAP_IP_QUALITY, CAP_PING_HTTP, CAP_PING_ICMP,
    };
    use tokio::sync::{broadcast, mpsc};

    use super::*;
    use crate::config::AppConfig;
    use crate::test_utils::setup_test_db;

    fn test_addr(port: u16) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
    }

    fn test_reconciler(
        db: &DatabaseConnection,
    ) -> (AgentDesiredStateReconciler, Arc<AgentManager>) {
        let (browser_tx, _) = broadcast::channel(16);
        let agent_manager = Arc::new(AgentManager::new(browser_tx.clone()));
        let firewall = Arc::new(FirewallService::new(
            db.clone(),
            Arc::new(AppConfig::default()),
            browser_tx,
        ));
        (
            AgentDesiredStateReconciler::new(db.clone(), agent_manager.clone(), firewall),
            agent_manager,
        )
    }

    async fn receive_messages(
        receiver: &mut mpsc::Receiver<ServerMessage>,
        count: usize,
    ) -> Vec<ServerMessage> {
        let mut messages = Vec::with_capacity(count);
        for _ in 0..count {
            messages.push(
                tokio::time::timeout(std::time::Duration::from_secs(1), receiver.recv())
                    .await
                    .expect("desired-state message timed out")
                    .expect("agent channel closed"),
            );
        }
        messages
    }

    #[tokio::test]
    async fn connection_reconcile_attempts_every_domain() {
        let (db, _tmp) = setup_test_db().await;
        let (reconciler, agent_manager) = test_reconciler(&db);
        let (sender, mut receiver) = mpsc::channel(16);
        agent_manager.add_connection(
            "server-1".to_string(),
            "Server 1".to_string(),
            sender,
            test_addr(10001),
        );
        agent_manager.update_agent_local_capabilities(
            "server-1",
            CAP_PING_ICMP | CAP_IP_QUALITY | CAP_FIREWALL_BLOCK,
        );
        agent_manager.set_protocol_version("server-1", FIREWALL_MIN_PROTOCOL);

        reconciler.reconcile_connection("server-1").await.unwrap();

        let messages = receive_messages(&mut receiver, 5).await;
        assert!(messages.iter().any(
            |message| matches!(message, ServerMessage::PingTasksSync { tasks } if tasks.is_empty())
        ));
        assert!(messages.iter().any(
            |message| matches!(message, ServerMessage::NetworkProbeSync { targets, .. } if targets.is_empty())
        ));
        assert!(
            messages
                .iter()
                .any(|message| matches!(message, ServerMessage::IpQualitySync { .. }))
        );
        assert!(
            messages
                .iter()
                .any(|message| matches!(message, ServerMessage::BlocklistReset))
        );
        assert!(messages.iter().any(
            |message| matches!(message, ServerMessage::BlocklistSync { entries } if entries.is_empty())
        ));
    }

    #[tokio::test]
    async fn ping_reconcile_filters_scope_and_live_capabilities() {
        let (db, _tmp) = setup_test_db().await;
        ping_task::ActiveModel {
            id: Set("http-task".to_string()),
            name: Set("HTTP".to_string()),
            probe_type: Set("http".to_string()),
            target: Set("https://example.com".to_string()),
            interval: Set(60),
            server_ids_json: Set("[]".to_string()),
            enabled: Set(true),
            created_at: Set(Utc::now()),
        }
        .insert(&db)
        .await
        .unwrap();
        ping_task::ActiveModel {
            id: Set("icmp-task".to_string()),
            name: Set("ICMP".to_string()),
            probe_type: Set("icmp".to_string()),
            target: Set("1.1.1.1".to_string()),
            interval: Set(60),
            server_ids_json: Set("[\"another-server\"]".to_string()),
            enabled: Set(true),
            created_at: Set(Utc::now()),
        }
        .insert(&db)
        .await
        .unwrap();
        ping_task::ActiveModel {
            id: Set("corrupt-task".to_string()),
            name: Set("Corrupt".to_string()),
            probe_type: Set("http".to_string()),
            target: Set("https://example.net".to_string()),
            interval: Set(60),
            server_ids_json: Set("not-json".to_string()),
            enabled: Set(true),
            created_at: Set(Utc::now()),
        }
        .insert(&db)
        .await
        .unwrap();

        let (reconciler, agent_manager) = test_reconciler(&db);
        let (sender, mut receiver) = mpsc::channel(4);
        agent_manager.add_connection(
            "server-1".to_string(),
            "Server 1".to_string(),
            sender,
            test_addr(10002),
        );
        agent_manager.update_agent_local_capabilities("server-1", CAP_PING_HTTP);

        reconciler
            .reconcile_agent("server-1", AgentDesiredStateDomain::PingTasks)
            .await
            .unwrap();

        let message = receiver.recv().await.unwrap();
        let ServerMessage::PingTasksSync { tasks } = message else {
            panic!("expected ping task sync");
        };
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].task_id, "http-task");
    }

    #[tokio::test]
    async fn connected_reconcile_only_sends_the_requested_domain() {
        let (db, _tmp) = setup_test_db().await;
        let (reconciler, agent_manager) = test_reconciler(&db);
        let (sender_a, mut receiver_a) = mpsc::channel(4);
        let (sender_b, mut receiver_b) = mpsc::channel(4);
        agent_manager.add_connection(
            "server-a".to_string(),
            "Server A".to_string(),
            sender_a,
            test_addr(10003),
        );
        agent_manager.add_connection(
            "server-b".to_string(),
            "Server B".to_string(),
            sender_b,
            test_addr(10004),
        );
        agent_manager.update_agent_local_capabilities("server-a", CAP_PING_ICMP);
        agent_manager.update_agent_local_capabilities("server-b", CAP_PING_ICMP);

        reconciler
            .reconcile_connected(AgentDesiredStateDomain::NetworkProbes)
            .await
            .unwrap();

        assert!(matches!(
            receiver_a.recv().await.unwrap(),
            ServerMessage::NetworkProbeSync { .. }
        ));
        assert!(matches!(
            receiver_b.recv().await.unwrap(),
            ServerMessage::NetworkProbeSync { .. }
        ));
        assert!(receiver_a.try_recv().is_err());
        assert!(receiver_b.try_recv().is_err());
    }

    #[tokio::test]
    async fn firewall_reconcile_resets_state_after_capability_revocation() {
        let (db, _tmp) = setup_test_db().await;
        let (reconciler, agent_manager) = test_reconciler(&db);
        let (sender, mut receiver) = mpsc::channel(4);
        agent_manager.add_connection(
            "server-1".to_string(),
            "Server 1".to_string(),
            sender,
            test_addr(10005),
        );
        agent_manager.update_agent_local_capabilities("server-1", 0);
        agent_manager.set_protocol_version("server-1", FIREWALL_MIN_PROTOCOL);

        reconciler
            .reconcile_agent("server-1", AgentDesiredStateDomain::Firewall)
            .await
            .unwrap();

        assert!(matches!(
            receiver.recv().await.unwrap(),
            ServerMessage::BlocklistReset
        ));
        assert!(receiver.try_recv().is_err());
    }
}
