use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU32;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use dashmap::DashMap;
use serverbee_common::constants::MAX_FILE_CHUNK_SIZE;
use serverbee_common::protocol::AgentMessage;
use serverbee_common::types::{FileEntry, FileType};
use tokio::sync::mpsc;

use crate::config::FileConfig;

/// Events produced by background file transfer tasks, sent to the reporter loop.
#[allow(clippy::enum_variant_names)]
pub enum FileEvent {
    DownloadReady {
        transfer_id: String,
        size: u64,
    },
    DownloadChunk {
        transfer_id: String,
        offset: u64,
        data: String,
    },
    DownloadEnd {
        transfer_id: String,
    },
    DownloadError {
        transfer_id: String,
        error: String,
    },
}

impl From<FileEvent> for AgentMessage {
    fn from(event: FileEvent) -> Self {
        match event {
            FileEvent::DownloadReady { transfer_id, size } => {
                AgentMessage::FileDownloadReady { transfer_id, size }
            }
            FileEvent::DownloadChunk {
                transfer_id,
                offset,
                data,
            } => AgentMessage::FileDownloadChunk {
                transfer_id,
                offset,
                data,
            },
            FileEvent::DownloadEnd { transfer_id } => AgentMessage::FileDownloadEnd { transfer_id },
            FileEvent::DownloadError { transfer_id, error } => {
                AgentMessage::FileDownloadError { transfer_id, error }
            }
        }
    }
}

/// Tracks state for an active upload transfer.
struct UploadState {
    path: PathBuf,
    tmp_path: PathBuf,
    #[allow(dead_code)]
    size: u64,
}

/// Tracks that a download is active (used for cancellation).
struct DownloadState {
    handle: tokio::task::JoinHandle<()>,
}

/// Manages file operations on the agent, enforcing path validation and deny patterns.
pub struct FileManager {
    config: FileConfig,
    /// Pre-canonicalized root paths, computed once at construction time.
    canonical_roots: Vec<PathBuf>,
    #[allow(dead_code)] // stored for future per-method capability checks
    capabilities: Arc<AtomicU32>,
    active_downloads: DashMap<String, DownloadState>,
    active_uploads: DashMap<String, UploadState>,
}

impl FileManager {
    pub fn new(config: FileConfig, capabilities: Arc<AtomicU32>) -> Self {
        let canonical_roots: Vec<PathBuf> = config
            .root_paths
            .iter()
            .filter_map(|root| std::fs::canonicalize(root).ok())
            .collect();
        Self {
            config,
            canonical_roots,
            capabilities,
            active_downloads: DashMap::new(),
            active_uploads: DashMap::new(),
        }
    }

    /// Check if file management is enabled via both config and capability.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Validate that the given path is within an allowed root and does not match deny patterns.
    pub fn validate_path(&self, path: &str) -> anyhow::Result<PathBuf> {
        if self.config.root_paths.is_empty() {
            anyhow::bail!("No root paths configured");
        }

        let canonical = std::fs::canonicalize(path)
            .map_err(|e| anyhow::anyhow!("Cannot resolve path '{}': {}", path, e))?;

        let within_root = self
            .canonical_roots
            .iter()
            .any(|root_canonical| canonical.starts_with(root_canonical));

        if !within_root {
            anyhow::bail!("Path '{}' is outside allowed root paths", path);
        }

        // Check deny patterns against the filename
        if let Some(filename) = canonical.file_name().and_then(|f| f.to_str()) {
            for pattern in &self.config.deny_patterns {
                if matches_deny_pattern(filename, pattern) {
                    anyhow::bail!("Path '{}' matches deny pattern '{}'", path, pattern);
                }
            }
        }

        Ok(canonical)
    }

    /// List directory entries, sorted with directories first, then alphabetically.
    /// When path is outside root_paths but is an ancestor of one or more root_paths,
    /// returns those root_paths as virtual directory entries for navigation.
    pub async fn list_dir(&self, path: &str) -> anyhow::Result<Vec<FileEntry>> {
        if self.config.root_paths.is_empty() {
            anyhow::bail!("No root paths configured");
        }

        // Try normal validation first
        match self.validate_path(path) {
            Ok(canonical) => {
                // Path is within root_paths, list normally
                return self.list_dir_entries(&canonical).await;
            }
            Err(_) => {
                // Path is outside root_paths. Check if it's an ancestor of any root_path.
                // If so, return those root_paths as virtual directory entries.
                let request_path = if path == "/" {
                    std::path::PathBuf::from("/")
                } else {
                    std::fs::canonicalize(path).unwrap_or_else(|_| std::path::PathBuf::from(path))
                };

                let mut entries = Vec::new();
                for root_canonical in &self.canonical_roots {
                    if root_canonical.starts_with(&request_path) {
                        let name = root_canonical.to_string_lossy().to_string();
                        let metadata = tokio::fs::metadata(root_canonical).await.ok();
                        let modified = metadata
                            .as_ref()
                            .and_then(|m| m.modified().ok())
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or(0);
                        let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
                        let (permissions, owner, group) = metadata
                            .as_ref()
                            .map(|m| get_platform_metadata(m, root_canonical))
                            .unwrap_or_default();
                        entries.push(FileEntry {
                            name,
                            path: root_canonical.to_string_lossy().to_string(),
                            file_type: FileType::Directory,
                            size,
                            modified,
                            permissions,
                            owner,
                            group,
                        });
                    }
                }

                if entries.is_empty() {
                    anyhow::bail!("Path '{}' is outside allowed root paths", path);
                }
                entries.sort_by_key(|a| a.name.to_lowercase());
                Ok(entries)
            }
        }
    }

