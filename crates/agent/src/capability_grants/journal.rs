//! Durable original authority transitions, removed only after owned Server Ack.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serverbee_common::protocol::CapabilityChangeEvent;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub msg_id: String,
    pub occurred_at: DateTime<Utc>,
    pub changes: Vec<CapabilityChangeEvent>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Journal {
    pub observed_active: u32,
    pub destination: Option<String>,
    pub events: Vec<Event>,
    #[serde(skip)]
    path: PathBuf,
    #[serde(skip)]
    pub dirty: bool,
}
impl Journal {
    pub fn open(grants_path: &Path, active: u32) -> io::Result<Self> {
        let path = grants_path.with_extension("events.json");
        let mut journal = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<Self>(&bytes).map_err(io::Error::other)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Self {
                observed_active: active,
                destination: None,
                events: Vec::new(),
                path: path.clone(),
                dirty: false,
            },
            Err(e) => return Err(e),
        };
        journal.path = path;
        journal.flush()?;
        Ok(journal)
    }
    pub fn flush(&self) -> io::Result<()> {
        if let Some(parent) = self.path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("tmp");
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&tmp)?;
        serde_json::to_writer(&file, self).map_err(io::Error::other)?;
        file.sync_all()?;
        fs::rename(tmp, &self.path)?;
        #[cfg(unix)]
        if let Some(parent) = self.path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::File::open(parent)?.sync_all()?;
        }
        Ok(())
    }
}
