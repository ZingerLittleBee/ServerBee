//! `SystemInfo` / `IpChanged` handling: agent identity, GeoIP resolution,
//! capability mirroring, IP-change detection, and connection-time desired-state
//! reconciliation.

use std::sync::Arc;

use crate::{entity::server, error::AppError, service::alert_event_intents};
use sea_orm::{ActiveModelTrait, ConnectionTrait, EntityTrait, Set, TransactionTrait};

use crate::service::audit::AuditService;
use crate::service::geoip;
use crate::service::server::ServerService;
use crate::service::upgrade_tracker::UpgradeLookup;
use crate::state::AppState;
use serverbee_common::protocol::{BrowserMessage, ServerMessage, TemporaryGrant};
use serverbee_common::types::SystemInfo;

/// Pick the first public candidate from agent-reported IPs, falling back to
/// the connection's remote address. Loopback/private addresses are skipped —
/// GeoIP can't resolve those (e.g. agents inside a docker container report
/// the bridge gateway 172.17.0.1 as their primary IP).
fn resolve_public_ip(
    state: &AppState,
    server_id: &str,
    ipv4: Option<&str>,
    ipv6: Option<&str>,
) -> Option<std::net::IpAddr> {
    let parse = |s: Option<&str>| s.and_then(|v| v.parse::<std::net::IpAddr>().ok());
    let candidates = [
        parse(ipv4),
        parse(ipv6),
        state
            .agent_manager
            .get_remote_addr(server_id)
            .map(|addr| addr.ip()),
    ];
    candidates
        .into_iter()
        .flatten()
        .find(|ip| !ip.is_loopback() && !geoip::is_private(ip))
}

/// A detected change of a server's externally visible addresses.
struct IpChange {
    old_ipv4: Option<String>,
    new_ipv4: Option<String>,
    old_ipv6: Option<String>,
    new_ipv6: Option<String>,
    old_remote_addr: Option<String>,
    new_remote_addr: Option<String>,
}

impl IpChange {
    /// Any difference at all, including first population (`None` → `Some`).
    /// Drives the alert check and the browser broadcast.
    fn changed(&self) -> bool {
        self.old_ipv4 != self.new_ipv4
            || self.old_ipv6 != self.new_ipv6
            || self.old_remote_addr != self.new_remote_addr
    }

    /// A real transition (`Some` → different value) on at least one field.
    /// Drives the audit trail: first population happens on every fresh
    /// registration and must not spam the audit log.
    fn is_transition(&self) -> bool {
        fn transitioned(old: &Option<String>, new: &Option<String>) -> bool {
            old.is_some() && old != new
        }
        transitioned(&self.old_ipv4, &self.new_ipv4)
            || transitioned(&self.old_ipv6, &self.new_ipv6)
            || transitioned(&self.old_remote_addr, &self.new_remote_addr)
    }

    fn detail(&self, server_id: &str) -> String {
        let mut parts = Vec::new();
        if self.old_ipv4 != self.new_ipv4 {
            parts.push(format!("ipv4 {:?} -> {:?}", self.old_ipv4, self.new_ipv4));
        }
        if self.old_ipv6 != self.new_ipv6 {
            parts.push(format!("ipv6 {:?} -> {:?}", self.old_ipv6, self.new_ipv6));
        }
        if self.old_remote_addr != self.new_remote_addr {
            parts.push(format!(
                "remote {:?} -> {:?}",
                self.old_remote_addr, self.new_remote_addr
            ));
        }
        format!("IP changed for server {server_id}: {}", parts.join(", "))
    }
}