    /// Internal: list actual directory entries from the filesystem.
    async fn list_dir_entries(
        &self,
        canonical: &std::path::Path,
    ) -> anyhow::Result<Vec<FileEntry>> {
        let mut entries = Vec::new();
        let mut read_dir = tokio::fs::read_dir(&canonical).await?;

        while let Some(entry) = read_dir.next_entry().await? {
            let metadata = entry.metadata().await?;
            let file_type = if metadata.is_dir() {
                FileType::Directory
            } else if metadata.is_symlink() {
                FileType::Symlink
            } else {
                FileType::File
            };

            let name = entry.file_name().to_string_lossy().to_string();
            let entry_path = entry.path().to_string_lossy().to_string();
            let modified = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            let (permissions, owner, group) = get_platform_metadata(&metadata, &entry.path());

            entries.push(FileEntry {
                name,
                path: entry_path,
                file_type,
                size: metadata.len(),
                modified,
                permissions,
                owner,
                group,
            });
        }

        // Sort: directories first, then alphabetical by name
        entries.sort_by(|a, b| {
            let a_is_dir = matches!(a.file_type, FileType::Directory);
            let b_is_dir = matches!(b.file_type, FileType::Directory);
            b_is_dir
                .cmp(&a_is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        Ok(entries)
    }

    /// Get metadata for a single file or directory.
    pub async fn stat(&self, path: &str) -> anyhow::Result<FileEntry> {
        let canonical = self.validate_path(path)?;
        let metadata = tokio::fs::metadata(&canonical).await?;

        let file_type = if metadata.is_dir() {
            FileType::Directory
        } else if metadata.is_symlink() {
            FileType::Symlink
        } else {
            FileType::File
        };

        let name = canonical
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();

        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let (permissions, owner, group) = get_platform_metadata(&metadata, &canonical);

        Ok(FileEntry {
            name,
            path: canonical.to_string_lossy().to_string(),
            file_type,
            size: metadata.len(),
            modified,
            permissions,
            owner,
            group,
        })
    }

    /// Read a file and return its content as base64-encoded string.
    pub async fn read_file(&self, path: &str, max_size: u64) -> anyhow::Result<String> {
        let canonical = self.validate_path(path)?;
        let metadata = tokio::fs::metadata(&canonical).await?;

        if metadata.len() > max_size {
            anyhow::bail!("File size {} exceeds max_size {}", metadata.len(), max_size);
        }

        let content = tokio::fs::read(&canonical).await?;
        Ok(BASE64.encode(&content))
    }

    /// Write base64-encoded content to a file atomically (via .sb-tmp rename).
    pub async fn write_file(&self, path: &str, content: &str) -> anyhow::Result<()> {
        let canonical = self.validate_path(path)?;
        let tmp_path = canonical.with_extension("sb-tmp");

        let decoded = BASE64
            .decode(content)
            .map_err(|e| anyhow::anyhow!("Invalid base64 content: {}", e))?;

        tokio::fs::write(&tmp_path, &decoded).await?;
        tokio::fs::rename(&tmp_path, &canonical).await?;

        Ok(())
    }

    /// Delete a file or directory.
    pub async fn delete(&self, path: &str, recursive: bool) -> anyhow::Result<()> {
        let canonical = self.validate_path(path)?;
        let metadata = tokio::fs::metadata(&canonical).await?;

        if metadata.is_dir() {
            if recursive {
                tokio::fs::remove_dir_all(&canonical).await?;
            } else {
                tokio::fs::remove_dir(&canonical).await?;
            }
        } else {
            tokio::fs::remove_file(&canonical).await?;
        }

        Ok(())
    }

    /// Create a directory and all parent directories.
    pub async fn mkdir(&self, path: &str) -> anyhow::Result<()> {
        // For mkdir, the target path doesn't exist yet, so we find the closest
        // existing ancestor and validate it is within an allowed root.
        let target = PathBuf::from(path);

        let existing_ancestor = find_existing_ancestor(&target)
            .ok_or_else(|| anyhow::anyhow!("Cannot find any existing ancestor for '{}'", path))?;

        let _ancestor_canonical = self.validate_path(
            existing_ancestor
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid ancestor path"))?,
        )?;

        tokio::fs::create_dir_all(&target).await?;
        Ok(())
    }

    /// Rename/move a file or directory. Both paths must be validated.
    pub async fn rename_path(&self, from: &str, to: &str) -> anyhow::Result<()> {
        let from_canonical = self.validate_path(from)?;

        // For the destination, validate the parent directory (the target may not exist yet).
        let to_path = PathBuf::from(to);
        let to_parent = to_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Cannot determine destination parent directory"))?;
        let _to_parent_canonical = self.validate_path(
            to_parent
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid destination parent path"))?,
        )?;

        // Also check that the destination filename doesn't match deny patterns
        if let Some(filename) = to_path.file_name().and_then(|f| f.to_str()) {
            for pattern in &self.config.deny_patterns {
                if matches_deny_pattern(filename, pattern) {
                    anyhow::bail!(
                        "Destination filename '{}' matches deny pattern '{}'",
                        filename,
                        pattern
                    );
                }
            }
        }

        tokio::fs::rename(&from_canonical, &to_path).await?;
        Ok(())
    }

    /// Start a background download task that streams file chunks.
    pub fn start_download(&self, transfer_id: String, path: String, tx: mpsc::Sender<FileEvent>) {
        let validated = match self.validate_path(&path) {
            Ok(p) => p,
            Err(e) => {
                let tid = transfer_id.clone();
                let error = e.to_string();
                tokio::spawn(async move {
                    let _ = tx
                        .send(FileEvent::DownloadError {
                            transfer_id: tid,
                            error,
                        })
                        .await;
                });
                return;
            }
        };

        let tid = transfer_id.clone();
        let handle = tokio::spawn(async move {
            if let Err(e) = download_file(tid.clone(), validated, tx.clone()).await {
                let _ = tx
                    .send(FileEvent::DownloadError {
                        transfer_id: tid,
                        error: e.to_string(),
                    })
                    .await;
            }
        });

        self.active_downloads
            .insert(transfer_id, DownloadState { handle });
    }

    /// Cancel a single download transfer.
    pub fn cancel_download(&self, transfer_id: &str) {
        if let Some((_, state)) = self.active_downloads.remove(transfer_id) {
            state.handle.abort();
            tracing::debug!("Cancelled download {transfer_id}");
        }
    }

    /// Cancel all active transfers (downloads and uploads).
    pub fn cancel_all_transfers(&self) {
        for entry in self.active_downloads.iter() {
            entry.value().handle.abort();
        }
        self.active_downloads.clear();
        self.active_uploads.clear();
    }

    /// Start an upload: create the .sb-part temporary file.
    pub async fn start_upload(
        &self,
        transfer_id: String,
        path: String,
        size: u64,
    ) -> anyhow::Result<()> {
        // For upload, the target file may not exist yet. Validate the parent directory.
        let target = PathBuf::from(&path);
        let parent = target
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Cannot determine parent directory"))?;
        let _parent_canonical = self.validate_path(
            parent
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid parent path"))?,
        )?;

        // Check the target filename against deny patterns
        if let Some(filename) = target.file_name().and_then(|f| f.to_str()) {
            for pattern in &self.config.deny_patterns {
                if matches_deny_pattern(filename, pattern) {
                    anyhow::bail!("Filename '{}' matches deny pattern '{}'", filename, pattern);
                }
            }
        }

        if size > self.config.max_file_size {
            anyhow::bail!(
                "Upload size {} exceeds max_file_size {}",
                size,
                self.config.max_file_size
            );
        }

        let tmp_path = target.with_extension("sb-part");
        // Create empty file
        tokio::fs::write(&tmp_path, b"").await?;

        self.active_uploads.insert(
            transfer_id,
            UploadState {
                path: target,
                tmp_path,
                size,
            },
        );

        Ok(())
    }

    /// Receive a chunk for an active upload. Returns the new written offset.
    pub async fn receive_chunk(
        &self,
        transfer_id: &str,
        offset: u64,
        data: &str,
    ) -> anyhow::Result<u64> {
        let state = self
            .active_uploads
            .get(transfer_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown upload transfer '{}'", transfer_id))?;

        let decoded = BASE64
            .decode(data)
            .map_err(|e| anyhow::anyhow!("Invalid base64 chunk: {}", e))?;

        use tokio::io::{AsyncSeekExt, AsyncWriteExt};
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .open(&state.tmp_path)
            .await?;
        file.seek(std::io::SeekFrom::Start(offset)).await?;
        file.write_all(&decoded).await?;
        file.flush().await?;

        Ok(offset + decoded.len() as u64)
    }

    /// Finalize an upload: rename .sb-part to the target path.
    pub async fn finish_upload(&self, transfer_id: &str) -> anyhow::Result<()> {
        let (_, state) = self
            .active_uploads
            .remove(transfer_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown upload transfer '{}'", transfer_id))?;

        tokio::fs::rename(&state.tmp_path, &state.path).await?;
        Ok(())
    }
}

/// Walk up the directory tree to find the closest existing ancestor.
fn find_existing_ancestor(path: &std::path::Path) -> Option<PathBuf> {
    let mut current = path.to_path_buf();
    loop {
        if current.exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Check if a filename matches a deny pattern.
///
/// Patterns:
/// - `*.ext` — file ends with `.ext`
/// - `prefix*` — file starts with `prefix`
/// - `exact` — exact match
fn matches_deny_pattern(filename: &str, pattern: &str) -> bool {
    if let Some(suffix) = pattern.strip_prefix('*') {
        // *.key -> ends with .key
        filename.ends_with(suffix)
    } else if let Some(prefix) = pattern.strip_suffix('*') {
        // id_rsa* -> starts with id_rsa
        filename.starts_with(prefix)
    } else {
        // exact match
        filename == pattern
    }
}

/// Get platform-specific file metadata (permissions, owner, group).
#[cfg(unix)]
fn get_platform_metadata(
    metadata: &std::fs::Metadata,
    path: &std::path::Path,
) -> (Option<String>, Option<String>, Option<String>) {
    use std::os::unix::fs::PermissionsExt;

    let mode = metadata.permissions().mode();
    let permissions = Some(format_unix_permissions(mode));

    // Get owner/group names via nix-style uid/gid lookup
    use std::os::unix::fs::MetadataExt;
    let uid = metadata.uid();
    let gid = metadata.gid();

    let owner = get_username_by_uid(uid);
    let group = get_groupname_by_gid(gid);

    let _ = path; // used to avoid warning
    (permissions, owner, group)
}

#[cfg(not(unix))]
fn get_platform_metadata(
    _metadata: &std::fs::Metadata,
    _path: &std::path::Path,
) -> (Option<String>, Option<String>, Option<String>) {
    (None, None, None)
}

#[cfg(unix)]
fn format_unix_permissions(mode: u32) -> String {
    let mut s = String::with_capacity(9);
    let flags = [
        (0o400, 'r'),
        (0o200, 'w'),
        (0o100, 'x'),
        (0o040, 'r'),
        (0o020, 'w'),
        (0o010, 'x'),
        (0o004, 'r'),
        (0o002, 'w'),
        (0o001, 'x'),
    ];
    for (bit, ch) in flags {
        if mode & bit != 0 {
            s.push(ch);
        } else {
            s.push('-');
        }
    }
    s
}

#[cfg(unix)]
fn get_username_by_uid(uid: u32) -> Option<String> {
    let mut buf = vec![0u8; 1024];
    let mut passwd = unsafe { std::mem::zeroed::<libc::passwd>() };
    let mut result: *mut libc::passwd = std::ptr::null_mut();

    loop {
        let ret = unsafe {
            libc::getpwuid_r(
                uid,
                &mut passwd,
                buf.as_mut_ptr() as *mut libc::c_char,
                buf.len(),
                &mut result,
            )
        };

        if ret == libc::ERANGE {
            buf.resize(buf.len() * 2, 0);
            if buf.len() > 65536 {
                return Some(uid.to_string());
            }
            continue;
        }

        if ret != 0 || result.is_null() {
            return Some(uid.to_string());
        }

        let name = unsafe { std::ffi::CStr::from_ptr(passwd.pw_name) };
        return Some(name.to_string_lossy().to_string());
    }
}

#[cfg(unix)]
fn get_groupname_by_gid(gid: u32) -> Option<String> {
    let mut buf = vec![0u8; 1024];
    let mut group = unsafe { std::mem::zeroed::<libc::group>() };
    let mut result: *mut libc::group = std::ptr::null_mut();

    loop {
        let ret = unsafe {
            libc::getgrgid_r(
                gid,
                &mut group,
                buf.as_mut_ptr() as *mut libc::c_char,
                buf.len(),
                &mut result,
            )
        };

        if ret == libc::ERANGE {
            buf.resize(buf.len() * 2, 0);
            if buf.len() > 65536 {
                return Some(gid.to_string());
            }
            continue;
        }

        if ret != 0 || result.is_null() {
            return Some(gid.to_string());
        }

        let name = unsafe { std::ffi::CStr::from_ptr(group.gr_name) };
        return Some(name.to_string_lossy().to_string());
    }
}

/// Background task: read a file in chunks and stream via the channel.
async fn download_file(
    transfer_id: String,
    path: PathBuf,
    tx: mpsc::Sender<FileEvent>,
) -> anyhow::Result<()> {
    use tokio::io::AsyncReadExt;

    let mut file = tokio::fs::File::open(&path).await?;
    let metadata = file.metadata().await?;
    let file_size = metadata.len();

    // Send ready signal with file size so the server can create the temp file
    tx.send(FileEvent::DownloadReady {
        transfer_id: transfer_id.clone(),
        size: file_size,
    })
    .await
    .map_err(|_| anyhow::anyhow!("Channel closed"))?;

    let mut offset: u64 = 0;
    let mut buf = vec![0u8; MAX_FILE_CHUNK_SIZE];

    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }

        let data = BASE64.encode(&buf[..n]);
        tx.send(FileEvent::DownloadChunk {
            transfer_id: transfer_id.clone(),
            offset,
            data,
        })
        .await
        .map_err(|_| anyhow::anyhow!("Channel closed"))?;

        offset += n as u64;
    }

    tx.send(FileEvent::DownloadEnd {
        transfer_id: transfer_id.clone(),
    })
    .await
    .map_err(|_| anyhow::anyhow!("Channel closed"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serverbee_common::constants::CAP_FILE;
    use std::sync::atomic::AtomicU32;
    use tempfile::TempDir;

    fn make_config(root: &str) -> FileConfig {
        FileConfig {
            enabled: true,
            root_paths: vec![root.to_string()],
            max_file_size: 1_073_741_824,
            deny_patterns: vec![
                "*.key".into(),
                "*.pem".into(),
                "id_rsa*".into(),
                ".env*".into(),
                "shadow".into(),
                "passwd".into(),
            ],
        }
    }

    fn make_manager(config: FileConfig) -> FileManager {
        let caps = Arc::new(AtomicU32::new(CAP_FILE));
        FileManager::new(config, caps)
    }

    #[test]
    fn test_validate_path_within_root() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_str().unwrap();
        let config = make_config(root);
        let mgr = make_manager(config);

        // Create a file inside root
        let file_path = tmp.path().join("hello.txt");
        std::fs::write(&file_path, "content").unwrap();

        let result = mgr.validate_path(file_path.to_str().unwrap());
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_path_outside_root() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("allowed");
        std::fs::create_dir_all(&root).unwrap();

        let config = make_config(root.to_str().unwrap());
        let mgr = make_manager(config);

        // A path outside the allowed root
        let outside = tmp.path().join("outside.txt");
        std::fs::write(&outside, "content").unwrap();

        let result = mgr.validate_path(outside.to_str().unwrap());
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("outside allowed root")
        );
    }

    #[test]
    fn test_validate_path_traversal_rejected() {
        let tmp = TempDir::new().unwrap();
        let allowed = tmp.path().join("allowed");
        std::fs::create_dir_all(&allowed).unwrap();

        let config = make_config(allowed.to_str().unwrap());
        let mgr = make_manager(config);

        // Attempt traversal: allowed/../outside.txt
        let outside = tmp.path().join("outside.txt");
        std::fs::write(&outside, "content").unwrap();

        let traversal = format!(
            "{}/../../{}",
            allowed.display(),
            outside.file_name().unwrap().to_str().unwrap()
        );
        // This will fail either because canonicalize resolves the traversal outside root,
        // or because the path doesn't exist
        let result = mgr.validate_path(&traversal);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_path_deny_patterns() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_str().unwrap();
        let config = make_config(root);
        let mgr = make_manager(config);

        // Create files matching deny patterns
        let key_file = tmp.path().join("server.key");
        std::fs::write(&key_file, "secret").unwrap();
        assert!(mgr.validate_path(key_file.to_str().unwrap()).is_err());

        let pem_file = tmp.path().join("cert.pem");
        std::fs::write(&pem_file, "secret").unwrap();
        assert!(mgr.validate_path(pem_file.to_str().unwrap()).is_err());

        let rsa_file = tmp.path().join("id_rsa");
        std::fs::write(&rsa_file, "secret").unwrap();
        assert!(mgr.validate_path(rsa_file.to_str().unwrap()).is_err());

        let rsa_pub = tmp.path().join("id_rsa.pub");
        std::fs::write(&rsa_pub, "public").unwrap();
        assert!(mgr.validate_path(rsa_pub.to_str().unwrap()).is_err());

        let env_file = tmp.path().join(".env");
        std::fs::write(&env_file, "secret").unwrap();
        assert!(mgr.validate_path(env_file.to_str().unwrap()).is_err());

        let env_local = tmp.path().join(".env.local");
        std::fs::write(&env_local, "secret").unwrap();
        assert!(mgr.validate_path(env_local.to_str().unwrap()).is_err());

        let shadow = tmp.path().join("shadow");
        std::fs::write(&shadow, "secret").unwrap();
        assert!(mgr.validate_path(shadow.to_str().unwrap()).is_err());

        let passwd = tmp.path().join("passwd");
        std::fs::write(&passwd, "secret").unwrap();
        assert!(mgr.validate_path(passwd.to_str().unwrap()).is_err());

        // Regular file should pass
        let ok_file = tmp.path().join("readme.txt");
        std::fs::write(&ok_file, "ok").unwrap();
        assert!(mgr.validate_path(ok_file.to_str().unwrap()).is_ok());
    }

    #[test]
    fn test_validate_path_empty_roots_denies_all() {
        let config = FileConfig {
            enabled: true,
            root_paths: vec![],
            max_file_size: 1_073_741_824,
            deny_patterns: vec![],
        };
        let mgr = make_manager(config);

        let result = mgr.validate_path("/tmp/anything");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No root paths configured")
        );
    }

    #[tokio::test]
    async fn test_list_dir_sorts_dirs_first() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_str().unwrap();
        let config = make_config(root);
        let mgr = make_manager(config);

        // Create files and directories
        std::fs::write(tmp.path().join("banana.txt"), "b").unwrap();
        std::fs::write(tmp.path().join("apple.txt"), "a").unwrap();
        std::fs::create_dir(tmp.path().join("zebra_dir")).unwrap();
        std::fs::create_dir(tmp.path().join("alpha_dir")).unwrap();

        let entries = mgr.list_dir(root).await.unwrap();
        assert_eq!(entries.len(), 4);

        // First two should be directories (sorted alphabetically)
        assert!(matches!(entries[0].file_type, FileType::Directory));
        assert_eq!(entries[0].name, "alpha_dir");

        assert!(matches!(entries[1].file_type, FileType::Directory));
        assert_eq!(entries[1].name, "zebra_dir");

        // Then files (sorted alphabetically)
        assert!(matches!(entries[2].file_type, FileType::File));
        assert_eq!(entries[2].name, "apple.txt");

        assert!(matches!(entries[3].file_type, FileType::File));
        assert_eq!(entries[3].name, "banana.txt");
    }

    #[test]
    fn test_matches_deny_pattern() {
        // Suffix patterns
        assert!(matches_deny_pattern("server.key", "*.key"));
        assert!(matches_deny_pattern("cert.pem", "*.pem"));
        assert!(!matches_deny_pattern("server.txt", "*.key"));

        // Prefix patterns
        assert!(matches_deny_pattern("id_rsa", "id_rsa*"));
        assert!(matches_deny_pattern("id_rsa.pub", "id_rsa*"));
        assert!(matches_deny_pattern(".env", ".env*"));
        assert!(matches_deny_pattern(".env.local", ".env*"));
        assert!(!matches_deny_pattern("myenv", ".env*"));

        // Exact match
        assert!(matches_deny_pattern("shadow", "shadow"));
        assert!(matches_deny_pattern("passwd", "passwd"));
        assert!(!matches_deny_pattern("shadow_copy", "shadow"));
    }

    #[tokio::test]
    async fn test_read_and_write_file() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_str().unwrap();
        let config = make_config(root);
        let mgr = make_manager(config);

        // Write a file
        let file_path = tmp.path().join("test.txt");
        std::fs::write(&file_path, "").unwrap(); // create so validate_path can canonicalize
        let content_b64 = BASE64.encode(b"Hello, World!");
        mgr.write_file(file_path.to_str().unwrap(), &content_b64)
            .await
            .unwrap();

        // Read it back
        let read_b64 = mgr
            .read_file(file_path.to_str().unwrap(), 1024)
            .await
            .unwrap();
        let decoded = BASE64.decode(&read_b64).unwrap();
        assert_eq!(decoded, b"Hello, World!");
    }

    #[tokio::test]
    async fn test_read_file_size_limit() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_str().unwrap();
        let config = make_config(root);
        let mgr = make_manager(config);

        let file_path = tmp.path().join("big.txt");
        std::fs::write(&file_path, "x".repeat(1000)).unwrap();

        // Should fail with a small max_size
        let result = mgr.read_file(file_path.to_str().unwrap(), 100).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("exceeds max_size"));
    }

