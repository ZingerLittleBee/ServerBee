# Temporary Capability Grants Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a host operator temporarily enable an otherwise-off agent capability for a bounded duration via a local CLI; it auto-expires, survives agent restarts, opens the server's control-plane gates while active, shows a live countdown in the read-only web UI, and is audited + alertable.

**Architecture:** The grants file `capability_grants.json` (absolute `expires_at`) is the single source of truth; the in-memory `Arc<AtomicU32>` effective-caps cache is derived. The **CLI subcommands are the only writer**; the daemon (startup + a ~3s supervisor task) is read-only. The agent reports `effective = base | active_grants` plus `temporary[]` metadata to the server via a new `AgentMessage::CapabilitiesChanged`; the server mirrors it for display, gates on it live, audits each change, and alerts on high-risk grants. The server cannot create, push, or modify grants.

**Tech Stack:** Rust (agent + Axum/sea-orm server + `serverbee-common`), React 19 / TanStack (web), Fumadocs MDX (docs). Spec: `docs/superpowers/specs/2026-06-20-temporary-capability-grants-design.md`.

---

## File Structure

**Create:**
- `crates/agent/src/capability_grants/mod.rs` — module root + `parse_duration_secs`
- `crates/agent/src/capability_grants/store.rs` — `CapabilityGrantStore`, `GrantRecord`
- `crates/agent/src/capability_grants/cli.rs` — `run_grant` / `run_revoke` / `run_list`
- `crates/agent/src/capability_grants/supervisor.rs` — `evaluate` + `run_grant_supervisor`
- `apps/web/src/hooks/use-countdown.ts` — reusable live countdown hook
- `apps/web/src/lib/capabilities.test.ts` — vitest for the classify helper

**Modify:**
- `crates/common/src/protocol.rs` — new DTOs + `AgentMessage::CapabilitiesChanged` + `SystemInfo` variant field + `BrowserMessage::CapabilitiesChanged` field
- `crates/common/src/constants.rs` — `PROTOCOL_VERSION` 5→6
- `crates/agent/src/config.rs` — `CapabilitiesConfig.temporary_max_duration` + `.state_dir` + helpers
- `crates/agent/src/capability_policy.rs` — test struct-literal fixups
- `crates/agent/src/main.rs` — subcommand dispatch + `flag_value` helper + module decl
- `crates/agent/src/reporter.rs` — fold grants into effective caps before SystemInfo, send `temporary`, spawn supervisor, select-loop arm
- `crates/server/src/service/agent_manager.rs` — `temporary_grants` DashMap + accessors
- `crates/server/src/router/ws/agent.rs` — handle `CapabilitiesChanged`, extend SystemInfo arm, clear on disconnect
- `crates/server/src/service/alert.rs` — register `capability_grant_detected`
- `crates/server/src/router/api/server.rs` — `temporary` on the server REST DTO
- `apps/web/src/lib/capabilities.ts` — `classifyCapability` + types
- `apps/web/src/hooks/use-servers-ws.ts` — `temporary` plumbing
- `apps/web/src/components/server/capabilities-dialog.tsx` + `apps/web/src/routes/_authed/settings/capabilities.tsx` — amber Temporary badge + countdown
- `apps/web/src/components/security/alert-presets.tsx` + alert rule editor + i18n — `capability_grant_detected`
- `apps/web/openapi.json` + `apps/web/src/lib/api-types.ts` — REST DTO `temporary`
- docs: `capabilities.mdx`, `configuration.mdx`, `admin.mdx`, `security.mdx`/`alerts.mdx` (en+zh), `ENV.md`, `CHANGELOG.md`, `tests/`

---

# Phase 1 — Protocol (`serverbee-common`)

### Task 1: Add capability-grant DTOs and protocol variants

**Files:**
- Modify: `crates/common/src/protocol.rs`
- Modify: `crates/common/src/constants.rs:4` and the `protocol_version` test (`:240-243`)

- [ ] **Step 1: Bump the protocol version test (failing)** — edit `crates/common/src/constants.rs`:

```rust
pub const PROTOCOL_VERSION: u32 = 6;
```

and update the test:

```rust
#[cfg(test)]
#[test]
fn protocol_version() {
    assert_eq!(PROTOCOL_VERSION, 6);
}
```

- [ ] **Step 2: Add the three DTOs** near the other protocol types in `crates/common/src/protocol.rs` (top-level, after the `use` block):

```rust
/// A capability that is temporarily enabled on the agent host until `expires_at`.
/// Reported by the agent for UI countdown; the agent host is the only authority.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TemporaryGrant {
    pub cap: String,
    pub granted_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityChangeAction {
    Granted,
    Expired,
    Revoked,
}

/// A single transition emitted by the agent's grant supervisor, used by the
/// server for audit + alerting. `expires_at`/`granted_by`/`reason` are present
/// only for `granted`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityChangeEvent {
    pub cap: String,
    pub action: CapabilityChangeAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub granted_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}
```

- [ ] **Step 3: Add the `AgentMessage::CapabilitiesChanged` variant** and extend `AgentMessage::SystemInfo`. In the `AgentMessage` enum, change the `SystemInfo` variant to add a `temporary` field, and add a new variant (place it right after `SystemInfo`):

```rust
    SystemInfo {
        msg_id: String,
        #[serde(flatten)]
        info: SystemInfo,
        #[serde(default)]
        agent_local_capabilities: Option<u32>,
        #[serde(default)]
        temporary: Vec<TemporaryGrant>,
    },
    CapabilitiesChanged {
        msg_id: String,
        capabilities: u32,
        #[serde(default)]
        temporary: Vec<TemporaryGrant>,
        #[serde(default)]
        changes: Vec<CapabilityChangeEvent>,
    },
```

- [ ] **Step 4: Extend `BrowserMessage::CapabilitiesChanged`** with a `temporary` field:

```rust
    CapabilitiesChanged {
        server_id: String,
        capabilities: u32,
        agent_local_capabilities: Option<u32>,
        effective_capabilities: Option<u32>,
        #[serde(default)]
        temporary: Vec<TemporaryGrant>,
    },
```

- [ ] **Step 5: Add a round-trip test** at the bottom of `crates/common/src/protocol.rs` (in or after the existing tests module; if none, add `#[cfg(test)] mod grant_tests { use super::*; ... }`):

```rust
#[cfg(test)]
mod capability_grant_protocol_tests {
    use super::*;

    #[test]
    fn capabilities_changed_round_trips_with_snake_case_tag() {
        let msg = AgentMessage::CapabilitiesChanged {
            msg_id: "m1".into(),
            capabilities: 1 | 1852,
            temporary: vec![TemporaryGrant { cap: "terminal".into(), granted_at: 10, expires_at: 1810 }],
            changes: vec![CapabilityChangeEvent {
                cap: "terminal".into(),
                action: CapabilityChangeAction::Granted,
                expires_at: Some(1810),
                granted_by: Some("root".into()),
                reason: Some("debug".into()),
            }],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"capabilities_changed\""));
        assert!(json.contains("\"action\":\"granted\""));
        let back: AgentMessage = serde_json::from_str(&json).unwrap();
        match back {
            AgentMessage::CapabilitiesChanged { capabilities, temporary, changes, .. } => {
                assert_eq!(capabilities, 1 | 1852);
                assert_eq!(temporary.len(), 1);
                assert_eq!(changes[0].action, CapabilityChangeAction::Granted);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn system_info_temporary_defaults_to_empty_when_absent() {
        // Old agents (protocol 5) send SystemInfo without `temporary`.
        let json = r#"{"type":"system_info","msg_id":"x","cpu_name":"c","cpu_cores":1,"cpu_arch":"a","os":"o","kernel_version":"k","mem_total":1,"swap_total":0,"disk_total":1,"ipv4":null,"ipv6":null,"virtualization":null,"agent_version":"1","protocol_version":5,"features":[]}"#;
        let msg: AgentMessage = serde_json::from_str(json).unwrap();
        match msg {
            AgentMessage::SystemInfo { temporary, .. } => assert!(temporary.is_empty()),
            _ => panic!("wrong variant"),
        }
    }
}
```

- [ ] **Step 6: Run + commit**

Run: `cargo test -p serverbee-common`
Expected: PASS (including `protocol_version`).

```bash
git add crates/common/src/protocol.rs crates/common/src/constants.rs
git commit -m "feat(common): add temporary capability grant protocol (CapabilitiesChanged, PROTOCOL_VERSION 6)"
```

---

# Phase 2 — Agent core (store, duration, config)

### Task 2: `CapabilityGrantStore` + `GrantRecord`

**Files:**
- Create: `crates/agent/src/capability_grants/mod.rs`
- Create: `crates/agent/src/capability_grants/store.rs`
- Modify: `crates/agent/src/main.rs` (add `mod capability_grants;`)

