use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use serde::Serialize;
use tokio::sync::Mutex;
use utoipa::ToSchema;

use serverbee_common::constants::MAX_FILE_CONCURRENT_TRANSFERS;

#[derive(Debug, Clone)]
pub enum TransferDirection {
    Download,
    Upload,
}

impl TransferDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            TransferDirection::Download => "download",
            TransferDirection::Upload => "upload",
        }
    }
}

#[derive(Debug, Clone)]
pub enum TransferStatus {
    Pending,
    InProgress,
    Ready,
    Failed(String),
}

impl TransferStatus {
    pub fn as_str(&self) -> &str {
        match self {
            TransferStatus::Pending => "pending",
            TransferStatus::InProgress => "in_progress",
            TransferStatus::Ready => "ready",
            TransferStatus::Failed(_) => "failed",
        }
    }
}

#[derive(Debug)]
pub struct TransferMeta {
    pub transfer_id: String,
    pub server_id: String,
    pub user_id: String,
    pub direction: TransferDirection,
    pub file_path: String,
    pub file_size: Option<u64>,
    pub bytes_transferred: u64,
    pub temp_file: PathBuf,
    pub status: TransferStatus,
    pub created_at: Instant,
    pub last_activity: Instant,
}

/// Serializable transfer info for API responses.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TransferInfo {
    pub transfer_id: String,
    pub server_id: String,
    pub direction: String,
    pub file_path: String,
    pub file_size: Option<u64>,
    pub bytes_transferred: u64,
    pub status: String,
    pub created_at_secs_ago: u64,
}

pub struct FileTransferManager {
    transfers: DashMap<String, TransferMeta>,
    /// Persistent file handles for download transfers, keyed by transfer_id.
    /// Avoids opening/closing the file on every chunk write.
    open_files: DashMap<String, Arc<Mutex<tokio::fs::File>>>,
    temp_dir: PathBuf,
}

impl FileTransferManager {
    pub fn new(temp_dir: PathBuf) -> Self {
        // Create temp dir if it doesn't exist
        std::fs::create_dir_all(&temp_dir).ok();
        Self {
            transfers: DashMap::new(),
            open_files: DashMap::new(),
            temp_dir,
        }
    }

    pub fn temp_dir(&self) -> &std::path::Path {
        &self.temp_dir
    }

    pub fn create_transfer(
        &self,
        server_id: String,
        user_id: String,
        direction: TransferDirection,
        file_path: String,
    ) -> Result<String, String> {
        // Check concurrent limit per server (only count active transfers)
        let count = self.server_transfer_count(&server_id);
        if count >= MAX_FILE_CONCURRENT_TRANSFERS {
            return Err("Too many concurrent transfers".into());
        }
        let transfer_id = uuid::Uuid::new_v4().to_string();
        let temp_file = self.temp_dir.join(format!("{}.part", transfer_id));
        let now = Instant::now();
        self.transfers.insert(
            transfer_id.clone(),
            TransferMeta {
                transfer_id: transfer_id.clone(),
                server_id,
                user_id,
                direction,
                file_path,
                file_size: None,
                bytes_transferred: 0,
                temp_file,
                status: TransferStatus::Pending,
                created_at: now,
                last_activity: now,
            },
        );
        Ok(transfer_id)
    }

    pub fn get(&self, transfer_id: &str) -> Option<TransferInfo> {
        self.transfers.get(transfer_id).map(|meta| TransferInfo {
            transfer_id: meta.transfer_id.clone(),
            server_id: meta.server_id.clone(),
            direction: meta.direction.as_str().to_string(),
            file_path: meta.file_path.clone(),
            file_size: meta.file_size,
            bytes_transferred: meta.bytes_transferred,
            status: meta.status.as_str().to_string(),
            created_at_secs_ago: meta.created_at.elapsed().as_secs(),
        })
    }

    pub fn update_size(&self, transfer_id: &str, size: u64) {
        if let Some(mut meta) = self.transfers.get_mut(transfer_id) {
            meta.file_size = Some(size);
            meta.last_activity = Instant::now();
        }
    }

