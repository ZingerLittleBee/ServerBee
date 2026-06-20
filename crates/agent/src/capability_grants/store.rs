#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serverbee_common::constants::{CapabilityKey, CAP_VALID_MASK};
use serverbee_common::protocol::TemporaryGrant;

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GrantRecord {
    pub cap: String,
    pub granted_at: i64,
    pub expires_at: i64,
    pub granted_by: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Persisted {
    version: u32,
    grants: Vec<GrantRecord>,
}

/// Read-modify-write store for `capability_grants.json`. The CLI is the only
/// writer; the daemon loads it read-only. Keyed by cap, at most one per cap.
#[derive(Debug, Clone, Default)]
pub struct CapabilityGrantStore {
    path: PathBuf,
    records: BTreeMap<String, GrantRecord>,
}

impl CapabilityGrantStore {
    /// Load (or start empty). Corrupt / unknown-version / missing → empty
    /// (fail-safe: no temporary grants).
    pub fn load(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let records = match Self::load_from(&path) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e,
                    "capability grants file corrupt/unreadable; treating as empty");
                BTreeMap::new()
            }
        };
        Self { path, records }
    }

    fn load_from(path: &Path) -> io::Result<BTreeMap<String, GrantRecord>> {
        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
            Err(e) => return Err(e),
        };
        if bytes.is_empty() {
            return Ok(BTreeMap::new());
        }
        let parsed: Persisted = serde_json::from_slice(&bytes)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        if parsed.version != SCHEMA_VERSION {
            tracing::warn!(version = parsed.version,
                "unknown capability grants schema version; treating as empty");
            return Ok(BTreeMap::new());
        }
        Ok(parsed
            .grants
            .into_iter()
            .map(|r| (r.cap.clone(), r))
            .collect())
    }

    pub fn records(&self) -> impl Iterator<Item = &GrantRecord> {
        self.records.values()
    }

    /// OR of bits for caps with an active grant (`expires_at > now`) that are
    /// currently OFF in `base`. A grant can only turn ON an off bit.
    pub fn active_bits(&self, now: i64, base: u32) -> u32 {
        let mut bits = 0u32;
        for rec in self.records.values() {
            if rec.expires_at <= now {
                continue;
            }
            if let Ok(key) = rec.cap.parse::<CapabilityKey>() {
                let bit = key.to_bit();
                if base & bit == 0 {
                    bits |= bit;
                }
            }
        }
        bits & CAP_VALID_MASK
    }

    /// Active grants as protocol DTOs (sorted by cap for stable output). Only
    /// grants that actually turn a cap ON — i.e. the cap is a known key and is
    /// OFF in `base` — are returned. A grant for a cap already permanently
    /// enabled in `base` is a no-op (e.g. the cap was enabled via the daemon's
    /// `--allow-cap` flag), so it is NOT reported as temporary, keeping the
    /// reported state aligned with `active_bits`/`effective`.
    pub fn active_grants(&self, now: i64, base: u32) -> Vec<TemporaryGrant> {
        let mut out: Vec<TemporaryGrant> = self
            .records
            .values()
            .filter(|r| r.expires_at > now)
            .filter(|r| {
                r.cap
                    .parse::<CapabilityKey>()
                    .map(|key| base & key.to_bit() == 0)
                    .unwrap_or(false)
            })
            .map(|r| TemporaryGrant {
                cap: r.cap.clone(),
                granted_at: r.granted_at,
                expires_at: r.expires_at,
            })
            .collect();
        out.sort_by(|a, b| a.cap.cmp(&b.cap));
        out
    }

    /// Insert/replace a grant and prune expired records (CLI writer path).
    pub fn upsert(&mut self, record: GrantRecord, now: i64) {
        self.prune_expired(now);
        self.records.insert(record.cap.clone(), record);
    }

    /// Remove a cap's grant; returns whether a record existed. Prunes expired.
    pub fn remove(&mut self, cap: &str, now: i64) -> bool {
        let existed = self.records.remove(cap).is_some();
        self.prune_expired(now);
        existed
    }

    fn prune_expired(&mut self, now: i64) {
        self.records.retain(|_, r| r.expires_at > now);
    }

    /// Atomic write: temp file → rename, mode 0600.
    pub fn flush(&self) -> io::Result<()> {
        if let Some(parent) = self.path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }
        let payload = Persisted {
            version: SCHEMA_VERSION,
            grants: self.records.values().cloned().collect(),
        };
        let bytes = serde_json::to_vec_pretty(&payload).map_err(io::Error::other)?;
        let tmp = self.path.with_extension("tmp");
        fs::write(&tmp, &bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
        }
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serverbee_common::constants::{CAP_DEFAULT, CAP_TERMINAL};

    fn rec(cap: &str, expires_at: i64) -> GrantRecord {
        GrantRecord {
            cap: cap.into(),
            granted_at: 0,
            expires_at,
            granted_by: "root".into(),
            reason: None,
        }
    }

    #[test]
    fn missing_file_is_empty() {
        let store = CapabilityGrantStore::load("/nonexistent/dir/grants.json");
        assert_eq!(store.active_bits(0, CAP_DEFAULT), 0);
    }

    #[test]
    fn active_bits_only_turns_on_off_caps() {
        let dir = std::env::temp_dir().join(format!("sbtest-grants-{}", std::process::id()));
        let path = dir.join("grants.json");
        let mut store = CapabilityGrantStore::load(&path);
        store.upsert(rec("terminal", 1000), 0);
        store.flush().unwrap();

        let reloaded = CapabilityGrantStore::load(&path);
        assert_eq!(reloaded.active_bits(0, CAP_DEFAULT), CAP_TERMINAL);
        assert_eq!(reloaded.active_bits(0, CAP_DEFAULT | CAP_TERMINAL), 0);
        assert_eq!(reloaded.active_bits(2000, CAP_DEFAULT), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn upsert_replaces_and_prune_drops_expired() {
        let mut store = CapabilityGrantStore::default();
        store.upsert(rec("terminal", 100), 0);
        store.upsert(rec("terminal", 500), 0);
        store.upsert(rec("file", 50), 0);
        store.remove("nonexistent", 200);
        let active: Vec<_> = store.active_grants(200, CAP_DEFAULT);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].cap, "terminal");
        assert_eq!(active[0].expires_at, 500);
    }

    #[test]
    fn active_grants_skips_caps_already_on_in_base() {
        // A grant for a cap already permanently enabled in `base` (e.g. enabled
        // via the daemon's --allow-cap flag) is a no-op and must NOT be reported
        // as temporary, or the UI would show a misleading countdown for a cap
        // that never turns off.
        let mut store = CapabilityGrantStore::default();
        store.upsert(rec("terminal", 1000), 0);
        assert_eq!(store.active_grants(0, CAP_DEFAULT).len(), 1);
        assert_eq!(store.active_grants(0, CAP_DEFAULT | CAP_TERMINAL).len(), 0);
    }

    #[test]
    fn corrupt_file_is_empty() {
        let dir = std::env::temp_dir().join(format!("sbtest-corrupt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("grants.json");
        std::fs::write(&path, b"{ not json").unwrap();
        let store = CapabilityGrantStore::load(&path);
        assert_eq!(store.active_grants(0, CAP_DEFAULT).len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
