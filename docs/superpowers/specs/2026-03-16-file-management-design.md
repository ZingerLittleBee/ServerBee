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
  │  - mkdir/edit   │  - FileWrite   │  - read_file()
  │                 │                │  - write_file()
  │  HTTP (数据)    │  临时文件       │  - delete/mkdir/move
  │  - GET 下载     │  /tmp/sb-*     │
  │  - POST 上传    │  TTL 清理      │
  │                 │                │
  │  Monaco Editor  │  FileTransfer  │  配置:
  │  文件浏览器 UI   │  Manager       │  file_root_paths
```

**三层职责**:

- **Agent**: 文件系统操作 + 分片读写，受 `CAP_FILE` + `file_root_paths` 约束
- **Server**: HTTP API 转 WS 控制消息 + HTTP 文件中转（存储转发）+ 传输状态管理
- **Browser**: 文件浏览器 UI + Monaco Editor + HTTP 上传/下载

**传输模式**: Store-and-forward — 下载时 Agent 分片发到 Server 写临时文件，Browser 从 Server HTTP 下载；上传反向。支持断点续传、浏览器原生下载进度条。

## Protocol

### New ServerMessage Variants (Server → Agent)

```rust
// 控制类（通过现有 Agent WS 通道）
FileList { msg_id: String, path: String }
FileDelete { msg_id: String, path: String, recursive: bool }
FileMkdir { msg_id: String, path: String }
FileMove { msg_id: String, from: String, to: String }
FileStat { msg_id: String, path: String }

// 传输类
FileDownloadStart { transfer_id: String, path: String }
FileDownloadCancel { transfer_id: String }
FileUploadStart { transfer_id: String, path: String, size: u64 }
FileUploadChunk { transfer_id: String, offset: u64, data: String }  // base64, 512KB/chunk
FileUploadEnd { transfer_id: String }
```

### New AgentMessage Variants (Agent → Server)

```rust
FileListResult { msg_id: String, path: String, entries: Vec<FileEntry>, error: Option<String> }
FileOpResult { msg_id: String, success: bool, error: Option<String> }
FileStatResult { msg_id: String, entry: Option<FileEntry>, error: Option<String> }

FileDownloadReady { transfer_id: String, size: u64 }
FileDownloadChunk { transfer_id: String, offset: u64, data: String }  // base64
FileDownloadEnd { transfer_id: String }
FileDownloadError { transfer_id: String, error: String }
FileUploadAck { transfer_id: String, offset: u64 }
FileUploadError { transfer_id: String, error: String }
```

### New Shared Types (common/types.rs)

```rust
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub file_type: FileType,
    pub size: u64,
    pub modified: i64,       // Unix timestamp
    pub permissions: String, // "rwxr-xr-x"
    pub owner: String,
    pub group: String,
}

