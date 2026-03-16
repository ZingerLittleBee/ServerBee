# File Management Design Spec

> Date: 2026-03-16
> Status: Approved

## Overview

ServerBee 新增 Web 文件管理功能，支持远程浏览目录、上传/下载文件（最大 1GB+）、删除/移动/重命名/新建目录、以及内置 Monaco Editor 在线编辑文本文件。通过 `CAP_FILE` 能力位 + 可配置 `file_root_paths` 路径沙箱实现安全控制。

## Architecture

```
Browser ──HTTP──→ Server ──WS──→ Agent (文件系统)
  │                 │                │
  │  HTTP (控制)    │  WS (控制)     │
  │  - list/stat    │  - FileList    │  file_manager.rs
  │  - delete/move  │  - FileDelete  │  - list_dir()
  │  - read/write   │  - FileRead   │  - read_file()
  │  - mkdir/edit   │  - FileWrite   │  - write_file()
  │                 │                │  - delete/mkdir/move
  │  HTTP (数据)    │  临时文件       │
  │  - GET 下载     │  /tmp/sb-*     │
  │  - POST 上传    │  TTL 清理      │
  │                 │                │
  │  Monaco Editor  │  FileTransfer  │  配置:
  │  文件浏览器 UI   │  Manager       │  file_root_paths
```

**三层职责**:

- **Agent**: 文件系统操作 + 分片读写，受 `CAP_FILE` + `file_root_paths` 约束
- **Server**: HTTP API 转 WS 控制消息（通过 Request-Response Relay）+ HTTP 文件中转（存储转发）+ 传输状态管理
- **Browser**: 文件浏览器 UI + Monaco Editor + HTTP 上传/下载

**传输模式**: Store-and-forward — 下载时 Agent 分片发到 Server 写临时文件，Browser 从 Server HTTP 下载；上传反向。支持断点续传、浏览器原生下载进度条。

## Protocol

### New ServerMessage Variants (Server → Agent)

```rust
// 控制类（通过现有 Agent WS 通道，使用 Request-Response Relay）
FileList { msg_id: String, path: String }
FileDelete { msg_id: String, path: String, recursive: bool }
FileMkdir { msg_id: String, path: String }
FileMove { msg_id: String, from: String, to: String }
FileStat { msg_id: String, path: String }
FileRead { msg_id: String, path: String, max_size: u64 }    // 内联读取小文件 (≤512KB)
FileWrite { msg_id: String, path: String, content: String }  // 内联写入小文件 (编辑保存)

// 传输类（大文件上传/下载）
FileDownloadStart { transfer_id: String, path: String }
FileDownloadCancel { transfer_id: String }
FileUploadStart { transfer_id: String, path: String, size: u64 }
FileUploadChunk { transfer_id: String, offset: u64, data: String }  // base64, 384KB/chunk
FileUploadEnd { transfer_id: String }
```

### New AgentMessage Variants (Agent → Server)

```rust
// 控制类响应
FileListResult { msg_id: String, path: String, entries: Vec<FileEntry>, error: Option<String> }
FileOpResult { msg_id: String, success: bool, error: Option<String> }  // delete/mkdir/move/write
FileStatResult { msg_id: String, entry: Option<FileEntry>, error: Option<String> }
FileReadResult { msg_id: String, content: Option<String>, error: Option<String> }  // base64 encoded

// 传输类响应
FileDownloadReady { transfer_id: String, size: u64 }
FileDownloadChunk { transfer_id: String, offset: u64, data: String }  // base64, 384KB/chunk
FileDownloadEnd { transfer_id: String }
FileDownloadError { transfer_id: String, error: String }
FileUploadAck { transfer_id: String, offset: u64 }
FileUploadComplete { transfer_id: String }                            // rename 成功后发送
FileUploadError { transfer_id: String, error: String }
```

### New Shared Types (common/types.rs)

```rust
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub file_type: FileType,
    pub size: u64,
    pub modified: i64,              // Unix timestamp
    pub permissions: Option<String>, // "rwxr-xr-x", Unix-only (None on Windows)
    pub owner: Option<String>,       // Unix-only
    pub group: Option<String>,       // Unix-only
}

pub enum FileType { File, Directory, Symlink }
```

