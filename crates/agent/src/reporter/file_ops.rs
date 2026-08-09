//! File-management message handling.
//!
//! Every file operation shares one inbound gate — the `file` capability bit
//! AND the local `[file]` config (enabled + root_paths) must both hold — and
//! one reply-shape rule for denials. `FileDownloadCancel` is exempt: cancel
//! must always work so a revoked agent can still be cleaned up.

use futures_util::SinkExt;
use serverbee_common::constants::CAP_FILE;
use serverbee_common::protocol::{AgentMessage, ServerMessage};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use super::wire::send_msg;
use crate::capability_grants::CapabilityAuthority;
use crate::file_manager::{FileEvent, FileManager};

const DISABLED: &str = "File capability disabled";

/// Handle any `File*` server message: gate, execute, reply.
pub(super) async fn handle_file_message<S>(
    msg: ServerMessage,
    write: &mut S,
    file_manager: &FileManager,
    file_tx: &mpsc::Sender<FileEvent>,
    capabilities: &CapabilityAuthority,
) -> anyhow::Result<()>
where
    S: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    // Cancel is exempt from the gate: it only tears down an in-flight
    // transfer and must stay reachable after the capability is revoked.
    if let ServerMessage::FileDownloadCancel { transfer_id } = &msg {
        file_manager.cancel_download(transfer_id);
        return Ok(());
    }

    if !capabilities.has(CAP_FILE) || !file_manager.is_enabled() {
        if let Some(reply) = capability_disabled_reply(msg) {
            send_msg(write, &reply).await?;
        }
        return Ok(());
    }

    match msg {
        ServerMessage::FileList { msg_id, path } => {
            let msg = match file_manager.list_dir(&path).await {
                Ok(entries) => AgentMessage::FileListResult {
                    msg_id,
                    path,
                    entries,
                    error: None,
                },
                Err(e) => AgentMessage::FileListResult {
                    msg_id,
                    path,
                    entries: vec![],
                    error: Some(e.to_string()),
                },
            };
            send_msg(write, &msg).await?;
        }
        ServerMessage::FileStat { msg_id, path } => {
            let msg = match file_manager.stat(&path).await {
                Ok(entry) => AgentMessage::FileStatResult {
                    msg_id,
                    entry: Some(entry),
                    error: None,
                },
                Err(e) => AgentMessage::FileStatResult {
                    msg_id,
                    entry: None,
                    error: Some(e.to_string()),
                },
            };
            send_msg(write, &msg).await?;
        }
        ServerMessage::FileRead {
            msg_id,
            path,
            max_size,
        } => {
            let msg = match file_manager.read_file(&path, max_size).await {
                Ok(content) => AgentMessage::FileReadResult {
                    msg_id,
                    content: Some(content),
                    error: None,
                },
                Err(e) => AgentMessage::FileReadResult {
                    msg_id,
                    content: None,
                    error: Some(e.to_string()),
                },
            };
            send_msg(write, &msg).await?;
        }
        ServerMessage::FileWrite {
            msg_id,
            path,
            content,
        } => {
            let result = file_manager.write_file(&path, &content).await;
            send_msg(write, &op_result(msg_id, result)).await?;
        }
        ServerMessage::FileDelete {
            msg_id,
            path,
            recursive,
        } => {
            let result = file_manager.delete(&path, recursive).await;
            send_msg(write, &op_result(msg_id, result)).await?;
        }
        ServerMessage::FileMkdir { msg_id, path } => {
            let result = file_manager.mkdir(&path).await;
            send_msg(write, &op_result(msg_id, result)).await?;
        }
        ServerMessage::FileMove { msg_id, from, to } => {
            let result = file_manager.rename_path(&from, &to).await;
            send_msg(write, &op_result(msg_id, result)).await?;
        }
        ServerMessage::FileDownloadStart { transfer_id, path } => {
            file_manager.start_download(transfer_id, path, file_tx.clone());
        }
        ServerMessage::FileUploadStart {
            transfer_id,
            path,
            size,
        } => {
            let msg = match file_manager
                .start_upload(transfer_id.clone(), path, size)
                .await
            {
                Ok(()) => AgentMessage::FileUploadAck {
                    transfer_id,
                    offset: 0,
                },
                Err(e) => AgentMessage::FileUploadError {
                    transfer_id,
                    error: e.to_string(),
                },
            };
            send_msg(write, &msg).await?;
        }
        ServerMessage::FileUploadChunk {
            transfer_id,
            offset,
            data,
        } => {
            let msg = match file_manager
                .receive_chunk(&transfer_id, offset, &data)
                .await
            {
                Ok(new_offset) => AgentMessage::FileUploadAck {
                    transfer_id,
                    offset: new_offset,
                },
                Err(e) => {
                    file_manager.abort_upload(&transfer_id).await;
                    AgentMessage::FileUploadError {
                        transfer_id,
                        error: e.to_string(),
                    }
                }
            };
            send_msg(write, &msg).await?;
        }
        ServerMessage::FileUploadEnd { transfer_id } => {
            let msg = match file_manager.finish_upload(&transfer_id).await {
                Ok(()) => AgentMessage::FileUploadComplete { transfer_id },
                Err(e) => AgentMessage::FileUploadError {
                    transfer_id,
                    error: e.to_string(),
                },
            };
            send_msg(write, &msg).await?;
        }
        other => {
            tracing::warn!("handle_file_message called with non-file message: {other:?}");
        }
    }

    Ok(())
}

/// Map a denied file request to its per-request error reply. Requests without
/// a reply channel (e.g. cancel) return `None`.
fn capability_disabled_reply(msg: ServerMessage) -> Option<AgentMessage> {
    Some(match msg {
        ServerMessage::FileList { msg_id, path } => AgentMessage::FileListResult {
            msg_id,
            path,
            entries: vec![],
            error: Some(DISABLED.into()),
        },
        ServerMessage::FileStat { msg_id, .. } => AgentMessage::FileStatResult {
            msg_id,
            entry: None,
            error: Some(DISABLED.into()),
        },
        ServerMessage::FileRead { msg_id, .. } => AgentMessage::FileReadResult {
            msg_id,
            content: None,
            error: Some(DISABLED.into()),
        },
        ServerMessage::FileWrite { msg_id, .. }
        | ServerMessage::FileDelete { msg_id, .. }
        | ServerMessage::FileMkdir { msg_id, .. }
        | ServerMessage::FileMove { msg_id, .. } => AgentMessage::FileOpResult {
            msg_id,
            success: false,
            error: Some(DISABLED.into()),
        },
        ServerMessage::FileDownloadStart { transfer_id, .. } => AgentMessage::FileDownloadError {
            transfer_id,
            error: DISABLED.into(),
        },
        ServerMessage::FileUploadStart { transfer_id, .. }
        | ServerMessage::FileUploadChunk { transfer_id, .. }
        | ServerMessage::FileUploadEnd { transfer_id } => AgentMessage::FileUploadError {
            transfer_id,
            error: DISABLED.into(),
        },
        _ => return None,
    })
}

/// Collapse an `Ok/Err` file mutation outcome into a `FileOpResult`.
fn op_result(msg_id: String, result: anyhow::Result<()>) -> AgentMessage {
    match result {
        Ok(()) => AgentMessage::FileOpResult {
            msg_id,
            success: true,
            error: None,
        },
        Err(e) => AgentMessage::FileOpResult {
            msg_id,
            success: false,
            error: Some(e.to_string()),
        },
    }
}
