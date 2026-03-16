# File Management Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add web-based file management (browse, upload/download 1GB+, edit with Monaco) to ServerBee with CAP_FILE capability toggle and path sandbox.

**Architecture:** Agent-side FileManager handles filesystem ops with path validation. Server relays control messages via new Request-Response Relay (oneshot channels) and large file transfers via store-and-forward temp files. Frontend uses TanStack Query for control ops, native HTTP for transfers, and Monaco Editor for text editing.

**Tech Stack:** Rust (tokio::fs, base64, DashMap, oneshot), Axum (multipart, streaming), React 19 (TanStack Query, Monaco Editor, shadcn/ui DataTable)

**Spec:** `docs/superpowers/specs/2026-03-16-file-management-design.md`

---

## Chunk 1: Common Layer (Protocol + Constants + Types)

### Task 1: Add FileEntry and FileType to common types

**Files:**
- Modify: `crates/common/src/types.rs:139` (append after ServerStatus)

- [ ] **Step 1: Add FileEntry and FileType structs**

After line 139 in `types.rs`, add:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileType {
    File,
    Directory,
    Symlink,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub file_type: FileType,
    pub size: u64,
    pub modified: i64,
    pub permissions: Option<String>,
    pub owner: Option<String>,
    pub group: Option<String>,
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p serverbee-common`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/common/src/types.rs
git commit -m "feat(common): add FileEntry and FileType types for file management"
```

---

### Task 2: Add CAP_FILE and MAX_FILE_CHUNK_SIZE constants

**Files:**
- Modify: `crates/common/src/constants.rs:39,42,53-60,77-130`
- Test: same file (inline tests)

- [ ] **Step 1: Write failing test for CAP_FILE**

Add test at end of `mod tests` block (before closing `}` at line 130):

```rust
#[test]
fn test_cap_file_bit() {
    assert_eq!(CAP_FILE, 64);
    assert!(has_capability(CAP_FILE, CAP_FILE));
    assert!(!has_capability(CAP_DEFAULT, CAP_FILE)); // disabled by default
    assert!(CAP_FILE & CAP_VALID_MASK == CAP_FILE); // within valid mask
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p serverbee-common test_cap_file_bit`
Expected: FAIL — `CAP_FILE` not found

- [ ] **Step 3: Add constants**

After line 39 (`CAP_PING_HTTP`), add:
```rust
pub const CAP_FILE: u32 = 1 << 6; // 64
```

Update line 42:
```rust
pub const CAP_VALID_MASK: u32 = 0b0111_1111; // 127
```

After line 12 (`MAX_BINARY_FRAME_SIZE`), add:
```rust
pub const MAX_FILE_CHUNK_SIZE: usize = 384 * 1024; // 384KB raw → ~512KB base64
pub const MAX_FILE_CONCURRENT_TRANSFERS: usize = 3;
pub const FILE_TRANSFER_TIMEOUT_SECS: u64 = 1800; // 30 min
```

Add entry to `ALL_CAPABILITIES` array (after line 59, the `CAP_PING_HTTP` entry):
```rust
CapabilityMeta { bit: CAP_FILE, key: "file", display_name: "File Manager", default_enabled: false, risk_level: "high" },
```

- [ ] **Step 4: Update existing test_valid_mask**

The existing test at line 107-113 asserts `CAP_VALID_MASK == 63` and `64 & !CAP_VALID_MASK != 0`. Update:
```rust
#[test]
fn test_valid_mask() {
    assert_eq!(CAP_VALID_MASK, 127);
    for meta in ALL_CAPABILITIES {
        assert!(meta.bit & CAP_VALID_MASK == meta.bit);
    }
    assert!(128 & !CAP_VALID_MASK != 0); // next bit beyond valid
}
```

- [ ] **Step 5: Run all tests**

Run: `cargo test -p serverbee-common`
Expected: ALL PASS

- [ ] **Step 6: Commit**

```bash
git add crates/common/src/constants.rs
git commit -m "feat(common): add CAP_FILE capability bit and file transfer constants"
```

---

### Task 3: Add file protocol messages

**Files:**
- Modify: `crates/common/src/protocol.rs:1-6,11-44,49-98`
- Test: same file (inline tests)

- [ ] **Step 1: Add FileEntry import**

Update line 3-6 to include `FileEntry`:
```rust
use crate::types::{
    FileEntry, NetworkProbeResultData, NetworkProbeTarget, PingResult, PingTaskConfig, SystemInfo,
    SystemReport, TaskResult,
};
```

- [ ] **Step 2: Add AgentMessage variants**

Before `Pong` (line 43), add these variants:

```rust
    // File management responses
    FileListResult {
        msg_id: String,
        path: String,
        entries: Vec<FileEntry>,
        error: Option<String>,
    },
    FileStatResult {
        msg_id: String,
        entry: Option<FileEntry>,
        error: Option<String>,
    },
    FileReadResult {
        msg_id: String,
        content: Option<String>, // base64 encoded
        error: Option<String>,
    },
    FileOpResult {
        msg_id: String,
        success: bool,
        error: Option<String>,
    },
    // File transfer responses
    FileDownloadReady {
        transfer_id: String,
        size: u64,
    },
    FileDownloadChunk {
        transfer_id: String,
        offset: u64,
        data: String, // base64 encoded, max 384KB raw
    },
    FileDownloadEnd {
        transfer_id: String,
    },
    FileDownloadError {
        transfer_id: String,
        error: String,
    },
    FileUploadAck {
        transfer_id: String,
        offset: u64,
    },
    FileUploadComplete {
        transfer_id: String,
    },
    FileUploadError {
        transfer_id: String,
        error: String,
    },
```

- [ ] **Step 3: Add ServerMessage variants**

Before `Ping` (line 90), add:

```rust
    // File management commands
    FileList {
        msg_id: String,
        path: String,
    },
    FileDelete {
        msg_id: String,
        path: String,
        recursive: bool,
    },
    FileMkdir {
        msg_id: String,
        path: String,
    },
    FileMove {
        msg_id: String,
        from: String,
        to: String,
    },
    FileStat {
        msg_id: String,
        path: String,
    },
    FileRead {
        msg_id: String,
        path: String,
        max_size: u64,
    },
    FileWrite {
        msg_id: String,
        path: String,
        content: String, // base64 encoded
    },
    // File transfer commands
    FileDownloadStart {
        transfer_id: String,
        path: String,
    },
    FileDownloadCancel {
        transfer_id: String,
    },
    FileUploadStart {
        transfer_id: String,
        path: String,
        size: u64,
    },
    FileUploadChunk {
        transfer_id: String,
        offset: u64,
        data: String, // base64 encoded
    },
    FileUploadEnd {
        transfer_id: String,
    },
```

- [ ] **Step 4: Add serialization round-trip test**

Add to `mod tests`:
```rust
#[test]
fn test_file_list_round_trip() {
    let msg = ServerMessage::FileList {
        msg_id: "m1".into(),
        path: "/home".into(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("file_list"));
    let parsed: ServerMessage = serde_json::from_str(&json).unwrap();
    match parsed {
        ServerMessage::FileList { msg_id, path } => {
            assert_eq!(msg_id, "m1");
            assert_eq!(path, "/home");
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn test_file_list_result_round_trip() {
    use crate::types::{FileEntry, FileType};
    let entry = FileEntry {
        name: "test.txt".into(),
        path: "/home/test.txt".into(),
        file_type: FileType::File,
        size: 1024,
        modified: 1710000000,
        permissions: Some("rw-r--r--".into()),
        owner: Some("root".into()),
        group: Some("root".into()),
    };
    let msg = AgentMessage::FileListResult {
        msg_id: "m1".into(),
        path: "/home".into(),
        entries: vec![entry],
        error: None,
    };
    let json = serde_json::to_string(&msg).unwrap();
    let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
    match parsed {
        AgentMessage::FileListResult { entries, .. } => {
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].name, "test.txt");
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn test_file_download_chunk_round_trip() {
    let msg = AgentMessage::FileDownloadChunk {
        transfer_id: "t1".into(),
        offset: 0,
        data: "aGVsbG8=".into(), // "hello" in base64
    };
    let json = serde_json::to_string(&msg).unwrap();
    let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
    match parsed {
        AgentMessage::FileDownloadChunk { transfer_id, offset, data } => {
            assert_eq!(transfer_id, "t1");
            assert_eq!(offset, 0);
            assert_eq!(data, "aGVsbG8=");
        }
        _ => panic!("Wrong variant"),
    }
}
```

- [ ] **Step 5: Run all tests**

Run: `cargo test -p serverbee-common`
Expected: ALL PASS

- [ ] **Step 6: Add placeholder match arms to keep workspace compilable**

After adding protocol variants, the `match msg` blocks in `reporter.rs:251` and `router/ws/agent.rs` will fail due to non-exhaustive patterns. Add temporary wildcard arms:

In `crates/agent/src/reporter.rs`, in the `match msg` block (after `ServerMessage::NetworkProbeSync` arm, line 361), add:
```rust
            _ => {
                tracing::debug!("Unhandled server message variant");
            }
```

In `crates/server/src/router/ws/agent.rs`, in the `handle_agent_message` match block, add the same wildcard for new `AgentMessage` file variants:
```rust
            _ => {
                tracing::debug!("Unhandled agent message variant");
            }
```

These placeholders will be removed in Tasks 6 and 11 when proper handlers are added.

- [ ] **Step 7: Verify workspace compilation**

Run: `cargo check --workspace`
Expected: PASS (all crates compile with placeholder match arms)

- [ ] **Step 8: Commit**

```bash
git add crates/common/src/protocol.rs crates/agent/src/reporter.rs crates/server/src/router/ws/agent.rs
git commit -m "feat(common): add file management protocol messages"
```

---

## Chunk 2: Agent Side

### Task 4: Add FileConfig to agent config

**Files:**
- Modify: `crates/agent/src/config.rs:7-18,58`

- [ ] **Step 1: Add FileConfig struct**

After `LogConfig` (line 36), add:

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub root_paths: Vec<String>,
    #[serde(default = "default_max_file_size")]
    pub max_file_size: u64,
    #[serde(default = "default_deny_patterns")]
    pub deny_patterns: Vec<String>,
}