### New Constant (common/constants.rs)

```rust
pub const MAX_FILE_CHUNK_SIZE: usize = 384 * 1024;  // 384KB raw → ~512KB base64
// Ensures base64 chunk + JSON envelope stays well under MAX_WS_MESSAGE_SIZE (1MB)
```

### Design Notes

- **msg_id pattern**: Control operations use request-response pairing via the new Request-Response Relay subsystem
- **transfer_id**: Transfer operations use independent IDs for concurrent transfer support
- **Chunk size**: 384KB raw → ~512KB base64 + ~100B JSON envelope = ~512KB total, 50% headroom under 1MB WS frame limit
- **Small file fast path**: `FileRead`/`FileWrite` for files ≤512KB avoids the transfer machinery entirely — editor save is a single WS round-trip
- **Large file path**: `FileDownloadStart`/`FileUploadStart` for files >512KB, uses chunked store-and-forward
- **No session concept**: Unlike terminal PTY sessions, file operations are stateless request-response through the existing Agent WS channel
- **Cross-platform**: `permissions`/`owner`/`group` are `Option<String>`, populated only on Unix via `std::os::unix::fs::PermissionsExt`

## Request-Response Relay

This is a new subsystem enabling HTTP handlers to send a WS message to an Agent and wait for the correlated response. No such infrastructure exists in the current codebase — the existing task/exec system stores results to DB and the frontend polls for them.

### Location: `AgentManager` (in `crates/server/src/service/agent_manager.rs`)

```rust
// New field on AgentManager
pending_requests: DashMap<String, oneshot::Sender<AgentMessage>>,

// New methods
impl AgentManager {
    /// Register a pending request. Returns a oneshot::Receiver to await the response.
    pub fn register_request(&self, msg_id: String) -> oneshot::Receiver<AgentMessage> {
        let (tx, rx) = oneshot::channel();
        self.pending_requests.insert(msg_id, tx);
        rx
    }

    /// Called from handle_agent_message when a response arrives.
    /// Returns true if the message was dispatched to a pending request.
    pub fn dispatch_response(&self, msg_id: &str, message: AgentMessage) -> bool {
        if let Some((_, tx)) = self.pending_requests.remove(msg_id) {
            let _ = tx.send(message);
            true
        } else {
            false
        }
    }

    /// Cleanup expired pending requests (called by session_cleaner task)
    pub fn cleanup_expired_requests(&self, max_age: Duration) { ... }
}
```

### Usage Pattern (in HTTP handlers)

```rust
async fn file_list(state: &AppState, server_id: i32, path: String) -> Result<Vec<FileEntry>> {
    let msg_id = Uuid::new_v4().to_string();
    let rx = state.agent_manager.register_request(msg_id.clone());
    state.agent_manager.send_to_agent(server_id, ServerMessage::FileList { msg_id, path })?;

    match tokio::time::timeout(Duration::from_secs(30), rx).await {
        Ok(Ok(AgentMessage::FileListResult { entries, error, .. })) => {
            match error {
                Some(e) => Err(AppError::BadRequest(e)),
                None => Ok(entries),
            }
        }
        Ok(Ok(_)) => Err(AppError::Internal("unexpected response type")),
        Ok(Err(_)) => Err(AppError::Internal("agent disconnected")),
        Err(_) => Err(AppError::RequestTimeout("agent did not respond within 30s")),
    }
}
```

### Dispatch Integration (in `router/ws/agent.rs`)

```rust
// In handle_agent_message, before existing match arms:
AgentMessage::FileListResult { ref msg_id, .. }
| AgentMessage::FileOpResult { ref msg_id, .. }
| AgentMessage::FileStatResult { ref msg_id, .. }
| AgentMessage::FileReadResult { ref msg_id, .. } => {
    if state.agent_manager.dispatch_response(msg_id, message) {
        return;  // Handled by waiting HTTP handler
    }
    // else: orphaned response, log and discard
}
```

### Cleanup

The `session_cleaner` background task is extended to call `cleanup_expired_requests()` periodically (every 60s), removing any pending requests older than 60s whose oneshot senders have not been consumed.

### Design Notes