    pub fn update_progress(&self, transfer_id: &str, bytes: u64) {
        if let Some(mut meta) = self.transfers.get_mut(transfer_id) {
            meta.bytes_transferred = bytes;
            meta.last_activity = Instant::now();
        }
    }

    pub fn mark_in_progress(&self, transfer_id: &str) {
        if let Some(mut meta) = self.transfers.get_mut(transfer_id) {
            meta.status = TransferStatus::InProgress;
            meta.last_activity = Instant::now();
        }
    }

    pub fn mark_ready(&self, transfer_id: &str) {
        if let Some(mut meta) = self.transfers.get_mut(transfer_id) {
            meta.status = TransferStatus::Ready;
            meta.last_activity = Instant::now();
        }
    }

    pub fn mark_failed(&self, transfer_id: &str, error: String) {
        if let Some(mut meta) = self.transfers.get_mut(transfer_id) {
            meta.status = TransferStatus::Failed(error);
            meta.last_activity = Instant::now();
        }
    }

    /// Store an open file handle for a download transfer.
    pub fn store_file_handle(&self, transfer_id: &str, file: tokio::fs::File) {
        self.open_files
            .insert(transfer_id.to_string(), Arc::new(Mutex::new(file)));
    }

    /// Get a cloned `Arc<Mutex<File>>` handle for the given transfer.
    pub fn get_file_handle(&self, transfer_id: &str) -> Option<Arc<Mutex<tokio::fs::File>>> {
        self.open_files
            .get(transfer_id)
            .map(|entry| entry.value().clone())
    }

    /// Remove and drop the file handle for a transfer (closes the file).
    pub fn remove_file_handle(&self, transfer_id: &str) {
        self.open_files.remove(transfer_id);
    }

    pub fn remove(&self, transfer_id: &str) {
        // Remove file handle first (closes the file before we delete it)
        self.open_files.remove(transfer_id);
        if let Some((_, meta)) = self.transfers.remove(transfer_id) {
            // Delete temp file if it exists
            let _ = std::fs::remove_file(&meta.temp_file);
        }
    }

    pub fn server_transfer_count(&self, server_id: &str) -> usize {
        self.transfers
            .iter()
            .filter(|entry| {
                let meta = entry.value();
                meta.server_id == server_id
                    && matches!(
                        meta.status,
                        TransferStatus::Pending | TransferStatus::InProgress
                    )
            })
            .count()
    }

    pub fn get_user_id(&self, transfer_id: &str) -> Option<String> {
        self.transfers
            .get(transfer_id)
            .map(|meta| meta.user_id.clone())
    }

    pub fn temp_file_path(&self, transfer_id: &str) -> Option<PathBuf> {
        self.transfers
            .get(transfer_id)
            .map(|meta| meta.temp_file.clone())
    }

    pub fn list_for_user(&self, user_id: &str) -> Vec<TransferInfo> {
        self.transfers
            .iter()
            .filter(|entry| entry.value().user_id == user_id)
            .map(|entry| {
                let meta = entry.value();
                TransferInfo {
                    transfer_id: meta.transfer_id.clone(),
                    server_id: meta.server_id.clone(),
                    direction: meta.direction.as_str().to_string(),
                    file_path: meta.file_path.clone(),
                    file_size: meta.file_size,
                    bytes_transferred: meta.bytes_transferred,
                    status: meta.status.as_str().to_string(),
                    created_at_secs_ago: meta.created_at.elapsed().as_secs(),
                }
            })
            .collect()
    }

    pub fn list_active(&self) -> Vec<TransferInfo> {
        self.transfers
            .iter()
            .map(|entry| {
                let meta = entry.value();
                TransferInfo {
                    transfer_id: meta.transfer_id.clone(),
                    server_id: meta.server_id.clone(),
                    direction: meta.direction.as_str().to_string(),
                    file_path: meta.file_path.clone(),
                    file_size: meta.file_size,
                    bytes_transferred: meta.bytes_transferred,
                    status: meta.status.as_str().to_string(),
                    created_at_secs_ago: meta.created_at.elapsed().as_secs(),
                }
            })
            .collect()
    }