/// The single reaction to a detected IP change, shared by the `SystemInfo`
/// and `IpChanged` paths: audit trail, alert event rules, browser broadcast.
/// The caller atomically commits source addresses and durable alert intents
/// before this best-effort audit, replay attempt and browser broadcast.
async fn apply_ip_change(state: &Arc<AppState>, server_id: &str, change: IpChange) {
    if !change.changed() {
        return;
    }

    if change.is_transition() {
        let detail = change.detail(server_id);
        tracing::info!("{detail}");

        let audit_ip = change
            .new_remote_addr
            .clone()
            .or_else(|| {
                state
                    .agent_manager
                    .get_remote_addr(server_id)
                    .map(|a| a.ip().to_string())
            })
            .unwrap_or_default();
        if let Err(e) =
            AuditService::log(&state.db, "system", "ip_changed", Some(&detail), &audit_ip).await
        {
            tracing::error!("Failed to write audit log for IP change: {e}");
        }
    }

    // The source transaction already captured every admitted event intent.
    // Queue failures are retried automatically, without another IP change.
    if let Err(error) =
        alert_event_intents::replay(&state.db, &state.config, &state.alert_state_manager).await
    {
        tracing::error!("Failed to replay captured IP alert intents: {error}");
    }

    state
        .agent_manager
        .broadcast_browser(BrowserMessage::ServerIpChanged {
            server_id: server_id.to_string(),
            old_ipv4: change.old_ipv4,
            new_ipv4: change.new_ipv4,
            old_ipv6: change.old_ipv6,
            new_ipv6: change.new_ipv6,
            old_remote_addr: change.old_remote_addr,
            new_remote_addr: change.new_remote_addr,
        });
}

pub(super) async fn on_system_info(
    state: &Arc<AppState>,
    server_id: &str,
    msg_id: String,
    info: SystemInfo,
    agent_local_capabilities: Option<u32>,
    temporary: Vec<TemporaryGrant>,
) -> bool {
    // Mirror the agent-reported temporary grants so the REST DTO and
    // browser broadcasts can render live countdowns. The agent host is
    // the only authority; this is a display cache.
    state
        .agent_manager
        .update_temporary_grants(server_id, temporary.clone());

    // Resolve GeoIP from the candidate chain agent ipv4 → ipv6 → remote_addr.
    let ip = resolve_public_ip(state, server_id, info.ipv4.as_deref(), info.ipv6.as_deref());

    let (region, country_code) = match ip {
        Some(ip) => {
            let guard = state.geoip.read().unwrap();
            match guard.as_ref() {
                Some(g) => {
                    let geo = g.lookup(ip);
                    (geo.region, geo.country_code)
                }
                None => (None, None),
            }
        }
        None => (None, None),
    };

    // --- Passive IP change detection (remote_addr) ---
    let current_remote_addr = state
        .agent_manager
        .get_remote_addr(server_id)
        .map(|a| a.ip().to_string());

    let change = match persist_system_info(
        state,
        server_id,
        &info,
        region,
        country_code,
        current_remote_addr,
    )
    .await
    {
        Ok(change) => change,
        Err(error) => {
            // No address update/Ack consumes a failed capture. Close this socket
            // so the Agent reconnects with SystemInfo against the old baseline.
            tracing::error!("System info and event-intent transaction failed: {error}");
            return false;
        }
    };
    apply_ip_change(state, server_id, change).await;

    let _ = ServerService::update_features(&state.db, server_id, &info.features).await;
    state
        .agent_manager
        .update_features(server_id, info.features.clone());

    // Update in-memory protocol_version
    let agent_pv = info.protocol_version;
    state
        .agent_manager
        .set_protocol_version(server_id, agent_pv);

    // Store os/arch for upgrade platform mapping
    state
        .agent_manager
        .update_agent_platform(server_id, info.os.clone(), info.cpu_arch.clone());

    if let Some(bits) = agent_local_capabilities {
        state
            .agent_manager
            .update_agent_local_capabilities(server_id, bits);

        // Persist the agent-reported caps into the read-only mirror
        // column so the dashboard can display them while the agent is
        // offline and so the cache survives a server restart.
        if let Err(e) =
            ServerService::update_capabilities_mirror(&state.db, server_id, bits).await
        {
            tracing::error!("Failed to mirror capabilities for {server_id}: {e}");
        }

        // Capabilities are agent-owned: effective == what the agent
        // reports, and `capabilities` mirrors the same value.
        state
            .agent_manager
            .broadcast_browser(BrowserMessage::CapabilitiesChanged {
                server_id: server_id.to_string(),
                capabilities: bits,
                agent_local_capabilities: Some(bits),
                effective_capabilities: Some(bits),
                temporary: temporary.clone(),
            });
    }

    // Broadcast to browsers
    state
        .agent_manager
        .broadcast_browser(BrowserMessage::AgentInfoUpdated {
            server_id: server_id.to_string(),
            protocol_version: agent_pv,
            agent_version: Some(info.agent_version.clone()),
        });

    if let Some(job) = state.upgrade_tracker.get(server_id)
        && job.status == serverbee_common::protocol::UpgradeStatus::Running
        && job.target_version == info.agent_version
    {
        state
            .upgrade_tracker
            .mark_succeeded(UpgradeLookup::from_job(&job), None);
    }

    // Record agent's external IP so the firewall guardrail's
    // dynamic allow-list keeps the agent from blocking itself.
    let fw_ip = info
        .ipv4
        .as_deref()
        .or(info.ipv6.as_deref())
        .and_then(|s| s.parse::<std::net::IpAddr>().ok());
    state
        .firewall
        .note_agent_external_ip(server_id, fw_ip)
        .await;

    // Send Ack
    if let Some(tx) = state.agent_manager.get_sender(server_id) {
        let _ = tx.send(ServerMessage::Ack { msg_id }).await;

        if state.docker_viewers.has_viewers(server_id)
            && info.features.iter().any(|feature| feature == "docker")
        {
            let _ = tx
                .send(ServerMessage::DockerStartStats { interval_secs: 3 })
                .await;
            let _ = tx.send(ServerMessage::DockerEventsStart).await;
        }
    }

    if let Err(error) = state
        .agent_desired_state
        .reconcile_connection(server_id)
        .await
    {
        tracing::warn!(
            server_id,
            error = %error,
            "connection desired-state reconcile was incomplete"
        );
    }
    true
}