    #[tokio::test]
    async fn test_delete_file() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_str().unwrap();
        let config = make_config(root);
        let mgr = make_manager(config);

        let file_path = tmp.path().join("to_delete.txt");
        std::fs::write(&file_path, "gone").unwrap();
        assert!(file_path.exists());

        mgr.delete(file_path.to_str().unwrap(), false)
            .await
            .unwrap();
        assert!(!file_path.exists());
    }

    #[tokio::test]
    async fn test_delete_dir_recursive() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_str().unwrap();
        let config = make_config(root);
        let mgr = make_manager(config);

        let dir_path = tmp.path().join("mydir");
        std::fs::create_dir(&dir_path).unwrap();
        std::fs::write(dir_path.join("file.txt"), "content").unwrap();

        mgr.delete(dir_path.to_str().unwrap(), true).await.unwrap();
        assert!(!dir_path.exists());
    }

    #[tokio::test]
    async fn test_mkdir() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_str().unwrap();
        let config = make_config(root);
        let mgr = make_manager(config);

        let dir_path = tmp.path().join("new_dir").join("nested");
        assert!(!dir_path.exists());

        mgr.mkdir(dir_path.to_str().unwrap()).await.unwrap();
        assert!(dir_path.exists());
        assert!(dir_path.is_dir());
    }

    #[tokio::test]
    async fn test_rename_path() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_str().unwrap();
        let config = make_config(root);
        let mgr = make_manager(config);

        let from = tmp.path().join("original.txt");
        let to = tmp.path().join("renamed.txt");
        std::fs::write(&from, "content").unwrap();

        mgr.rename_path(from.to_str().unwrap(), to.to_str().unwrap())
            .await
            .unwrap();

        assert!(!from.exists());
        assert!(to.exists());
    }

    #[tokio::test]
    async fn test_stat() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_str().unwrap();
        let config = make_config(root);
        let mgr = make_manager(config);

        let file_path = tmp.path().join("stat_test.txt");
        std::fs::write(&file_path, "hello").unwrap();

        let entry = mgr.stat(file_path.to_str().unwrap()).await.unwrap();
        assert_eq!(entry.name, "stat_test.txt");
        assert_eq!(entry.size, 5);
        assert!(matches!(entry.file_type, FileType::File));
    }

    #[tokio::test]
    async fn test_upload_flow() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_str().unwrap();
        let config = make_config(root);
        let mgr = make_manager(config);

        let file_path = tmp.path().join("upload.txt");
        // Touch the parent (root) — it's already there from TempDir.
        // start_upload validates the parent, not the file itself.

        mgr.start_upload("t1".into(), file_path.to_str().unwrap().into(), 100)
            .await
            .unwrap();

        let data = BASE64.encode(b"Hello upload!");
        let new_offset = mgr.receive_chunk("t1", 0, &data).await.unwrap();
        assert_eq!(new_offset, 13); // "Hello upload!" is 13 bytes

        mgr.finish_upload("t1").await.unwrap();
        assert!(file_path.exists());
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello upload!");
    }

    #[test]
    fn test_deny_pattern_env_variants() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_str().unwrap();
        let config = make_config(root);
        let mgr = make_manager(config);

        for name in &[".env", ".env.local", ".env.production"] {
            let path = tmp.path().join(name);
            std::fs::write(&path, "SECRET=x").unwrap();
            assert!(
                mgr.validate_path(path.to_str().unwrap()).is_err(),
                "Should deny {name}"
            );
        }
    }

    #[test]
    fn test_deny_pattern_id_rsa_variants() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_str().unwrap();
        let config = make_config(root);
        let mgr = make_manager(config);

        for name in &["id_rsa", "id_rsa.pub", "id_rsa_backup"] {
            let path = tmp.path().join(name);
            std::fs::write(&path, "key").unwrap();
            assert!(
                mgr.validate_path(path.to_str().unwrap()).is_err(),
                "Should deny {name}"
            );
        }
    }

    #[test]
    fn test_validate_path_allows_normal_files() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_str().unwrap();
        let config = make_config(root);
        let mgr = make_manager(config);

        for name in &["config.yaml", "app.log", "start.sh", "README.md"] {
            let path = tmp.path().join(name);
            std::fs::write(&path, "content").unwrap();
            assert!(
                mgr.validate_path(path.to_str().unwrap()).is_ok(),
                "Should allow {name}"
            );
        }
    }

    #[test]
    fn test_validate_path_multiple_roots() {
        let tmp1 = TempDir::new().unwrap();
        let tmp2 = TempDir::new().unwrap();
        let config = FileConfig {
            enabled: true,
            root_paths: vec![
                tmp1.path().to_str().unwrap().to_string(),
                tmp2.path().to_str().unwrap().to_string(),
            ],
            max_file_size: 1_073_741_824,
            deny_patterns: vec![
                "*.key".into(),
                "*.pem".into(),
                "id_rsa*".into(),
                ".env*".into(),
                "shadow".into(),
                "passwd".into(),
            ],
        };
        let caps = Arc::new(AtomicU32::new(u32::MAX));
        let mgr = FileManager::new(config, caps);

        let f1 = tmp1.path().join("a.txt");
        let f2 = tmp2.path().join("b.txt");
        std::fs::write(&f1, "a").unwrap();
        std::fs::write(&f2, "b").unwrap();

        assert!(mgr.validate_path(f1.to_str().unwrap()).is_ok());
        assert!(mgr.validate_path(f2.to_str().unwrap()).is_ok());
    }

    #[tokio::test]
    async fn test_read_file_base64_encoding() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_str().unwrap();
        let config = make_config(root);
        let mgr = make_manager(config);
        let path = tmp.path().join("test.txt");
        std::fs::write(&path, "hello world").unwrap();

        let content = mgr.read_file(path.to_str().unwrap(), 1024).await.unwrap();
        let decoded = BASE64.decode(&content).unwrap();
        assert_eq!(std::str::from_utf8(&decoded).unwrap(), "hello world");
    }

    #[tokio::test]
    async fn test_write_file_base64_decoding() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_str().unwrap();
        let config = make_config(root);
        let mgr = make_manager(config);
        let path = tmp.path().join("output.txt");
        // Create the file first so validate_path can canonicalize it
        std::fs::write(&path, "").unwrap();

        let encoded = BASE64.encode("written content");
        mgr.write_file(path.to_str().unwrap(), &encoded)
            .await
            .unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "written content");
    }

    #[tokio::test]
    async fn test_list_dir_empty_directory() {
        let tmp = TempDir::new().unwrap();
        let subdir = tmp.path().join("empty");
        std::fs::create_dir(&subdir).unwrap();
        let root = tmp.path().to_str().unwrap();
        let config = make_config(root);
        let mgr = make_manager(config);

        let entries = mgr.list_dir(subdir.to_str().unwrap()).await.unwrap();
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn test_list_dir_file_metadata() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_str().unwrap();
        let config = make_config(root);
        let mgr = make_manager(config);
        std::fs::write(tmp.path().join("hello.txt"), "12345").unwrap();

        let entries = mgr.list_dir(tmp.path().to_str().unwrap()).await.unwrap();
        let file_entry = entries.iter().find(|e| e.name == "hello.txt").unwrap();
        assert_eq!(file_entry.size, 5);
        assert!(matches!(file_entry.file_type, FileType::File));
        assert!(file_entry.modified > 0);
    }

    #[tokio::test]
    async fn test_download_sends_chunks() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_str().unwrap();
        let config = make_config(root);
        let mgr = make_manager(config);

        let file_path = tmp.path().join("download.txt");
        std::fs::write(&file_path, "Download me!").unwrap();

        let (tx, mut rx) = mpsc::channel::<FileEvent>(16);
        mgr.start_download("d1".into(), file_path.to_str().unwrap().into(), tx);

        // Collect events
        let mut chunks = Vec::new();
        let mut got_ready = false;
        let mut got_end = false;
        while let Some(event) = rx.recv().await {
            match event {
                FileEvent::DownloadReady { size, .. } => {
                    assert_eq!(size, 12); // "Download me!" is 12 bytes
                    got_ready = true;
                }
                FileEvent::DownloadChunk { data, .. } => {
                    chunks.push(data);
                }
                FileEvent::DownloadEnd { .. } => {
                    got_end = true;
                    break;
                }
                FileEvent::DownloadError { error, .. } => {
                    panic!("Unexpected error: {error}");
                }
            }
        }

        assert!(got_ready);
        assert!(got_end);
        assert!(!chunks.is_empty());
        let decoded: Vec<u8> = chunks
            .iter()
            .flat_map(|c| BASE64.decode(c).unwrap())
            .collect();
        assert_eq!(String::from_utf8(decoded).unwrap(), "Download me!");
    }

    // ---- is_enabled ----

    #[test]
    fn test_is_enabled_true() {
        let tmp = TempDir::new().unwrap();
        let config = make_config(tmp.path().to_str().unwrap());
        let mgr = make_manager(config);
        assert!(mgr.is_enabled());
    }

    #[test]
    fn test_is_enabled_false() {
        let tmp = TempDir::new().unwrap();
        let mut config = make_config(tmp.path().to_str().unwrap());
        config.enabled = false;
        let mgr = make_manager(config);
        assert!(!mgr.is_enabled());
    }

    // ---- FileEvent -> AgentMessage conversion (all 4 variants) ----

    #[test]
    fn test_file_event_into_agent_message_ready() {
        let msg: AgentMessage = FileEvent::DownloadReady {
            transfer_id: "t".into(),
            size: 42,
        }
        .into();
        match msg {
            AgentMessage::FileDownloadReady { transfer_id, size } => {
                assert_eq!(transfer_id, "t");
                assert_eq!(size, 42);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_file_event_into_agent_message_chunk() {
        let msg: AgentMessage = FileEvent::DownloadChunk {
            transfer_id: "t".into(),
            offset: 7,
            data: "abc".into(),
        }
        .into();
        match msg {
            AgentMessage::FileDownloadChunk {
                transfer_id,
                offset,
                data,
            } => {
                assert_eq!(transfer_id, "t");
                assert_eq!(offset, 7);
                assert_eq!(data, "abc");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_file_event_into_agent_message_end() {
        let msg: AgentMessage = FileEvent::DownloadEnd {
            transfer_id: "tend".into(),
        }
        .into();
        match msg {
            AgentMessage::FileDownloadEnd { transfer_id } => assert_eq!(transfer_id, "tend"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_file_event_into_agent_message_error() {
        let msg: AgentMessage = FileEvent::DownloadError {
            transfer_id: "terr".into(),
            error: "boom".into(),
        }
        .into();
        match msg {
            AgentMessage::FileDownloadError { transfer_id, error } => {
                assert_eq!(transfer_id, "terr");
                assert_eq!(error, "boom");
            }
            _ => panic!("wrong variant"),
        }
    }

    // ---- validate_path error branches ----

    #[test]
    fn test_validate_path_nonexistent_cannot_resolve() {
        let tmp = TempDir::new().unwrap();
        let config = make_config(tmp.path().to_str().unwrap());
        let mgr = make_manager(config);

        let missing = tmp.path().join("does_not_exist.txt");
        let result = mgr.validate_path(missing.to_str().unwrap());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Cannot resolve path"));
    }

    // ---- list_dir branches ----

    #[tokio::test]
    async fn test_list_dir_empty_roots_bails() {
        let config = FileConfig {
            enabled: true,
            root_paths: vec![],
            max_file_size: 1_073_741_824,
            deny_patterns: vec![],
        };
        let mgr = make_manager(config);
        let result = mgr.list_dir("/tmp").await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No root paths configured")
        );
    }

    #[tokio::test]
    async fn test_list_dir_ancestor_returns_virtual_entries() {
        // Root is a nested subdir; requesting its parent (outside root) should
        // return the root as a virtual directory entry for navigation.
        let tmp = TempDir::new().unwrap();
        let nested = tmp.path().join("nested_root");
        std::fs::create_dir(&nested).unwrap();

        let config = make_config(nested.to_str().unwrap());
        let mgr = make_manager(config);

        // The parent (tmp) is an ancestor of the root but not within it.
        let entries = mgr.list_dir(tmp.path().to_str().unwrap()).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert!(matches!(entries[0].file_type, FileType::Directory));
        // The virtual entry path is the canonical root path.
        let canonical_nested = std::fs::canonicalize(&nested).unwrap();
        assert_eq!(entries[0].path, canonical_nested.to_string_lossy());
    }

    #[tokio::test]
    async fn test_list_dir_root_slash_ancestor() {
        // Requesting "/" should surface the root path as a virtual entry,
        // exercising the `path == "/"` branch.
        let tmp = TempDir::new().unwrap();
        let nested = tmp.path().join("only_root");
        std::fs::create_dir(&nested).unwrap();

        let config = make_config(nested.to_str().unwrap());
        let mgr = make_manager(config);

        let entries = mgr.list_dir("/").await.unwrap();
        assert!(!entries.is_empty());
        assert!(entries.iter().all(|e| matches!(e.file_type, FileType::Directory)));
    }

    #[tokio::test]
    async fn test_list_dir_outside_and_not_ancestor_bails() {
        let tmp = TempDir::new().unwrap();
        let allowed = tmp.path().join("allowed");
        std::fs::create_dir(&allowed).unwrap();
        let sibling = tmp.path().join("sibling");
        std::fs::create_dir(&sibling).unwrap();

        let config = make_config(allowed.to_str().unwrap());
        let mgr = make_manager(config);

        // sibling is neither within nor an ancestor of allowed.
        let result = mgr.list_dir(sibling.to_str().unwrap()).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("outside allowed root paths")
        );
    }

    #[tokio::test]
    async fn test_list_dir_includes_symlink_entry() {
        let tmp = TempDir::new().unwrap();
        let config = make_config(tmp.path().to_str().unwrap());
        let mgr = make_manager(config);

        let target = tmp.path().join("target.txt");
        std::fs::write(&target, "data").unwrap();
        let link = tmp.path().join("link.txt");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link).unwrap();

        #[cfg(unix)]
        {
            let entries = mgr.list_dir(tmp.path().to_str().unwrap()).await.unwrap();
            let link_entry = entries.iter().find(|e| e.name == "link.txt").unwrap();
            assert!(matches!(link_entry.file_type, FileType::Symlink));
        }
    }

    // ---- stat branches ----

    #[tokio::test]
    async fn test_stat_nonexistent_path_errors() {
        let tmp = TempDir::new().unwrap();
        let config = make_config(tmp.path().to_str().unwrap());
        let mgr = make_manager(config);

        let missing = tmp.path().join("nope.txt");
        let result = mgr.stat(missing.to_str().unwrap()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_stat_directory() {
        let tmp = TempDir::new().unwrap();
        let config = make_config(tmp.path().to_str().unwrap());
        let mgr = make_manager(config);

        let dir = tmp.path().join("a_dir");
        std::fs::create_dir(&dir).unwrap();

        let entry = mgr.stat(dir.to_str().unwrap()).await.unwrap();
        assert_eq!(entry.name, "a_dir");
        assert!(matches!(entry.file_type, FileType::Directory));
        #[cfg(unix)]
        assert!(entry.permissions.is_some());
    }

    // ---- read_file boundaries ----

    #[tokio::test]
    async fn test_read_file_nonexistent_errors() {
        let tmp = TempDir::new().unwrap();
        let config = make_config(tmp.path().to_str().unwrap());
        let mgr = make_manager(config);

        let missing = tmp.path().join("missing.txt");
        let result = mgr.read_file(missing.to_str().unwrap(), 1024).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_read_file_exactly_at_limit_ok() {
        let tmp = TempDir::new().unwrap();
        let config = make_config(tmp.path().to_str().unwrap());
        let mgr = make_manager(config);

        let path = tmp.path().join("exact.txt");
        std::fs::write(&path, vec![b'a'; 100]).unwrap();

        // size == max_size must pass (boundary is strictly greater-than).
        let result = mgr.read_file(path.to_str().unwrap(), 100).await;
        assert!(result.is_ok());
        let decoded = BASE64.decode(result.unwrap()).unwrap();
        assert_eq!(decoded.len(), 100);
    }

    #[tokio::test]
    async fn test_read_file_one_over_limit_errors() {
        let tmp = TempDir::new().unwrap();
        let config = make_config(tmp.path().to_str().unwrap());
        let mgr = make_manager(config);

        let path = tmp.path().join("over.txt");
        std::fs::write(&path, vec![b'a'; 101]).unwrap();

        let result = mgr.read_file(path.to_str().unwrap(), 100).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("exceeds max_size"));
    }

    // ---- write_file error branches ----

    #[tokio::test]
    async fn test_write_file_invalid_base64_errors() {
        let tmp = TempDir::new().unwrap();
        let config = make_config(tmp.path().to_str().unwrap());
        let mgr = make_manager(config);

        let path = tmp.path().join("bad.txt");
        std::fs::write(&path, "").unwrap();

        // "!!!" is not valid base64.
        let result = mgr.write_file(path.to_str().unwrap(), "!!!not-base64!!!").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid base64 content"));
    }

    #[tokio::test]
    async fn test_write_file_outside_root_errors() {
        let tmp = TempDir::new().unwrap();
        let allowed = tmp.path().join("allowed");
        std::fs::create_dir(&allowed).unwrap();
        let config = make_config(allowed.to_str().unwrap());
        let mgr = make_manager(config);

        // Path outside the allowed root, but file exists so canonicalize succeeds.
        let outside = tmp.path().join("outside.txt");
        std::fs::write(&outside, "x").unwrap();

        let encoded = BASE64.encode(b"data");
        let result = mgr.write_file(outside.to_str().unwrap(), &encoded).await;
        assert!(result.is_err());
    }

    // ---- delete error branches ----

    #[tokio::test]
    async fn test_delete_nonexistent_errors() {
        let tmp = TempDir::new().unwrap();
        let config = make_config(tmp.path().to_str().unwrap());
        let mgr = make_manager(config);

        let missing = tmp.path().join("ghost.txt");
        let result = mgr.delete(missing.to_str().unwrap(), false).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delete_nonempty_dir_non_recursive_errors() {
        let tmp = TempDir::new().unwrap();
        let config = make_config(tmp.path().to_str().unwrap());
        let mgr = make_manager(config);

        let dir = tmp.path().join("nonempty");
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("child.txt"), "x").unwrap();

        // Non-recursive remove of a non-empty directory must fail.
        let result = mgr.delete(dir.to_str().unwrap(), false).await;
        assert!(result.is_err());
        assert!(dir.exists());
    }

    #[tokio::test]
    async fn test_delete_empty_dir_non_recursive_ok() {
        let tmp = TempDir::new().unwrap();
        let config = make_config(tmp.path().to_str().unwrap());
        let mgr = make_manager(config);

        let dir = tmp.path().join("empty_to_remove");
        std::fs::create_dir(&dir).unwrap();

        mgr.delete(dir.to_str().unwrap(), false).await.unwrap();
        assert!(!dir.exists());
    }

    // ---- mkdir error branches ----

    #[tokio::test]
    async fn test_mkdir_outside_root_errors() {
        let tmp = TempDir::new().unwrap();
        let allowed = tmp.path().join("allowed");
        std::fs::create_dir(&allowed).unwrap();
        let config = make_config(allowed.to_str().unwrap());
        let mgr = make_manager(config);

        // Closest existing ancestor (tmp) is outside the allowed root.
        let target = tmp.path().join("brand_new").join("child");
        let result = mgr.mkdir(target.to_str().unwrap()).await;
        assert!(result.is_err());
        assert!(!target.exists());
    }

    #[tokio::test]
    async fn test_mkdir_existing_dir_ok() {
        let tmp = TempDir::new().unwrap();
        let config = make_config(tmp.path().to_str().unwrap());
        let mgr = make_manager(config);

        let dir = tmp.path().join("already_there");
        std::fs::create_dir(&dir).unwrap();

        // create_dir_all on an existing directory is idempotent.
        mgr.mkdir(dir.to_str().unwrap()).await.unwrap();
        assert!(dir.is_dir());
    }

    // ---- rename_path error branches ----

    #[tokio::test]
    async fn test_rename_path_from_missing_errors() {
        let tmp = TempDir::new().unwrap();
        let config = make_config(tmp.path().to_str().unwrap());
        let mgr = make_manager(config);

        let from = tmp.path().join("not_here.txt");
        let to = tmp.path().join("dest.txt");
        let result = mgr.rename_path(from.to_str().unwrap(), to.to_str().unwrap()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rename_path_dest_deny_pattern_errors() {
        let tmp = TempDir::new().unwrap();
        let config = make_config(tmp.path().to_str().unwrap());
        let mgr = make_manager(config);

        let from = tmp.path().join("plain.txt");
        std::fs::write(&from, "x").unwrap();
        // Destination filename matches a deny pattern (*.key).
        let to = tmp.path().join("secret.key");

        let result = mgr.rename_path(from.to_str().unwrap(), to.to_str().unwrap()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("deny pattern"));
        assert!(from.exists());
        assert!(!to.exists());
    }

    #[tokio::test]
    async fn test_rename_path_dest_parent_outside_root_errors() {
        let tmp = TempDir::new().unwrap();
        let allowed = tmp.path().join("allowed");
        std::fs::create_dir(&allowed).unwrap();
        let config = make_config(allowed.to_str().unwrap());
        let mgr = make_manager(config);

        let from = allowed.join("src.txt");
        std::fs::write(&from, "x").unwrap();
        // Destination parent (tmp) is outside the allowed root.
        let to = tmp.path().join("escaped.txt");

        let result = mgr.rename_path(from.to_str().unwrap(), to.to_str().unwrap()).await;
        assert!(result.is_err());
        assert!(from.exists());
    }

    // ---- start_download error path ----

    #[tokio::test]
    async fn test_start_download_invalid_path_emits_error() {
        let tmp = TempDir::new().unwrap();
        let config = make_config(tmp.path().to_str().unwrap());
        let mgr = make_manager(config);

        let missing = tmp.path().join("nope.bin");
        let (tx, mut rx) = mpsc::channel::<FileEvent>(4);
        mgr.start_download("derr".into(), missing.to_str().unwrap().into(), tx);

        let event = rx.recv().await.expect("should receive an error event");
        match event {
            FileEvent::DownloadError { transfer_id, error } => {
                assert_eq!(transfer_id, "derr");
                assert!(!error.is_empty());
            }
            _ => panic!("expected DownloadError"),
        }
    }

    #[tokio::test]
    async fn test_download_empty_file_sends_ready_and_end_only() {
        let tmp = TempDir::new().unwrap();
        let config = make_config(tmp.path().to_str().unwrap());
        let mgr = make_manager(config);

        let path = tmp.path().join("empty.bin");
        std::fs::write(&path, b"").unwrap();

        let (tx, mut rx) = mpsc::channel::<FileEvent>(16);
        mgr.start_download("dempty".into(), path.to_str().unwrap().into(), tx);

        let mut got_ready = false;
        let mut got_end = false;
        let mut chunk_count = 0;
        while let Some(event) = rx.recv().await {
            match event {
                FileEvent::DownloadReady { size, .. } => {
                    assert_eq!(size, 0);
                    got_ready = true;
                }
                FileEvent::DownloadChunk { .. } => chunk_count += 1,
                FileEvent::DownloadEnd { .. } => {
                    got_end = true;
                    break;
                }
                FileEvent::DownloadError { error, .. } => panic!("unexpected error: {error}"),
            }
        }
        assert!(got_ready);
        assert!(got_end);
        assert_eq!(chunk_count, 0);
    }

    #[tokio::test]
    async fn test_download_multi_chunk_file() {
        let tmp = TempDir::new().unwrap();
        let config = make_config(tmp.path().to_str().unwrap());
        let mgr = make_manager(config);

        // Slightly larger than one chunk to force a second read iteration.
        let total = MAX_FILE_CHUNK_SIZE + 1234;
        let payload: Vec<u8> = (0..total).map(|i| (i % 251) as u8).collect();
        let path = tmp.path().join("multi.bin");
        std::fs::write(&path, &payload).unwrap();

        let (tx, mut rx) = mpsc::channel::<FileEvent>(64);
        mgr.start_download("dmulti".into(), path.to_str().unwrap().into(), tx);

        let mut reported_size = 0u64;
        let mut reassembled: Vec<u8> = Vec::new();
        let mut chunk_count = 0;
        let mut last_offset = 0u64;
        while let Some(event) = rx.recv().await {
            match event {
                FileEvent::DownloadReady { size, .. } => reported_size = size,
                FileEvent::DownloadChunk { offset, data, .. } => {
                    assert_eq!(offset, last_offset);
                    let bytes = BASE64.decode(&data).unwrap();
                    last_offset += bytes.len() as u64;
                    reassembled.extend_from_slice(&bytes);
                    chunk_count += 1;
                }
                FileEvent::DownloadEnd { .. } => break,
                FileEvent::DownloadError { error, .. } => panic!("unexpected error: {error}"),
            }
        }
        assert_eq!(reported_size, total as u64);
        assert!(chunk_count >= 2, "expected multiple chunks, got {chunk_count}");
        assert_eq!(reassembled, payload);
    }

    // ---- cancel_download / cancel_all_transfers ----

    #[tokio::test]
    async fn test_cancel_download_removes_entry() {
        let tmp = TempDir::new().unwrap();
        let config = make_config(tmp.path().to_str().unwrap());
        let mgr = make_manager(config);

        let path = tmp.path().join("cancel.bin");
        std::fs::write(&path, vec![0u8; MAX_FILE_CHUNK_SIZE * 2]).unwrap();

        let (tx, _rx) = mpsc::channel::<FileEvent>(1);
        mgr.start_download("c1".into(), path.to_str().unwrap().into(), tx);
        assert!(mgr.active_downloads.contains_key("c1"));

        mgr.cancel_download("c1");
        assert!(!mgr.active_downloads.contains_key("c1"));

        // Cancelling an unknown transfer is a no-op.
        mgr.cancel_download("does-not-exist");
    }

    #[tokio::test]
    async fn test_cancel_all_transfers_clears_maps() {
        let tmp = TempDir::new().unwrap();
        let config = make_config(tmp.path().to_str().unwrap());
        let mgr = make_manager(config);

        // Seed an active download.
        let dpath = tmp.path().join("dl.bin");
        std::fs::write(&dpath, vec![0u8; MAX_FILE_CHUNK_SIZE * 2]).unwrap();
        let (tx, _rx) = mpsc::channel::<FileEvent>(1);
        mgr.start_download("d".into(), dpath.to_str().unwrap().into(), tx);

        // Seed an active upload.
        let upath = tmp.path().join("up.bin");
        mgr.start_upload("u".into(), upath.to_str().unwrap().into(), 10)
            .await
            .unwrap();

        assert!(!mgr.active_downloads.is_empty());
        assert!(!mgr.active_uploads.is_empty());

        mgr.cancel_all_transfers();

        assert!(mgr.active_downloads.is_empty());
        assert!(mgr.active_uploads.is_empty());
    }

    // ---- start_upload error branches ----

    #[tokio::test]
    async fn test_start_upload_deny_pattern_errors() {
        let tmp = TempDir::new().unwrap();
        let config = make_config(tmp.path().to_str().unwrap());
        let mgr = make_manager(config);

        let path = tmp.path().join("secret.key");
        let result = mgr
            .start_upload("up_deny".into(), path.to_str().unwrap().into(), 10)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("deny pattern"));
        assert!(!mgr.active_uploads.contains_key("up_deny"));
    }

    #[tokio::test]
    async fn test_start_upload_size_exceeds_limit_errors() {
        let tmp = TempDir::new().unwrap();
        let mut config = make_config(tmp.path().to_str().unwrap());
        config.max_file_size = 50;
        let mgr = make_manager(config);

        let path = tmp.path().join("toobig.bin");
        let result = mgr
            .start_upload("up_big".into(), path.to_str().unwrap().into(), 51)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("exceeds max_file_size"));
    }

    #[tokio::test]
    async fn test_start_upload_parent_outside_root_errors() {
        let tmp = TempDir::new().unwrap();
        let allowed = tmp.path().join("allowed");
        std::fs::create_dir(&allowed).unwrap();
        let config = make_config(allowed.to_str().unwrap());
        let mgr = make_manager(config);

        // Parent (tmp) is outside the allowed root.
        let path = tmp.path().join("escape.bin");
        let result = mgr
            .start_upload("up_out".into(), path.to_str().unwrap().into(), 10)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_start_upload_size_exactly_at_limit_ok() {
        let tmp = TempDir::new().unwrap();
        let mut config = make_config(tmp.path().to_str().unwrap());
        config.max_file_size = 50;
        let mgr = make_manager(config);

        let path = tmp.path().join("exact_upload.bin");
        // size == max_file_size must be allowed (boundary is strictly greater-than).
        mgr.start_upload("up_exact".into(), path.to_str().unwrap().into(), 50)
            .await
            .unwrap();
        assert!(mgr.active_uploads.contains_key("up_exact"));
    }

    // ---- receive_chunk error branches ----

    #[tokio::test]
    async fn test_receive_chunk_unknown_transfer_errors() {
        let tmp = TempDir::new().unwrap();
        let config = make_config(tmp.path().to_str().unwrap());
        let mgr = make_manager(config);

        let data = BASE64.encode(b"x");
        let result = mgr.receive_chunk("ghost", 0, &data).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown upload transfer"));
    }

    #[tokio::test]
    async fn test_receive_chunk_invalid_base64_errors() {
        let tmp = TempDir::new().unwrap();
        let config = make_config(tmp.path().to_str().unwrap());
        let mgr = make_manager(config);

        let path = tmp.path().join("chunk.bin");
        mgr.start_upload("uc".into(), path.to_str().unwrap().into(), 100)
            .await
            .unwrap();

        let result = mgr.receive_chunk("uc", 0, "%%%not-base64%%%").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid base64 chunk"));
    }

    #[tokio::test]
    async fn test_receive_chunk_multi_chunk_offsets() {
        let tmp = TempDir::new().unwrap();
        let config = make_config(tmp.path().to_str().unwrap());
        let mgr = make_manager(config);

        let path = tmp.path().join("multi_upload.bin");
        mgr.start_upload("um".into(), path.to_str().unwrap().into(), 1000)
            .await
            .unwrap();

        let chunk_a = BASE64.encode(b"AAAAA");
        let chunk_b = BASE64.encode(b"BBBBB");

        let off1 = mgr.receive_chunk("um", 0, &chunk_a).await.unwrap();
        assert_eq!(off1, 5);
        let off2 = mgr.receive_chunk("um", off1, &chunk_b).await.unwrap();
        assert_eq!(off2, 10);

        mgr.finish_upload("um").await.unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "AAAAABBBBB");
    }

    #[tokio::test]
    async fn test_receive_chunk_empty_data_ok() {
        let tmp = TempDir::new().unwrap();
        let config = make_config(tmp.path().to_str().unwrap());
        let mgr = make_manager(config);

        let path = tmp.path().join("empty_chunk.bin");
        mgr.start_upload("ue".into(), path.to_str().unwrap().into(), 100)
            .await
            .unwrap();

        // Empty (but valid) base64 -> zero bytes, offset unchanged.
        let new_offset = mgr.receive_chunk("ue", 0, "").await.unwrap();
        assert_eq!(new_offset, 0);

        mgr.finish_upload("ue").await.unwrap();
        assert!(path.exists());
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
    }

    // ---- finish_upload error branch ----

    #[tokio::test]
    async fn test_finish_upload_unknown_transfer_errors() {
        let tmp = TempDir::new().unwrap();
        let config = make_config(tmp.path().to_str().unwrap());
        let mgr = make_manager(config);

        let result = mgr.finish_upload("never-started").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown upload transfer"));
    }

    // ---- find_existing_ancestor ----

    #[test]
    fn test_find_existing_ancestor_finds_root() {
        let tmp = TempDir::new().unwrap();
        let deep = tmp.path().join("a").join("b").join("c");
        let found = find_existing_ancestor(&deep);
        assert_eq!(found, Some(tmp.path().to_path_buf()));
    }

    #[test]
    fn test_find_existing_ancestor_returns_self_when_exists() {
        let tmp = TempDir::new().unwrap();
        let found = find_existing_ancestor(tmp.path());
        assert_eq!(found, Some(tmp.path().to_path_buf()));
    }

    #[test]
    fn test_find_existing_ancestor_none_for_relative_missing() {
        // A relative path with no existing component returns None once pop()
        // exhausts the path.
        let p = std::path::Path::new("definitely_missing_dir_xyz123/sub/leaf");
        let found = find_existing_ancestor(p);
        assert!(found.is_none());
    }

    // ---- format_unix_permissions (via stat, unix only) ----

    #[cfg(unix)]
    #[tokio::test]
    async fn test_stat_reports_permission_string() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = TempDir::new().unwrap();
        let config = make_config(tmp.path().to_str().unwrap());
        let mgr = make_manager(config);

        let path = tmp.path().join("perm.txt");
        std::fs::write(&path, "x").unwrap();
        // rw-r--r-- = 0o644
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        let entry = mgr.stat(path.to_str().unwrap()).await.unwrap();
        let perms = entry.permissions.unwrap();
        assert_eq!(perms.len(), 9);
        assert_eq!(perms, "rw-r--r--");
    }

    #[cfg(unix)]
    #[test]
    fn test_format_unix_permissions_full_and_none() {
        assert_eq!(format_unix_permissions(0o777), "rwxrwxrwx");
        assert_eq!(format_unix_permissions(0o000), "---------");
        assert_eq!(format_unix_permissions(0o755), "rwxr-xr-x");
    }
}