- This relay pattern is generic and reusable for future features that need HTTP-to-WS request-response
- The `pending_requests` map lives on `AgentManager` (not `AppState`) because it's scoped to agent communication
- `DashMap` ensures thread-safe concurrent access from multiple HTTP handler tasks
- Memory bounded: each pending entry is ~200 bytes (msg_id String + oneshot Sender), max ~100 concurrent entries

## Security Model

### Capability Bit

```rust
pub const CAP_FILE: u32 = 1 << 6;  // 64, bit 6
// CAP_VALID_MASK updated: 0b0111_1111 = 127
// CAP_DEFAULT unchanged = 56 (CAP_FILE disabled by default)
```

Defense-in-depth (same pattern as existing capabilities):
- **Server side**: File API endpoints check `has_cap(CAP_FILE)`, return 403 if disabled
- **Agent side**: `handle_server_message` checks local atomic capabilities, sends error response if disabled
  - Control operations (FileList/FileRead/etc): send `FileOpResult { success: false, error: "capability denied" }` or corresponding error variant
  - Transfer operations (FileDownloadStart/FileUploadStart): send `FileDownloadError`/`FileUploadError` with "capability denied" message (avoids modifying existing `CapabilityDenied` variant which lacks `transfer_id`)

### Path Sandbox (Agent Side)

```toml
# Agent configuration
[file]
enabled = true
root_paths = ["/home", "/var/log", "/etc", "/opt"]
max_file_size = 1073741824  # 1GB
deny_patterns = ["*.key", "*.pem", "id_rsa*", ".env*", "shadow", "passwd"]
```

**Default values** (when `[file]` section is omitted):
- `enabled`: `false` (deny-all, must opt-in)
- `root_paths`: `[]` (empty = block all access, safe default)
- `max_file_size`: `1073741824` (1GB)
- `deny_patterns`: `["*.key", "*.pem", "id_rsa*", ".env*", "shadow", "passwd"]`

Validation logic (`FileManager::validate_path`):

1. Check `root_paths` is non-empty → else reject all access
2. `canonicalize()` to resolve symlinks to real absolute path
3. Check path is under at least one `root_paths` prefix → reject path traversal
4. Check filename against `deny_patterns` → reject sensitive files
5. Symlink targets must also be within `root_paths` (prevent link escape)

### Server Transfer Security

```
temp_dir:                /tmp/serverbee-transfers/
file_pattern:            {transfer_id}.part        (random UUID)
ttl:                     30 min                    (auto-cleanup)
max_concurrent_transfers: 3 per server             (matches agent-side limit)
max_total_temp_size:     5 GB
```

- HTTP download endpoint requires valid session/API key + transfer_id match
- Upload endpoint validates Content-Length vs declared size
- Cleanup by existing `cleanup` background task (extended)

### Audit Logging

All file operations recorded to `audit_log` using existing schema. File-specific details stored as structured JSON in the `detail` column:

```json
{
    "server_id": 1,
    "path": "/home/app/config.yaml",
    "size": 2048,
    "operation": "download"
}
```

Actions: `file_download`, `file_upload`, `file_delete`, `file_edit`, `file_mkdir`, `file_move`

## Agent File Manager

### Module: `crates/agent/src/file_manager.rs`

```rust
pub struct FileManager {
    config: FileConfig,
    capabilities: Arc<AtomicU32>,
    active_transfers: DashMap<String, TransferState>,
}

pub struct FileConfig {
    pub enabled: bool,                    // default: false
    pub root_paths: Vec<PathBuf>,         // default: [] (deny all)
    pub max_file_size: u64,               // default: 1GB
    pub deny_patterns: Vec<String>,       // default: ["*.key", "*.pem", ...]
}

struct TransferState {
    path: PathBuf,
    direction: Direction,  // Upload / Download
    offset: u64,
    total_size: u64,
    file_handle: Option<tokio::fs::File>,
    started_at: Instant,
}
```

### Core Methods

