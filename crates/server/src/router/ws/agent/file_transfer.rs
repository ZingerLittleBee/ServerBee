//! File control-response relay and download/upload transfer streaming.
//!
//! Control responses (list/stat/read/op) relay to pending HTTP requests via
//! the AgentManager. Download chunks stream into a server-side temp file;
//! upload acks wake the HTTP upload handler through per-transfer pending keys.

use std::sync::Arc;

use crate::state::AppState;
use serverbee_common::protocol::AgentMessage;

/// Relay a file control response (list/stat/read/op result) to the pending
/// HTTP request that initiated it.
pub(super) fn relay_control_response(state: &Arc<AppState>, msg_id: &str, msg: &AgentMessage) {
    if !state
        .agent_manager
        .dispatch_pending_response(msg_id, msg.clone())
    {
        tracing::debug!("Orphaned file control response for msg_id={msg_id}");
    }
}

pub(super) async fn on_download_ready(state: &Arc<AppState>, transfer_id: &str, size: u64) {
    state.file_transfers.update_size(transfer_id, size);
    state.file_transfers.mark_in_progress(transfer_id);
    // Create the temp file and keep it open for the duration of the transfer
    if let Some(path) = state.file_transfers.temp_file_path(transfer_id) {
        match tokio::fs::File::create(&path).await {
            Ok(file) => {
                state.file_transfers.store_file_handle(transfer_id, file);
            }
            Err(e) => {
                tracing::error!("Failed to create temp file for transfer {transfer_id}: {e}");
                state
                    .file_transfers
                    .mark_failed(transfer_id, format!("Failed to create temp file: {e}"));
            }
        }
    }
}

pub(super) async fn on_download_chunk(
    state: &Arc<AppState>,
    transfer_id: &str,
    offset: u64,
    data: &str,
) {
    use base64::Engine;
    use tokio::io::{AsyncSeekExt, AsyncWriteExt};
    if let Some(file_handle) = state.file_transfers.get_file_handle(transfer_id) {
        match base64::engine::general_purpose::STANDARD.decode(data) {
            Ok(bytes) => {
                let result = async {
                    let mut file = file_handle.lock().await;
                    file.seek(std::io::SeekFrom::Start(offset)).await?;
                    file.write_all(&bytes).await?;
                    Ok::<(), std::io::Error>(())
                }
                .await;
                match result {
                    Ok(()) => {
                        state
                            .file_transfers
                            .update_progress(transfer_id, offset + bytes.len() as u64);
                    }
                    Err(e) => {
                        tracing::error!("Failed to write chunk for transfer {transfer_id}: {e}");
                        state
                            .file_transfers
                            .mark_failed(transfer_id, format!("Write error: {e}"));
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to decode base64 chunk for transfer {transfer_id}: {e}");
                state
                    .file_transfers
                    .mark_failed(transfer_id, format!("Base64 decode error: {e}"));
            }
        }
    }
}

pub(super) fn on_download_end(state: &Arc<AppState>, transfer_id: &str) {
    state.file_transfers.remove_file_handle(transfer_id);
    state.file_transfers.mark_ready(transfer_id);
}

pub(super) fn on_download_error(state: &Arc<AppState>, transfer_id: &str, error: &str) {
    state.file_transfers.remove_file_handle(transfer_id);
    state
        .file_transfers
        .mark_failed(transfer_id, error.to_string());
}

pub(super) fn on_upload_ack(
    state: &Arc<AppState>,
    transfer_id: &str,
    offset: u64,
    msg: &AgentMessage,
) {
    state.file_transfers.update_progress(transfer_id, offset);
    let ack_key = format!("upload-ack-{transfer_id}");
    state
        .agent_manager
        .dispatch_pending_response(&ack_key, msg.clone());
}

pub(super) fn on_upload_complete(state: &Arc<AppState>, transfer_id: &str, msg: &AgentMessage) {
    state.file_transfers.mark_ready(transfer_id);
    let complete_key = format!("upload-complete-{transfer_id}");
    state
        .agent_manager
        .dispatch_pending_response(&complete_key, msg.clone());
}

pub(super) fn on_upload_error(
    state: &Arc<AppState>,
    transfer_id: &str,
    error: &str,
    msg: &AgentMessage,
) {
    state
        .file_transfers
        .mark_failed(transfer_id, error.to_string());
    // The HTTP handler may be waiting on either an ack or complete key — try both.
    let ack_key = format!("upload-ack-{transfer_id}");
    let complete_key = format!("upload-complete-{transfer_id}");
    if !state
        .agent_manager
        .dispatch_pending_response(&complete_key, msg.clone())
    {
        state
            .agent_manager
            .dispatch_pending_response(&ack_key, msg.clone());
    }
}