fn default_max_file_size() -> u64 {
    1_073_741_824 // 1GB
}

fn default_deny_patterns() -> Vec<String> {
    vec![
        "*.key".into(), "*.pem".into(), "id_rsa*".into(),
        ".env*".into(), "shadow".into(), "passwd".into(),
    ]
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            root_paths: Vec::new(),
            max_file_size: default_max_file_size(),
            deny_patterns: default_deny_patterns(),
        }
    }
}
```

- [ ] **Step 2: Add file field to AgentConfig**

Add after line 17 (`pub log: LogConfig`):
```rust
    #[serde(default)]
    pub file: FileConfig,
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p serverbee-agent`
Expected: PASS (or warnings about non-exhaustive match — acceptable at this stage)

- [ ] **Step 4: Commit**

```bash
git add crates/agent/src/config.rs
git commit -m "feat(agent): add FileConfig with root_paths and deny_patterns"
```

---

### Task 5: Create file_manager.rs (Agent)

**Files:**
- Create: `crates/agent/src/file_manager.rs`
- Modify: `crates/agent/src/main.rs:8` (add `mod file_manager;`)

- [ ] **Step 1: Declare module**

Add `mod file_manager;` after line 8 in `main.rs`.

- [ ] **Step 2: Write failing path validation tests first (TDD — security-critical)**

Create `crates/agent/src/file_manager.rs` with the module structure and test block first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_manager(root_paths: Vec<&str>) -> (FileManager, TempDir) {
        let tmp = TempDir::new().unwrap();
        let config = FileConfig {
            enabled: true,
            root_paths: root_paths.iter().map(|s| s.to_string()).collect(),
            max_file_size: 1_073_741_824,
            deny_patterns: vec!["*.key".into(), "*.pem".into(), "id_rsa*".into(), ".env*".into(), "shadow".into()],
        };
        let caps = Arc::new(AtomicU32::new(u32::MAX));
        (FileManager::new(config, caps), tmp)
    }

    #[test]
    fn test_validate_path_within_root() {
        let tmp = TempDir::new().unwrap();
        let (mgr, _) = make_manager(vec![tmp.path().to_str().unwrap()]);
        let test_file = tmp.path().join("test.txt");
        fs::write(&test_file, "hello").unwrap();
        assert!(mgr.validate_path(test_file.to_str().unwrap()).is_ok());
    }

    #[test]
    fn test_validate_path_outside_root() {
        let (mgr, _tmp) = make_manager(vec!["/nonexistent/path"]);
        assert!(mgr.validate_path("/etc/hosts").is_err());
    }

    #[test]
    fn test_validate_path_traversal_rejected() {
        let tmp = TempDir::new().unwrap();
        let (mgr, _) = make_manager(vec![tmp.path().to_str().unwrap()]);
        let evil = format!("{}/../../../etc/passwd", tmp.path().display());
        assert!(mgr.validate_path(&evil).is_err());
    }

    #[test]
    fn test_validate_path_deny_patterns() {
        let tmp = TempDir::new().unwrap();
        let (mgr, _) = make_manager(vec![tmp.path().to_str().unwrap()]);
        let key_file = tmp.path().join("server.key");
        fs::write(&key_file, "secret").unwrap();
        assert!(mgr.validate_path(key_file.to_str().unwrap()).is_err());
    }

    #[test]
    fn test_validate_path_empty_roots_denies_all() {
        let (mgr, _tmp) = make_manager(vec![]);
        assert!(mgr.validate_path("/tmp/anything").is_err());
    }
}
```

