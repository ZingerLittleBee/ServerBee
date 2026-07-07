//! Network measurement handling: ping results, network probe results, and
//! the traceroute round-update pipeline (including legacy single-shot
//! results re-dispatched as one round).

use std::sync::Arc;

use crate::service::network_probe::NetworkProbeService;
use crate::state::AppState;
use serverbee_common::protocol::{AgentMessage, BrowserMessage};
use serverbee_common::types::NetworkProbeResultData;

pub(super) async fn on_ping_result(
    state: &Arc<AppState>,
    server_id: &str,
    result: serverbee_common::types::PingResult,
) {
    if let Err(e) = save_ping_result(&state.db, server_id, &result).await {
        tracing::error!("Failed to save ping result for {server_id}: {e}");
    }
}

pub(super) async fn on_network_probe_results(
    state: &Arc<AppState>,
    server_id: &str,
    results: Vec<NetworkProbeResultData>,
) {
    // Broadcast to browsers before saving (clone needed for save)
    let _ = state.browser_tx.send(BrowserMessage::NetworkProbeUpdate {
        server_id: server_id.to_string(),
        results: results.clone(),
    });
    if let Err(e) = NetworkProbeService::save_results(&state.db, server_id, results).await {
        tracing::error!("Failed to save network probe results for {server_id}: {e}");
    }
}

pub(super) async fn on_traceroute_result(
    state: &Arc<AppState>,
    server_id: &str,
    request_id: String,
    target: String,
    hops: Vec<serverbee_common::types::TracerouteHop>,
    completed: bool,
    error: Option<String>,
) {
    tracing::info!("Received legacy TracerouteResult from {server_id} (request_id={request_id})");
    // Legacy agent does not report which probe protocol actually ran (UDP
    // for Unix `traceroute`, ICMP for `mtr` / Windows `tracert`). Persist
    // with the "legacy" sentinel.
    state.agent_manager.set_traceroute_meta_protocol(
        &request_id,
        serverbee_common::protocol::RecordedProtocol::Legacy,
    );
    // Re-dispatch into the new pipeline as a single-round update.
    let synthetic = AgentMessage::TracerouteRoundUpdate {
        request_id,
        target,
        round: 1,
        total_rounds: 1,
        hops,
        completed,
        error,
    };
    on_traceroute_round_update(state, server_id, synthetic).await;
}

pub(super) async fn on_traceroute_round_update(
    state: &Arc<AppState>,
    server_id: &str,
    msg: AgentMessage,
) {
    let AgentMessage::TracerouteRoundUpdate {
        request_id,
        target: _,
        round,
        total_rounds,
        mut hops,
        completed,
        error,
    } = msg
    else {
        unreachable!("on_traceroute_round_update called with non-TracerouteRoundUpdate msg");
    };

    // Defense-in-depth: reject updates whose request_id was registered for a
    // different server. The placeholder is keyed by request_id only, so a
    // compromised agent that learned another server's request_id could
    // otherwise overwrite the victim's cache and trigger a poisoned DB insert.
    if let Some(meta) = state.agent_manager.get_traceroute_meta(&request_id)
        && meta.server_id != server_id
    {
        tracing::warn!(
            "Dropping TracerouteRoundUpdate {request_id}: server_id mismatch (placeholder={}, sender={server_id})",
            meta.server_id
        );
        return;
    }

    // Server-side enrich: PTR hostnames and ASN (when MMDB is installed).
    state.traceroute_enricher.enrich(&mut hops).await;

    // Update in-memory cache
    let Some(snapshot) = state.agent_manager.update_traceroute_round(
        &request_id,
        round,
        total_rounds,
        hops.clone(),
        completed,
        error.clone(),
    ) else {
        tracing::warn!("Dropping TracerouteRoundUpdate {request_id}: no cached placeholder");
        return;
    };

    // On completion, persist a DB row
    if completed {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let new_record = crate::service::traceroute::NewTracerouteRecord {
            id: request_id.clone(),
            server_id: snapshot.server_id.clone(),
            target: snapshot.target.clone(),
            protocol: snapshot.protocol,
            started_at: snapshot.started_at,
            completed_at: Some(now_ms),
            total_rounds: snapshot.total_rounds,
            completed_rounds: snapshot.round,
            hops: snapshot.hops.clone(),
            error: snapshot.error.clone(),
        };
        if let Err(e) =
            crate::service::traceroute::insert_completed_record(&state.db, new_record).await
        {
            tracing::warn!("Failed to persist traceroute record {request_id}: {e:?}");
        }
    }

    // Broadcast to subscribed browsers
    let _ = state.browser_tx.send(BrowserMessage::TracerouteUpdate {
        server_id: snapshot.server_id.clone(),
        request_id: request_id.clone(),
        target: snapshot.target.clone(),
        protocol: snapshot.protocol,
        started_at: snapshot.started_at,
        round: snapshot.round,
        total_rounds: snapshot.total_rounds,
        hops: snapshot.hops,
        completed: snapshot.completed,
        error: snapshot.error,
    });
}

/// Save a ping result to the database.
async fn save_ping_result(
    db: &sea_orm::DatabaseConnection,
    server_id: &str,
    result: &serverbee_common::types::PingResult,
) -> Result<(), crate::error::AppError> {
    use crate::entity::ping_record;
    use sea_orm::{ActiveModelTrait, NotSet, Set};

    let new_record = ping_record::ActiveModel {
        id: NotSet,
        task_id: Set(result.task_id.clone()),
        server_id: Set(server_id.to_string()),
        latency: Set(result.latency),
        success: Set(result.success),
        error: Set(result.error.clone()),
        time: Set(result.time),
    };
    new_record.insert(db).await?;
    Ok(())
}
