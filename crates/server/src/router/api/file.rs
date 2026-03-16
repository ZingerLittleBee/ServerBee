use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Extension, Multipart, Path, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::error::{ok, ApiResponse, AppError};
use crate::middleware::auth::CurrentUser;
use crate::service::audit::AuditService;
use crate::service::file_transfer::{TransferDirection, TransferInfo};
use crate::service::server::ServerService;
use crate::state::AppState;
use serverbee_common::constants::{has_capability, CAP_FILE, MAX_FILE_CHUNK_SIZE};
use serverbee_common::protocol::{AgentMessage, ServerMessage};
use serverbee_common::types::FileEntry;

// ---------------------------------------------------------------------------
// Request / Response DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ListFilesRequest {
    path: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ListFilesResponse {
    entries: Vec<FileEntry>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct StatRequest {
    path: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct StatResponse {
    entry: FileEntry,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ReadRequest {
    path: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ReadResponse {
    content: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct WriteRequest {
    path: String,
    content: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct DeleteRequest {
    path: String,
    #[serde(default)]
    recursive: bool,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct MkdirRequest {
    path: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct MoveRequest {
    from: String,
    to: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct DownloadRequest {
    path: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DownloadResponse {
    transfer_id: String,
    status: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SuccessResponse {
    success: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TransfersResponse {
    transfers: Vec<TransferInfo>,
}

// ---------------------------------------------------------------------------
// Routers
// ---------------------------------------------------------------------------

/// Read endpoints accessible to all authenticated users (admin + member).
pub fn read_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/files/{server_id}/list", post(list_files))
        .route("/files/{server_id}/stat", post(stat_file))
        .route("/files/{server_id}/read", post(read_file))
        .route("/files/download/{transfer_id}", get(download_file))
        .route("/files/transfers", get(list_transfers))
}

/// Write endpoints (POST/DELETE) restricted to admin users only.
pub fn write_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/files/{server_id}/write", post(write_file))
        .route("/files/{server_id}/delete", post(delete_file))
        .route("/files/{server_id}/mkdir", post(mkdir))
        .route("/files/{server_id}/move", post(move_file))
        .route("/files/{server_id}/download", post(start_download))
        .route("/files/{server_id}/upload", post(upload_file))
        .route("/files/transfers/{transfer_id}", delete(cancel_transfer))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn extract_client_ip(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or("unknown").trim().to_string())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "unknown".to_string())
}

/// Validate that the server exists, has CAP_FILE capability, and is online.
/// Returns the server model on success.
async fn validate_file_access(
    state: &AppState,
    server_id: &str,
) -> Result<(), AppError> {
    let server = ServerService::get_server(&state.db, server_id).await?;
    let caps = server.capabilities as u32;
    if !has_capability(caps, CAP_FILE) {
        return Err(AppError::Forbidden(
            "File capability disabled for this server".into(),
        ));
    }
    if !state.agent_manager.is_online(server_id) {
        return Err(AppError::NotFound("Server offline".into()));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Read handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/files/{server_id}/list",
    tag = "files",
    params(("server_id" = String, Path, description = "Server ID")),
    request_body = ListFilesRequest,
    responses(
        (status = 200, description = "Directory listing", body = ListFilesResponse),
        (status = 403, description = "File capability disabled"),
        (status = 404, description = "Server not found or offline"),
        (status = 408, description = "Agent timeout"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn list_files(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
    Json(body): Json<ListFilesRequest>,
) -> Result<Json<ApiResponse<ListFilesResponse>>, AppError> {
    validate_file_access(&state, &server_id).await?;

    let msg_id = uuid::Uuid::new_v4().to_string();
    let rx = state.agent_manager.register_pending_request(msg_id.clone());

    let sender = state
        .agent_manager
        .get_sender(&server_id)
        .ok_or(AppError::NotFound("Server offline".into()))?;
    sender
        .send(ServerMessage::FileList {
            msg_id: msg_id.clone(),
            path: body.path,
        })
        .await
        .map_err(|_| AppError::Internal("Failed to send to agent".into()))?;

    match tokio::time::timeout(Duration::from_secs(30), rx).await {
        Ok(Ok(AgentMessage::FileListResult {
            entries, error, ..
        })) => {
            if let Some(e) = error {
                return Err(AppError::BadRequest(e));
            }
            ok(ListFilesResponse { entries })
        }
        Ok(Ok(_)) => Err(AppError::Internal("Unexpected response from agent".into())),
        Ok(Err(_)) => Err(AppError::Internal("Agent disconnected".into())),
        Err(_) => Err(AppError::RequestTimeout(
            "Agent did not respond within 30s".into(),
        )),
    }
}

#[utoipa::path(
    post,
    path = "/api/files/{server_id}/stat",
    tag = "files",
    params(("server_id" = String, Path, description = "Server ID")),
    request_body = StatRequest,
    responses(
        (status = 200, description = "File metadata", body = StatResponse),
        (status = 403, description = "File capability disabled"),
        (status = 404, description = "Server not found or offline"),
        (status = 408, description = "Agent timeout"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn stat_file(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
    Json(body): Json<StatRequest>,
) -> Result<Json<ApiResponse<StatResponse>>, AppError> {
    validate_file_access(&state, &server_id).await?;

    let msg_id = uuid::Uuid::new_v4().to_string();
    let rx = state.agent_manager.register_pending_request(msg_id.clone());

    let sender = state
        .agent_manager
        .get_sender(&server_id)
        .ok_or(AppError::NotFound("Server offline".into()))?;
    sender
        .send(ServerMessage::FileStat {
            msg_id: msg_id.clone(),
            path: body.path,
        })
        .await
        .map_err(|_| AppError::Internal("Failed to send to agent".into()))?;

    match tokio::time::timeout(Duration::from_secs(30), rx).await {
        Ok(Ok(AgentMessage::FileStatResult { entry, error, .. })) => {
            if let Some(e) = error {
                return Err(AppError::BadRequest(e));
            }
            let entry = entry.ok_or(AppError::Internal("No entry returned".into()))?;
            ok(StatResponse { entry })
        }
        Ok(Ok(_)) => Err(AppError::Internal("Unexpected response from agent".into())),
        Ok(Err(_)) => Err(AppError::Internal("Agent disconnected".into())),
        Err(_) => Err(AppError::RequestTimeout(
            "Agent did not respond within 30s".into(),
        )),
    }
}

#[utoipa::path(
    post,
    path = "/api/files/{server_id}/read",
    tag = "files",
    params(("server_id" = String, Path, description = "Server ID")),
    request_body = ReadRequest,
    responses(
        (status = 200, description = "File content (UTF-8)", body = ReadResponse),
        (status = 403, description = "File capability disabled"),
        (status = 404, description = "Server not found or offline"),
        (status = 408, description = "Agent timeout"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn read_file(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
    Json(body): Json<ReadRequest>,
) -> Result<Json<ApiResponse<ReadResponse>>, AppError> {
    validate_file_access(&state, &server_id).await?;

    let msg_id = uuid::Uuid::new_v4().to_string();
    let rx = state.agent_manager.register_pending_request(msg_id.clone());

    let sender = state
        .agent_manager
        .get_sender(&server_id)
        .ok_or(AppError::NotFound("Server offline".into()))?;
    sender
        .send(ServerMessage::FileRead {
            msg_id: msg_id.clone(),
            path: body.path,
            max_size: MAX_FILE_CHUNK_SIZE as u64, // 384KB — stays under WS limit after base64
        })
        .await
        .map_err(|_| AppError::Internal("Failed to send to agent".into()))?;

    match tokio::time::timeout(Duration::from_secs(30), rx).await {
        Ok(Ok(AgentMessage::FileReadResult {
            content, error, ..
        })) => {
            if let Some(e) = error {
                return Err(AppError::BadRequest(e));
            }
            let content = content.unwrap_or_default();
            ok(ReadResponse { content })
        }
        Ok(Ok(_)) => Err(AppError::Internal("Unexpected response from agent".into())),
        Ok(Err(_)) => Err(AppError::Internal("Agent disconnected".into())),
        Err(_) => Err(AppError::RequestTimeout(
            "Agent did not respond within 30s".into(),
        )),
    }
}

#[utoipa::path(
    get,
    path = "/api/files/download/{transfer_id}",
    tag = "files",
    params(("transfer_id" = String, Path, description = "Transfer ID")),
    responses(
        (status = 200, description = "File download stream"),
        (status = 404, description = "Transfer not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn download_file(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(transfer_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    // Verify ownership: only the user who started the transfer can download it
    let owner = state
        .file_transfers
        .get_user_id(&transfer_id)
        .ok_or(AppError::NotFound("Transfer not found".into()))?;
    if owner != current_user.user_id {
        return Err(AppError::NotFound("Transfer not found".into()));
    }

    let info = state
        .file_transfers
        .get(&transfer_id)
        .ok_or(AppError::NotFound("Transfer not found".into()))?;

    if info.status != "ready" {
        return Err(AppError::BadRequest(format!(
            "Transfer not ready, current status: {}",
            info.status
        )));
    }

    let temp_path = state
        .file_transfers
        .temp_file_path(&transfer_id)
        .ok_or(AppError::NotFound("Transfer not found".into()))?;

    let file = tokio::fs::File::open(&temp_path)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to open temp file: {e}")))?;

    let stream = tokio_util::io::ReaderStream::new(file);
    let body = Body::from_stream(stream);

    // Extract filename from path
    let filename = info
        .file_path
        .rsplit('/')
        .next()
        .or_else(|| info.file_path.rsplit('\\').next())
        .unwrap_or("download");

    let headers = [
        (
            axum::http::header::CONTENT_TYPE,
            "application/octet-stream".to_string(),
        ),
        (
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        ),
    ];

    // Clean up the transfer after starting the download
    // (temp file will be streamed and then the OS can reclaim it)
    state.file_transfers.remove(&transfer_id);

    Ok((headers, body))
}

#[utoipa::path(
    get,
    path = "/api/files/transfers",
    tag = "files",
    responses(
        (status = 200, description = "Active file transfers", body = TransfersResponse),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn list_transfers(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<ApiResponse<TransfersResponse>>, AppError> {
    let transfers = state.file_transfers.list_for_user(&current_user.user_id);
    ok(TransfersResponse { transfers })
}

// ---------------------------------------------------------------------------
// Write handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/files/{server_id}/write",
    tag = "files",
    params(("server_id" = String, Path, description = "Server ID")),
    request_body = WriteRequest,
    responses(
        (status = 200, description = "File written", body = SuccessResponse),
        (status = 403, description = "File capability disabled"),
        (status = 404, description = "Server not found or offline"),
        (status = 408, description = "Agent timeout"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn write_file(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    headers: HeaderMap,
    Path(server_id): Path<String>,
    Json(body): Json<WriteRequest>,
) -> Result<Json<ApiResponse<SuccessResponse>>, AppError> {
    validate_file_access(&state, &server_id).await?;

    let msg_id = uuid::Uuid::new_v4().to_string();
    let rx = state.agent_manager.register_pending_request(msg_id.clone());

    let sender = state
        .agent_manager
        .get_sender(&server_id)
        .ok_or(AppError::NotFound("Server offline".into()))?;
    sender
        .send(ServerMessage::FileWrite {
            msg_id: msg_id.clone(),
            path: body.path.clone(),
            content: body.content,
        })
        .await
        .map_err(|_| AppError::Internal("Failed to send to agent".into()))?;

    let result = match tokio::time::timeout(Duration::from_secs(30), rx).await {
        Ok(Ok(AgentMessage::FileOpResult {
            success, error, ..
        })) => {
            if let Some(e) = error {
                return Err(AppError::BadRequest(e));
            }
            ok(SuccessResponse { success })
        }
        Ok(Ok(_)) => Err(AppError::Internal("Unexpected response from agent".into())),
        Ok(Err(_)) => Err(AppError::Internal("Agent disconnected".into())),
        Err(_) => Err(AppError::RequestTimeout(
            "Agent did not respond within 30s".into(),
        )),
    };

    // Audit log (fire-and-forget)
    let ip = extract_client_ip(&headers);
    let detail = serde_json::json!({
        "server_id": server_id,
        "path": body.path,
    })
    .to_string();
    let _ = AuditService::log(
        &state.db,
        &current_user.user_id,
        "file_write",
        Some(&detail),
        &ip,
    )
    .await;

    result
}

#[utoipa::path(
    post,
    path = "/api/files/{server_id}/delete",
    tag = "files",
    params(("server_id" = String, Path, description = "Server ID")),
    request_body = DeleteRequest,
    responses(
        (status = 200, description = "File/directory deleted", body = SuccessResponse),
        (status = 403, description = "File capability disabled"),
        (status = 404, description = "Server not found or offline"),
        (status = 408, description = "Agent timeout"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn delete_file(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    headers: HeaderMap,
    Path(server_id): Path<String>,
    Json(body): Json<DeleteRequest>,
) -> Result<Json<ApiResponse<SuccessResponse>>, AppError> {
    validate_file_access(&state, &server_id).await?;

    let msg_id = uuid::Uuid::new_v4().to_string();
    let rx = state.agent_manager.register_pending_request(msg_id.clone());

    let sender = state
        .agent_manager
        .get_sender(&server_id)
        .ok_or(AppError::NotFound("Server offline".into()))?;
    sender
        .send(ServerMessage::FileDelete {
            msg_id: msg_id.clone(),
            path: body.path.clone(),
            recursive: body.recursive,
        })
        .await
        .map_err(|_| AppError::Internal("Failed to send to agent".into()))?;

    let result = match tokio::time::timeout(Duration::from_secs(30), rx).await {
        Ok(Ok(AgentMessage::FileOpResult {
            success, error, ..
        })) => {
            if let Some(e) = error {
                return Err(AppError::BadRequest(e));
            }
            ok(SuccessResponse { success })
        }
        Ok(Ok(_)) => Err(AppError::Internal("Unexpected response from agent".into())),
        Ok(Err(_)) => Err(AppError::Internal("Agent disconnected".into())),
        Err(_) => Err(AppError::RequestTimeout(
            "Agent did not respond within 30s".into(),
        )),
    };

    let ip = extract_client_ip(&headers);
    let detail = serde_json::json!({
        "server_id": server_id,
        "path": body.path,
        "recursive": body.recursive,
    })
    .to_string();
    let _ = AuditService::log(
        &state.db,
        &current_user.user_id,
        "file_delete",
        Some(&detail),
        &ip,
    )
    .await;

    result
}

#[utoipa::path(
    post,
    path = "/api/files/{server_id}/mkdir",
    tag = "files",
    params(("server_id" = String, Path, description = "Server ID")),
    request_body = MkdirRequest,
    responses(
        (status = 200, description = "Directory created", body = SuccessResponse),
        (status = 403, description = "File capability disabled"),
        (status = 404, description = "Server not found or offline"),
        (status = 408, description = "Agent timeout"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn mkdir(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    headers: HeaderMap,
    Path(server_id): Path<String>,
    Json(body): Json<MkdirRequest>,
) -> Result<Json<ApiResponse<SuccessResponse>>, AppError> {
    validate_file_access(&state, &server_id).await?;

    let msg_id = uuid::Uuid::new_v4().to_string();
    let rx = state.agent_manager.register_pending_request(msg_id.clone());

    let sender = state
        .agent_manager
        .get_sender(&server_id)
        .ok_or(AppError::NotFound("Server offline".into()))?;
    sender
        .send(ServerMessage::FileMkdir {
            msg_id: msg_id.clone(),
            path: body.path.clone(),
        })
        .await
        .map_err(|_| AppError::Internal("Failed to send to agent".into()))?;

    let result = match tokio::time::timeout(Duration::from_secs(30), rx).await {
        Ok(Ok(AgentMessage::FileOpResult {
            success, error, ..
        })) => {
            if let Some(e) = error {
                return Err(AppError::BadRequest(e));
            }
            ok(SuccessResponse { success })
        }
        Ok(Ok(_)) => Err(AppError::Internal("Unexpected response from agent".into())),
        Ok(Err(_)) => Err(AppError::Internal("Agent disconnected".into())),
        Err(_) => Err(AppError::RequestTimeout(
            "Agent did not respond within 30s".into(),
        )),
    };

    let ip = extract_client_ip(&headers);
    let detail = serde_json::json!({
        "server_id": server_id,
        "path": body.path,
    })
    .to_string();
    let _ = AuditService::log(
        &state.db,
        &current_user.user_id,
        "file_mkdir",
        Some(&detail),
        &ip,
    )
    .await;

    result
}

#[utoipa::path(
    post,
    path = "/api/files/{server_id}/move",
    tag = "files",
    params(("server_id" = String, Path, description = "Server ID")),
    request_body = MoveRequest,
    responses(
        (status = 200, description = "File/directory moved", body = SuccessResponse),
        (status = 403, description = "File capability disabled"),
        (status = 404, description = "Server not found or offline"),
        (status = 408, description = "Agent timeout"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn move_file(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    headers: HeaderMap,
    Path(server_id): Path<String>,
    Json(body): Json<MoveRequest>,
) -> Result<Json<ApiResponse<SuccessResponse>>, AppError> {
    validate_file_access(&state, &server_id).await?;

    let msg_id = uuid::Uuid::new_v4().to_string();
    let rx = state.agent_manager.register_pending_request(msg_id.clone());

    let sender = state
        .agent_manager
        .get_sender(&server_id)
        .ok_or(AppError::NotFound("Server offline".into()))?;
    sender
        .send(ServerMessage::FileMove {
            msg_id: msg_id.clone(),
            from: body.from.clone(),
            to: body.to.clone(),
        })
        .await
        .map_err(|_| AppError::Internal("Failed to send to agent".into()))?;

    let result = match tokio::time::timeout(Duration::from_secs(30), rx).await {
        Ok(Ok(AgentMessage::FileOpResult {
            success, error, ..
        })) => {
            if let Some(e) = error {
                return Err(AppError::BadRequest(e));
            }
            ok(SuccessResponse { success })
        }
        Ok(Ok(_)) => Err(AppError::Internal("Unexpected response from agent".into())),
        Ok(Err(_)) => Err(AppError::Internal("Agent disconnected".into())),
        Err(_) => Err(AppError::RequestTimeout(
            "Agent did not respond within 30s".into(),
        )),
    };

    let ip = extract_client_ip(&headers);
    let detail = serde_json::json!({
        "server_id": server_id,
        "from": body.from,
        "to": body.to,
    })
    .to_string();
    let _ = AuditService::log(
        &state.db,
        &current_user.user_id,
        "file_move",
        Some(&detail),
        &ip,
    )
    .await;

    result
}

#[utoipa::path(
    post,
    path = "/api/files/{server_id}/download",
    tag = "files",
    params(("server_id" = String, Path, description = "Server ID")),
    request_body = DownloadRequest,
    responses(
        (status = 200, description = "Download transfer started", body = DownloadResponse),
        (status = 403, description = "File capability disabled"),
        (status = 404, description = "Server not found or offline"),
        (status = 429, description = "Too many concurrent transfers"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn start_download(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    headers: HeaderMap,
    Path(server_id): Path<String>,
    Json(body): Json<DownloadRequest>,
) -> Result<Json<ApiResponse<DownloadResponse>>, AppError> {
    validate_file_access(&state, &server_id).await?;

    let transfer_id = state
        .file_transfers
        .create_transfer(
            server_id.clone(),
            current_user.user_id.clone(),
            TransferDirection::Download,
            body.path.clone(),
        )
        .map_err(AppError::TooManyRequests)?;

    // Send FileDownloadStart to agent
    let sender = state
        .agent_manager
        .get_sender(&server_id)
        .ok_or(AppError::NotFound("Server offline".into()))?;
    sender
        .send(ServerMessage::FileDownloadStart {
            transfer_id: transfer_id.clone(),
            path: body.path.clone(),
        })
        .await
        .map_err(|_| {
            state.file_transfers.remove(&transfer_id);
            AppError::Internal("Failed to send to agent".into())
        })?;

    let ip = extract_client_ip(&headers);
    let detail = serde_json::json!({
        "server_id": server_id,
        "path": body.path,
        "transfer_id": transfer_id,
    })
    .to_string();
    let _ = AuditService::log(
        &state.db,
        &current_user.user_id,
        "file_download",
        Some(&detail),
        &ip,
    )
    .await;

    ok(DownloadResponse {
        transfer_id,
        status: "pending".to_string(),
    })
}

#[allow(clippy::too_many_lines)]
#[utoipa::path(
    post,
    path = "/api/files/{server_id}/upload",
    tag = "files",
    params(("server_id" = String, Path, description = "Server ID")),
    request_body(content = String, content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "File uploaded", body = SuccessResponse),
        (status = 403, description = "File capability disabled"),
        (status = 404, description = "Server not found or offline"),
        (status = 429, description = "Too many concurrent transfers"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn upload_file(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    headers: HeaderMap,
    Path(server_id): Path<String>,
    mut multipart: Multipart,
) -> Result<Json<ApiResponse<SuccessResponse>>, AppError> {
    validate_file_access(&state, &server_id).await?;

    let mut remote_path: Option<String> = None;
    let temp_upload = state
        .file_transfers
        .temp_dir()
        .join(format!("{}.upload", uuid::Uuid::new_v4()));
    let mut file_size: u64 = 0;

    // Extract fields from multipart — stream file data to a temp file to avoid OOM
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Multipart error: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "path" => {
                remote_path = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| AppError::BadRequest(format!("Failed to read path: {e}")))?,
                );
            }
            "file" => {
                use tokio::io::AsyncWriteExt;
                let mut temp_file = tokio::fs::File::create(&temp_upload)
                    .await
                    .map_err(|e| AppError::Internal(format!("Failed to create temp file: {e}")))?;
                let mut field = field;
                while let Some(chunk) = field
                    .chunk()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read file chunk: {e}")))?
                {
                    file_size += chunk.len() as u64;
                    temp_file.write_all(&chunk).await.map_err(|e| {
                        AppError::Internal(format!("Failed to write temp file: {e}"))
                    })?;
                }
                temp_file.flush().await.map_err(|e| {
                    AppError::Internal(format!("Failed to flush temp file: {e}"))
                })?;
            }
            _ => {}
        }
    }

    let remote_path =
        remote_path.ok_or_else(|| AppError::BadRequest("Missing 'path' field".into()))?;
    if file_size == 0 {
        let _ = tokio::fs::remove_file(&temp_upload).await;
        return Err(AppError::BadRequest("Missing or empty 'file' field".into()));
    }

    let transfer_id = state
        .file_transfers
        .create_transfer(
            server_id.clone(),
            current_user.user_id.clone(),
            TransferDirection::Upload,
            remote_path.clone(),
        )
        .map_err(AppError::TooManyRequests)?;

    // Send FileUploadStart to agent
    let sender = state
        .agent_manager
        .get_sender(&server_id)
        .ok_or_else(|| {
            state.file_transfers.remove(&transfer_id);
            let _ = std::fs::remove_file(&temp_upload);
            AppError::NotFound("Server offline".into())
        })?;

    sender
        .send(ServerMessage::FileUploadStart {
            transfer_id: transfer_id.clone(),
            path: remote_path.clone(),
            size: file_size,
        })
        .await
        .map_err(|_| {
            state.file_transfers.remove(&transfer_id);
            let _ = std::fs::remove_file(&temp_upload);
            AppError::Internal("Failed to send to agent".into())
        })?;

    state.file_transfers.mark_in_progress(&transfer_id);
    state.file_transfers.update_size(&transfer_id, file_size);

    // Read from temp file in chunks and send to agent
    use tokio::io::AsyncReadExt;
    let mut temp_reader = tokio::fs::File::open(&temp_upload).await.map_err(|e| {
        state
            .file_transfers
            .mark_failed(&transfer_id, format!("Failed to open temp file: {e}"));
        AppError::Internal(format!("Failed to open temp file: {e}"))
    })?;

    let mut offset: u64 = 0;
    let mut buf = vec![0u8; MAX_FILE_CHUNK_SIZE];
    loop {
        let n = temp_reader.read(&mut buf).await.map_err(|e| {
            state
                .file_transfers
                .mark_failed(&transfer_id, format!("Failed to read temp file: {e}"));
            AppError::Internal(format!("Failed to read temp file: {e}"))
        })?;
        if n == 0 {
            break;
        }

        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&buf[..n]);

        // Register pending request for the ack (one at a time since upload is sequential)
        let ack_msg_id = format!("upload-ack-{transfer_id}");
        let ack_rx = state
            .agent_manager
            .register_pending_request(ack_msg_id.clone());

        sender
            .send(ServerMessage::FileUploadChunk {
                transfer_id: transfer_id.clone(),
                offset,
                data: encoded,
            })
            .await
            .map_err(|_| {
                state
                    .file_transfers
                    .mark_failed(&transfer_id, "Failed to send chunk".into());
                AppError::Internal("Failed to send chunk to agent".into())
            })?;

        // Wait for ack with timeout
        match tokio::time::timeout(Duration::from_secs(30), ack_rx).await {
            Ok(Ok(AgentMessage::FileUploadAck {
                offset: ack_offset, ..
            })) => {
                state.file_transfers.update_progress(&transfer_id, ack_offset);
            }
            Ok(Ok(AgentMessage::FileUploadError { error, .. })) => {
                state.file_transfers.mark_failed(&transfer_id, error.clone());
                let _ = tokio::fs::remove_file(&temp_upload).await;
                return Err(AppError::BadRequest(format!("Upload failed: {error}")));
            }
            Ok(Ok(_)) => {
                // Might be upload ack received via different path; continue
            }
            Ok(Err(_)) => {
                state
                    .file_transfers
                    .mark_failed(&transfer_id, "Agent disconnected".into());
                let _ = tokio::fs::remove_file(&temp_upload).await;
                return Err(AppError::Internal("Agent disconnected during upload".into()));
            }
            Err(_) => {
                state
                    .file_transfers
                    .mark_failed(&transfer_id, "Upload timeout".into());
                let _ = tokio::fs::remove_file(&temp_upload).await;
                return Err(AppError::RequestTimeout(
                    "Upload chunk ack timeout".into(),
                ));
            }
        }

        offset += n as u64;
    }

    // Clean up the upload temp file
    let _ = tokio::fs::remove_file(&temp_upload).await;

    // Send upload end
    sender
        .send(ServerMessage::FileUploadEnd {
            transfer_id: transfer_id.clone(),
        })
        .await
        .map_err(|_| AppError::Internal("Failed to send upload end".into()))?;

    // Wait for upload complete or error
    let complete_msg_id = format!("upload-complete-{transfer_id}");
    let complete_rx = state
        .agent_manager
        .register_pending_request(complete_msg_id);

    match tokio::time::timeout(Duration::from_secs(30), complete_rx).await {
        Ok(Ok(AgentMessage::FileUploadComplete { .. })) => {
            state.file_transfers.mark_ready(&transfer_id);
        }
        Ok(Ok(AgentMessage::FileUploadError { error, .. })) => {
            state.file_transfers.mark_failed(&transfer_id, error.clone());
            return Err(AppError::BadRequest(format!("Upload failed: {error}")));
        }
        Ok(Ok(_)) => {}
        Ok(Err(_)) => {
            state
                .file_transfers
                .mark_failed(&transfer_id, "Agent disconnected".into());
            return Err(AppError::Internal("Agent disconnected".into()));
        }
        Err(_) => {
            // Timeout waiting for complete, but data was sent. Mark as ready optimistically.
            state.file_transfers.mark_ready(&transfer_id);
        }
    }

    // Cleanup transfer entry since upload is done
    state.file_transfers.remove(&transfer_id);

    let ip = extract_client_ip(&headers);
    let detail = serde_json::json!({
        "server_id": server_id,
        "path": remote_path,
        "size": file_size,
    })
    .to_string();
    let _ = AuditService::log(
        &state.db,
        &current_user.user_id,
        "file_upload",
        Some(&detail),
        &ip,
    )
    .await;

    ok(SuccessResponse { success: true })
}

#[utoipa::path(
    delete,
    path = "/api/files/transfers/{transfer_id}",
    tag = "files",
    params(("transfer_id" = String, Path, description = "Transfer ID")),
    responses(
        (status = 200, description = "Transfer cancelled", body = SuccessResponse),
        (status = 404, description = "Transfer not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn cancel_transfer(
    State(state): State<Arc<AppState>>,
    Path(transfer_id): Path<String>,
) -> Result<Json<ApiResponse<SuccessResponse>>, AppError> {
    let info = state
        .file_transfers
        .get(&transfer_id)
        .ok_or(AppError::NotFound("Transfer not found".into()))?;

    // If it's a download in progress, send cancel to agent
    if info.direction == "download" && (info.status == "pending" || info.status == "in_progress") {
        let server_id = info.server_id.to_string();
        if let Some(sender) = state.agent_manager.get_sender(&server_id) {
            let _ = sender
                .send(ServerMessage::FileDownloadCancel {
                    transfer_id: transfer_id.clone(),
                })
                .await;
        }
    }

    state.file_transfers.remove(&transfer_id);
    ok(SuccessResponse { success: true })
}