Run: `cargo test -p serverbee-agent -- file_manager`
Expected: FAIL — `FileManager`, `FileConfig` not yet implemented

- [ ] **Step 3: Create file_manager.rs with core types and implementation**

Implement the full `FileManager` in `crates/agent/src/file_manager.rs`:

- `FileEvent` enum (DownloadChunk, DownloadEnd, DownloadError) — implements `From<FileEvent> for AgentMessage`
- `FileManager` struct (config: FileConfig, capabilities: Arc<AtomicU32>, active_transfers: DashMap<String, TransferState>)
- `TransferState` struct (path, direction, offset, total_size, file_handle, started_at)
- `validate_path(&self, path: &str) -> Result<PathBuf>` — canonicalize + root_paths check + deny_patterns
- `list_dir(&self, path: &str) -> Result<Vec<FileEntry>>` — tokio::fs::read_dir, dirs first, alpha sort
- `stat(&self, path: &str) -> Result<FileEntry>` — metadata to FileEntry
- `read_file(&self, path: &str, max_size: u64) -> Result<String>` — check size first, read and base64 encode; return error if file > max_size
- `write_file(&self, path: &str, content: &str) -> Result<()>` — base64 decode, atomic write via .sb-tmp rename
- `delete(&self, path: &str, recursive: bool) -> Result<()>`
- `mkdir(&self, path: &str) -> Result<()>`
- `rename(&self, from: &str, to: &str) -> Result<()>` — both paths validated
- `start_download(&self, transfer_id: &str, path: &str, tx: mpsc::Sender<FileEvent>)` — spawn task, bounded channel (capacity 4) for backpressure
- `cancel_download(&self, transfer_id: &str)`
- `cancel_all_transfers(&self)` — cancel all active transfers (for disconnect cleanup)
- `start_upload(&self, transfer_id: &str, path: &str, size: u64) -> Result<()>` — create .sb-part
- `receive_chunk(&self, transfer_id: &str, offset: u64, data: &str) -> Result<u64>` — base64 decode, write
- `finish_upload(&self, transfer_id: &str) -> Result<()>` — rename .sb-part → target, cleanup state

Key implementation notes:
- Use `#[cfg(unix)]` for `std::os::unix::fs::PermissionsExt` and `std::os::unix::fs::MetadataExt` (owner/group)
- On Windows, set permissions/owner/group to `None`
- `validate_path`: `std::fs::canonicalize()` then check `starts_with` for each root_path
- deny_patterns: simple matching (`*.key` → endswith `.key`, `id_rsa*` → startswith `id_rsa`, `.env*` → startswith `.env`)
- Download chunking: read `MAX_FILE_CHUNK_SIZE` (384KB) bytes at a time, base64 encode, send via bounded mpsc
- Concurrent transfer limit: `MAX_FILE_CONCURRENT_TRANSFERS` (3)

- [ ] **Step 4: Add list_dir test**

Add to the existing `#[cfg(test)]` block:
```rust
#[tokio::test]
async fn test_list_dir_sorts_dirs_first() {
    let tmp = TempDir::new().unwrap();
    let (mgr, _) = make_manager(vec![tmp.path().to_str().unwrap()]);
    fs::create_dir(tmp.path().join("zdir")).unwrap();
    fs::write(tmp.path().join("afile.txt"), "hi").unwrap();
    fs::create_dir(tmp.path().join("adir")).unwrap();
    let entries = mgr.list_dir(tmp.path().to_str().unwrap()).await.unwrap();
    // dirs first (sorted), then files (sorted)
    assert_eq!(entries[0].name, "adir");
    assert_eq!(entries[1].name, "zdir");
    assert_eq!(entries[2].name, "afile.txt");
}
```