pub(super) async fn on_ip_changed(
    state: &Arc<AppState>,
    server_id: &str,
    ipv4: Option<String>,
    ipv6: Option<String>,
) -> bool {
    // Refresh the firewall guardrail's dynamic allow-list with the
    // agent's new external IP. Done first so that any later auto-block
    // evaluation in this scope sees the up-to-date value.
    let fw_ip = ipv4
        .as_deref()
        .or(ipv6.as_deref())
        .and_then(|s| s.parse::<std::net::IpAddr>().ok());
    state
        .firewall
        .note_agent_external_ip(server_id, fw_ip)
        .await;

    let change = match persist_ip_change(state, server_id, ipv4.clone(), ipv6.clone()).await {
        Ok(change) => change,
        Err(error) => {
            tracing::error!("IP change and event-intent transaction failed: {error}");
            return false;
        }
    };
    if let Some(change) = change {
        // Re-run GeoIP only after addresses and intents have committed.
        let ip = resolve_public_ip(state, server_id, ipv4.as_deref(), ipv6.as_deref());
        let geo = ip.and_then(|ip| {
            let guard = state.geoip.read().unwrap();
            guard.as_ref().map(|g| g.lookup(ip))
        });
        if let Some(geo) = geo
            && let Err(error) =
                update_server_geo(&state.db, server_id, geo.region, geo.country_code).await
        {
            tracing::error!("Failed to update GeoIP for {server_id}: {error}");
        }
        apply_ip_change(state, server_id, change).await;
    }
    true
}

async fn source_transaction(
    state: &AppState,
    server_id: &str,
) -> Result<sea_orm::DatabaseTransaction, AppError> {
    let txn = state.db.begin().await?;
    txn.execute(sea_orm::Statement::from_sql_and_values(
        sea_orm::DatabaseBackend::Sqlite,
        "UPDATE servers SET updated_at=updated_at WHERE id=?",
        [server_id.into()],
    ))
    .await?;
    Ok(txn)
}

async fn source_server(
    txn: &sea_orm::DatabaseTransaction,
    server_id: &str,
) -> Result<server::Model, AppError> {
    server::Entity::find_by_id(server_id)
        .one(txn)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Server {server_id} not found")))
}