- [ ] **Step 1: Declare the module.** In `crates/agent/src/main.rs`, add alongside the other `mod` declarations:

```rust
mod capability_grants;
```

- [ ] **Step 2: Create `crates/agent/src/capability_grants/mod.rs`:**

```rust
pub mod cli;
pub mod store;
pub mod supervisor;

pub use store::{CapabilityGrantStore, GrantRecord};

/// Parse a human duration (`90s`, `30m`, `2h`, `1d`) into seconds. Must be a
/// positive integer followed by a single unit char. Footgun-guard only.
pub fn parse_duration_secs(input: &str) -> anyhow::Result<i64> {
    let trimmed = input.trim();
    if trimmed.len() < 2 {
        anyhow::bail!("invalid duration '{input}': expected <number><s|m|h|d>, e.g. 30m");
    }
    let (num, unit) = trimmed.split_at(trimmed.len() - 1);
    let value: i64 = num
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid duration '{input}': '{num}' is not an integer"))?;
    if value <= 0 {
        anyhow::bail!("invalid duration '{input}': must be positive");
    }
    let secs = match unit {
        "s" => value,
        "m" => value.checked_mul(60),
        "h" => value.checked_mul(3600),
        "d" => value.checked_mul(86_400),
        other => anyhow::bail!("invalid duration unit '{other}': expected s, m, h, or d"),
    }
    .ok_or_else(|| anyhow::anyhow!("duration '{input}' overflows"))?;
    Ok(secs)
}

#[cfg(test)]
mod duration_tests {
    use super::parse_duration_secs;

    #[test]
    fn parses_each_unit() {
        assert_eq!(parse_duration_secs("90s").unwrap(), 90);
        assert_eq!(parse_duration_secs("30m").unwrap(), 1800);
        assert_eq!(parse_duration_secs("2h").unwrap(), 7200);
        assert_eq!(parse_duration_secs("1d").unwrap(), 86_400);
    }

    #[test]
    fn rejects_bad_input() {
        assert!(parse_duration_secs("").is_err());
        assert!(parse_duration_secs("h").is_err());
        assert!(parse_duration_secs("0m").is_err());
        assert!(parse_duration_secs("-5m").is_err());
        assert!(parse_duration_secs("10y").is_err());
        assert!(parse_duration_secs("abcm").is_err());
    }
}
```

Note: `value.checked_mul(60)` returns `Option`; the `match` arms for `s` returns `i64` while others return `Option<i64>` — fix by wrapping `"s" => Some(value),`. Use this corrected arm:

```rust
    let secs = match unit {
        "s" => Some(value),
        "m" => value.checked_mul(60),
        "h" => value.checked_mul(3600),
        "d" => value.checked_mul(86_400),
        other => anyhow::bail!("invalid duration unit '{other}': expected s, m, h, or d"),
    }
    .ok_or_else(|| anyhow::anyhow!("duration '{input}' overflows"))?;
```