- [ ] **Step 4: Verify compilation and run tests**

Run: `cargo test -p serverbee-agent -- file_manager`
Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
git add crates/agent/src/file_manager.rs crates/agent/src/main.rs
git commit -m "feat(agent): add FileManager with path validation and file operations"
```

---

### Task 6: Integrate FileManager into reporter

**Files:**
- Modify: `crates/agent/src/reporter.rs:1-19,118-128,137-224,228-361`

- [ ] **Step 1: Add imports and FileManager initialization**

Add import at top:
```rust
use crate::file_manager::{FileEvent, FileManager};
```

After line 128 (network_prober init), add:
```rust
        // File manager
        let (file_tx, mut file_rx) = mpsc::channel::<FileEvent>(16);
        let mut file_manager = FileManager::new(
            self.config.file.clone(),
            Arc::clone(&capabilities),
        );
```

- [ ] **Step 2: Add file_rx branch to select! loop**

After the `network_probe_rx` branch (line 179-189), add:
```rust
                Some(file_event) = file_rx.recv() => {
                    let msg: AgentMessage = file_event.into();
                    let json = serde_json::to_string(&msg)?;
                    write.send(Message::Text(json.into())).await?;
                    tracing::debug!("Sent file event");
                }
```

Note: Implement `From<FileEvent> for AgentMessage` in file_manager.rs.

- [ ] **Step 3: Add file_manager to handle_server_message signature**

Update the function signature (line 228) to accept `file_manager: &mut FileManager` and `file_tx: &mpsc::Sender<FileEvent>` as additional parameters. Update the call site at line 194.

- [ ] **Step 4: Add file message match arms**

In the `match msg` block (after `ServerMessage::NetworkProbeSync` arm, before the closing `}`), add match arms for all file ServerMessage variants:

```rust
ServerMessage::FileList { msg_id, path } => {
    let caps = capabilities.load(Ordering::SeqCst);
    if !has_capability(caps, CAP_FILE) {
        let result = AgentMessage::FileOpResult { msg_id, success: false, error: Some("File capability disabled".into()) };
        let json = serde_json::to_string(&result)?;
        write.send(Message::Text(json.into())).await?;
        return Ok(());
    }
    let result = file_manager.list_dir(&path);
    let msg = match result {
        Ok(entries) => AgentMessage::FileListResult { msg_id, path, entries, error: None },
        Err(e) => AgentMessage::FileListResult { msg_id, path, entries: vec![], error: Some(e.to_string()) },
    };
    let json = serde_json::to_string(&msg)?;
    write.send(Message::Text(json.into())).await?;
}
// ... similar for FileStat, FileRead, FileWrite, FileDelete, FileMkdir, FileMove
// For FileDownloadStart: call file_manager.start_download(transfer_id, path, file_tx.clone())
// For FileUploadStart/Chunk/End: call corresponding file_manager methods
// For FileDownloadCancel: call file_manager.cancel_download
```

- [ ] **Step 5: Add cleanup on disconnect**

In the three disconnect paths (lines 198-200, 209-211, 217-219), add:
```rust
file_manager.cancel_all_transfers();
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check -p serverbee-agent`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/agent/src/reporter.rs crates/agent/src/file_manager.rs
git commit -m "feat(agent): integrate FileManager into reporter message loop"
```

---

## Chunk 3: Server Side — Infrastructure

### Task 7: Add Request-Response Relay to AgentManager

**Files:**
- Modify: `crates/server/src/service/agent_manager.rs:1-5,20-26,45-53,247`

- [ ] **Step 1: Write failing test**

Add to `mod tests`:
```rust
#[test]
fn test_pending_request_lifecycle() {
    let (mgr, _rx) = make_manager();
    let rx = mgr.register_pending_request("req1".into());
    assert!(rx.try_recv().is_err()); // not yet dispatched

    let dispatched = mgr.dispatch_pending_response(
        "req1",
        AgentMessage::FileOpResult { msg_id: "req1".into(), success: true, error: None },
    );
    assert!(dispatched);

    // Should be removed after dispatch
    let dispatched2 = mgr.dispatch_pending_response(
        "req1",
        AgentMessage::FileOpResult { msg_id: "req1".into(), success: true, error: None },
    );
    assert!(!dispatched2);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p serverbee-server test_pending_request_lifecycle`
Expected: FAIL — methods not found

- [ ] **Step 3: Add pending_requests field and imports**

Add `use tokio::sync::oneshot;` to imports (line 5).

Add field to `AgentManager` struct (after line 25):
```rust
    /// Maps msg_id -> oneshot sender for HTTP→WS request-response relay
    pending_requests: DashMap<String, (oneshot::Sender<AgentMessage>, std::time::Instant)>,
```

Update `new()` (line 51):
```rust
            pending_requests: DashMap::new(),
```

- [ ] **Step 4: Add relay methods**

After line 247 (`broadcast_browser`), add:
```rust
    /// Register a pending request and return a receiver to await the agent's response.
    pub fn register_pending_request(&self, msg_id: String) -> oneshot::Receiver<AgentMessage> {
        let (tx, rx) = oneshot::channel();
        self.pending_requests.insert(msg_id, (tx, std::time::Instant::now()));
        rx
    }

    /// Dispatch an agent response to a waiting HTTP handler. Returns true if matched.
    pub fn dispatch_pending_response(&self, msg_id: &str, message: AgentMessage) -> bool {
        if let Some((_, (tx, _))) = self.pending_requests.remove(msg_id) {
            let _ = tx.send(message);
            true
        } else {
            false
        }
    }

    /// Remove pending requests older than max_age.
    pub fn cleanup_expired_requests(&self, max_age: std::time::Duration) {
        let now = std::time::Instant::now();
        self.pending_requests.retain(|_, (_, created_at)| {
            now.duration_since(*created_at) < max_age
        });
    }
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p serverbee-server -- agent_manager`
Expected: ALL PASS

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/service/agent_manager.rs
git commit -m "feat(server): add Request-Response Relay to AgentManager"
```

---

### Task 8: Add RequestTimeout to AppError

**Files:**
- Modify: `crates/server/src/error.rs:33-51,53-64`

- [ ] **Step 1: Add RequestTimeout variant**

After line 48 (`Validation`), add:
```rust
    #[error("Request timeout: {0}")]
    RequestTimeout(String),