```
validate_path(path) → Result<PathBuf>
list_dir(path) → Result<Vec<FileEntry>>     // tokio::fs::read_dir, sort: dirs first → alpha
stat(path) → Result<FileEntry>
read_file(path, max_size) → Result<String>   // read ≤max_size bytes, base64 encode, reject if too large
write_file(path, content) → Result<()>       // base64 decode → atomic write via .sb-tmp rename
delete(path, recursive) → Result<()>         // remove_file or remove_dir_all
mkdir(path) → Result<()>                     // create_dir_all
rename(from, to) → Result<()>               // both paths validated

// Download (Agent → Server), large files
start_download(transfer_id, path, tx)        // open handle, send Ready(size), spawn chunk loop
  → loop: read 384KB → base64 → FileDownloadChunk → until EOF → FileDownloadEnd
cancel_download(transfer_id)

// Upload (Server → Agent), large files
start_upload(transfer_id, path, size)        // create temp file {path}.sb-part
receive_chunk(transfer_id, offset, data)     // base64 decode → write, verify offset continuity
finish_upload(transfer_id)                   // rename .sb-part → target, send FileUploadComplete
```

### Design Notes

- **Async I/O**: All operations use `tokio::fs`, non-blocking to reporter main loop
- **Download chunking**: Spawns independent task, sends `FileEvent` via bounded mpsc channel (capacity 4) — channel backpressure naturally throttles disk reads when WS send is slow (replaces naive 1ms delay)
- **Upload atomicity**: Writes to `.sb-part` temp file, renames on completion — disconnect leaves no half-written files
- **Edit atomicity**: `write_file` writes to `.sb-tmp`, renames on success
- **Concurrency limit**: `MAX_CONCURRENT_TRANSFERS = 3`, excess returns error
- **Timeout**: 30 min without new data → auto-cancel transfer

### reporter.rs Integration

```rust
// handle_server_message new branches:
ServerMessage::FileList { .. }          => check_cap(CAP_FILE) → file_manager.list_dir()
ServerMessage::FileRead { .. }          => check_cap(CAP_FILE) → file_manager.read_file()
ServerMessage::FileWrite { .. }         => check_cap(CAP_FILE) → file_manager.write_file()
ServerMessage::FileDelete { .. }        => check_cap(CAP_FILE) → file_manager.delete()
ServerMessage::FileDownloadStart { .. } => check_cap(CAP_FILE) → file_manager.start_download()
ServerMessage::FileUploadChunk { .. }   => check_cap(CAP_FILE) → file_manager.receive_chunk()
// ... etc

// Main loop select! new branch:
event = file_rx.recv() => {
    match event {
        FileEvent::DownloadChunk { .. } => send AgentMessage::FileDownloadChunk
        FileEvent::DownloadEnd { .. }   => send AgentMessage::FileDownloadEnd
        FileEvent::DownloadError { .. } => send AgentMessage::FileDownloadError
    }
}
```

## Server File Transfer Service

### Module: `crates/server/src/service/file_transfer.rs`

```rust
pub struct FileTransferManager {
    transfers: DashMap<String, TransferMeta>,
    temp_dir: PathBuf,
    max_total_size: u64,
}

struct TransferMeta {
    transfer_id: String,
    server_id: i32,
    user_id: i32,
    direction: Direction,
    file_path: String,
    file_size: Option<u64>,
    temp_file: PathBuf,
    status: TransferStatus,
    created_at: Instant,
    last_activity: Instant,
}

enum TransferStatus { Pending, InProgress, Ready, Failed(String) }
```

### Download Flow (Agent → Server → Browser)

```
1. Browser POST /api/files/{server_id}/download  { path }
2. Server checks CAP_FILE → creates transfer_id, status=Pending
3. Server sends FileDownloadStart { transfer_id, path } via Agent WS
4. Server returns immediately: { transfer_id, status: "pending" }
5. Agent validates → sends FileDownloadReady { transfer_id, size }
6. Server updates status=InProgress, creates temp file, receives chunks
7. Agent sends FileDownloadChunk (384KB each) → Server writes to temp file
8. Agent sends FileDownloadEnd → Server marks status=Ready
9. Browser polls GET /api/files/transfers (or uses the transfer_id directly)
10. Browser GET /api/files/download/{transfer_id} → streams temp file
    (Content-Disposition: attachment, supports Range headers for resume)
11. Download complete or TTL expired → cleanup temp file
```

Note: The POST at step 1 returns immediately (no blocking wait for large files). Browser polls transfer status or can attempt the download URL after a reasonable delay.