- [ ] **Step 3: Create `crates/agent/src/capability_grants/store.rs`** (mirrors `FirstSeenStore`'s atomic-write idiom):

```rust
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

    /// Active grants as protocol DTOs (sorted by cap for stable output).
    pub fn active_grants(&self, now: i64) -> Vec<TemporaryGrant> {
        let mut out: Vec<TemporaryGrant> = self
            .records
            .values()
            .filter(|r| r.expires_at > now)
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
        // terminal is off in CAP_DEFAULT → bit appears
        assert_eq!(reloaded.active_bits(0, CAP_DEFAULT), CAP_TERMINAL);
        // already-on caps are never re-added: base already has terminal → 0
        assert_eq!(reloaded.active_bits(0, CAP_DEFAULT | CAP_TERMINAL), 0);
        // expired → ignored
        assert_eq!(reloaded.active_bits(2000, CAP_DEFAULT), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn upsert_replaces_and_prune_drops_expired() {
        let mut store = CapabilityGrantStore::default();
        store.upsert(rec("terminal", 100), 0);
        store.upsert(rec("terminal", 500), 0); // replace
        store.upsert(rec("file", 50), 0);
        // now=200 prunes the file (expired) on next write
        store.remove("nonexistent", 200);
        let active: Vec<_> = store.active_grants(200);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].cap, "terminal");
        assert_eq!(active[0].expires_at, 500);
    }

    #[test]
    fn corrupt_file_is_empty() {
        let dir = std::env::temp_dir().join(format!("sbtest-corrupt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("grants.json");
        std::fs::write(&path, b"{ not json").unwrap();
        let store = CapabilityGrantStore::load(&path);
        assert_eq!(store.active_grants(0).len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

- [ ] **Step 4: Run + commit**

Run: `cargo test -p serverbee-agent capability_grants::`
Expected: PASS (store + duration tests). The `supervisor` and `cli` modules don't exist yet — create empty stubs so the crate compiles: in this commit add `crates/agent/src/capability_grants/cli.rs` and `supervisor.rs` containing only `// filled in later` is NOT enough (the `mod.rs` `pub use`/`pub mod` references symbols). Instead, for this step temporarily comment out `pub mod cli;` and `pub mod supervisor;` in `mod.rs`, run the test, then restore them in Task 3/Task 6. (Simplest: create the three files in order — do Task 5/6/8 before running the full crate build. If running incrementally, gate the `pub mod` lines.)

```bash
git add crates/agent/src/capability_grants/ crates/agent/src/main.rs
git commit -m "feat(agent): add capability grants store and duration parser"
```

---

### Task 3: Config fields `temporary_max_duration` + `state_dir`

**Files:**
- Modify: `crates/agent/src/config.rs:38-44` (`CapabilitiesConfig`)
- Modify: `crates/agent/src/capability_policy.rs` (test struct literals at `:192`, `:206`, `:220`)

- [ ] **Step 1: Replace `CapabilitiesConfig`** in `crates/agent/src/config.rs`. Remove `Default` from the derive list and add a manual impl + helpers:

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CapabilitiesConfig {
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
    /// Footgun guard: max `--for` the grant CLI accepts. Not a security
    /// boundary (host root can edit the file directly). Default `24h`.
    #[serde(default = "default_temporary_max_duration")]
    pub temporary_max_duration: String,
    /// Directory holding `capability_grants.json`.
    #[serde(default = "default_capability_state_dir")]
    pub state_dir: String,
}

impl Default for CapabilitiesConfig {
    fn default() -> Self {
        Self {
            allow: Vec::new(),
            deny: Vec::new(),
            temporary_max_duration: default_temporary_max_duration(),
            state_dir: default_capability_state_dir(),
        }
    }
}

fn default_temporary_max_duration() -> String {
    "24h".to_string()
}

fn default_capability_state_dir() -> String {
    "/var/lib/serverbee".to_string()
}

impl CapabilitiesConfig {
    pub fn grants_path(&self) -> std::path::PathBuf {
        std::path::Path::new(&self.state_dir).join("capability_grants.json")
    }

    pub fn temporary_max_duration_secs(&self) -> anyhow::Result<i64> {
        crate::capability_grants::parse_duration_secs(&self.temporary_max_duration)
    }
}
```

- [ ] **Step 2: Fix the three test struct literals** in `crates/agent/src/capability_policy.rs` that construct `CapabilitiesConfig { allow, deny }` (around lines 192, 206, 220). Append `..Default::default()`:

```rust
        let config = CapabilitiesConfig {
            allow: vec!["terminal".to_string(), "file".to_string()],
            deny: vec!["ip_quality".to_string()],
            ..Default::default()
        };
```

```rust
        let config = CapabilitiesConfig {
            allow: vec!["docker".to_string()],
            deny: vec![],
            ..Default::default()
        };
```

```rust
        let config = CapabilitiesConfig {
            allow: vec!["definitely_not_a_cap".to_string()],
            deny: vec![],
            ..Default::default()
        };
```

- [ ] **Step 3: Add a config test** at the bottom of `crates/agent/src/config.rs` (in its `#[cfg(test)]` module, or add one):

```rust
#[cfg(test)]
mod capabilities_config_tests {
    use super::CapabilitiesConfig;

    #[test]
    fn defaults_resolve_grants_path_and_max_duration() {
        let c = CapabilitiesConfig::default();
        assert_eq!(
            c.grants_path(),
            std::path::Path::new("/var/lib/serverbee/capability_grants.json")
        );
        assert_eq!(c.temporary_max_duration_secs().unwrap(), 86_400);
    }
}
```

- [ ] **Step 4: Run + commit**

Run: `cargo test -p serverbee-agent config:: && cargo test -p serverbee-agent capability_policy::`
Expected: PASS.

```bash
git add crates/agent/src/config.rs crates/agent/src/capability_policy.rs
git commit -m "feat(agent): add capability grants config (state_dir, temporary_max_duration)"
```

---

# Phase 3 — Agent CLI

### Task 4: `run_grant` / `run_revoke` / `run_list`

**Files:**
- Create/replace: `crates/agent/src/capability_grants/cli.rs`

- [ ] **Step 1: Write `crates/agent/src/capability_grants/cli.rs`:**

```rust
use anyhow::{bail, Context};

use serverbee_common::constants::CapabilityKey;

use super::store::{CapabilityGrantStore, GrantRecord};
use super::parse_duration_secs;
use crate::config::AgentConfig;

pub struct GrantArgs {
    pub cap: String,
    pub for_duration: String,
    pub reason: Option<String>,
}

/// Temporarily enable `cap`. `base` is the agent's permanent (config-computed)
/// capability set; granting an already-on cap is rejected.
pub fn run_grant(
    config: &AgentConfig,
    base: u32,
    args: &GrantArgs,
    now: i64,
    granted_by: String,
) -> anyhow::Result<String> {
    let key: CapabilityKey = args.cap.parse().map_err(|e: String| anyhow::anyhow!(e))?;
    if base & key.to_bit() != 0 {
        bail!("'{}' is already enabled in agent.toml; nothing to grant", key.as_str());
    }
    let dur = parse_duration_secs(&args.for_duration)?;
    let max = config.capabilities.temporary_max_duration_secs()?;
    if dur > max {
        bail!(
            "duration '{}' exceeds temporary_max_duration ('{}'); refusing",
            args.for_duration,
            config.capabilities.temporary_max_duration
        );
    }
    let path = config.capabilities.grants_path();
    let mut store = CapabilityGrantStore::load(&path);
    store.upsert(
        GrantRecord {
            cap: key.as_str().to_string(),
            granted_at: now,
            expires_at: now + dur,
            granted_by,
            reason: args.reason.clone(),
        },
        now,
    );
    store
        .flush()
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(format!(
        "Granted '{}' for {} (expires_at epoch {}). The running agent applies it within a few seconds.",
        key.as_str(),
        args.for_duration,
        now + dur
    ))
}

pub fn run_revoke(config: &AgentConfig, cap: &str, now: i64) -> anyhow::Result<String> {
    let key: CapabilityKey = cap.parse().map_err(|e: String| anyhow::anyhow!(e))?;
    let path = config.capabilities.grants_path();
    let mut store = CapabilityGrantStore::load(&path);
    let existed = store.remove(key.as_str(), now);
    store
        .flush()
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(if existed {
        format!("Revoked temporary grant for '{}'.", key.as_str())
    } else {
        format!("No active temporary grant for '{}'.", key.as_str())
    })
}

pub fn run_list(config: &AgentConfig, now: i64) -> anyhow::Result<String> {
    let store = CapabilityGrantStore::load(config.capabilities.grants_path());
    let mut lines: Vec<String> = store
        .records()
        .filter(|r| r.expires_at > now)
        .map(|r| {
            let reason = r
                .reason
                .as_deref()
                .map(|s| format!("  ({s})"))
                .unwrap_or_default();
            format!(
                "{:<16} expires in {:>7}s  by {}{}",
                r.cap,
                r.expires_at - now,
                r.granted_by,
                reason
            )
        })
        .collect();
    lines.sort();
    Ok(if lines.is_empty() {
        "No active temporary capability grants.".to_string()
    } else {
        lines.join("\n")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serverbee_common::constants::{CAP_DEFAULT, CAP_TERMINAL};

    fn config_with(dir: &std::path::Path) -> AgentConfig {
        let mut c = AgentConfig::default();
        c.capabilities.state_dir = dir.to_string_lossy().to_string();
        c.capabilities.temporary_max_duration = "24h".to_string();
        c
    }

    #[test]
    fn grant_then_revoke_round_trip() {
        let dir = std::env::temp_dir().join(format!("sbtest-cli-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let config = config_with(&dir);

        let out = run_grant(
            &config,
            CAP_DEFAULT,
            &GrantArgs { cap: "terminal".into(), for_duration: "30m".into(), reason: Some("x".into()) },
            1000,
            "root".into(),
        )
        .unwrap();
        assert!(out.contains("Granted 'terminal'"));

        let store = CapabilityGrantStore::load(config.capabilities.grants_path());
        assert_eq!(store.active_bits(1000, CAP_DEFAULT), CAP_TERMINAL);

        let out = run_revoke(&config, "terminal", 1001).unwrap();
        assert!(out.contains("Revoked"));
        let store = CapabilityGrantStore::load(config.capabilities.grants_path());
        assert_eq!(store.active_bits(1001, CAP_DEFAULT), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn grant_rejects_already_on_cap() {
        let dir = std::env::temp_dir().join(format!("sbtest-cli2-{}", std::process::id()));
        let config = config_with(&dir);
        let err = run_grant(
            &config,
            CAP_DEFAULT | CAP_TERMINAL,
            &GrantArgs { cap: "terminal".into(), for_duration: "30m".into(), reason: None },
            0,
            "root".into(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("already enabled"));
    }

    #[test]
    fn grant_rejects_over_max_duration() {
        let dir = std::env::temp_dir().join(format!("sbtest-cli3-{}", std::process::id()));
        let config = config_with(&dir);
        let err = run_grant(
            &config,
            CAP_DEFAULT,
            &GrantArgs { cap: "terminal".into(), for_duration: "2d".into(), reason: None },
            0,
            "root".into(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("exceeds temporary_max_duration"));
    }
}
```

Note: `AgentConfig::default()` must exist. If `AgentConfig` does not derive `Default`, the test should build it via `AgentConfig::load()` is wrong (reads real files). Instead, check whether `AgentConfig` derives `Default`; if not, add `#[derive(Default)]` to `AgentConfig` (all its fields are `#[serde(default)]` with their own defaults, except `server_url: String` which defaults to `""` — acceptable for tests). Add the derive in this task if missing.

- [ ] **Step 2: Run + commit**

Run: `cargo test -p serverbee-agent capability_grants::cli`
Expected: PASS.

```bash
git add crates/agent/src/capability_grants/cli.rs crates/agent/src/config.rs
git commit -m "feat(agent): add grant/revoke/list capability CLI handlers"
```

---

### Task 5: `main.rs` subcommand dispatch

**Files:**
- Modify: `crates/agent/src/main.rs` (after `AgentConfig::load()` at `:51`)

- [ ] **Step 1: Add a `flag_value` helper** near the top of `main.rs` (module scope):

```rust
/// Return the token following `flag` in `argv`, if present (`--reason foo` → `foo`).
fn flag_value(argv: &[String], flag: &str) -> Option<String> {
    argv.iter().position(|a| a == flag).and_then(|i| argv.get(i + 1).cloned())
}
```

- [ ] **Step 2: Insert the dispatch block** immediately after the `AgentConfig::load()` call (after `crates/agent/src/main.rs:55`, before `parse_capability_args`):

```rust
    // Host-local capability grant subcommands. One-shot: write the grants file
    // and exit; the running daemon picks the change up within a few seconds.
    let argv: Vec<String> = std::env::args().collect();
    if let Some(sub) = argv.get(1).map(String::as_str)
        && matches!(sub, "grant" | "revoke" | "grants")
    {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let base = compute_local_capabilities(
            &config.capabilities,
            &crate::capability_policy::CapabilityCliOverrides {
                allow_caps: vec![],
                deny_caps: vec![],
            },
        )?;
        let granted_by = std::env::var("SUDO_USER")
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_else(|_| "unknown".to_string());

        let result = match sub {
            "grant" => match (argv.get(2).cloned(), flag_value(&argv, "--for")) {
                (Some(cap), Some(for_duration)) => crate::capability_grants::cli::run_grant(
                    &config,
                    base,
                    &crate::capability_grants::cli::GrantArgs {
                        cap,
                        for_duration,
                        reason: flag_value(&argv, "--reason"),
                    },
                    now,
                    granted_by,
                ),
                _ => Err(anyhow::anyhow!(
                    "usage: serverbee-agent grant <cap> --for <30m|2h|1d> [--reason \"...\"]"
                )),
            },
            "revoke" => match argv.get(2) {
                Some(cap) => crate::capability_grants::cli::run_revoke(&config, cap, now),
                None => Err(anyhow::anyhow!("usage: serverbee-agent revoke <cap>")),
            },
            "grants" => crate::capability_grants::cli::run_list(&config, now),
            _ => unreachable!(),
        };

        match result {
            Ok(msg) => {
                println!("{msg}");
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
    }
```

Ensure `CapabilityCliOverrides` is reachable (it's `pub` in `capability_policy.rs`). If `compute_local_capabilities` / `parse_capability_args` are imported with `use` at the top of `main.rs`, also `use crate::capability_policy::CapabilityCliOverrides;` or reference the full path as above.

- [ ] **Step 3: Build + manual smoke + commit**

Run: `cargo build -p serverbee-agent`
Expected: compiles.

Manual smoke (uses a temp state dir so it doesn't touch `/var/lib`):

```bash
SERVERBEE_CAPABILITIES__STATE_DIR=/tmp/sb-grants \
  cargo run -p serverbee-agent -- grant terminal --for 30m --reason demo
SERVERBEE_CAPABILITIES__STATE_DIR=/tmp/sb-grants \
  cargo run -p serverbee-agent -- grants
SERVERBEE_CAPABILITIES__STATE_DIR=/tmp/sb-grants \
  cargo run -p serverbee-agent -- revoke terminal
```

Expected: "Granted 'terminal'…", then a listing, then "Revoked…". (The agent will print a config-load error first if no `agent.toml`/`server_url`; set `SERVERBEE_SERVER_URL=ws://x` to satisfy load, or run where `agent.toml` exists.)

```bash
git add crates/agent/src/main.rs
git commit -m "feat(agent): wire grant/revoke/grants subcommands into the CLI"
```

---

# Phase 4 — Agent daemon (supervisor + reporter wiring)

### Task 6: Grant supervisor (`evaluate` + task loop)

**Files:**
- Create/replace: `crates/agent/src/capability_grants/supervisor.rs`

- [ ] **Step 1: Write `crates/agent/src/capability_grants/supervisor.rs`:**

```rust
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use serverbee_common::constants::{ALL_CAPABILITIES, CAP_DOCKER, CAP_EXEC, CAP_FILE, CAP_TERMINAL, CAP_VALID_MASK};
use serverbee_common::protocol::{
    AgentMessage, CapabilityChangeAction, CapabilityChangeEvent, TemporaryGrant,
};

use super::store::CapabilityGrantStore;

/// Caps that warrant an alert when temporarily granted.
pub const HIGH_RISK_BITS: u32 = CAP_TERMINAL | CAP_EXEC | CAP_FILE | CAP_DOCKER;

/// Pure: given the previous active-grant bits and a freshly-loaded store,
/// compute new effective caps, new active bits, the active-grant DTOs, and the
/// change events to emit.
pub fn evaluate(
    store: &CapabilityGrantStore,
    base: u32,
    prev_active_bits: u32,
    now: i64,
) -> (u32, u32, Vec<TemporaryGrant>, Vec<CapabilityChangeEvent>) {
    let active_bits = store.active_bits(now, base);
    let effective = (base | active_bits) & CAP_VALID_MASK;
    let temporary = store.active_grants(now);

    let granted = active_bits & !prev_active_bits;
    let removed = prev_active_bits & !active_bits;
    let mut changes = Vec::new();

    for meta in ALL_CAPABILITIES {
        if granted & meta.bit != 0 {
            let rec = store.records().find(|r| r.cap == meta.key);
            changes.push(CapabilityChangeEvent {
                cap: meta.key.to_string(),
                action: CapabilityChangeAction::Granted,
                expires_at: rec.map(|r| r.expires_at),
                granted_by: rec.map(|r| r.granted_by.clone()),
                reason: rec.and_then(|r| r.reason.clone()),
            });
        }
        if removed & meta.bit != 0 {
            // A still-present record means time elapsed (expired); a gone
            // record means the operator revoked it.
            let still_present = store.records().any(|r| r.cap == meta.key);
            changes.push(CapabilityChangeEvent {
                cap: meta.key.to_string(),
                action: if still_present {
                    CapabilityChangeAction::Expired
                } else {
                    CapabilityChangeAction::Revoked
                },
                expires_at: None,
                granted_by: None,
                reason: None,
            });
        }
    }
    (effective, active_bits, temporary, changes)
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Long-running per-connection task: re-reads the grants file, updates the
/// shared effective-caps cache, and emits `CapabilitiesChanged` on transitions.
/// Read-only on the file (the CLI is the only writer). Stops when `tx` closes
/// (i.e. the connection ended).
pub async fn run_grant_supervisor(
    grants_path: PathBuf,
    base: u32,
    capabilities: Arc<AtomicU32>,
    tx: mpsc::Sender<AgentMessage>,
    tick: Duration,
) {
    // Seed prev_active from the current file so grants already active at connect
    // time are NOT re-announced as new (avoids alert spam on every reconnect).
    let mut prev_active = CapabilityGrantStore::load(&grants_path).active_bits(now_unix(), base);
    let mut interval = tokio::time::interval(tick);
    interval.tick().await; // consume the immediate first tick

    loop {
        interval.tick().await;
        let now = now_unix();
        let store = CapabilityGrantStore::load(&grants_path);
        let (effective, active_bits, temporary, changes) =
            evaluate(&store, base, prev_active, now);

        if effective != capabilities.load(Ordering::SeqCst) {
            capabilities.store(effective, Ordering::SeqCst);
            let msg = AgentMessage::CapabilitiesChanged {
                msg_id: uuid::Uuid::new_v4().to_string(),
                capabilities: effective,
                temporary,
                changes,
            };
            if tx.send(msg).await.is_err() {
                tracing::debug!("grant supervisor channel closed; stopping");
                break;
            }
            tracing::info!(effective, "capability grant state changed");
        }
        prev_active = active_bits;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serverbee_common::constants::{CAP_DEFAULT, CAP_TERMINAL};
    use crate::capability_grants::store::GrantRecord;

    fn store_with(cap: &str, expires_at: i64) -> CapabilityGrantStore {
        let mut s = CapabilityGrantStore::default();
        s.upsert(
            GrantRecord {
                cap: cap.into(),
                granted_at: 0,
                expires_at,
                granted_by: "root".into(),
                reason: None,
            },
            0,
        );
        s
    }

    #[test]
    fn newly_active_emits_granted() {
        let store = store_with("terminal", 1000);
        let (eff, active, temp, changes) = evaluate(&store, CAP_DEFAULT, 0, 0);
        assert_eq!(eff, CAP_DEFAULT | CAP_TERMINAL);
        assert_eq!(active, CAP_TERMINAL);
        assert_eq!(temp.len(), 1);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].action, CapabilityChangeAction::Granted);
        assert_eq!(changes[0].cap, "terminal");
    }

    #[test]
    fn no_change_when_prev_equals_active() {
        let store = store_with("terminal", 1000);
        let (_eff, _active, _temp, changes) = evaluate(&store, CAP_DEFAULT, CAP_TERMINAL, 0);
        assert!(changes.is_empty());
    }

    #[test]
    fn expiry_emits_expired_revoke_emits_revoked() {
        // grant still in file but now past expiry → Expired
        let store = store_with("terminal", 100);
        let (_e, active, _t, changes) = evaluate(&store, CAP_DEFAULT, CAP_TERMINAL, 200);
        assert_eq!(active, 0);
        assert_eq!(changes[0].action, CapabilityChangeAction::Expired);

        // grant removed from file entirely → Revoked
        let empty = CapabilityGrantStore::default();
        let (_e, _a, _t, changes) = evaluate(&empty, CAP_DEFAULT, CAP_TERMINAL, 50);
        assert_eq!(changes[0].action, CapabilityChangeAction::Revoked);
    }
}
```

- [ ] **Step 2: Run + commit**

Run: `cargo test -p serverbee-agent capability_grants::supervisor`
Expected: PASS.

```bash
git add crates/agent/src/capability_grants/supervisor.rs
git commit -m "feat(agent): add capability grant supervisor with transition diffing"
```

---

### Task 7: Reporter wiring — effective caps, SystemInfo, supervisor spawn

**Files:**
- Modify: `crates/agent/src/reporter.rs` (`:138` Arc creation, `:233-246` SystemInfo, the manager-spawn block `~:288`, and the `tokio::select!` loop `~:360`)

- [ ] **Step 1: Compute effective caps before the Arc.** Replace `crates/agent/src/reporter.rs:138`:

```rust
let capabilities = Arc::new(AtomicU32::new(self.agent_local_capabilities));
```

with:

```rust
// Fold any active temporary grants into the initial effective caps so the
// first SystemInfo already reflects grants that survived a restart.
let base_caps = self.agent_local_capabilities;
let grants_path = self.config.capabilities.grants_path();
let now0 = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|d| d.as_secs() as i64)
    .unwrap_or(0);
let grant_store0 = crate::capability_grants::CapabilityGrantStore::load(&grants_path);
let effective_caps =
    (base_caps | grant_store0.active_bits(now0, base_caps)) & serverbee_common::constants::CAP_VALID_MASK;
let initial_temporary = grant_store0.active_grants(now0);
let capabilities = Arc::new(AtomicU32::new(effective_caps));
```

- [ ] **Step 2: Report effective caps + temporary in SystemInfo.** In the `AgentMessage::SystemInfo { ... }` construction (`:233-244`), change the capabilities field and add `temporary`:

```rust
        let info_msg = AgentMessage::SystemInfo {
            msg_id: uuid::Uuid::new_v4().to_string(),
            info: serverbee_common::types::SystemInfo {
                protocol_version: PROTOCOL_VERSION,
                features,
                ipv4: initial_ipv4.clone(),
                ipv6: initial_ipv6.clone(),
                ..info
            },
            agent_local_capabilities: Some(effective_caps),
            temporary: initial_temporary.clone(),
        };
```

- [ ] **Step 3: Create the grant channel + spawn the supervisor.** In the manager-setup region (after the other `mpsc::channel` managers, ~`:288`), add:

```rust
        // Capability grant supervisor: re-reads the grants file and pushes
        // CapabilitiesChanged through `grant_tx` (forwarded onto the WS below).
        let (grant_tx, mut grant_rx) = mpsc::channel::<AgentMessage>(8);
        {
            let grants_path = grants_path.clone();
            let caps = Arc::clone(&capabilities);
            let tx = grant_tx.clone();
            tokio::spawn(async move {
                crate::capability_grants::supervisor::run_grant_supervisor(
                    grants_path,
                    base_caps,
                    caps,
                    tx,
                    std::time::Duration::from_secs(3),
                )
                .await;
            });
        }
```

- [ ] **Step 4: Forward `grant_rx` onto the WebSocket.** In the main `tokio::select!` loop in `connect_and_report` (next to the `Some(external_msg) = ...` arm at `:364`), add a new branch:

```rust
                Some(grant_msg) = grant_rx.recv() => {
                    let json = serde_json::to_string(&grant_msg)?;
                    write.send(Message::Text(json.into())).await?;
                    tracing::debug!("Sent CapabilitiesChanged");
                }
```

- [ ] **Step 5: Build + commit**

Run: `cargo build -p serverbee-agent && cargo clippy -p serverbee-agent -- -D warnings`
Expected: compiles, no warnings.

```bash
git add crates/agent/src/reporter.rs
git commit -m "feat(agent): fold grants into effective caps and run the grant supervisor per connection"
```

---

# Phase 5 — Server

### Task 8: `AgentManager` temporary-grant storage

**Files:**
- Modify: `crates/server/src/service/agent_manager.rs` (struct `:71-101`, `new()`, methods near `:514-549`)

- [ ] **Step 1: Add the field + import.** Add to `use` imports `use serverbee_common::protocol::TemporaryGrant;` (if not already imported via a glob), and add to the `AgentManager` struct:

```rust
    /// Active temporary capability grants reported by each agent (in-memory,
    /// transient — re-reported on reconnect). Drives the UI countdown.
    temporary_grants: DashMap<String, Vec<TemporaryGrant>>,
```

Add `temporary_grants: DashMap::new(),` to the struct initializer in `AgentManager::new()`.

- [ ] **Step 2: Add accessors** (next to `update_agent_local_capabilities` at `:514`):

```rust
    pub fn update_temporary_grants(&self, server_id: &str, grants: Vec<TemporaryGrant>) {
        if grants.is_empty() {
            self.temporary_grants.remove(server_id);
        } else {
            self.temporary_grants.insert(server_id.to_string(), grants);
        }
    }

    pub fn get_temporary_grants(&self, server_id: &str) -> Vec<TemporaryGrant> {
        self.temporary_grants
            .get(server_id)
            .map(|g| g.clone())
            .unwrap_or_default()
    }
```

- [ ] **Step 3: Add a test** in the `agent_manager.rs` test module (or add one):

```rust
    #[test]
    fn temporary_grants_round_trip_and_clear() {
        let (tx, _rx) = tokio::sync::broadcast::channel(16);
        let mgr = AgentManager::new(tx);
        mgr.update_temporary_grants(
            "s1",
            vec![serverbee_common::protocol::TemporaryGrant {
                cap: "terminal".into(),
                granted_at: 1,
                expires_at: 100,
            }],
        );
        assert_eq!(mgr.get_temporary_grants("s1").len(), 1);
        mgr.update_temporary_grants("s1", vec![]);
        assert!(mgr.get_temporary_grants("s1").is_empty());
    }
```

(Adjust `AgentManager::new(...)` call to match its real constructor signature — it takes the `broadcast::Sender<BrowserMessage>`. Inspect `new()` and match it.)

- [ ] **Step 4: Run + commit**

Run: `cargo test -p serverbee-server agent_manager`
Expected: PASS.

```bash
git add crates/server/src/service/agent_manager.rs
git commit -m "feat(server): store agent-reported temporary capability grants in AgentManager"
```

---

### Task 9: Handle `AgentMessage::CapabilitiesChanged` (mirror + audit + alert + broadcast)

**Files:**
- Modify: `crates/server/src/router/ws/agent.rs` (SystemInfo arm `:396-638`; add a sibling arm; disconnect cleanup)
- Modify: `crates/server/src/service/alert.rs` (`EVENT_DRIVEN_RULE_TYPES` `:14-20`, and any rule-type validation set)

- [ ] **Step 1: Register the alert rule type.** In `crates/server/src/service/alert.rs`, add `"capability_grant_detected"` to `EVENT_DRIVEN_RULE_TYPES`:

```rust
const EVENT_DRIVEN_RULE_TYPES: &[&str] = &[
    "ip_changed",
    "ssh_brute_force_detected",
    "ssh_new_ip_login",
    "port_scan_detected",
    "capability_grant_detected",
];
```

If there is a master "all valid rule types" set used for rule-create validation (grep for `EVENT_DRIVEN_RULE_TYPES` and for where metric rule types are validated), add `"capability_grant_detected"` there too. Do **not** add it to `SECURITY_RULE_TYPES` or `SOURCE_IP_RULE_TYPES`.

- [ ] **Step 2: Add a test** in `alert.rs` tests:

```rust
    #[test]
    fn capability_grant_detected_is_event_driven_only() {
        assert!(EVENT_DRIVEN_RULE_TYPES.contains(&"capability_grant_detected"));
        assert!(!SECURITY_RULE_TYPES.contains(&"capability_grant_detected"));
        assert!(!SOURCE_IP_RULE_TYPES.contains(&"capability_grant_detected"));
    }
```

Run: `cargo test -p serverbee-server alert::` → PASS. Commit:

```bash
git add crates/server/src/service/alert.rs
git commit -m "feat(server): register capability_grant_detected event-driven alert rule type"
```

- [ ] **Step 3: Add the `CapabilitiesChanged` handler arm** in `crates/server/src/router/ws/agent.rs`, as a sibling to the `AgentMessage::SystemInfo` arm. First add a small helper near the top of the file:

```rust
fn is_high_risk_cap(cap: &str) -> bool {
    matches!(cap, "terminal" | "exec" | "file" | "docker")
}
```

Then the arm (use the same access pattern the SystemInfo arm uses: `state`, `server_id`, `server_name`, `remote_addr`):

```rust
        AgentMessage::CapabilitiesChanged {
            msg_id: _,
            capabilities,
            temporary,
            changes,
        } => {
            // Live gate value + mirror (agent-owned: effective == reported).
            state
                .agent_manager
                .update_agent_local_capabilities(server_id, capabilities);
            state
                .agent_manager
                .update_temporary_grants(server_id, temporary.clone());
            if let Err(e) = crate::service::server::ServerService::update_capabilities_mirror(
                &state.db, server_id, capabilities,
            )
            .await
            {
                tracing::error!("Failed to mirror capabilities for {server_id}: {e}");
            }

            // Audit every change; alert on high-risk grants.
            let ip = remote_addr.to_string();
            for ch in &changes {
                let action = match ch.action {
                    serverbee_common::protocol::CapabilityChangeAction::Granted => {
                        "capability_temporarily_granted"
                    }
                    serverbee_common::protocol::CapabilityChangeAction::Expired => {
                        "capability_grant_expired"
                    }
                    serverbee_common::protocol::CapabilityChangeAction::Revoked => {
                        "capability_grant_revoked"
                    }
                };
                let detail = serde_json::json!({
                    "server_id": server_id,
                    "server_name": server_name,
                    "cap": ch.cap,
                    "expires_at": ch.expires_at,
                    "granted_by": ch.granted_by,
                    "reason": ch.reason,
                })
                .to_string();
                let _ = crate::service::audit::AuditService::log(
                    &state.db, "", action, Some(&detail), &ip,
                )
                .await;

                if matches!(
                    ch.action,
                    serverbee_common::protocol::CapabilityChangeAction::Granted
                ) && is_high_risk_cap(&ch.cap)
                {
                    if let Err(e) = crate::service::alert::AlertService::check_event_rules(
                        &state.db,
                        &state.config,
                        &state.alert_state_manager,
                        server_id,
                        "capability_grant_detected",
                    )
                    .await
                    {
                        tracing::error!("capability_grant_detected alert eval failed: {e}");
                    }
                }
            }

            // Browser fan-out (carries temporary[] for the countdown).
            state
                .agent_manager
                .broadcast_browser(BrowserMessage::CapabilitiesChanged {
                    server_id: server_id.to_string(),
                    capabilities,
                    agent_local_capabilities: Some(capabilities),
                    effective_capabilities: Some(capabilities),
                    temporary,
                });
        }
```

Confirm the exact names: `AuditService::log` (`crates/server/src/service/audit.rs:23`), `AlertService::check_event_rules` (`alert.rs:872`), `state.alert_state_manager`, `state.config`. Adjust the `AuditService`/`AlertService` type names to the real ones in the file (grep shows `AuditService::log(...)` and `AlertService::check_event_rules(...)` call sites already exist in this same file — copy their exact paths).

- [ ] **Step 4: Extend the SystemInfo arm** to also record `temporary` and include it in the broadcast. Change the SystemInfo destructure to add `temporary`, store it, and add `temporary` to the existing `BrowserMessage::CapabilitiesChanged` broadcast it emits (`:559-566`):

```rust
        AgentMessage::SystemInfo {
            msg_id,
            info,
            agent_local_capabilities,
            temporary,
        } => {
            // ... existing GeoIP logic unchanged ...
            state
                .agent_manager
                .update_temporary_grants(server_id, temporary.clone());

            if let Some(bits) = agent_local_capabilities {
                // ... existing update_agent_local_capabilities + mirror unchanged ...
                state
                    .agent_manager
                    .broadcast_browser(BrowserMessage::CapabilitiesChanged {
                        server_id: server_id.to_string(),
                        capabilities: bits,
                        agent_local_capabilities: Some(bits),
                        effective_capabilities: Some(bits),
                        temporary: temporary.clone(),
                    });
            }
            // ... rest unchanged ...
        }
```

- [ ] **Step 5: Clear temporary grants on disconnect.** Find where the agent WS handler cleans up on disconnect (after the receive loop ends, near where `ServerOffline` is emitted / `remove_connection` is called). Add:

```rust
    state.agent_manager.update_temporary_grants(&server_id, vec![]);
```

so the REST DTO and any late browser read don't show stale countdowns for an offline server.

- [ ] **Step 6: Build + commit**

Run: `cargo build -p serverbee-server && cargo clippy -p serverbee-server -- -D warnings`
Expected: compiles, no warnings.

```bash
git add crates/server/src/router/ws/agent.rs
git commit -m "feat(server): handle CapabilitiesChanged — mirror, audit, alert, broadcast"
```

---

### Task 10: Expose `temporary` on the server REST DTO

**Files:**
- Modify: `crates/server/src/router/api/server.rs` (the `ServerResponse` DTO + its construction)
- Modify: `apps/web/openapi.json` + `apps/web/src/lib/api-types.ts` (surgical add)

- [ ] **Step 1: Define a ToSchema DTO.** In `crates/server/src/router/api/server.rs`, add a server-side DTO (common's `TemporaryGrant` has no `ToSchema`):

```rust
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct TemporaryGrantDto {
    pub cap: String,
    pub granted_at: i64,
    pub expires_at: i64,
}

impl From<serverbee_common::protocol::TemporaryGrant> for TemporaryGrantDto {
    fn from(g: serverbee_common::protocol::TemporaryGrant) -> Self {
        Self { cap: g.cap, granted_at: g.granted_at, expires_at: g.expires_at }
    }
}
```

- [ ] **Step 2: Add the field to `ServerResponse`** (the DTO that already carries `capabilities`). Add:

```rust
    #[serde(default)]
    pub temporary: Vec<TemporaryGrantDto>,
```

- [ ] **Step 3: Populate it** wherever `ServerResponse` is built from a `server` model + `AppState`. At each construction site (list + detail), set:

```rust
        temporary: state
            .agent_manager
            .get_temporary_grants(&server.id)
            .into_iter()
            .map(Into::into)
            .collect(),
```

(If a construction site has no `state`/`AgentManager` in scope, thread it in or default to `vec![]` for that path — but the list/detail handlers do have `State<Arc<AppState>>`.)

- [ ] **Step 4: Sync OpenAPI + TS types (surgical).** Add the `temporary` field to the `ServerResponse` schema in `apps/web/openapi.json` and to the `ServerResponse` type in `apps/web/src/lib/api-types.ts`:

```jsonc
// openapi.json — under components.schemas.ServerResponse.properties:
"temporary": {
  "type": "array",
  "items": { "$ref": "#/components/schemas/TemporaryGrantDto" }
}
// and add the TemporaryGrantDto schema:
"TemporaryGrantDto": {
  "type": "object",
  "required": ["cap", "granted_at", "expires_at"],
  "properties": {
    "cap": { "type": "string" },
    "granted_at": { "type": "integer", "format": "int64" },
    "expires_at": { "type": "integer", "format": "int64" }
  }
}
```

```typescript
// api-types.ts — add to the ServerResponse interface:
temporary?: { cap: string; granted_at: number; expires_at: number }[]
```

Keep the diff minimal (do NOT regenerate the whole file — surgical edits only, matching the established convention).

- [ ] **Step 5: Build + commit**

Run: `cargo build -p serverbee-server && (cd apps/web && bun run typecheck)`
Expected: compiles + typechecks.

```bash
git add crates/server/src/router/api/server.rs apps/web/openapi.json apps/web/src/lib/api-types.ts
git commit -m "feat(server): expose temporary capability grants on the server REST DTO"
```

---

# Phase 6 — Web

### Task 11: `classifyCapability` helper + types

**Files:**
- Modify: `apps/web/src/lib/capabilities.ts`
- Create: `apps/web/src/lib/capabilities.test.ts`

- [ ] **Step 1: Write the failing vitest** `apps/web/src/lib/capabilities.test.ts`:

```typescript
import { describe, expect, it } from 'vitest'
import { CAP_DEFAULT, CAP_TERMINAL, classifyCapability, temporaryGrantFor } from './capabilities'

const base = { capabilities: CAP_DEFAULT, effective_capabilities: CAP_DEFAULT }

describe('classifyCapability', () => {
  it('returns off when the bit is not set', () => {
    expect(classifyCapability(base, CAP_TERMINAL)).toBe('off')
  })

  it('returns temporary when a matching active grant exists', () => {
    const server = {
      capabilities: CAP_DEFAULT | CAP_TERMINAL,
      effective_capabilities: CAP_DEFAULT | CAP_TERMINAL,
      temporary: [{ cap: 'terminal', granted_at: 0, expires_at: 9_999_999_999 }]
    }
    expect(classifyCapability(server, CAP_TERMINAL)).toBe('temporary')
    expect(temporaryGrantFor(server, CAP_TERMINAL)?.expires_at).toBe(9_999_999_999)
  })

  it('returns enabled when the bit is set but not via a grant', () => {
    const server = { capabilities: CAP_DEFAULT | CAP_TERMINAL, effective_capabilities: CAP_DEFAULT | CAP_TERMINAL }
    expect(classifyCapability(server, CAP_TERMINAL)).toBe('enabled')
  })
})
```

Run: `cd apps/web && bun run test capabilities` → FAIL (functions undefined).

- [ ] **Step 2: Implement** — append to `apps/web/src/lib/capabilities.ts`:

```typescript
export type CapabilityState = 'off' | 'enabled' | 'temporary'

export interface TemporaryGrantView {
  cap: string
  granted_at: number
  expires_at: number
}

interface CapabilityHost {
  capabilities?: number | null
  effective_capabilities?: number | null
  temporary?: TemporaryGrantView[] | null
}

const CAP_BY_BIT = new Map(CAPABILITIES.map((c) => [c.bit, c.key]))

// Returns the active grant for a capability bit, if any (expiry checked client-side).
export function temporaryGrantFor(host: CapabilityHost, bit: number): TemporaryGrantView | undefined {
  const key = CAP_BY_BIT.get(bit)
  if (!(key && host.temporary)) {
    return undefined
  }
  const nowSecs = Math.floor(Date.now() / 1000)
  return host.temporary.find((g) => g.cap === key && g.expires_at > nowSecs)
}

export function classifyCapability(host: CapabilityHost, bit: number): CapabilityState {
  const enabled = getEffectiveCapabilityEnabled(host.effective_capabilities, host.capabilities, bit)
  if (!enabled) {
    return 'off'
  }
  return temporaryGrantFor(host, bit) ? 'temporary' : 'enabled'
}
```

Run: `cd apps/web && bun run test capabilities` → PASS.

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/lib/capabilities.ts apps/web/src/lib/capabilities.test.ts
git commit -m "feat(web): add classifyCapability helper for temporary grant state"
```

---

### Task 12: WebSocket `temporary` plumbing

**Files:**
- Modify: `apps/web/src/hooks/use-servers-ws.ts` (`ServerMetrics` `:20-67`, `capabilities_changed` type `:74-80`, `setServerCapabilities` `:279-296`, `handleCapabilityMessage` `:367-401`)

- [ ] **Step 1: Extend types.** Add to the `ServerMetrics` interface (after the capability fields):

```typescript
  temporary?: Array<{ cap: string; granted_at: number; expires_at: number }>
```

Add the same field to the `capabilities_changed` WsMessage union member:

```typescript
  temporary?: Array<{ cap: string; granted_at: number; expires_at: number }>
```

- [ ] **Step 2: Thread it through `setServerCapabilities`.** Add a trailing optional param and merge it:

```typescript
export function setServerCapabilities(
  prev: ServerMetrics[],
  serverId: string,
  capabilities: number,
  agentLocalCapabilities: number | null | undefined,
  effectiveCapabilities: number | null | undefined,
  temporary?: Array<{ cap: string; granted_at: number; expires_at: number }> | null
): ServerMetrics[] {
  return prev.map((server) =>
    server.id === serverId
      ? {
          ...server,
          capabilities,
          agent_local_capabilities: agentLocalCapabilities ?? null,
          effective_capabilities: effectiveCapabilities ?? null,
          temporary: temporary ?? []
        }
      : server
  )
}
```

- [ ] **Step 3: Pass `msg.temporary`** in `handleCapabilityMessage`. Destructure `temporary` from `msg` and pass it to `setServerCapabilities`, and add `temporary: msg.temporary ?? []` to the `['servers', server_id]` and `['servers-list']` cache merges:

```typescript
    const { server_id, capabilities, agent_local_capabilities, effective_capabilities, temporary } = msg
    queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) =>
      prev
        ? setServerCapabilities(prev, server_id, capabilities, agent_local_capabilities, effective_capabilities, temporary)
        : prev
    )
    // ...and add `temporary: temporary ?? []` to the other two setQueryData merges.
```

- [ ] **Step 4: Typecheck + commit**

Run: `cd apps/web && bun run typecheck`
Expected: passes.

```bash
git add apps/web/src/hooks/use-servers-ws.ts
git commit -m "feat(web): plumb temporary grant metadata through the servers WS cache"
```

---

### Task 13: `useCountdown` hook + amber Temporary badge

**Files:**
- Create: `apps/web/src/hooks/use-countdown.ts`
- Modify: `apps/web/src/components/server/capabilities-dialog.tsx` (`:116-124`)
- Modify: `apps/web/src/routes/_authed/settings/capabilities.tsx` (`:100-112`)
- Modify: `apps/web/src/locales/{en,zh}/servers.json` (add `cap_temporary` + `cap_temporary_tooltip`)

- [ ] **Step 1: Create `apps/web/src/hooks/use-countdown.ts`** (extracted from the `mobile-pair-dialog.tsx` setInterval pattern):

```typescript
import { useEffect, useState } from 'react'

// Live seconds remaining until `expiresAtSecs` (Unix epoch seconds). Ticks every
// second; returns 0 once elapsed. `null` expiry → null (no countdown).
export function useCountdown(expiresAtSecs: number | null | undefined): number | null {
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000))
  useEffect(() => {
    if (expiresAtSecs == null) {
      return
    }
    const id = setInterval(() => setNow(Math.floor(Date.now() / 1000)), 1000)
    return () => clearInterval(id)
  }, [expiresAtSecs])
  if (expiresAtSecs == null) {
    return null
  }
  return Math.max(0, expiresAtSecs - now)
}