```

- [ ] **Step 2: Add match arm in IntoResponse**

In the match block (before `Internal`), add:
```rust
            AppError::RequestTimeout(_) => (StatusCode::REQUEST_TIMEOUT, "REQUEST_TIMEOUT"),
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/error.rs
git commit -m "feat(server): add RequestTimeout error variant"
```

---

### Task 9: Create FileTransferManager service

**Files:**
- Create: `crates/server/src/service/file_transfer.rs`
- Modify: `crates/server/src/service/mod.rs` (add `pub mod file_transfer;`)

- [ ] **Step 1: Create file_transfer.rs**

Implement `FileTransferManager`:
```rust
pub struct FileTransferManager {
    transfers: DashMap<String, TransferMeta>,
    temp_dir: PathBuf,
    max_total_size: u64,
}

pub struct TransferMeta {
    pub transfer_id: String,
    pub server_id: i32,
    pub user_id: i32,
    pub direction: TransferDirection,
    pub file_path: String,
    pub file_size: Option<u64>,
    pub bytes_transferred: u64,
    pub temp_file: PathBuf,
    pub status: TransferStatus,
    pub created_at: Instant,
    pub last_activity: Instant,
}

pub enum TransferDirection { Download, Upload }
pub enum TransferStatus { Pending, InProgress, Ready, Failed(String) }
```

Methods:
- `new(temp_dir: PathBuf) -> Self`
- `create_download(server_id, user_id, file_path) -> Result<String>` — create transfer_id, status=Pending
- `create_upload(server_id, user_id, file_path, size) -> Result<String>` — similar
- `get(&self, transfer_id: &str) -> Option<TransferMeta>` — read-only view
- `update_status(transfer_id, status)`
- `update_progress(transfer_id, bytes)`
- `mark_ready(transfer_id)`
- `mark_failed(transfer_id, error)`
- `remove(transfer_id)` — remove meta and delete temp file
- `list_active() -> Vec<TransferMeta>`
- `cleanup_expired(max_age: Duration)` — remove transfers older than max_age
- `server_transfer_count(server_id) -> usize` — for concurrency limit check
- `temp_file_path(transfer_id) -> PathBuf` — returns `{temp_dir}/{transfer_id}.part`
- `total_temp_size() -> u64` — sum of all temp file sizes

- [ ] **Step 2: Add to service/mod.rs**

Add `pub mod file_transfer;` to `crates/server/src/service/mod.rs`.

- [ ] **Step 3: Add unit tests**

```rust
#[cfg(test)]
mod tests {
    // test_create_and_get — create download, verify fields
    // test_concurrent_limit — create 3, verify 4th returns error
    // test_cleanup_expired — create old transfer, cleanup removes it
    // test_mark_ready — create → mark_ready → verify status
}
```

- [ ] **Step 4: Verify compilation and tests**

Run: `cargo test -p serverbee-server -- file_transfer`
Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/file_transfer.rs crates/server/src/service/mod.rs
git commit -m "feat(server): add FileTransferManager service"
```

---

### Task 10: Add FileTransferManager to AppState

**Files:**
- Modify: `crates/server/src/state.rs`

- [ ] **Step 1: Add field to AppState**

Add `pub file_transfers: Arc<FileTransferManager>` field and initialize it in `AppState::new()` or equivalent constructor. Create temp_dir at `{data_dir}/transfers/` or `/tmp/serverbee-transfers/`.

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/state.rs
git commit -m "feat(server): add FileTransferManager to AppState"
```

---

### Task 11: Dispatch file messages in Agent WS handler

**Files:**
- Modify: `crates/server/src/router/ws/agent.rs`

- [ ] **Step 1: Add file response dispatch logic**

In `handle_agent_message` (the function that matches `AgentMessage` variants), add match arms for all file-related AgentMessage variants. For control messages (`FileListResult`, `FileStatResult`, `FileReadResult`, `FileOpResult`), extract `msg_id` and call `state.agent_manager.dispatch_pending_response(msg_id, message)`.

For transfer messages (`FileDownloadReady`, `FileDownloadChunk`, `FileDownloadEnd`, `FileDownloadError`, `FileUploadAck`, `FileUploadComplete`, `FileUploadError`), update `FileTransferManager` state accordingly:
- `FileDownloadReady` → update file_size, status=InProgress, create temp file
- `FileDownloadChunk` → write chunk to temp file, update progress
- `FileDownloadEnd` → mark_ready
- `FileDownloadError` → mark_failed
- `FileUploadAck` → update progress
- `FileUploadComplete` → mark_ready, cleanup temp
- `FileUploadError` → mark_failed

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/router/ws/agent.rs
git commit -m "feat(server): dispatch file protocol messages in agent WS handler"
```

---

## Chunk 4: Server Side — API & Background Tasks

### Task 12: Create file API router

**Files:**
- Create: `crates/server/src/router/api/file.rs`
- Modify: `crates/server/src/router/api/mod.rs:1-14,29-57`

- [ ] **Step 1: Create file.rs with control endpoints**

Implement endpoints using the Request-Response Relay pattern:

```rust
// All endpoints: check server online, check CAP_FILE, use relay pattern

/// POST /api/files/{server_id}/list
async fn list_files(state, server_id, body: { path }) -> Result<Vec<FileEntry>>
    // register_pending_request(msg_id)
    // send_to_agent(FileList { msg_id, path })
    // tokio::time::timeout(30s, rx.await)
    // return entries

/// POST /api/files/{server_id}/stat
async fn stat_file(state, server_id, body: { path }) -> Result<FileEntry>

/// POST /api/files/{server_id}/read
async fn read_file(state, server_id, body: { path }) -> Result<{ content }>

/// POST /api/files/{server_id}/write
async fn write_file(state, server_id, body: { path, content }) -> Result<{ success }>

/// POST /api/files/{server_id}/delete
async fn delete_file(state, server_id, body: { path, recursive }) -> Result<{ success }>

/// POST /api/files/{server_id}/mkdir
async fn mkdir(state, server_id, body: { path }) -> Result<{ success }>

/// POST /api/files/{server_id}/move
async fn move_file(state, server_id, body: { from, to }) -> Result<{ success }>
```

All control endpoints should:
1. Validate auth (via middleware already)
2. Check `has_capability(server.capabilities, CAP_FILE)` → 403 if disabled
3. Check `agent_manager.is_online(server_id)` → 404 if offline
4. Use relay: `register_pending_request` → `send_to_agent` → `timeout(30s, rx)` → return or error
5. Record audit log on mutation operations using `AuditService::log()` with structured JSON detail:
   ```rust
   // For write/delete/mkdir/move:
   AuditService::log(&db, user_id, "file_delete", &serde_json::json!({
       "server_id": server_id,
       "path": path,
   }).to_string(), ip).await?;
   ```
   Actions: `file_download`, `file_upload`, `file_delete`, `file_edit`, `file_mkdir`, `file_move`

- [ ] **Step 2: Add transfer endpoints**

```rust
/// POST /api/files/{server_id}/download — initiate download, return transfer_id
async fn start_download(state, server_id, body: { path }) -> Result<{ transfer_id, status }>
    // CHECK: state.file_transfers.server_transfer_count(server_id) >= MAX_FILE_CONCURRENT_TRANSFERS
    //   → return AppError::TooManyRequests("Too many concurrent transfers")
    // create transfer in FileTransferManager
    // send FileDownloadStart to agent via WS
    // audit log: "file_download"
    // return immediately with transfer_id + "pending"

/// GET /api/files/download/{transfer_id} — download the ready file
async fn download_file(state, transfer_id) -> Result<StreamBody>
    // check transfer exists and status=Ready
    // stream temp file with Content-Disposition: attachment
    // support Range header for resume

/// POST /api/files/{server_id}/upload — receive file and relay to agent
async fn upload_file(state, server_id, multipart) -> Result<{ success }>
    // CHECK: concurrent transfer limit (same as download)
    // CHECK: Content-Length vs configured max_file_size
    // receive multipart file to temp dir (streaming, no full memory buffer)
    // create transfer, send FileUploadStart to agent
    // read temp file in 384KB chunks, send FileUploadChunk, wait for Ack
    // send FileUploadEnd, wait for FileUploadComplete
    // audit log: "file_upload"
    // cleanup temp file

/// GET /api/files/transfers — list active transfers
async fn list_transfers(state) -> Result<Vec<TransferInfo>>

/// DELETE /api/files/transfers/{transfer_id} — cancel transfer
async fn cancel_transfer(state, transfer_id) -> Result<{ success }>
```

- [ ] **Step 3: Add routers**

```rust
pub fn read_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/files/{server_id}/list", post(list_files))
        .route("/files/{server_id}/stat", post(stat_file))
        .route("/files/{server_id}/read", post(read_file))
        .route("/files/download/{transfer_id}", get(download_file))
        .route("/files/transfers", get(list_transfers))
}

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
```

- [ ] **Step 4: Register in api/mod.rs**

Add `pub mod file;` to module declarations (line 1-14).
Add `.merge(file::read_router())` in read routes (line 36).
Add `.merge(file::write_router())` in admin routes (line 49).

- [ ] **Step 5: Add utoipa::path annotations**

Add OpenAPI annotations to all endpoints. Register paths and schemas in `openapi.rs`.

- [ ] **Step 6: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/router/api/file.rs crates/server/src/router/api/mod.rs crates/server/src/openapi.rs
git commit -m "feat(server): add file management API endpoints with transfer support"
```

---

### Task 13: Add cleanup for transfers and pending requests

**Files:**
- Modify: `crates/server/src/task/session_cleaner.rs`
- Modify: `crates/server/src/task/cleanup.rs`

- [ ] **Step 1: Add pending_requests cleanup to session_cleaner**

Note: `session_cleaner` runs every 3600s which is too infrequent for pending requests (30s timeout). Add a separate 60-second cleanup interval within the cleanup task, or call `cleanup_expired_requests` from the existing `offline_checker` task which runs every 10s:

In the cleanup function (or `offline_checker` task), add:
```rust
// Clean expired pending request-response relays (60s max age)
state.agent_manager.cleanup_expired_requests(std::time::Duration::from_secs(60));
```

- [ ] **Step 2: Add transfer file cleanup to cleanup task**

In the cleanup function, add:
```rust
// Clean expired file transfers and their temp files (30 min)
state.file_transfers.cleanup_expired(std::time::Duration::from_secs(1800));
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/task/session_cleaner.rs crates/server/src/task/cleanup.rs
git commit -m "feat(server): add cleanup for pending requests and file transfers"
```

---

## Chunk 5: Frontend

### Task 14: Update frontend capability constants

**Files:**
- Modify: `apps/web/src/lib/capabilities.ts`

- [ ] **Step 1: Add CAP_FILE constant**

Add after existing capability constants:
```typescript
export const CAP_FILE = 64
```

Add entry to capabilities array:
```typescript
{ bit: CAP_FILE, key: 'file', labelKey: 'cap_file' as const, risk: 'high' as const },
```

- [ ] **Step 2: Commit**

```bash
git add apps/web/src/lib/capabilities.ts
git commit -m "feat(web): add CAP_FILE capability constant"
```

---

### Task 15: Add Files button to Server Detail page

**Files:**
- Modify: `apps/web/src/routes/_authed/servers/$id.tsx`

- [ ] **Step 1: Add Files button**

Import `FileText` from lucide-react. Add a Files button next to the existing Terminal button, gated by:
```typescript
const fileEnabled = hasCap(server.capabilities, CAP_FILE)
```

Link to `/files/${id}` route. Only show when server is online and `fileEnabled`.

- [ ] **Step 2: Verify with typecheck**

Run: `cd apps/web && bun run typecheck`
Expected: PASS (route doesn't exist yet, but Link typechecking may be loose)

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/routes/_authed/servers/\$id.tsx
git commit -m "feat(web): add Files button to server detail page"
```