### Upload Flow (Browser → Server → Agent)

```
1. Browser POST /api/files/{server_id}/upload (multipart: file + remote_path)
2. Server receives file to temp dir (streaming write, no full memory buffer)
3. Server creates transfer_id, sends FileUploadStart { transfer_id, path, size } via WS
4. Server reads temp file in 384KB chunks → FileUploadChunk → Agent
5. Agent writes each chunk, responds FileUploadAck { offset }
6. Server sends FileUploadEnd after last ack
7. Agent renames .sb-part → target, sends FileUploadComplete { transfer_id }
8. Server cleans temp file, returns success to Browser
```

### API Endpoints: `crates/server/src/router/api/file.rs`

```
// Control (HTTP → WS relay via Request-Response Relay, 30s timeout)
POST   /api/files/{server_id}/list        { path }              → directory listing
POST   /api/files/{server_id}/stat        { path }              → file details
POST   /api/files/{server_id}/delete      { path, recursive }
POST   /api/files/{server_id}/mkdir       { path }
POST   /api/files/{server_id}/move        { from, to }
POST   /api/files/{server_id}/read        { path }              → small file content (≤512KB)
POST   /api/files/{server_id}/write       { path, content }     → save text edit (≤512KB)

// Transfer (large files, async)
POST   /api/files/{server_id}/download    { path }              → start, returns transfer_id (non-blocking)
GET    /api/files/download/{transfer_id}                        → get file (supports Range)
POST   /api/files/{server_id}/upload      multipart             → upload file
GET    /api/files/transfers                                     → list active transfers (with progress)
DELETE /api/files/transfers/{transfer_id}                       → cancel transfer
```

### Design Notes

- **Control operations**: Use the new Request-Response Relay subsystem (msg_id → oneshot channel, 30s timeout)
- **Edit save**: `POST /write` sends content inline via `FileWrite` message — single WS round-trip, no transfer machinery. For files >512KB, client should use the upload flow instead
- **Download is async**: `POST /download` returns immediately with `transfer_id`. Browser polls transfer status, then downloads when ready. No long-blocking HTTP request for large files
- **Cleanup**: Extends existing `cleanup` task, scans temp_dir every 5 min, deletes files older than 30 min
- **OpenAPI**: All endpoints annotated with `utoipa::path`

## Frontend

### Routes & Pages

```
/files/{serverId}              → File browser + editor page
/settings/capabilities         → Existing page adds CAP_FILE toggle
```

### Server Detail Integration

The existing Server Detail page (`routes/_authed/servers/$id.tsx`) adds a **Files** button next to the existing Terminal button:
- Only shown when `hasCap(server.capabilities, CAP_FILE)` and server is online
- Links to `/files/{serverId}`

### Page Layout

```
┌─────────────────────────────────────────────────────┐
│ Breadcrumb: / home / app / config                    │
│ [Upload] [New Folder] [Refresh]           Search [__]│
├──────────────────────────┬──────────────────────────┤
│  File List (left 40%)     │  Preview/Editor (right)  │
│                          │                          │
│  📁 ..                   │  ┌── Monaco Editor ────┐ │
│  📁 logs/                │  │ server:              │ │
│  📁 scripts/             │  │   port: 8080         │ │
│  📄 config.yaml  2.1KB   │  │   host: 0.0.0.0      │ │
│  📄 app.log     156MB    │  │   workers: 4         │ │
│  📄 start.sh    512B     │  │                      │ │
│                          │  └──────────────────────┘ │
│  Context menu:           │  [Save] [Undo] [Save As]  │
│  Download / Delete       │  Lang: yaml  Ln: 3 Col: 12│
│  / Rename / Copy path    │                          │
├──────────────────────────┴──────────────────────────┤
│ Transfers: config.yaml ████████░░ 80%  1.6/2.1KB    │
│            app.log    ██░░░░░░░░ 20% 31/156MB  [✕]  │
└─────────────────────────────────────────────────────┘
```

### Components

