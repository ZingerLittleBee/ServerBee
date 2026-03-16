use std::path::PathBuf;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use serde::Serialize;
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
    temp_dir: PathBuf,
}

impl FileTransferManager {
    pub fn new(temp_dir: PathBuf) -> Self {
        // Create temp dir if it doesn't exist
        std::fs::create_dir_all(&temp_dir).ok();
        Self {
            transfers: DashMap::new(),
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

    pub fn remove(&self, transfer_id: &str) {
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
                    && matches!(meta.status, TransferStatus::Pending | TransferStatus::InProgress)
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
            .create_transfer("srv1".into(), "usr1".into(), TransferDirection::Download, "/tmp/file.txt".into())
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
        let result = mgr.create_transfer("srv42".into(), "usr1".into(), TransferDirection::Upload, "/tmp/extra.txt".into());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Too many concurrent transfers"));

        // Different server should still work
        let id = mgr
            .create_transfer("srv99".into(), "usr1".into(), TransferDirection::Download, "/tmp/other.txt".into())
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
            .create_transfer("srv1".into(), "usr1".into(), TransferDirection::Download, "/test".into())
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
            .create_transfer("srv1".into(), "usr1".into(), TransferDirection::Upload, "/test".into())
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
            .create_transfer("srv1".into(), "usr1".into(), TransferDirection::Download, "/test".into())
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
            .create_transfer("srv1".into(), "usr1".into(), TransferDirection::Download, "/test".into())
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
            .create_transfer("srv1".into(), "usr1".into(), TransferDirection::Download, "/test".into())
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
            .create_transfer("srv1".into(), "usr1".into(), TransferDirection::Download, "/a".into())
            .unwrap();
        let id2 = mgr
            .create_transfer("srv1".into(), "usr1".into(), TransferDirection::Upload, "/b".into())
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
            .create_transfer("srv1".into(), "usr1".into(), TransferDirection::Download, "/tmp/old.txt".into())
            .unwrap();

        // With a very large max_age, nothing should be cleaned
        mgr.cleanup_expired(Duration::from_secs(9999));
        assert!(mgr.get(&id).is_some());

        // With zero max_age, everything should be cleaned
        mgr.cleanup_expired(Duration::ZERO);
        assert!(mgr.get(&id).is_none());
    }
}