export function formatCountdown(secs: number): string {
  if (secs >= 3600) {
    const h = Math.floor(secs / 3600)
    const m = Math.floor((secs % 3600) / 60)
    return `${h}h ${m}m`
  }
  const m = Math.floor(secs / 60)
  const s = secs % 60
  return `${m}:${s.toString().padStart(2, '0')}`
}
```

- [ ] **Step 2: Add i18n keys** to `apps/web/src/locales/en/servers.json`:

```json
"cap_temporary": "Temporary",
"cap_temporary_tooltip": "Temporarily enabled on the agent host. Manage with `serverbee-agent grant` / `revoke` on the host."
```

and `apps/web/src/locales/zh/servers.json`:

```json
"cap_temporary": "临时",
"cap_temporary_tooltip": "在 agent 主机上临时开启。用 `serverbee-agent grant` / `revoke` 在主机管理。"
```

- [ ] **Step 3: Render the badge in `capabilities-dialog.tsx`.** Replace the enabled/disabled ternary (`:116-124`) with a three-state render. The component already has the `server` object and `bit`; compute the state:

```tsx
{(() => {
  const state = classifyCapability(server, bit)
  if (state === 'temporary') {
    const grant = temporaryGrantFor(server, bit)
    return <TemporaryBadge expiresAt={grant?.expires_at ?? null} />
  }
  if (state === 'enabled') {
    return (
      <Badge className="border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400">
        {t('cap_enabled', { defaultValue: 'Enabled' })}
      </Badge>
    )
  }
  return (
    <Badge className="text-muted-foreground" variant="outline">
      {t('cap_disabled', { defaultValue: 'Disabled' })}
    </Badge>
  )
})()}
```

Add a small `TemporaryBadge` component in the same file (or a shared component file):

```tsx
function TemporaryBadge({ expiresAt }: { expiresAt: number | null }) {
  const { t } = useTranslation('servers')
  const remaining = useCountdown(expiresAt)
  return (
    <Badge
      className="border-amber-500/30 bg-amber-500/10 text-amber-600 dark:text-amber-400"
      title={t('cap_temporary_tooltip')}
    >
      {t('cap_temporary')}
      {remaining != null && remaining > 0 ? ` · ${formatCountdown(remaining)}` : ''}
    </Badge>
  )
}
```

Add imports: `classifyCapability`, `temporaryGrantFor` from `@/lib/capabilities`; `useCountdown`, `formatCountdown` from `@/hooks/use-countdown`; `useTranslation`.

- [ ] **Step 4: Render in the settings matrix** (`settings/capabilities.tsx:100-112`). Replace the Check/Minus ternary with a three-state cell — temporary renders an amber dot/badge with the same `TemporaryBadge` (or an amber `Clock` icon with a `title` countdown). Keep it compact for the matrix cell:

```tsx
{(() => {
  const state = classifyCapability(server, bit)
  if (state === 'temporary') {
    return <TemporaryBadge expiresAt={temporaryGrantFor(server, bit)?.expires_at ?? null} />
  }
  if (state === 'enabled') {
    return <Check aria-label={`${label}: ${t('cap_enabled', { ns: 'servers' })}`} className="size-4 text-emerald-500" />
  }
  return <Minus aria-label={`${label}: ${t('cap_disabled', { ns: 'servers' })}`} className="size-4 text-muted-foreground/40" />
})()}
```

- [ ] **Step 5: Typecheck + visual verification + commit**

Run: `cd apps/web && bun run typecheck && bun x ultracite check`
Expected: passes.

**Visual:** per project convention, UI changes need a real browser check. Run `make web-dev-prod` (or `bun run dev:prod`), connect a test agent, grant `terminal` on the agent host, and confirm the capabilities dialog + settings matrix show the amber **Temporary · mm:ss** badge counting down, then flip to off at expiry. (No browser tool in this env → state that the check was done manually.)

```bash
git add apps/web/src/hooks/use-countdown.ts apps/web/src/components/server/capabilities-dialog.tsx apps/web/src/routes/_authed/settings/capabilities.tsx apps/web/src/locales/en/servers.json apps/web/src/locales/zh/servers.json
git commit -m "feat(web): show amber temporary badge with live countdown for granted caps"
```

---

### Task 14: Alert rule editor option + preset card

**Files:**
- Modify: `apps/web/src/components/security/alert-presets.tsx` (`PresetKind` `:42`, `PRESETS` `:44`)
- Modify: the alert rule editor's rule-type options (grep `ssh_brute_force_detected` under `apps/web/src/components` / `routes` to find the `<Select>` of event rule types)
- Modify: `apps/web/src/locales/{en,zh}/*` alert i18n (preset title/description + rule-type label)

- [ ] **Step 1: Extend `PresetKind`** and add a preset entry:

```typescript
type PresetKind =
  | 'ssh_brute_force_detected'
  | 'ssh_new_ip_login'
  | 'port_scan_detected'
  | 'capability_grant_detected'
```

```typescript
{
  kind: 'capability_grant_detected',
  icon: KeyRound, // import from lucide-react
  titleKey: 'preset.capability_grant_title',
  titleDefault: 'Capability Temporarily Granted',
  descriptionKey: 'preset.capability_grant_description',
  descriptionDefault: 'Notify when a high-risk capability (terminal/exec/file/docker) is temporarily granted on a server.',
  defaultName: 'Capability Granted'
}
```

`capability_grant_detected` carries no extra security params, so `buildSecurityParams` returns just the default `{ dedupe_window_seconds }` for it (no new branch needed — the fallthrough already does this). Ensure the rule item POSTed uses `rule_type: 'capability_grant_detected'` with no `security` filters required.

- [ ] **Step 2: Add the rule-type to the editor select** (so it's selectable outside the preset too), and add i18n keys for the preset title/description and the rule-type label in `en` + `zh` alert locale files. Mirror the existing `ssh_brute_force_detected` entries.

- [ ] **Step 3: Typecheck + visual + commit**

Run: `cd apps/web && bun run typecheck && bun x ultracite check`
Expected: passes.

**Visual:** confirm the new preset card appears on the Alerts page and creating it yields a working rule. Grant a high-risk cap on a test agent and confirm a notification fires (cross-check with the audit log entry).

```bash
git add apps/web/src/components/security/alert-presets.tsx apps/web/src/locales
git commit -m "feat(web): add capability_grant_detected alert preset and rule-type option"
```

---

# Phase 7 — Integration tests + docs

### Task 15: Server integration test (gate opens, audit, expiry)

**Files:**
- Modify/Create: `crates/server/tests/integration.rs` (or a new `crates/server/tests/capability_grants_integration.rs`)

- [ ] **Step 1: Write an integration test** that drives the full path. Model it on the existing integration harness (a test agent WS + a browser WS via x-api-key, as used by `test_file_list_server_offline`). Steps the test performs:
  1. Connect a test agent; send `SystemInfo` with `agent_local_capabilities = CAP_DEFAULT` (terminal off), `temporary = []`.
  2. Assert an `exec`/terminal control-plane request is denied (`capability_denied_reason` → `agent_capability_disabled`).
  3. Send `AgentMessage::CapabilitiesChanged { capabilities: CAP_DEFAULT | CAP_TERMINAL, temporary: [TemporaryGrant{cap:"terminal",..}], changes: [granted terminal] }`.
  4. Assert the same control-plane request now passes the gate (`capability_denied_reason` → `None`).
  5. Assert an `audit_logs` row with action `capability_temporarily_granted` exists.
  6. Assert a browser WS received `capabilities_changed` carrying `temporary` with the terminal grant.
  7. Send `CapabilitiesChanged` back to `CAP_DEFAULT` with `changes: [expired terminal]`; assert the gate is denied again and an `capability_grant_expired` audit row exists.

Use the exact harness helpers already in `crates/server/tests/`. Keep assertions on `AuditService`/DB via a direct query of the `audit_log` entity.

- [ ] **Step 2: Run + commit**

Run: `cargo test -p serverbee-server --test integration` (or the new test file)
Expected: PASS.

```bash
git add crates/server/tests/
git commit -m "test(server): cover temporary capability grant gate, audit, and expiry"
```

---

### Task 16: Documentation + changelog + env

**Files:**
- Modify: `apps/docs/content/docs/en/capabilities.mdx` + `apps/docs/content/docs/zh/capabilities.mdx`
- Modify: `apps/docs/content/docs/{en,zh}/configuration.mdx` + `ENV.md`
- Modify: `apps/docs/content/docs/{en,zh}/admin.mdx` (audit actions)
- Modify: `apps/docs/content/docs/{en,zh}/security.mdx` and/or `alerts.mdx` (`capability_grant_detected`)
- Modify: `CHANGELOG.md`
- Create: `tests/1.0.0/<n>-temporary-capability-grants.md` (manual E2E checklist)

- [ ] **Step 1: Capabilities page** — add a "Temporary grants" section to both `capabilities.mdx` files: the `grant <cap> --for <30m|2h|1d> [--reason]`, `revoke <cap>`, `grants` CLI; that it runs on the **agent host** (typically `sudo`), shares the daemon's config; restart/expiry semantics (absolute `expires_at`, survives restart, expires at the original deadline; crash/corrupt → fails safe to off); that the server gates open live and the change is audited + alertable; and the host-only trust note. Document the edge case: grants created while the agent is disconnected from the server are applied/shown on reconnect but do not emit a `granted` audit/alert.

- [ ] **Step 2: Configuration + ENV** — add to both `configuration.mdx` files and `ENV.md` the two keys (per the project convention that ENV + configuration.mdx change together):

| Key | Env | Default |
|---|---|---|
| `capabilities.temporary_max_duration` | `SERVERBEE_CAPABILITIES__TEMPORARY_MAX_DURATION` | `24h` |
| `capabilities.state_dir` | `SERVERBEE_CAPABILITIES__STATE_DIR` | `/var/lib/serverbee` |

- [ ] **Step 3: Admin page** — add the three audit actions (`capability_temporarily_granted`, `capability_grant_expired`, `capability_grant_revoked`) to the "Recorded Events" list in both `admin.mdx` files.

- [ ] **Step 4: Alerts/security page** — document the `capability_grant_detected` rule type (event-driven; fires on high-risk temp grants; preset card available) in both languages.

- [ ] **Step 5: CHANGELOG** — add an Unreleased entry: "Temporary, host-local, auto-expiring capability grants (`serverbee-agent grant/revoke/grants`) with live UI countdown, audit, and `capability_grant_detected` alerts."

- [ ] **Step 6: Manual E2E checklist** — `tests/` entry covering: grant → use via web within the window → restart agent mid-window → still active with correct remaining time → expire → denied again + audit + alert. Add it to `tests/README.md` index.

- [ ] **Step 7: Typecheck docs + commit**

Run: `bun run typecheck` (web + docs) and a docs build smoke if available.

```bash
git add apps/docs ENV.md CHANGELOG.md tests/
git commit -m "docs: document temporary capability grants (CLI, config, audit, alerts)"
```

---

## Self-Review

**Spec coverage** (spec §→task):
- §1 architecture → Tasks 1–14 collectively. ✔
- §2 data model / single-writer → Task 2 (store) + Task 4 (CLI writer). ✔
- §3 effective computation → Task 2 (`active_bits`) + Task 6 (`evaluate`) + Task 7 (reporter fold). ✔
- §4 restart/clock → Task 2 (absolute `expires_at`, prune on load) + Task 7 (fold before SystemInfo) + Task 6 (read-only supervisor). ✔
- §5 supervisor → Task 6 + Task 7. ✔
- §6 CLI → Task 4 + Task 5. ✔
- §7 config → Task 3 + Task 16. ✔
- §8 protocol + server handler → Task 1 + Task 8 + Task 9 + Task 10. ✔
- §9 audit + alert → Task 9 (audit + alert call) + Task 9 step 1–2 (rule type) + Task 14 (preset). ✔
- §10 UI → Task 11 + Task 12 + Task 13. ✔
- §11 testing → tests embedded per task + Task 15 (server integration). ✔
- §12 docs → Task 16. ✔
- §13 trust model → enforced by design (no server→agent message added; Task 9 only reads agent-reported values). ✔

**Placeholder scan:** none — every code step contains complete code. The two "grep to locate exact site" notes (Task 9 alert-validation set; Task 14 editor select) are unavoidable codebase-discovery steps, each with the exact symbol to search and the exact edit to make.

**Type consistency:** `CapabilityGrantStore` methods (`load`/`active_bits`/`active_grants`/`upsert`/`remove`/`flush`/`records`) are used identically across Tasks 2/4/6/7. `TemporaryGrant`/`CapabilityChangeEvent`/`CapabilityChangeAction` (Task 1) are consumed unchanged in Tasks 6/8/9/10. `update_temporary_grants`/`get_temporary_grants` (Task 8) are called in Tasks 9/10. `classifyCapability`/`temporaryGrantFor` (Task 11) are used in Task 13. `useCountdown`/`formatCountdown` (Task 13) match their definitions. Effective caps are reported as `agent_local_capabilities: Some(effective_caps)` consistently (Task 7) and gated on the same value server-side (Task 9, via the existing `capability_denied_reason`). ✔