```
apps/web/src/
├── routes/_authed/files.$serverId.tsx    # File manager route
├── components/file/
│   ├── file-browser.tsx                  # File list (DataTable, sort/search)
│   ├── file-breadcrumb.tsx               # Breadcrumb path navigation
│   ├── file-editor.tsx                   # Monaco Editor wrapper
│   ├── file-preview.tsx                  # Image/text preview switch
│   ├── file-context-menu.tsx             # Right-click context menu
│   ├── file-upload-dialog.tsx            # Upload dialog (drag-drop zone)
│   ├── transfer-bar.tsx                  # Bottom transfer progress bar
│   └── mkdir-dialog.tsx                  # New folder dialog
├── hooks/
│   └── use-file-api.ts                   # TanStack Query hooks
└── lib/
    └── file-utils.ts                     # formatFileSize, fileIcon, extensionToLanguage
```

### Interaction Details

- **Directory browsing**: Click to enter, breadcrumb supports jumping to any level
- **File click**: Small files (<5MB) load content via `POST /read` (fast path), open in Monaco; large files show details + download button
- **Edit save**: `Ctrl+S` triggers `POST /write` (fast path for ≤512KB), Monaco auto-detects syntax
- **Drag upload**: Drag files onto left panel to upload with progress
- **Download**: Context menu → Download, triggers async download flow, progress shown in transfer bar
- **Transfer progress**: Fixed bottom bar showing all active uploads/downloads (polls `/transfers`), cancellable
- **Monaco lazy load**: `React.lazy()` + code split, isolated from main bundle (~2MB gzip)
- **Save conflict**: Pre-save stat check compares modified time, prompts "File modified externally, overwrite?" if changed

### Sidebar & Navigation

- Server Detail page adds **Files** button (like existing Terminal button, gated by `CAP_FILE`)
- Only shown when `hasCap(server.capabilities, CAP_FILE)`

### i18n

- New `file` namespace, ~40 keys (Chinese + English)

## Error Handling

| Scenario | Handling |
|----------|----------|
| Agent offline | HTTP 404 "Server offline", in-flight transfer marked Failed |
| Path outside root_paths | Agent error "Access denied: path outside allowed roots" |
| File not found | Agent returns error, frontend toast notification |
| deny_patterns matched | Agent rejects "Access denied: file type blocked" |
| CAP_FILE disabled (server) | Server returns 403 immediately |
| CAP_FILE disabled (agent) | Agent sends typed error response (FileDownloadError/FileUploadError/FileOpResult with error), not CapabilityDenied (which lacks transfer_id) |
| Transfer timeout (30min) | Both sides cleanup: Agent deletes .sb-part, Server deletes temp |
| Disk space exhausted | Agent/Server detect on write, return error, cleanup temp |
| WS disconnect mid-transfer | Server marks Failed, Agent cleans .sb-part, frontend offers retry |
| Concurrent transfer limit (3) | Server returns 429 immediately (matches agent limit, no wasted requests) |
| File locked/permission denied | Agent forwards OS error verbatim |
| Edit save conflict | Pre-save stat compares modified time, prompt if changed |
| File too large for inline read | Agent returns error on FileRead if >max_size, frontend falls back to download flow |

## Testing

### Rust Unit Tests (~18)

- `FileManager`: validate_path (root_paths check ×3, symlink escape, deny_patterns ×2, empty root_paths deny-all)
- `FileManager`: list_dir sorting, stat, read_file, write_file, delete/mkdir/move happy paths
- `FileTransferManager`: create/query/cleanup transfer, TTL expiry, concurrency limit (3)
- `AgentManager`: register_request/dispatch_response/cleanup_expired_requests
- `constants`: CAP_FILE bit operations, CAP_VALID_MASK=127

### Rust Integration Tests (~3)

- File browsing: register Agent → list_dir → verify response
- Upload/download roundtrip: upload → download → content consistency check
- Permission enforcement: CAP_FILE=0 → API returns 403

### Frontend Vitest (~8)

- `use-file-api` hooks: list/stat/delete/read/write/upload/download request format
- `file-utils`: formatFileSize, extensionToLanguage mapping
- CAP_FILE toggle integration

## Out of Scope

- File search (grep/find) — use terminal
- Batch download as zip — future enhancement
- File permission modification (chmod/chown) — use terminal
- Real-time file tail (log tailing) — use terminal `tail -f`