    pub fn cleanup_expired(&self, max_age: Duration) {
        let now = Instant::now();
        let expired: Vec<String> = self
            .transfers
            .iter()
            .filter(|entry| now.duration_since(entry.value().last_activity) >= max_age)
            .map(|entry| entry.key().clone())
            .collect();
        for id in expired {
            self.remove(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manager() -> FileTransferManager {
        let dir = std::env::temp_dir().join("serverbee-test-transfers");
        FileTransferManager::new(dir)
    }

    #[test]
    fn test_create_and_get() {
        let mgr = make_manager();
        let id = mgr
            .create_transfer(
                "srv1".into(),
                "usr1".into(),
                TransferDirection::Download,
                "/tmp/file.txt".into(),
            )
            .unwrap();
        let info = mgr.get(&id).unwrap();
        assert_eq!(info.server_id, "srv1");
        assert_eq!(info.direction, "download");
        assert_eq!(info.file_path, "/tmp/file.txt");
        assert_eq!(info.status, "pending");
        assert_eq!(info.bytes_transferred, 0);

        // Cleanup
        mgr.remove(&id);
    }

    #[test]
    fn test_concurrent_limit() {
        let mgr = make_manager();
        let mut ids = Vec::new();
        for i in 0..MAX_FILE_CONCURRENT_TRANSFERS {
            let id = mgr
                .create_transfer(
                    "srv42".into(),
                    "usr1".into(),
                    TransferDirection::Download,
                    format!("/tmp/file{i}.txt"),
                )
                .unwrap();
            ids.push(id);
        }

        // 4th transfer for same server should fail
        let result = mgr.create_transfer(
            "srv42".into(),
            "usr1".into(),
            TransferDirection::Upload,
            "/tmp/extra.txt".into(),
        );
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("Too many concurrent transfers")
        );

        // Different server should still work
        let id = mgr
            .create_transfer(
                "srv99".into(),
                "usr1".into(),
                TransferDirection::Download,
                "/tmp/other.txt".into(),
            )
            .unwrap();
        ids.push(id);

        // Cleanup
        for id in ids {
            mgr.remove(&id);
        }
    }

    #[test]
    fn test_mark_status_transitions() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = FileTransferManager::new(tmp.path().to_path_buf());
        let id = mgr
            .create_transfer(
                "srv1".into(),
                "usr1".into(),
                TransferDirection::Download,
                "/test".into(),
            )
            .unwrap();

        // Pending -> InProgress
        mgr.mark_in_progress(&id);
        let info = mgr.get(&id).unwrap();
        assert_eq!(info.status, "in_progress");

        // InProgress -> Ready
        mgr.mark_ready(&id);
        let info = mgr.get(&id).unwrap();
        assert_eq!(info.status, "ready");

        mgr.remove(&id);
    }

    #[test]
    fn test_mark_failed() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = FileTransferManager::new(tmp.path().to_path_buf());
        let id = mgr
            .create_transfer(
                "srv1".into(),
                "usr1".into(),
                TransferDirection::Upload,
                "/test".into(),
            )
            .unwrap();

        mgr.mark_failed(&id, "Disk full".into());
        let info = mgr.get(&id).unwrap();
        assert_eq!(info.status, "failed");

        mgr.remove(&id);
    }

    #[test]
    fn test_update_progress() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = FileTransferManager::new(tmp.path().to_path_buf());
        let id = mgr
            .create_transfer(
                "srv1".into(),
                "usr1".into(),
                TransferDirection::Download,
                "/test".into(),
            )
            .unwrap();

        mgr.update_size(&id, 1000);
        mgr.update_progress(&id, 500);
        let info = mgr.get(&id).unwrap();
        assert_eq!(info.bytes_transferred, 500);
        assert_eq!(info.file_size.unwrap(), 1000);