---

### Task 16: Add file API hooks

**Files:**
- Create: `apps/web/src/hooks/use-file-api.ts`

- [ ] **Step 1: Create hooks file**

Use TanStack Query hooks wrapping `api-client.ts` calls:
- `useFileList(serverId, path)` — POST /api/files/{serverId}/list
- `useFileStat(serverId, path)` — POST /api/files/{serverId}/stat
- `useFileRead(serverId, path)` — POST /api/files/{serverId}/read
- `useFileWriteMutation(serverId)` — POST /api/files/{serverId}/write
- `useFileDeleteMutation(serverId)` — POST /api/files/{serverId}/delete
- `useFileMkdirMutation(serverId)` — POST /api/files/{serverId}/mkdir
- `useFileMoveMutation(serverId)` — POST /api/files/{serverId}/move
- `useFileDownloadMutation(serverId)` — POST /api/files/{serverId}/download
- `useFileUpload(serverId)` — POST /api/files/{serverId}/upload (multipart)
- `useFileTransfers()` — GET /api/files/transfers
- `useCancelTransferMutation()` — DELETE /api/files/transfers/{id}

- [ ] **Step 2: Add file-utils.ts**

Create `apps/web/src/lib/file-utils.ts`:
- `extensionToLanguage(ext: string): string` — map file extensions to Monaco language IDs
- `fileIcon(fileType, name): ReactNode` — return appropriate icon based on type
- `isTextFile(name: string): boolean` — check if file can be opened in editor
- `isImageFile(name: string): boolean`

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/hooks/use-file-api.ts apps/web/src/lib/file-utils.ts
git commit -m "feat(web): add file API hooks and file utility functions"
```

---

### Task 17: Install Monaco Editor and create wrapper

**Files:**
- Modify: `apps/web/package.json`
- Create: `apps/web/src/components/file/file-editor.tsx`

- [ ] **Step 1: Install Monaco**

Run: `cd apps/web && bun add @monaco-editor/react monaco-editor`

- [ ] **Step 2: Create file-editor.tsx**

```typescript
// Lazy-loaded Monaco Editor wrapper
// Props: content, language, onSave(content), readOnly
// Features: Ctrl+S triggers onSave, auto language detection
// Code-split via React.lazy in the parent
```

Configure Monaco with:
- Theme: match app theme (light/dark)
- minimap enabled
- word wrap on
- font size 14
- tab size 2

- [ ] **Step 3: Verify build**

Run: `cd apps/web && bun run build`
Expected: PASS — Monaco in separate chunk

- [ ] **Step 4: Commit**

```bash
git add apps/web/package.json bun.lockb apps/web/src/components/file/file-editor.tsx
git commit -m "feat(web): add Monaco Editor wrapper component"
```

---

### Task 18: Create file browser components

**Files:**
- Create: `apps/web/src/components/file/file-browser.tsx`
- Create: `apps/web/src/components/file/file-breadcrumb.tsx`
- Create: `apps/web/src/components/file/file-context-menu.tsx`
- Create: `apps/web/src/components/file/file-preview.tsx`
- Create: `apps/web/src/components/file/file-upload-dialog.tsx`
- Create: `apps/web/src/components/file/transfer-bar.tsx`
- Create: `apps/web/src/components/file/mkdir-dialog.tsx`

- [ ] **Step 1: Create file-breadcrumb.tsx**

Path breadcrumb component: splits path by `/`, each segment is clickable to navigate.

- [ ] **Step 2: Create file-browser.tsx**

DataTable (shadcn/ui) showing files:
- Columns: icon, name, size (formatted), modified (relative time), permissions
- Click directory → navigate (call setPath)
- Click file → if text & <5MB, call onFileSelect to open in editor; if large, show info
- Sort: directories first, then alphabetical

- [ ] **Step 3: Create file-preview.tsx**

Preview component that switches based on file type:
- Text files: renders lazy-loaded Monaco Editor (via `file-editor.tsx`)
- Image files (png/jpg/gif/svg): renders `<img>` tag with blob URL from download
- Other files: shows file info (name, size, modified) + Download button
- Large files (>5MB): shows info + download button instead of loading content

- [ ] **Step 4: Create file-context-menu.tsx**

Right-click context menu with: Download, Delete, Rename, Copy Path.

- [ ] **Step 5: Create file-upload-dialog.tsx**

Dialog with drag-and-drop zone. On drop/select, triggers upload mutation with progress.

- [ ] **Step 6: Create mkdir-dialog.tsx**

Simple dialog with folder name input.

- [ ] **Step 7: Create transfer-bar.tsx**

Fixed bottom bar showing active transfers with progress bars and cancel buttons. Polls `useFileTransfers()`.

- [ ] **Step 8: Verify typecheck**

Run: `cd apps/web && bun run typecheck`
Expected: PASS

- [ ] **Step 9: Commit**

```bash
git add apps/web/src/components/file/
git commit -m "feat(web): add file browser UI components"
```

---

### Task 19: Create files route page

**Files:**
- Create: `apps/web/src/routes/_authed/files.$serverId.tsx`

- [ ] **Step 1: Create route file**

Main file manager page composing all components:
- State: `currentPath` (default: first root_path or `/`)
- Layout: left panel (FileBrowser), right panel (lazy-loaded FileEditor or preview)
- Top bar: FileBreadcrumb + Upload/NewFolder/Refresh buttons
- Bottom: TransferBar
- Uses `useFileList(serverId, currentPath)` for directory contents
- Uses `useFileRead(serverId, selectedFile)` when file selected for editing
- Tracks `loadedModifiedTime` state — the `modified` timestamp from when file was loaded
- Ctrl+S in editor triggers save conflict detection:
  1. Call `useFileStat` to get current `modified` timestamp
  2. If `modified !== loadedModifiedTime`, show confirm dialog: "File modified externally, overwrite?"
  3. If confirmed (or unchanged), call `useFileWriteMutation`
  4. Update `loadedModifiedTime` after successful save
- Note: reuse existing `formatBytes` from `@/lib/utils` for file sizes (don't create `formatFileSize`)

- [ ] **Step 2: Update sidebar navigation**

In `apps/web/src/components/layout/sidebar.tsx`, the Files route is accessed per-server (from detail page), so it may not need a sidebar entry. If needed, add a "Files" link only when a server is selected.

- [ ] **Step 3: Verify build**

Run: `cd apps/web && bun run build`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/routes/_authed/files.\$serverId.tsx apps/web/src/components/layout/sidebar.tsx
git commit -m "feat(web): add file manager route page with editor integration"
```

