//! `SystemInfo` / `IpChanged` handling: agent identity, GeoIP resolution,
//! capability mirroring, IP-change detection, and connection-time desired-state
//! reconciliation.

use std::sync::Arc;

use crate::service::alert::AlertService;
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

pub(super) async fn on_system_info(
    state: &Arc<AppState>,
    server_id: &str,
    msg_id: String,
    info: SystemInfo,
    agent_local_capabilities: Option<u32>,
    temporary: Vec<TemporaryGrant>,
) {
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

    if let Ok(srv) = ServerService::get_server(&state.db, server_id).await {
        let old_remote_addr = srv.last_remote_addr.clone();
        let old_ipv4 = srv.ipv4.clone();
        let old_ipv6 = srv.ipv6.clone();

        // Check if remote_addr changed
        if let Some(ref new_addr) = current_remote_addr
            && let Some(ref old_addr) = old_remote_addr
            && old_addr != new_addr
        {
            tracing::info!("Server {server_id} remote address changed: {old_addr} -> {new_addr}");
            if let Err(e) = AuditService::log(
                &state.db,
                "system",
                "ip_changed",
                Some(&format!(
                    "Remote address changed from {old_addr} to {new_addr} for server {server_id}"
                )),
                new_addr,
            )
            .await
            {
                tracing::error!("Failed to write audit log for IP change: {e}");
            }
        }

        // Check if agent-reported IPs changed
        let ipv4_changed = old_ipv4 != info.ipv4;
        let ipv6_changed = old_ipv6 != info.ipv6;
        let remote_changed = old_remote_addr.as_ref() != current_remote_addr.as_ref();

        if ipv4_changed || ipv6_changed || remote_changed {
            if let Err(e) = AlertService::check_event_rules(
                &state.db,
                &state.config,
                &state.alert_state_manager,
                server_id,
                "ip_changed",
            )
            .await
            {
                tracing::error!("Failed to check event rules for IP change: {e}");
            }

            state
                .agent_manager
                .broadcast_browser(BrowserMessage::ServerIpChanged {
                    server_id: server_id.to_string(),
                    old_ipv4,
                    new_ipv4: info.ipv4.clone(),
                    old_ipv6,
                    new_ipv6: info.ipv6.clone(),
                    old_remote_addr,
                    new_remote_addr: current_remote_addr.clone(),
                });
        }

        // Always update last_remote_addr
        if let Some(ref addr) = current_remote_addr
            && let Err(e) = update_last_remote_addr(&state.db, server_id, addr).await
        {
            tracing::error!("Failed to update last_remote_addr for {server_id}: {e}");
        }
    }

    if let Err(e) =
        ServerService::update_system_info(&state.db, server_id, &info, region, country_code).await
    {
        tracing::error!("Failed to update system info for {server_id}: {e}");
    }

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
}

pub(super) async fn on_ip_changed(
    state: &Arc<AppState>,
    server_id: &str,
    ipv4: Option<String>,
    ipv6: Option<String>,
) {
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

    match ServerService::get_server(&state.db, server_id).await {
        Ok(srv) => {
            let old_ipv4 = srv.ipv4.clone();
            let old_ipv6 = srv.ipv6.clone();
            let ipv4_changed = old_ipv4 != ipv4;
            let ipv6_changed = old_ipv6 != ipv6;

            if ipv4_changed || ipv6_changed {
                // Update ipv4/ipv6 in DB
                if let Err(e) = update_server_ips(&state.db, server_id, &ipv4, &ipv6).await {
                    tracing::error!("Failed to update IPs for {server_id}: {e}");
                }

                // Re-run GeoIP lookup. Same private/loopback filter as
                // the SystemInfo path; fall back to remote_addr when
                // the agent only knows internal/bridge addresses.
                let ip_to_lookup =
                    resolve_public_ip(state, server_id, ipv4.as_deref(), ipv6.as_deref());
                if let Some(ip) = ip_to_lookup {
                    let geo = {
                        let guard = state.geoip.read().unwrap();
                        guard.as_ref().map(|g| g.lookup(ip))
                    };
                    if let Some(geo) = geo
                        && let Err(e) =
                            update_server_geo(&state.db, server_id, geo.region, geo.country_code)
                                .await
                    {
                        tracing::error!("Failed to update GeoIP for {server_id}: {e}");
                    }
                }

                let detail = format!(
                    "IP changed for server {server_id}: ipv4 {:?} -> {:?}, ipv6 {:?} -> {:?}",
                    old_ipv4, ipv4, old_ipv6, ipv6
                );
                tracing::info!("{detail}");

                let remote_ip = state
                    .agent_manager
                    .get_remote_addr(server_id)
                    .map(|a| a.ip().to_string())
                    .unwrap_or_default();
                if let Err(e) =
                    AuditService::log(&state.db, "system", "ip_changed", Some(&detail), &remote_ip)
                        .await
                {
                    tracing::error!("Failed to write audit log for IP change: {e}");
                }

                if let Err(e) = AlertService::check_event_rules(
                    &state.db,
                    &state.config,
                    &state.alert_state_manager,
                    server_id,
                    "ip_changed",
                )
                .await
                {
                    tracing::error!("Failed to check event rules for IP change: {e}");
                }

                state
                    .agent_manager
                    .broadcast_browser(BrowserMessage::ServerIpChanged {
                        server_id: server_id.to_string(),
                        old_ipv4,
                        new_ipv4: ipv4,
                        old_ipv6,
                        new_ipv6: ipv6,
                        old_remote_addr: None,
                        new_remote_addr: None,
                    });
            }
        }
        Err(e) => {
            tracing::error!("Failed to load server {server_id} for IpChanged: {e}");
        }
    }
}

/// Update the `last_remote_addr` field on a server record.
async fn update_last_remote_addr(
    db: &sea_orm::DatabaseConnection,
    server_id: &str,
    addr: &str,
) -> Result<(), crate::error::AppError> {
    use crate::entity::server;
    use sea_orm::{ActiveModelTrait, Set};

    let model = ServerService::get_server(db, server_id).await?;
    let mut active: server::ActiveModel = model.into();
    active.last_remote_addr = Set(Some(addr.to_string()));
    active.updated_at = Set(chrono::Utc::now());
    active.update(db).await?;
    Ok(())
}

/// Update the `ipv4` and `ipv6` fields on a server record.
async fn update_server_ips(
    db: &sea_orm::DatabaseConnection,
    server_id: &str,
    ipv4: &Option<String>,
    ipv6: &Option<String>,
) -> Result<(), crate::error::AppError> {
    use crate::entity::server;
    use sea_orm::{ActiveModelTrait, Set};

    let model = ServerService::get_server(db, server_id).await?;
    let mut active: server::ActiveModel = model.into();
    active.ipv4 = Set(ipv4.clone());
    active.ipv6 = Set(ipv6.clone());
    active.updated_at = Set(chrono::Utc::now());
    active.update(db).await?;
    Ok(())
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