pub enum FileType { File, Directory, Symlink }
```

### Design Notes

- **msg_id pattern**: Control operations use request-response pairing (consistent with TaskResult)
- **transfer_id**: Transfer operations use independent IDs for concurrent transfer support
- **Chunk size**: 512KB raw → ~682KB base64, within 1MB WS frame limit
- **No session concept**: Unlike terminal PTY sessions, file operations are stateless request-response through the existing Agent WS channel

## Security Model

### Capability Bit

```rust
pub const CAP_FILE: u32 = 1 << 6;  // 64, bit 6
// CAP_VALID_MASK updated: 0b0111_1111 = 127
// CAP_DEFAULT unchanged = 56 (CAP_FILE disabled by default)
```

Defense-in-depth (same pattern as existing capabilities):
- **Server side**: File API endpoints check `has_cap(CAP_FILE)`, return 403 if disabled
- **Agent side**: `handle_server_message` checks local atomic capabilities, sends `CapabilityDenied` if disabled

### Path Sandbox (Agent Side)

```toml
# Agent configuration
[file]
enabled = true
root_paths = ["/home", "/var/log", "/etc", "/opt"]
max_file_size = 1073741824  # 1GB
deny_patterns = ["*.key", "*.pem", "id_rsa*", ".env", "shadow", "passwd"]
```

Validation logic (`FileManager::validate_path`):

1. `canonicalize()` to resolve symlinks to real absolute path
2. Check path is under at least one `root_paths` prefix → reject path traversal
3. Check filename against `deny_patterns` → reject sensitive files
4. Symlink targets must also be within `root_paths` (prevent link escape)

### Server Transfer Security

```
temp_dir:                /tmp/serverbee-transfers/
file_pattern:            {transfer_id}.part        (random UUID)
ttl:                     30 min                    (auto-cleanup)
max_concurrent_transfers: 10 per server
max_total_temp_size:     5 GB
```

- HTTP download endpoint requires valid session/API key + transfer_id match
- Upload endpoint validates Content-Length vs declared size
- Cleanup by existing `cleanup` background task (extended)

### Audit Logging

All file operations recorded to `audit_log`:
- Actions: `file_download`, `file_upload`, `file_delete`, `file_edit`, `file_mkdir`, `file_move`
- Fields: user_id, server_id, path, size, timestamp

## Agent File Manager

### Module: `crates/agent/src/file_manager.rs`

```rust
pub struct FileManager {
    config: FileConfig,
    capabilities: Arc<AtomicU32>,
    active_transfers: DashMap<String, TransferState>,
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
delete(path, recursive) → Result<()>         // remove_file or remove_dir_all
mkdir(path) → Result<()>                     // create_dir_all
rename(from, to) → Result<()>               // both paths validated

// Download (Agent → Server)
start_download(transfer_id, path, tx)        // open handle, send Ready(size), spawn chunk loop
  → loop: read 512KB → base64 → FileDownloadChunk → until EOF → FileDownloadEnd
cancel_download(transfer_id)

// Upload (Server → Agent)
start_upload(transfer_id, path, size)        // create temp file {path}.sb-part
receive_chunk(transfer_id, offset, data)     // base64 decode → write, verify offset continuity
finish_upload(transfer_id)                   // rename .sb-part → target, cleanup state
```

### Design Notes

- **Async I/O**: All operations use `tokio::fs`, non-blocking to reporter main loop
- **Download chunking**: Spawns independent task, sends `FileEvent` via mpsc channel (same pattern as TerminalEvent)
- **Upload atomicity**: Writes to `.sb-part` temp file, renames on completion — disconnect leaves no half-written files
- **Concurrency limit**: `MAX_CONCURRENT_TRANSFERS = 3`, excess returns error
- **Backpressure**: 1ms delay between download chunks to prevent WS channel flooding
- **Timeout**: 30 min without new data → auto-cancel transfer

### reporter.rs Integration

```rust
// handle_server_message new branches:
ServerMessage::FileList { .. }          => check_cap(CAP_FILE) → file_manager.list_dir()
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
2. Server creates transfer_id, status=Pending
3. Server sends FileDownloadStart { transfer_id, path } via Agent WS
4. Agent validates → sends FileDownloadReady { transfer_id, size }
5. Server updates status=InProgress, creates temp file, receives chunks
6. Agent sends FileDownloadChunk → Server writes to temp file
7. Agent sends FileDownloadEnd → Server marks Ready
8. Server returns to Browser: { transfer_id, size, download_url }
9. Browser GET /api/files/download/{transfer_id} → streams temp file
   (Content-Disposition: attachment, supports Range headers)
10. Download complete or TTL expired → cleanup temp file
```

### Upload Flow (Browser → Server → Agent)

```
1. Browser POST /api/files/{server_id}/upload (multipart: file + remote_path)
2. Server receives file to temp dir (streaming write, no full memory buffer)
3. Server creates transfer_id, sends FileUploadStart { transfer_id, path, size } via WS
4. Server reads temp file in chunks → FileUploadChunk → Agent
5. Agent writes each chunk, responds FileUploadAck { offset }
6. Server sends FileUploadEnd after last ack
7. Agent renames .sb-part → target, responds FileOpResult
8. Server cleans temp file, returns success to Browser
```

### API Endpoints: `crates/server/src/router/api/file.rs`

```
// Control (HTTP → WS relay, oneshot channel for response, 30s timeout)
POST   /api/files/{server_id}/list        { path }              → directory listing
POST   /api/files/{server_id}/stat        { path }              → file details
POST   /api/files/{server_id}/delete      { path, recursive }
POST   /api/files/{server_id}/mkdir       { path }
POST   /api/files/{server_id}/move        { from, to }
POST   /api/files/{server_id}/edit        { path, content }     → save text edit

// Transfer
POST   /api/files/{server_id}/download    { path }              → start, returns transfer_id
GET    /api/files/download/{transfer_id}                        → get file (supports Range)
POST   /api/files/{server_id}/upload      multipart             → upload file
GET    /api/files/transfers                                     → list active transfers
DELETE /api/files/transfers/{transfer_id}                       → cancel transfer
```

### Design Notes

- **Control operations**: Browser HTTP → Server converts to WS message → waits for Agent response via `tokio::oneshot` channel (msg_id → Sender map), 30s timeout
- **Edit save**: `POST /edit` submits full content, Server converts to FileUploadStart → chunks → End flow (small file upload from Agent perspective)
- **Cleanup**: Extends existing `cleanup` task, scans temp_dir every 5 min, deletes files older than 30 min
- **OpenAPI**: All endpoints annotated with `utoipa::path`

## Frontend

### Routes & Pages

```
/files/{serverId}              → File browser + editor page
/settings/capabilities         → Existing page adds CAP_FILE toggle
```

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
- **File click**: Small files (<5MB) auto-open in Monaco, large files show details + download button
- **Edit save**: `Ctrl+S` triggers save, Monaco auto-detects syntax (yaml/json/toml/sh/conf etc)
- **Drag upload**: Drag files onto left panel to upload with progress
- **Download**: Context menu → Download, browser native download (large file support)
- **Transfer progress**: Fixed bottom bar showing all active uploads/downloads, cancellable
- **Monaco lazy load**: `React.lazy()` + code split, isolated from main bundle
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
| CAP_FILE disabled | Server 403 + Agent CapabilityDenied (defense in depth) |
| Transfer timeout (30min) | Both sides cleanup: Agent deletes .sb-part, Server deletes temp |
| Disk space exhausted | Agent/Server detect on write, return error, cleanup temp |
| WS disconnect mid-transfer | Server marks Failed, Agent cleans .sb-part, frontend offers retry |
| Concurrent transfer limit | 429 "Too many concurrent transfers" |
| File locked/permission denied | Agent forwards OS error verbatim |
| Edit save conflict | Pre-save stat compares modified time, prompt if changed |

## Testing

### Rust Unit Tests (~15)

- `FileManager`: validate_path (root_paths check ×3, symlink escape, deny_patterns ×2)
- `FileManager`: list_dir sorting, stat, delete/mkdir/move happy paths
- `FileTransferManager`: create/query/cleanup transfer, TTL expiry, concurrency limit
- `constants`: CAP_FILE bit operations, CAP_VALID_MASK update

### Rust Integration Tests (~3)

- File browsing: register Agent → list_dir → verify response
- Upload/download roundtrip: upload → download → content consistency check
- Permission enforcement: CAP_FILE=0 → API returns 403

### Frontend Vitest (~8)

- `use-file-api` hooks: list/stat/delete/upload/download request format
- `file-utils`: formatFileSize, extensionToLanguage mapping
- CAP_FILE toggle integration

## Out of Scope

- File search (grep/find) — use terminal
- Batch download as zip — future enhancement
- File permission modification (chmod/chown) — use terminal
- Real-time file tail (log tailing) — use terminal `tail -f`