---

### Task 20: Add i18n keys and capability label

**Files:**
- Modify: i18n translation files for `file` namespace (~40 keys each for zh/en)
- Modify: settings/capabilities page to include CAP_FILE toggle

- [ ] **Step 1: Add translation keys**

Add `file` namespace translations (both languages):
- `title`, `breadcrumb_root`, `upload`, `new_folder`, `refresh`, `download`, `delete`, `rename`, `move`, `copy_path`, `edit`, `save`, `save_conflict_title`, `save_conflict_message`, `empty_directory`, `file_too_large`, `transfer_in_progress`, `transfer_complete`, `transfer_failed`, `cancel`, `confirm_delete`, `folder_name`, `cap_file` (capability label), etc.

- [ ] **Step 2: Verify build**

Run: `cd apps/web && bun run build`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/i18n/
git commit -m "feat(web): add file management i18n translations"
```

---

## Chunk 6: Testing & Documentation

### Task 21: Add integration tests

**Files:**
- Modify: `crates/server/tests/integration.rs` (or new test file)

- [ ] **Step 1: Add file browsing integration test**

Test: register agent → API call list files → verify response format.

- [ ] **Step 2: Add capability enforcement test**

Test: set CAP_FILE=0 → API call returns 403.

- [ ] **Step 3: Run all tests**

Run: `cargo test --workspace`
Expected: ALL PASS

- [ ] **Step 4: Commit**

```bash
git add crates/server/tests/
git commit -m "test: add file management integration tests"
```

---

### Task 22: Add frontend tests

**Files:**
- Create: `apps/web/src/lib/file-utils.test.ts`
- Create: `apps/web/src/hooks/use-file-api.test.ts`

- [ ] **Step 1: Test file-utils**

```typescript
// extensionToLanguage: yaml→yaml, json→json, ts→typescript, sh→shell, etc.
// isTextFile: config.yaml→true, image.png→false
// isImageFile: photo.jpg→true, data.csv→false
```

- [ ] **Step 2: Test use-file-api hooks**

Test hook initialization and query key structure.

- [ ] **Step 3: Run tests**

Run: `cd apps/web && bun run test`
Expected: ALL PASS

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/lib/file-utils.test.ts apps/web/src/hooks/use-file-api.test.ts
git commit -m "test: add frontend file management tests"
```

---

### Task 23: Update documentation

**Files:**
- Modify: `TESTING.md` — update test counts
- Modify: `ENV.md` — add `SERVERBEE_FILE__*` environment variables
- Modify: `apps/docs/content/docs/cn/configuration.mdx` — add file config section
- Modify: `apps/docs/content/docs/en/configuration.mdx` — same in English
- Modify: `docs/superpowers/plans/PROGRESS.md` — add file management entry

- [ ] **Step 1: Update all docs**

- TESTING.md: update test counts, add file management manual verification checklist
- ENV.md: add `SERVERBEE_FILE__ENABLED`, `SERVERBEE_FILE__ROOT_PATHS`, `SERVERBEE_FILE__MAX_FILE_SIZE`, `SERVERBEE_FILE__DENY_PATTERNS`
- Fumadocs: add file management section to configuration pages
- PROGRESS.md: add file management plan entry

- [ ] **Step 2: Commit**

```bash
git add TESTING.md ENV.md apps/docs/ docs/superpowers/plans/PROGRESS.md
git commit -m "docs: update documentation for file management feature"
```

---

### Task 24: Final verification

- [ ] **Step 1: Run full Rust test suite**

Run: `cargo test --workspace`
Expected: ALL PASS

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: 0 warnings

- [ ] **Step 3: Run frontend checks**

Run: `cd apps/web && bun run typecheck && bun run test && bun x ultracite check && bun run build`
Expected: ALL PASS

- [ ] **Step 4: Final commit if any fixes needed**

```bash
git add -A
git commit -m "chore: final polish for file management feature"
```