        mgr.remove(&id);
    }

    #[test]
    fn test_temp_file_path() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = FileTransferManager::new(tmp.path().to_path_buf());
        let id = mgr
            .create_transfer(
                "srv1".into(),
                "usr1".into(),
                TransferDirection::Download,
                "/test".into(),
            )
            .unwrap();

        let path = mgr.temp_file_path(&id).unwrap();
        assert!(path.to_str().unwrap().contains(&id));
        assert!(path.to_str().unwrap().ends_with(".part"));

        mgr.remove(&id);
    }

    #[test]
    fn test_remove_cleans_temp_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = FileTransferManager::new(tmp.path().to_path_buf());
        let id = mgr
            .create_transfer(
                "srv1".into(),
                "usr1".into(),
                TransferDirection::Download,
                "/test".into(),
            )
            .unwrap();

        // Create the temp file
        let path = mgr.temp_file_path(&id).unwrap();
        std::fs::write(&path, "data").unwrap();
        assert!(path.exists());

        mgr.remove(&id);
        assert!(!path.exists());
        assert!(mgr.get(&id).is_none());
    }

    #[test]
    fn test_list_active_returns_all() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = FileTransferManager::new(tmp.path().to_path_buf());
        let id1 = mgr
            .create_transfer(
                "srv1".into(),
                "usr1".into(),
                TransferDirection::Download,
                "/a".into(),
            )
            .unwrap();
        let id2 = mgr
            .create_transfer(
                "srv1".into(),
                "usr1".into(),
                TransferDirection::Upload,
                "/b".into(),
            )
            .unwrap();

        mgr.mark_ready(&id1);

        let active = mgr.list_active();
        // Both should appear (list_active shows all, not just pending)
        assert_eq!(active.len(), 2);

        mgr.remove(&id1);
        mgr.remove(&id2);
    }

    #[test]
    fn test_cleanup_expired() {
        let mgr = make_manager();
        let id = mgr
            .create_transfer(
                "srv1".into(),
                "usr1".into(),
                TransferDirection::Download,
                "/tmp/old.txt".into(),
            )
            .unwrap();

        // With a very large max_age, nothing should be cleaned
        mgr.cleanup_expired(Duration::from_secs(9999));
        assert!(mgr.get(&id).is_some());

        // With zero max_age, everything should be cleaned
        mgr.cleanup_expired(Duration::ZERO);
        assert!(mgr.get(&id).is_none());
    }

    #[test]
    fn test_transfer_direction_as_str() {
        // Both direction variants map to their lowercase string form
        assert_eq!(TransferDirection::Download.as_str(), "download");
        assert_eq!(TransferDirection::Upload.as_str(), "upload");
    }

    #[test]
    fn test_transfer_status_as_str() {
        // Every status variant maps to its stable wire string
        assert_eq!(TransferStatus::Pending.as_str(), "pending");
        assert_eq!(TransferStatus::InProgress.as_str(), "in_progress");
        assert_eq!(TransferStatus::Ready.as_str(), "ready");
        assert_eq!(
            TransferStatus::Failed("boom".into()).as_str(),
            "failed"
        );
    }

    #[test]
    fn test_temp_dir_accessor() {
        // temp_dir() returns the path the manager was constructed with
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = FileTransferManager::new(tmp.path().to_path_buf());
        assert_eq!(mgr.temp_dir(), tmp.path());
    }

    #[test]
    fn test_new_creates_temp_dir() {
        // new() should create the temp dir on disk if it doesn't exist yet
        let base = tempfile::TempDir::new().unwrap();
        let nested = base.path().join("nested").join("transfers");
        assert!(!nested.exists());
        let mgr = FileTransferManager::new(nested.clone());
        assert!(nested.exists());
        assert_eq!(mgr.temp_dir(), nested.as_path());
    }

    #[test]
    fn test_get_missing_returns_none() {
        // get() on an unknown transfer id yields None
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = FileTransferManager::new(tmp.path().to_path_buf());
        assert!(mgr.get("does-not-exist").is_none());
    }

    #[test]
    fn test_update_size_missing_is_noop() {
        // update_size on a missing transfer must not panic and changes nothing
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = FileTransferManager::new(tmp.path().to_path_buf());
        mgr.update_size("missing", 123);
        assert!(mgr.get("missing").is_none());
    }

    #[test]
    fn test_update_progress_missing_is_noop() {
        // update_progress on a missing transfer is a silent no-op
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = FileTransferManager::new(tmp.path().to_path_buf());
        mgr.update_progress("missing", 456);
        assert!(mgr.get("missing").is_none());
    }

    #[test]
    fn test_mark_helpers_missing_are_noops() {
        // mark_* helpers on a missing transfer must not panic
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = FileTransferManager::new(tmp.path().to_path_buf());
        mgr.mark_in_progress("missing");
        mgr.mark_ready("missing");
        mgr.mark_failed("missing", "err".into());
        assert!(mgr.get("missing").is_none());
    }

    #[test]
    fn test_get_user_id() {
        // get_user_id returns the owning user, and None for unknown ids
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = FileTransferManager::new(tmp.path().to_path_buf());
        let id = mgr
            .create_transfer(
                "srv1".into(),
                "alice".into(),
                TransferDirection::Download,
                "/x".into(),
            )
            .unwrap();
        assert_eq!(mgr.get_user_id(&id).unwrap(), "alice");
        assert!(mgr.get_user_id("missing").is_none());
        mgr.remove(&id);
    }

    #[test]
    fn test_temp_file_path_missing_returns_none() {
        // temp_file_path is None when the transfer doesn't exist
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = FileTransferManager::new(tmp.path().to_path_buf());
        assert!(mgr.temp_file_path("missing").is_none());
    }

    #[test]
    fn test_list_for_user_filters_by_owner() {
        // list_for_user only returns transfers belonging to the given user
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = FileTransferManager::new(tmp.path().to_path_buf());
        let a1 = mgr
            .create_transfer(
                "srv1".into(),
                "alice".into(),
                TransferDirection::Download,
                "/a1".into(),
            )
            .unwrap();
        let a2 = mgr
            .create_transfer(
                "srv2".into(),
                "alice".into(),
                TransferDirection::Upload,
                "/a2".into(),
            )
            .unwrap();
        let b1 = mgr
            .create_transfer(
                "srv1".into(),
                "bob".into(),
                TransferDirection::Download,
                "/b1".into(),
            )
            .unwrap();

        let alice = mgr.list_for_user("alice");
        assert_eq!(alice.len(), 2);
        assert!(alice.iter().all(|t| t.transfer_id == a1 || t.transfer_id == a2));

        let bob = mgr.list_for_user("bob");
        assert_eq!(bob.len(), 1);
        assert_eq!(bob[0].transfer_id, b1);

        // A user with no transfers gets an empty list
        assert!(mgr.list_for_user("nobody").is_empty());

        mgr.remove(&a1);
        mgr.remove(&a2);
        mgr.remove(&b1);
    }

    #[test]
    fn test_server_transfer_count_excludes_terminal_states() {
        // Only Pending/InProgress count toward the per-server concurrency limit
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = FileTransferManager::new(tmp.path().to_path_buf());
        let pending = mgr
            .create_transfer(
                "srv".into(),
                "u".into(),
                TransferDirection::Download,
                "/p".into(),
            )
            .unwrap();
        let in_progress = mgr
            .create_transfer(
                "srv".into(),
                "u".into(),
                TransferDirection::Download,
                "/i".into(),
            )
            .unwrap();
        let ready = mgr
            .create_transfer(
                "srv".into(),
                "u".into(),
                TransferDirection::Download,
                "/r".into(),
            )
            .unwrap();
        mgr.mark_in_progress(&in_progress);
        mgr.mark_ready(&ready);

        // Pending + InProgress are counted; Ready is not
        assert_eq!(mgr.server_transfer_count("srv"), 2);
        // Unknown server has zero active transfers
        assert_eq!(mgr.server_transfer_count("other"), 0);

        mgr.remove(&pending);
        mgr.remove(&in_progress);
        mgr.remove(&ready);
    }

    #[test]
    fn test_ready_transfers_do_not_block_new_ones() {
        // Marking a transfer Ready frees a concurrency slot for the same server
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = FileTransferManager::new(tmp.path().to_path_buf());
        let mut ids = Vec::new();
        for i in 0..MAX_FILE_CONCURRENT_TRANSFERS {
            ids.push(
                mgr.create_transfer(
                    "srv".into(),
                    "u".into(),
                    TransferDirection::Download,
                    format!("/f{i}"),
                )
                .unwrap(),
            );
        }
        // At the limit, a new one is rejected
        assert!(
            mgr.create_transfer(
                "srv".into(),
                "u".into(),
                TransferDirection::Upload,
                "/blocked".into(),
            )
            .is_err()
        );

        // Completing one (Ready) drops the active count below the limit
        mgr.mark_ready(&ids[0]);
        let extra = mgr
            .create_transfer(
                "srv".into(),
                "u".into(),
                TransferDirection::Upload,
                "/now-ok".into(),
            )
            .unwrap();
        ids.push(extra);

        for id in ids {
            mgr.remove(&id);
        }
    }

    #[tokio::test]
    async fn test_file_handle_lifecycle() {
        // store/get/remove of an open download file handle round-trips correctly
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = FileTransferManager::new(tmp.path().to_path_buf());
        let path = tmp.path().join("handle.part");
        let file = tokio::fs::File::create(&path).await.unwrap();

        // No handle before storing
        assert!(mgr.get_file_handle("t1").is_none());

        mgr.store_file_handle("t1", file);
        let handle = mgr.get_file_handle("t1");
        assert!(handle.is_some());
        // The Arc is cloned, so both refer to the same underlying file
        assert!(Arc::strong_count(&handle.unwrap()) >= 2);

        mgr.remove_file_handle("t1");
        assert!(mgr.get_file_handle("t1").is_none());
    }

    #[tokio::test]
    async fn test_remove_drops_file_handle() {
        // remove() also closes/drops any associated open file handle
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = FileTransferManager::new(tmp.path().to_path_buf());
        let id = mgr
            .create_transfer(
                "srv".into(),
                "u".into(),
                TransferDirection::Download,
                "/x".into(),
            )
            .unwrap();
        let handle_path = tmp.path().join("h.part");
        let file = tokio::fs::File::create(&handle_path).await.unwrap();
        mgr.store_file_handle(&id, file);
        assert!(mgr.get_file_handle(&id).is_some());

        mgr.remove(&id);
        // Handle and transfer are both gone after remove
        assert!(mgr.get_file_handle(&id).is_none());
        assert!(mgr.get(&id).is_none());
    }

    #[test]
    fn test_remove_missing_is_noop() {
        // Removing a non-existent transfer must not panic
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = FileTransferManager::new(tmp.path().to_path_buf());
        mgr.remove("missing");
        assert!(mgr.get("missing").is_none());
    }

    #[test]
    fn test_cleanup_expired_keeps_recently_active() {
        // Updating activity resets the clock so a recently-touched transfer survives cleanup
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = FileTransferManager::new(tmp.path().to_path_buf());
        let id = mgr
            .create_transfer(
                "srv".into(),
                "u".into(),
                TransferDirection::Download,
                "/x".into(),
            )
            .unwrap();
        // Refresh last_activity, then clean only transfers idle for >= 1h
        mgr.update_progress(&id, 10);
        mgr.cleanup_expired(Duration::from_secs(3600));
        assert!(mgr.get(&id).is_some());
        mgr.remove(&id);
    }

    #[test]
    fn test_list_active_empty() {
        // list_active on a fresh manager is empty
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = FileTransferManager::new(tmp.path().to_path_buf());
        assert!(mgr.list_active().is_empty());
    }

    #[test]
    fn test_transfer_info_fields_populated() {
        // get() projects all TransferMeta fields into TransferInfo, including size
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = FileTransferManager::new(tmp.path().to_path_buf());
        let id = mgr
            .create_transfer(
                "srvX".into(),
                "userX".into(),
                TransferDirection::Upload,
                "/data/big.bin".into(),
            )
            .unwrap();
        mgr.update_size(&id, 2048);
        mgr.update_progress(&id, 1024);
        mgr.mark_in_progress(&id);

        let info = mgr.get(&id).unwrap();
        assert_eq!(info.transfer_id, id);
        assert_eq!(info.server_id, "srvX");
        assert_eq!(info.direction, "upload");
        assert_eq!(info.file_path, "/data/big.bin");
        assert_eq!(info.file_size, Some(2048));
        assert_eq!(info.bytes_transferred, 1024);
        assert_eq!(info.status, "in_progress");
        // Newly created, so the elapsed time is essentially zero seconds
        assert!(info.created_at_secs_ago < 5);

        mgr.remove(&id);
    }
}