async fn persist_ip_change(
    state: &AppState,
    server_id: &str,
    ipv4: Option<String>,
    ipv6: Option<String>,
) -> Result<Option<IpChange>, AppError> {
    let txn = source_transaction(state, server_id).await?;
    let srv = source_server(&txn, server_id).await?;
    if srv.ipv4 == ipv4 && srv.ipv6 == ipv6 {
        txn.commit().await?;
        return Ok(None);
    }
    let change = IpChange {
        old_ipv4: srv.ipv4.clone(),
        new_ipv4: ipv4.clone(),
        old_ipv6: srv.ipv6.clone(),
        new_ipv6: ipv6.clone(),
        old_remote_addr: None,
        new_remote_addr: None,
    };
    let occurred_at = chrono::Utc::now();
    alert_event_intents::capture(&txn, server_id, "ip_changed", occurred_at).await?;
    let mut active: server::ActiveModel = srv.into();
    active.ipv4 = Set(ipv4);
    active.ipv6 = Set(ipv6);
    active.updated_at = Set(occurred_at);
    active.update(&txn).await?;
    txn.commit().await?;
    Ok(Some(change))
}

async fn persist_system_info(
    state: &AppState,
    server_id: &str,
    info: &SystemInfo,
    region: Option<String>,
    country_code: Option<String>,
    remote: Option<String>,
) -> Result<IpChange, AppError> {
    let txn = source_transaction(state, server_id).await?;
    let srv = source_server(&txn, server_id).await?;
    let change = IpChange {
        old_ipv4: srv.ipv4.clone(),
        new_ipv4: info.ipv4.clone(),
        old_ipv6: srv.ipv6.clone(),
        new_ipv6: info.ipv6.clone(),
        old_remote_addr: srv.last_remote_addr.clone(),
        new_remote_addr: remote.clone(),
    };
    if change.changed() {
        alert_event_intents::capture(&txn, server_id, "ip_changed", chrono::Utc::now()).await?;
    }
    ServerService::update_system_info(&txn, server_id, info, region, country_code).await?;
    if let Some(remote) = remote {
        let mut active: server::ActiveModel = source_server(&txn, server_id).await?.into();
        active.last_remote_addr = Set(Some(remote));
        active.update(&txn).await?;
    }
    txn.commit().await?;
    Ok(change)
}

/// Update the `region` and `country_code` GeoIP fields on a server record.
async fn update_server_geo(
    db: &sea_orm::DatabaseConnection,
    server_id: &str,
    region: Option<String>,
    country_code: Option<String>,
) -> Result<(), crate::error::AppError> {
    use crate::entity::server;
    use sea_orm::{ActiveModelTrait, Set};

    let model = ServerService::get_server(db, server_id).await?;
    // Respect a manual override: never clobber operator-corrected geo with GeoIP.
    if model.geo_manual {
        return Ok(());
    }
    let mut active: server::ActiveModel = model.into();
    active.region = Set(region);
    active.country_code = Set(country_code);
    active.updated_at = Set(chrono::Utc::now());
    active.update(db).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::IpChange;

    fn change(old: Option<&str>, new: Option<&str>) -> IpChange {
        IpChange {
            old_ipv4: old.map(str::to_string),
            new_ipv4: new.map(str::to_string),
            old_ipv6: None,
            new_ipv6: None,
            old_remote_addr: None,
            new_remote_addr: None,
        }
    }

    /// First population (None → Some) happens on every fresh registration: it
    /// must drive the alert/broadcast path but never the audit trail.
    #[test]
    fn first_population_changes_without_being_a_transition() {
        let c = change(None, Some("1.2.3.4"));
        assert!(c.changed());
        assert!(!c.is_transition());
    }

    /// A real address move (Some → different Some) drives both paths.
    #[test]
    fn address_move_is_a_transition() {
        let c = change(Some("1.2.3.4"), Some("5.6.7.8"));
        assert!(c.changed());
        assert!(c.is_transition());
    }

    /// Losing an address (Some → None) is a transition too — it must be
    /// audited, not mistaken for first population.
    #[test]
    fn address_loss_is_a_transition() {
        let c = change(Some("1.2.3.4"), None);
        assert!(c.changed());
        assert!(c.is_transition());
    }

    #[test]
    fn identical_addresses_are_no_change_at_all() {
        let c = change(Some("1.2.3.4"), Some("1.2.3.4"));
        assert!(!c.changed());
        assert!(!c.is_transition());
        assert!(!change(None, None).changed());
    }
}
