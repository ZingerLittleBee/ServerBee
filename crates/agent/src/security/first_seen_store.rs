//! Persistent `(user, ip)` registry used to flag the first successful SSH
//! login from a new tuple. Capped at `cap`, LRU-evicted, atomic-write flushed.

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const SEP: char = '\u{0}';

pub struct FirstSeenStore {
    path: PathBuf,
    cap: usize,
    map: HashMap<String, i64>,
    /// Insertion order, oldest first. Used for LRU eviction (O(1) pop_front).
    order: VecDeque<String>,
    dirty: bool,
}

impl FirstSeenStore {
    /// Open (or create) a store at `path` with capacity `cap`.
    ///
    /// A missing file is treated as an empty store. A corrupt or unreadable
    /// file is logged and discarded — recovery semantics described above.
    pub fn open(path: PathBuf, cap: usize) -> Self {
        let (map, order) = match Self::load_from(&path) {
            Ok(pair) => pair,
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "first_seen store corrupted or unreadable; starting fresh"
                );
                (HashMap::new(), VecDeque::new())
            }
        };
        Self {
            path,
            cap,
            map,
            order,
            dirty: false,
        }
    }

    fn load_from(path: &Path) -> io::Result<(HashMap<String, i64>, VecDeque<String>)> {
        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Ok((HashMap::new(), VecDeque::new()));
            }
            Err(e) => return Err(e),
        };
        if bytes.is_empty() {
            return Ok((HashMap::new(), VecDeque::new()));
        }
        let raw: Persisted = serde_json::from_slice(&bytes)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let mut map = HashMap::with_capacity(raw.entries.len());
        let mut order = VecDeque::with_capacity(raw.entries.len());
        for (k, v) in raw.entries {
            order.push_back(k.clone());
            map.insert(k, v);
        }
        Ok((map, order))
    }

    /// Return true if `(user, ip)` has *not* been seen before; record the
    /// observation and (lazily) flush on next call to `flush`.
    pub fn mark(&mut self, user: &str, ip: &str, now_ts: i64) -> bool {
        let key = make_key(user, ip);
        if self.map.contains_key(&key) {
            return false;
        }
        self.map.insert(key.clone(), now_ts);
        self.order.push_back(key);
        while self.map.len() > self.cap {
            let Some(old) = self.order.pop_front() else {
                break;
            };
            self.map.remove(&old);
        }
        self.dirty = true;
        true
    }

    /// Persist current state. Uses an atomic temp-file + rename. No-op if
    /// nothing has changed since open or last flush.
    pub fn flush(&mut self) -> io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        if let Some(parent) = self.path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }
        let payload = Persisted {
            entries: self
                .order
                .iter()
                .filter_map(|k| self.map.get(k).map(|v| (k.clone(), *v)))
                .collect(),
        };
        let tmp = self.path.with_extension("tmp");
        let bytes = serde_json::to_vec(&payload).map_err(io::Error::other)?;
        fs::write(&tmp, &bytes)?;
        fs::rename(&tmp, &self.path)?;
        self.dirty = false;
        Ok(())
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.map.len()
    }
}

fn make_key(user: &str, ip: &str) -> String {
    let mut s = String::with_capacity(user.len() + ip.len() + 1);
    s.push_str(user);
    s.push(SEP);
    s.push_str(ip);
    s
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Persisted {
    /// `(key, ts)` pairs in insertion order — Vec preserves order across
    /// reloads so LRU eviction stays stable.
    entries: Vec<(String, i64)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn path_in(dir: &TempDir) -> PathBuf {
        dir.path().join("first_seen.json")
    }

    #[test]
    fn mark_first_returns_true_initially() {
        let dir = TempDir::new().unwrap();
        let mut store = FirstSeenStore::open(path_in(&dir), 100);
        assert!(store.mark("root", "1.2.3.4", 1000));
    }

    #[test]
    fn mark_first_returns_false_on_repeat() {
        let dir = TempDir::new().unwrap();
        let mut store = FirstSeenStore::open(path_in(&dir), 100);
        assert!(store.mark("root", "1.2.3.4", 1000));
        assert!(!store.mark("root", "1.2.3.4", 1001));
    }

    #[test]
    fn persists_across_reload() {
        let dir = TempDir::new().unwrap();
        let p = path_in(&dir);
        {
            let mut store = FirstSeenStore::open(p.clone(), 100);
            assert!(store.mark("root", "1.2.3.4", 1000));
            store.flush().unwrap();
        }
        let mut store = FirstSeenStore::open(p, 100);
        assert!(!store.mark("root", "1.2.3.4", 2000));
    }

    #[test]
    fn corrupted_file_resets_and_continues() {
        let dir = TempDir::new().unwrap();
        let p = path_in(&dir);
        fs::write(&p, b"not json {{{").unwrap();
        let mut store = FirstSeenStore::open(p, 100);
        // Empty after corruption, so first mark must be `true`.
        assert!(store.mark("root", "1.2.3.4", 1000));
        store.flush().unwrap();
    }

    #[test]
    fn lru_evicts_when_over_cap() {
        let dir = TempDir::new().unwrap();
        let mut store = FirstSeenStore::open(path_in(&dir), 10);
        for i in 0..12 {
            assert!(store.mark("root", &format!("10.0.0.{i}"), 1000 + i));
        }
        assert_eq!(store.len(), 10);
        // Oldest two should be gone — re-marking should now report `true`.
        assert!(store.mark("root", "10.0.0.0", 9000));
        assert!(store.mark("root", "10.0.0.1", 9001));
        // A very recent one should still report `false`.
        assert!(!store.mark("root", "10.0.0.11", 9999));
    }

    #[test]
    fn flush_is_noop_when_nothing_changed() {
        let dir = TempDir::new().unwrap();
        let mut store = FirstSeenStore::open(path_in(&dir), 100);
        store.flush().unwrap();
        store.flush().unwrap();
    }

    #[test]
    fn key_separator_avoids_collision() {
        let dir = TempDir::new().unwrap();
        let mut store = FirstSeenStore::open(path_in(&dir), 100);
        assert!(store.mark("ab", "c", 1));
        // "ab\x00c" must not collide with "a\x00bc".
        assert!(store.mark("a", "bc", 2));
    }
}
