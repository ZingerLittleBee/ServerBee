# Network Quality Monitoring Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a network quality monitoring subsystem where VPS agents probe ISP/cloud targets, report latency/packet-loss stats, and display results in a dedicated frontend with overview + detail pages.

**Architecture:** Independent subsystem layered on the existing Agent→Server→Browser data flow. Agent-side `NetworkProber` uses shared probe utils extracted from `pinger.rs`. Server stores results in new DB tables, serves via new API routes, and broadcasts real-time updates via existing `BrowserMessage` channel. Frontend adds `/network` routes with Recharts multi-line charts.

**Tech Stack:** Rust (Axum, sea-orm, tokio), React 19 (TanStack Router/Query, Recharts, shadcn/ui), SQLite, WebSocket

**Spec:** `docs/superpowers/specs/2026-03-15-network-quality-monitoring-design.md`

---

## Chunk 1: Foundation — Common Types + Migration + Entities

### Task 1: Add protocol types and DTOs to common crate

**Files:**
- Modify: `crates/common/src/types.rs`
- Modify: `crates/common/src/protocol.rs`

- [ ] **Step 1: Add DTOs to `types.rs`**

Append to the end of `crates/common/src/types.rs`:

```rust
/// Agent-facing wire type for network probe targets (minimal fields for probing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkProbeTarget {
    pub target_id: String,
    pub name: String,
    pub target: String,
    pub probe_type: String,
}

/// Aggregated result from one probe round for one target
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkProbeResultData {
    pub target_id: String,
    pub avg_latency: Option<f64>,
    pub min_latency: Option<f64>,
    pub max_latency: Option<f64>,
    pub packet_loss: f64,
    pub packet_sent: u32,
    pub packet_received: u32,
    pub timestamp: DateTime<Utc>,
}
```

- [ ] **Step 2: Add protocol message variants to `protocol.rs`**

Add to `AgentMessage` enum:

```rust
NetworkProbeResults {
    results: Vec<NetworkProbeResultData>,
},
```

Add to `ServerMessage` enum:

```rust
NetworkProbeSync {
    targets: Vec<NetworkProbeTarget>,
    interval: u32,
    packet_count: u32,
},
```

Add to `BrowserMessage` enum:

```rust
NetworkProbeUpdate {
    server_id: String,
    results: Vec<NetworkProbeResultData>,
},
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p serverbee-common`
Expected: compiles with 0 errors

- [ ] **Step 4: Add serialization tests**

Append to `protocol.rs` test module:

```rust
#[test]
fn test_network_probe_sync_serializes() {
    let msg = ServerMessage::NetworkProbeSync {
        targets: vec![],
        interval: 60,
        packet_count: 10,
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("network_probe_sync") || json.contains("NetworkProbeSync"));
    let _: ServerMessage = serde_json::from_str(&json).unwrap();
}

#[test]
fn test_network_probe_results_serializes() {
    let msg = AgentMessage::NetworkProbeResults {
        results: vec![],
    };
    let json = serde_json::to_string(&msg).unwrap();
    let _: AgentMessage = serde_json::from_str(&json).unwrap();
}

#[test]
fn test_network_probe_result_with_null_latency() {
    let data = NetworkProbeResultData {
        target_id: "t1".into(),
        avg_latency: None,
        min_latency: None,
        max_latency: None,
        packet_loss: 1.0,
        packet_sent: 10,
        packet_received: 0,
        timestamp: Utc::now(),
    };
    let json = serde_json::to_string(&data).unwrap();
    assert!(json.contains("null"));
    let parsed: NetworkProbeResultData = serde_json::from_str(&json).unwrap();
    assert!(parsed.avg_latency.is_none());
    assert_eq!(parsed.packet_loss, 1.0);
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p serverbee-common`
Expected: all tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/common/src/types.rs crates/common/src/protocol.rs
git commit -m "feat(common): add network quality monitoring protocol types"
```

---

### Task 2: Create database migration with tables and seed data

**Files:**
- Create: `crates/server/src/migration/m20260315_000004_network_probe.rs`
- Modify: `crates/server/src/migration/mod.rs`

- [ ] **Step 1: Create migration file**

Create `crates/server/src/migration/m20260315_000004_network_probe.rs`:

```rust
use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260315_000004_network_probe"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // network_probe_target table
        db.execute_unprepared(
            "CREATE TABLE network_probe_target (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                provider TEXT NOT NULL,
                location TEXT NOT NULL,
                target TEXT NOT NULL,
                probe_type TEXT NOT NULL,
                is_builtin INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )"
        ).await?;

        // network_probe_config table (per-VPS target assignment)
        db.execute_unprepared(
            "CREATE TABLE network_probe_config (
                id TEXT PRIMARY KEY NOT NULL,
                server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
                target_id TEXT NOT NULL REFERENCES network_probe_target(id) ON DELETE CASCADE,
                created_at TEXT NOT NULL,
                UNIQUE(server_id, target_id)
            )"
        ).await?;

        // network_probe_record table (per-round aggregated results)
        db.execute_unprepared(
            "CREATE TABLE network_probe_record (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
                target_id TEXT NOT NULL REFERENCES network_probe_target(id) ON DELETE CASCADE,
                avg_latency REAL,
                min_latency REAL,
                max_latency REAL,
                packet_loss REAL NOT NULL,
                packet_sent INTEGER NOT NULL,
                packet_received INTEGER NOT NULL,
                timestamp TEXT NOT NULL
            )"
        ).await?;

        db.execute_unprepared(
            "CREATE INDEX idx_network_probe_record_lookup ON network_probe_record (server_id, target_id, timestamp)"
        ).await?;

        // network_probe_record_hourly table
        db.execute_unprepared(
            "CREATE TABLE network_probe_record_hourly (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
                target_id TEXT NOT NULL REFERENCES network_probe_target(id) ON DELETE CASCADE,
                avg_latency REAL,
                min_latency REAL,
                max_latency REAL,
                avg_packet_loss REAL NOT NULL,
                sample_count INTEGER NOT NULL,
                hour TEXT NOT NULL,
                UNIQUE(server_id, target_id, hour)
            )"
        ).await?;

        // Seed builtin probe targets
        let now = chrono::Utc::now().to_rfc3339();
        let targets = [
            ("cn-telecom-shanghai",  "Shanghai Telecom",  "Telecom",    "Shanghai",  "61.129.2.3"),
            ("cn-telecom-beijing",   "Beijing Telecom",   "Telecom",    "Beijing",   "106.37.67.29"),
            ("cn-telecom-guangzhou", "Guangzhou Telecom", "Telecom",    "Guangzhou", "14.215.116.1"),
            ("cn-unicom-shanghai",   "Shanghai Unicom",   "Unicom",     "Shanghai",  "210.22.84.3"),
            ("cn-unicom-beijing",    "Beijing Unicom",    "Unicom",     "Beijing",   "202.106.50.1"),
            ("cn-unicom-guangzhou",  "Guangzhou Unicom",  "Unicom",     "Guangzhou", "221.5.88.88"),
            ("cn-mobile-shanghai",   "Shanghai Mobile",   "Mobile",     "Shanghai",  "117.131.19.23"),
            ("cn-mobile-beijing",    "Beijing Mobile",    "Mobile",     "Beijing",   "221.179.155.161"),
            ("cn-mobile-guangzhou",  "Guangzhou Mobile",  "Mobile",     "Guangzhou", "120.196.165.24"),
            ("intl-cloudflare",      "Cloudflare",        "Cloudflare", "US",        "1.1.1.1"),
            ("intl-google",          "Google DNS",        "Google",     "US",        "8.8.8.8"),
            ("intl-aws-tokyo",       "AWS Tokyo",         "AWS",        "Tokyo",     "13.112.63.251"),
        ];

        for (id, name, provider, location, target) in &targets {
            db.execute_unprepared(&format!(
                "INSERT INTO network_probe_target (id, name, provider, location, target, probe_type, is_builtin, created_at, updated_at) VALUES ('{id}', '{name}', '{provider}', '{location}', '{target}', 'icmp', 1, '{now}', '{now}')"
            )).await?;
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP TABLE IF EXISTS network_probe_record_hourly").await?;
        db.execute_unprepared("DROP TABLE IF EXISTS network_probe_record").await?;
        db.execute_unprepared("DROP TABLE IF EXISTS network_probe_config").await?;
        db.execute_unprepared("DROP TABLE IF EXISTS network_probe_target").await?;
        Ok(())
    }
}
```

- [ ] **Step 2: Register migration in `mod.rs`**

Add to `crates/server/src/migration/mod.rs`:

```rust
mod m20260315_000004_network_probe;
```

And append to the `migrations()` vec:

```rust
Box::new(m20260315_000004_network_probe::Migration),
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: compiles with 0 errors

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/migration/
git commit -m "feat(server): add network probe database migration with seed data"
```

---

### Task 3: Create sea-orm entity files

**Files:**
- Create: `crates/server/src/entity/network_probe_target.rs`
- Create: `crates/server/src/entity/network_probe_config.rs`
- Create: `crates/server/src/entity/network_probe_record.rs`
- Create: `crates/server/src/entity/network_probe_record_hourly.rs`
- Modify: `crates/server/src/entity/mod.rs`

- [ ] **Step 1: Create `network_probe_target.rs`**

```rust
use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, utoipa::ToSchema)]
#[schema(as = NetworkProbeTarget)]
#[sea_orm(table_name = "network_probe_target")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    pub provider: String,
    pub location: String,
    pub target: String,
    pub probe_type: String,
    pub is_builtin: bool,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTimeUtc,
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

- [ ] **Step 2: Create `network_probe_config.rs`**

```rust
use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, utoipa::ToSchema)]
#[schema(as = NetworkProbeConfig)]
#[sea_orm(table_name = "network_probe_config")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub server_id: String,
    pub target_id: String,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

- [ ] **Step 3: Create `network_probe_record.rs`**

```rust
use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, utoipa::ToSchema)]
#[schema(as = NetworkProbeRecord)]
#[sea_orm(table_name = "network_probe_record")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub server_id: String,
    pub target_id: String,
    pub avg_latency: Option<f64>,
    pub min_latency: Option<f64>,
    pub max_latency: Option<f64>,
    pub packet_loss: f64,
    pub packet_sent: i32,
    pub packet_received: i32,
    #[schema(value_type = String, format = DateTime)]
    pub timestamp: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

- [ ] **Step 4: Create `network_probe_record_hourly.rs`**

```rust
use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, utoipa::ToSchema)]
#[schema(as = NetworkProbeRecordHourly)]
#[sea_orm(table_name = "network_probe_record_hourly")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub server_id: String,
    pub target_id: String,
    pub avg_latency: Option<f64>,
    pub min_latency: Option<f64>,
    pub max_latency: Option<f64>,
    pub avg_packet_loss: f64,
    pub sample_count: i32,
    #[schema(value_type = String, format = DateTime)]
    pub hour: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

- [ ] **Step 5: Register entities in `entity/mod.rs`**

Add these lines to `crates/server/src/entity/mod.rs`:

```rust
pub mod network_probe_config;
pub mod network_probe_record;
pub mod network_probe_record_hourly;
pub mod network_probe_target;
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: compiles with 0 errors

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/entity/
git commit -m "feat(server): add network probe entity models"
```

---

### Task 4: Add retention config fields

**Files:**
- Modify: `crates/server/src/config.rs`

- [ ] **Step 1: Add fields to `RetentionConfig`**

In `crates/server/src/config.rs`, add two fields to the `RetentionConfig` struct:

```rust
#[serde(default = "default_7")]
pub network_probe_days: u32,
#[serde(default = "default_90")]
pub network_probe_hourly_days: u32,
```

And update the `Default` impl to include:

```rust
network_probe_days: 7,
network_probe_hourly_days: 90,
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: compiles with 0 errors

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/config.rs
git commit -m "feat(server): add network probe retention config fields"
```

---

## Chunk 2: Agent — Probe Utils + NetworkProber + Reporter

### Task 5: Extract shared probe utils from pinger

**Files:**
- Create: `crates/agent/src/probe_utils.rs`
- Modify: `crates/agent/src/pinger.rs`
- Modify: `crates/agent/src/main.rs` (or `lib.rs` if present to add `mod probe_utils`)

- [ ] **Step 1: Create `probe_utils.rs` with shared probe functions**

Create `crates/agent/src/probe_utils.rs`. Extract `probe_icmp`, `probe_tcp`, `probe_http`, and `parse_ping_time` from `pinger.rs` as public standalone functions. Also add `probe_icmp_batch` for batch ICMP probing.

```rust
use std::time::{Duration, Instant};
use chrono::Utc;

pub struct ProbeResult {
    pub success: bool,
    pub latency_ms: f64,
    pub error: Option<String>,
}

pub struct BatchIcmpResult {
    pub avg_latency: Option<f64>,
    pub min_latency: Option<f64>,
    pub max_latency: Option<f64>,
    pub packet_loss: f64,
    pub packet_sent: u32,
    pub packet_received: u32,
}

pub async fn probe_icmp(host: &str, timeout: Duration) -> ProbeResult {
    // Move existing probe_icmp logic from pinger.rs here
    // Uses `ping -c 1 -W <timeout_secs> <host>`
    let start = Instant::now();
    let output = tokio::time::timeout(
        timeout,
        tokio::process::Command::new("ping")
            .args(["-c", "1", "-W", &timeout.as_secs().to_string(), host])
            .output(),
    )
    .await;

    match output {
        Ok(Ok(out)) => {
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let latency = parse_ping_time(&stdout).unwrap_or(elapsed);
                ProbeResult { success: true, latency_ms: latency, error: None }
            } else {
                let stderr = String::from_utf8_lossy(&out.stderr);
                ProbeResult { success: false, latency_ms: 0.0, error: Some(format!("Ping failed: {}", stderr.trim())) }
            }
        }
        Ok(Err(e)) => ProbeResult { success: false, latency_ms: 0.0, error: Some(format!("Failed to run ping: {e}")) },
        Err(_) => ProbeResult { success: false, latency_ms: 0.0, error: Some("Ping timed out".into()) },
    }
}

pub async fn probe_icmp_batch(host: &str, count: u32, timeout: Duration) -> BatchIcmpResult {
    let output = tokio::time::timeout(
        timeout,
        tokio::process::Command::new("ping")
            .args(["-c", &count.to_string(), "-W", &timeout.as_secs().to_string(), host])
            .output(),
    )
    .await;

    match output {
        Ok(Ok(out)) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            parse_ping_batch_output(&stdout, count)
        }
        _ => BatchIcmpResult {
            avg_latency: None,
            min_latency: None,
            max_latency: None,
            packet_loss: 1.0,
            packet_sent: count,
            packet_received: 0,
        },
    }
}

pub async fn probe_tcp(host: &str, port: u16, timeout: Duration) -> ProbeResult {
    // Move existing probe_tcp logic from pinger.rs here
    let start = Instant::now();
    let addr = format!("{host}:{port}");
    match tokio::time::timeout(timeout, tokio::net::TcpStream::connect(&addr)).await {
        Ok(Ok(_)) => {
            let latency = start.elapsed().as_secs_f64() * 1000.0;
            ProbeResult { success: true, latency_ms: latency, error: None }
        }
        Ok(Err(e)) => ProbeResult { success: false, latency_ms: 0.0, error: Some(format!("TCP connect failed: {e}")) },
        Err(_) => ProbeResult { success: false, latency_ms: 0.0, error: Some("TCP connect timed out".into()) },
    }
}

pub async fn probe_http(url: &str, timeout: Duration) -> ProbeResult {
    // Move existing probe_http logic from pinger.rs here
    let start = Instant::now();
    let full_url = if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("http://{url}")
    };
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .danger_accept_invalid_certs(true)
        .build();
    let client = match client {
        Ok(c) => c,
        Err(e) => return ProbeResult { success: false, latency_ms: 0.0, error: Some(format!("Client build error: {e}")) },
    };
    match client.get(&full_url).send().await {
        Ok(resp) => {
            let latency = start.elapsed().as_secs_f64() * 1000.0;
            let success = resp.status().is_success() || resp.status().is_redirection();
            ProbeResult { success, latency_ms: latency, error: if success { None } else { Some(format!("HTTP {}", resp.status())) } }
        }
        Err(e) => ProbeResult { success: false, latency_ms: 0.0, error: Some(format!("HTTP error: {e}")) },
    }
}

/// Parse "time=X.XX ms" from single ping output
pub fn parse_ping_time(output: &str) -> Option<f64> {
    // Existing logic from pinger.rs
    // String-based parsing (no regex dependency), matching existing pinger.rs pattern
    let time_marker = output.find("time=")?;
    let after = &output[time_marker + 5..];
    let end = after.find(' ')?;
    after[..end].parse().ok()
}

/// Parse batch ping output summary line:
/// "rtt min/avg/max/mdev = 1.234/5.678/9.012/1.234 ms"
/// and statistics line: "10 packets transmitted, 8 received, 20% packet loss"
fn parse_ping_batch_output(output: &str, sent: u32) -> BatchIcmpResult {
    // Parse packet loss from statistics line
    // String-based parsing (no regex dependency)
    // Parse "X% packet loss" from statistics line
    let loss_pct = output.find("% packet loss")
        .and_then(|pos| {
            let before = &output[..pos];
            let start = before.rfind(|c: char| !c.is_ascii_digit()).map(|p| p + 1).unwrap_or(0);
            before[start..].parse::<f64>().ok()
        })
        .unwrap_or(100.0);
    let packet_loss = loss_pct / 100.0;

    // Parse "N received" from statistics line
    let received = output.find(" received")
        .and_then(|pos| {
            let before = &output[..pos];
            let start = before.rfind(|c: char| !c.is_ascii_digit() && c != ' ').map(|p| p + 1).unwrap_or(0);
            before[start..].trim().parse::<u32>().ok()
        })
        .unwrap_or(0);

    if received == 0 {
        return BatchIcmpResult {
            avg_latency: None,
            min_latency: None,
            max_latency: None,
            packet_loss,
            packet_sent: sent,
            packet_received: 0,
        };
    }

    // Parse rtt summary: "rtt min/avg/max/mdev = A/B/C/D ms"
    // Parse "rtt min/avg/max/mdev = A/B/C/D ms"
    let (min, avg, max) = output.find("min/avg/max/")
        .and_then(|_| output.find("= "))
        .and_then(|pos| {
            let after = &output[pos + 2..];
            let end = after.find(' ').unwrap_or(after.len());
            let parts: Vec<&str> = after[..end].split('/').collect();
            if parts.len() >= 3 {
                let min = parts[0].parse().ok();
                let avg = parts[1].parse().ok();
                let max = parts[2].parse().ok();
                Some((min, avg, max))
            } else {
                None
            }
        })
        .unwrap_or((None, None, None));

    BatchIcmpResult {
        avg_latency: avg,
        min_latency: min,
        max_latency: max,
        packet_loss,
        packet_sent: sent,
        packet_received: received,
    }
}
```

- [ ] **Step 2: Refactor `pinger.rs` to use shared functions**

Replace the private `probe_icmp`, `probe_tcp`, `probe_http`, `parse_ping_time` functions in `pinger.rs` with calls to `crate::probe_utils::*`. The `PingManager` stays in `pinger.rs`; only the probe functions move. Wrap `ProbeResult` → `PingResult` conversion at call sites.

- [ ] **Step 3: Add `mod probe_utils;` to `main.rs`**

In `crates/agent/src/main.rs`, add:
```rust
mod probe_utils;
```

- [ ] **Step 4: Verify existing ping tests still pass**

Run: `cargo test -p serverbee-agent`
Expected: all existing tests pass (2 TCP ping tests)

- [ ] **Step 5: Add tests for `probe_icmp_batch` parser**

Add to bottom of `probe_utils.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_batch_output_success() {
        let output = "PING 1.1.1.1 (1.1.1.1): 56 data bytes\n\n--- 1.1.1.1 ping statistics ---\n10 packets transmitted, 9 received, 10% packet loss, time 9013ms\nrtt min/avg/max/mdev = 1.234/5.678/9.012/2.345 ms";
        let result = parse_ping_batch_output(output, 10);
        assert_eq!(result.packet_sent, 10);
        assert_eq!(result.packet_received, 9);
        assert!((result.packet_loss - 0.1).abs() < 0.01);
        assert!((result.min_latency.unwrap() - 1.234).abs() < 0.001);
        assert!((result.avg_latency.unwrap() - 5.678).abs() < 0.001);
        assert!((result.max_latency.unwrap() - 9.012).abs() < 0.001);
    }

    #[test]
    fn test_parse_batch_output_total_loss() {
        let output = "PING 192.0.2.1 (192.0.2.1): 56 data bytes\n\n--- 192.0.2.1 ping statistics ---\n10 packets transmitted, 0 received, 100% packet loss, time 9999ms";
        let result = parse_ping_batch_output(output, 10);
        assert_eq!(result.packet_received, 0);
        assert!((result.packet_loss - 1.0).abs() < 0.01);
        assert!(result.avg_latency.is_none());
        assert!(result.min_latency.is_none());
        assert!(result.max_latency.is_none());
    }

    #[test]
    fn test_parse_ping_time_extracts_latency() {
        assert!((parse_ping_time("time=12.34 ms").unwrap() - 12.34).abs() < 0.001);
        assert!((parse_ping_time("time=0.5 ms").unwrap() - 0.5).abs() < 0.001);
        assert!(parse_ping_time("no match here").is_none());
    }
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p serverbee-agent`
Expected: all tests pass

- [ ] **Step 7: Commit**

```bash
git add crates/agent/src/probe_utils.rs crates/agent/src/pinger.rs crates/agent/src/main.rs
git commit -m "refactor(agent): extract shared probe utils from pinger"
```

---

### Task 6: Implement NetworkProber module

**Files:**
- Create: `crates/agent/src/network_prober.rs`
- Modify: `crates/agent/src/main.rs` (add `mod network_prober`)

- [ ] **Step 1: Create `network_prober.rs`**

Implement `NetworkProber` struct following `PingManager` patterns:

```rust
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use chrono::Utc;
use rand::Rng;
use serverbee_common::constants::{has_capability, CAP_PING_HTTP, CAP_PING_ICMP, CAP_PING_TCP};
use serverbee_common::types::{NetworkProbeResultData, NetworkProbeTarget};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

use crate::probe_utils::{probe_icmp_batch, probe_tcp, probe_http};

pub struct NetworkProber {
    tasks: HashMap<String, RunningTask>,
    tx: mpsc::Sender<NetworkProbeResultData>,
    capabilities: Arc<AtomicU32>,
    // Store last-received config for re-sync on capability changes
    last_targets: Vec<NetworkProbeTarget>,
    last_interval: u32,
    last_packet_count: u32,
}

struct RunningTask {
    handle: JoinHandle<()>,
    target: NetworkProbeTarget,
    interval: u32,
    packet_count: u32,
}

impl NetworkProber {
    pub fn new(tx: mpsc::Sender<NetworkProbeResultData>, capabilities: Arc<AtomicU32>) -> Self {
        Self {
            tasks: HashMap::new(),
            tx,
            capabilities,
            last_targets: Vec::new(),
            last_interval: 60,
            last_packet_count: 10,
        }
    }

    /// Re-sync with updated capabilities using stored config
    pub fn resync_capabilities(&mut self) {
        let targets = self.last_targets.clone();
        let interval = self.last_interval;
        let packet_count = self.last_packet_count;
        self.sync(targets, interval, packet_count);
    }

    pub fn sync(&mut self, targets: Vec<NetworkProbeTarget>, interval: u32, packet_count: u32) {
        // Store config for later re-sync on capability changes
        self.last_targets = targets.clone();
        self.last_interval = interval;
        self.last_packet_count = packet_count;

        let caps = self.capabilities.load(Ordering::Relaxed);

        // Determine which targets are allowed by capabilities
        let allowed: Vec<_> = targets.into_iter().filter(|t| {
            match t.probe_type.as_str() {
                "icmp" => has_capability(caps, CAP_PING_ICMP),
                "tcp" => has_capability(caps, CAP_PING_TCP),
                "http" => has_capability(caps, CAP_PING_HTTP),
                _ => false,
            }
        }).collect();

        let new_ids: std::collections::HashSet<_> = allowed.iter().map(|t| t.target_id.clone()).collect();

        // Stop tasks no longer in list
        let to_remove: Vec<_> = self.tasks.keys()
            .filter(|id| !new_ids.contains(*id))
            .cloned().collect();
        for id in to_remove {
            if let Some(task) = self.tasks.remove(&id) {
                task.handle.abort();
                tracing::debug!("Stopped network probe task {id}");
            }
        }

        // Start or restart tasks
        for target in allowed {
            let needs_restart = self.tasks.get(&target.target_id)
                .map(|t| t.interval != interval || t.packet_count != packet_count || t.target.target != target.target)
                .unwrap_or(true);

            if needs_restart {
                if let Some(old) = self.tasks.remove(&target.target_id) {
                    old.handle.abort();
                }
                let handle = tokio::spawn(run_probe_task(
                    target.clone(), interval, packet_count, self.tx.clone(),
                ));
                self.tasks.insert(target.target_id.clone(), RunningTask {
                    handle, target, interval, packet_count,
                });
            }
        }
    }

    pub fn stop_all(&mut self) {
        for (id, task) in self.tasks.drain() {
            task.handle.abort();
            tracing::debug!("Stopped network probe task {id}");
        }
    }
}

async fn run_probe_task(
    target: NetworkProbeTarget,
    interval_secs: u32,
    packet_count: u32,
    tx: mpsc::Sender<NetworkProbeResultData>,
) {
    // Initial jitter to prevent synchronized probing
    let jitter = rand::thread_rng().gen_range(0..interval_secs);
    tokio::time::sleep(Duration::from_secs(jitter as u64)).await;

    let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs as u64));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    ticker.tick().await; // consume first immediate tick

    loop {
        ticker.tick().await;

        let result = match target.probe_type.as_str() {
            "icmp" => {
                let batch = probe_icmp_batch(&target.target, packet_count, Duration::from_secs(30)).await;
                NetworkProbeResultData {
                    target_id: target.target_id.clone(),
                    avg_latency: batch.avg_latency,
                    min_latency: batch.min_latency,
                    max_latency: batch.max_latency,
                    packet_loss: batch.packet_loss,
                    packet_sent: batch.packet_sent,
                    packet_received: batch.packet_received,
                    timestamp: Utc::now(),
                }
            }
            "tcp" | "http" => {
                run_multi_probe(&target, packet_count).await
            }
            _ => continue,
        };

        if tx.send(result).await.is_err() {
            tracing::debug!("Network probe result channel closed for {}", target.target_id);
            break;
        }
    }
}

async fn run_multi_probe(target: &NetworkProbeTarget, count: u32) -> NetworkProbeResultData {
    let mut latencies = Vec::new();
    let mut success_count = 0u32;
    let timeout = Duration::from_secs(10);

    for _ in 0..count {
        let result = match target.probe_type.as_str() {
            "tcp" => {
                let (host, port) = parse_host_port(&target.target);
                probe_tcp(host, port, timeout).await
            }
            "http" => probe_http(&target.target, timeout).await,
            _ => unreachable!(),
        };
        if result.success {
            success_count += 1;
            latencies.push(result.latency_ms);
        }
    }

    let packet_loss = 1.0 - (success_count as f64 / count as f64);
    let (avg, min, max) = if latencies.is_empty() {
        (None, None, None)
    } else {
        let sum: f64 = latencies.iter().sum();
        let min = latencies.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = latencies.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        (Some(sum / latencies.len() as f64), Some(min), Some(max))
    };

    NetworkProbeResultData {
        target_id: target.target_id.clone(),
        avg_latency: avg,
        min_latency: min,
        max_latency: max,
        packet_loss,
        packet_sent: count,
        packet_received: success_count,
        timestamp: Utc::now(),
    }
}

fn parse_host_port(target: &str) -> (&str, u16) {
    if let Some((host, port_str)) = target.rsplit_once(':') {
        if let Ok(port) = port_str.parse() {
            return (host, port);
        }
    }
    (target, 80)
}
```

- [ ] **Step 2: Add `mod network_prober` to `main.rs`**

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p serverbee-agent`
Expected: compiles with 0 errors

- [ ] **Step 4: Commit**

```bash
git add crates/agent/src/network_prober.rs crates/agent/src/main.rs
git commit -m "feat(agent): implement NetworkProber module"
```

---

### Task 7: Integrate NetworkProber into Reporter

**Files:**
- Modify: `crates/agent/src/reporter.rs`

- [ ] **Step 1: Add imports and create NetworkProber**

In `reporter.rs`, add imports for `NetworkProber` and `NetworkProbeResultData`. In the `connect_and_report` method, after creating `PingManager`, create:

```rust
let (network_probe_tx, mut network_probe_rx) = mpsc::channel::<NetworkProbeResultData>(256);
let mut network_prober = NetworkProber::new(network_probe_tx, capabilities.clone());
```

- [ ] **Step 2: Add 6th arm to `tokio::select!`**

Add after the terminal events arm:

```rust
Some(probe_result) = network_probe_rx.recv() => {
    let msg = AgentMessage::NetworkProbeResults {
        results: vec![probe_result],
    };
    let json = serde_json::to_string(&msg)?;
    write.send(Message::Text(json.into())).await?;
    tracing::debug!("Sent NetworkProbeResult");
}
```

- [ ] **Step 3: Handle `ServerMessage::NetworkProbeSync` in `handle_server_message`**

First, update `handle_server_message` signature to accept `network_prober: &mut NetworkProber` as an additional parameter. Update the call site in the `tokio::select!` server_msg arm accordingly.

Add match arm:

```rust
ServerMessage::NetworkProbeSync { targets, interval, packet_count } => {
    network_prober.sync(targets, interval, packet_count);
    tracing::info!("Synced {} network probe targets", targets.len());
}
```

- [ ] **Step 4: Handle `CapabilitiesSync` to re-sync network prober**

In the existing `CapabilitiesSync` handler, after updating the atomic capabilities, add:

```rust
network_prober.resync_capabilities();
```

This calls `resync_capabilities()` which re-runs `sync()` with the stored last-received targets/interval/packet_count, applying the updated capability filter.

- [ ] **Step 5: Stop network prober on disconnect**

In all disconnect/error paths where `ping_manager.stop_all()` and `terminal_manager.close_all()` are called, add:

```rust
network_prober.stop_all();
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check -p serverbee-agent`
Expected: compiles with 0 errors

- [ ] **Step 7: Commit**

```bash
git add crates/agent/src/reporter.rs
git commit -m "feat(agent): integrate NetworkProber into Reporter select loop"
```

---

## Chunk 3: Server Backend — Service + API + WS + Tasks

### Task 8: Implement NetworkProbeService

**Files:**
- Create: `crates/server/src/service/network_probe.rs`
- Modify: `crates/server/src/service/mod.rs`

- [ ] **Step 1: Create `network_probe.rs` service with target management**

Implement `NetworkProbeService` with `list_targets`, `create_target`, `update_target`, `delete_target`, `get_setting`, `update_setting`, `get_server_targets`, `set_server_targets`, `apply_defaults`.

Key patterns to follow:
- Use `sea_orm` queries (same as `PingService`)
- Use `ConfigService::get_typed/set_typed` for settings with key `"network_probe_setting"`
- Define `NetworkProbeSetting` struct:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkProbeSetting {
    pub interval: u32,
    pub packet_count: u32,
    pub default_target_ids: Vec<String>,
}

impl Default for NetworkProbeSetting {
    fn default() -> Self {
        Self { interval: 60, packet_count: 10, default_target_ids: vec![] }
    }
}
```

- Define `NetworkProbeAnomaly` as a service-local DTO (not in common crate, only used in REST API responses):

```rust
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct NetworkProbeAnomaly {
    pub timestamp: DateTime<Utc>,
    pub target_id: String,
    pub target_name: String,
    pub anomaly_type: String,
    pub value: f64,
}
```
- `set_server_targets`: delete all existing + insert new, enforce max 20 limit
- `delete_target`: cascade cleanup + remove from default_target_ids

- [ ] **Step 2: Add record methods**

Add `save_results`, `query_records` (smart interval: <1d raw, 1-30d hourly, >30d hourly), `get_server_summary` (with last_probe_at), `get_overview`, `get_anomalies`.

Note: In `save_results`, `packet_sent` and `packet_received` need `as i32` casts when converting from `NetworkProbeResultData` (which uses `u32`) to the entity `ActiveModel` (which uses `i32`, matching sea-orm's INTEGER mapping).

- [ ] **Step 3: Add aggregation and cleanup methods**

Add `aggregate_hourly` (uses INSERT OR REPLACE with unique constraint) and `cleanup_old_records` (reads retention from config).

- [ ] **Step 4: Add unit tests**

Test `NetworkProbeSetting` default values, serialization, and basic service method signatures.

- [ ] **Step 5: Register in `service/mod.rs`**

Add: `pub mod network_probe;`

- [ ] **Step 6: Verify compilation**

Run: `cargo check -p serverbee-server`

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/service/network_probe.rs crates/server/src/service/mod.rs
git commit -m "feat(server): implement NetworkProbeService"
```

---

### Task 9: Implement API routes

**Files:**
- Create: `crates/server/src/router/api/network_probe.rs`
- Modify: `crates/server/src/router/api/mod.rs`
- Modify: `crates/server/src/router/api/server.rs` (add per-server network probe routes)

- [ ] **Step 1: Create `network_probe.rs` with global routes**

Implement route handlers:
- `GET /api/network-probes/targets` → `list_targets`
- `POST /api/network-probes/targets` → `create_target`
- `PUT /api/network-probes/targets/{id}` → `update_target`
- `DELETE /api/network-probes/targets/{id}` → `delete_target`
- `GET /api/network-probes/setting` → `get_setting`
- `PUT /api/network-probes/setting` → `update_setting`
- `GET /api/network-probes/overview` → `get_overview`

Expose `read_router()` and `write_router()` following existing pattern.

- [ ] **Step 2: Add per-server routes to `server.rs`**

Add to `server.rs` read_router:
- `GET /api/servers/{id}/network-probes/targets`
- `GET /api/servers/{id}/network-probes/records`
- `GET /api/servers/{id}/network-probes/summary`
- `GET /api/servers/{id}/network-probes/anomalies`

Add to `server.rs` write_router:
- `PUT /api/servers/{id}/network-probes/targets`

- [ ] **Step 3: Register in `router/api/mod.rs`**

Add `pub mod network_probe;` and merge into the existing router composition at the exact positions:
- `network_probe::read_router()` → alongside `server::read_router()` and `ping::read_router()` in the non-admin protected block
- `network_probe::write_router()` → alongside `server::write_router()` and `ping::write_router()` inside the `require_admin` block

- [ ] **Step 4: Add `#[utoipa::path]` annotations**

Add OpenAPI annotations to all handlers following existing patterns.

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p serverbee-server`

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/router/api/
git commit -m "feat(server): add network probe API routes"
```

---

### Task 10: WebSocket integration and background tasks

**Files:**
- Modify: `crates/server/src/router/ws/agent.rs`
- Modify: `crates/server/src/task/aggregator.rs`
- Modify: `crates/server/src/task/cleanup.rs`
- Modify: `crates/server/src/router/api/agent.rs` (registration hook)

- [ ] **Step 1: Agent WS handler — send `NetworkProbeSync` on connect**

In `ws/agent.rs`, after the existing `PingTasksSync` message is sent, add logic to query this server's network probe targets and settings, then send `NetworkProbeSync`.

- [ ] **Step 2: Agent WS handler — handle `NetworkProbeResults`**

Add match arm for `AgentMessage::NetworkProbeResults { results }`:
1. Call `NetworkProbeService::save_results(db, server_id, results)` in a single transaction
2. Broadcast `BrowserMessage::NetworkProbeUpdate { server_id, results }` via `state.browser_tx`

- [ ] **Step 3: Extend aggregator task**

In `task/aggregator.rs`, add call to `NetworkProbeService::aggregate_hourly(db)`.

- [ ] **Step 4: Extend cleanup task**

In `task/cleanup.rs`, add call to `NetworkProbeService::cleanup_old_records(db, &config.retention)`.

- [ ] **Step 5: Agent registration hook**

In `router/api/agent.rs`, after the server record is created in the register handler, call `NetworkProbeService::apply_defaults(db, server_id)`.

- [ ] **Step 6: Verify compilation**

Run: `cargo check -p serverbee-server`

- [ ] **Step 7: Run existing tests**

Run: `cargo test --workspace`
Expected: all existing tests pass

- [ ] **Step 8: Commit**

```bash
git add crates/server/src/router/ws/agent.rs crates/server/src/task/ crates/server/src/router/api/agent.rs
git commit -m "feat(server): integrate network probes into WS handlers and background tasks"
```

---

### Task 11: Integration tests

**Files:**
- Modify: `crates/server/tests/integration.rs` (or create new test file)

- [ ] **Step 1: Write integration test for target CRUD**

Test create custom target → list targets → update → delete flow via API.

- [ ] **Step 2: Write integration test for probe config flow**

Test set server targets → get server targets → verify config persisted.

- [ ] **Step 3: Write integration test for probe result storage**

Simulate agent sending `NetworkProbeResults` → verify records in DB → query records API.

- [ ] **Step 4: Run integration tests**

Run: `cargo test -p serverbee-server --test integration`

- [ ] **Step 5: Commit**

```bash
git add crates/server/tests/
git commit -m "test(server): add network probe integration tests"
```

---

### Task 12: Alert integration for network metrics

**Files:**
- Modify: `crates/server/src/service/alert.rs`
- Modify: `apps/web/src/routes/_authed/settings/alerts.tsx`

- [ ] **Step 1: Add new `rule_type` values to `AlertRuleItem` handling**

In `service/alert.rs`, in the `extract_metric` function (or `check_server` function), add match arms for:
- `"network_latency"` — query `network_probe_record` for the server's recent records, compute the worst (highest) avg_latency across all targets
- `"network_packet_loss"` — query `network_probe_record` for the server's recent records, compute the worst (highest) packet_loss across all targets

These follow the same sampling window pattern as existing metrics (e.g., `ALERT_SAMPLE_MINUTES` and `ALERT_TRIGGER_RATIO`).

- [ ] **Step 2: Update frontend alert rule creation**

In `settings/alerts.tsx`, add "Network Latency" and "Network Packet Loss" to the metric type dropdown options, so users can create alert rules with `network_latency` and `network_packet_loss` rule types.

- [ ] **Step 3: Verify compilation and existing alert tests pass**

Run: `cargo test -p serverbee-server -- alert`
Expected: all alert tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/service/alert.rs apps/web/src/routes/_authed/settings/alerts.tsx
git commit -m "feat: add network_latency and network_packet_loss alert rule types"
```

---

## Chunk 4: Frontend — Hooks + Pages + i18n

### Task 13: Add API hooks and types

**Files:**
- Create: `apps/web/src/hooks/use-network-api.ts`
- Create: `apps/web/src/lib/network-types.ts`

- [ ] **Step 1: Create `network-types.ts`**

Define TypeScript interfaces matching the API response shapes:

```typescript
export interface NetworkProbeTarget {
  id: string
  name: string
  provider: string
  location: string
  target: string
  probe_type: string
  is_builtin: boolean
  created_at: string
  updated_at: string
}

export interface NetworkProbeSetting {
  interval: number
  packet_count: number
  default_target_ids: string[]
}

export interface NetworkProbeRecord {
  id: number
  server_id: string
  target_id: string
  avg_latency: number | null
  min_latency: number | null
  max_latency: number | null
  packet_loss: number
  packet_sent: number
  packet_received: number
  timestamp: string
}

export interface NetworkServerSummary {
  server_id: string
  server_name: string
  online: boolean
  last_probe_at: string | null
  targets: NetworkTargetSummary[]
  anomaly_count: number
}

export interface NetworkTargetSummary {
  target_id: string
  target_name: string
  provider: string
  avg_latency: number | null
  packet_loss: number
  availability: number
}

export interface NetworkProbeAnomaly {
  timestamp: string
  target_id: string
  target_name: string
  anomaly_type: string
  value: number
}
```

- [ ] **Step 2: Create `use-network-api.ts`**

Implement TanStack Query hooks:

```typescript
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { api } from '@/lib/api-client'
import type { NetworkProbeTarget, NetworkProbeSetting, NetworkProbeRecord, NetworkServerSummary, NetworkProbeAnomaly } from '@/lib/network-types'

export function useNetworkTargets() {
  return useQuery<NetworkProbeTarget[]>({
    queryKey: ['network-probes', 'targets'],
    queryFn: () => api.get('/api/network-probes/targets'),
  })
}

export function useNetworkSetting() {
  return useQuery<NetworkProbeSetting>({
    queryKey: ['network-probes', 'setting'],
    queryFn: () => api.get('/api/network-probes/setting'),
  })
}

export function useNetworkOverview() {
  return useQuery<NetworkServerSummary[]>({
    queryKey: ['network-probes', 'overview'],
    queryFn: () => api.get('/api/network-probes/overview'),
    refetchInterval: 60_000,
  })
}

export function useNetworkServerSummary(serverId: string) {
  return useQuery<NetworkServerSummary>({
    queryKey: ['servers', serverId, 'network-probes', 'summary'],
    queryFn: () => api.get(`/api/servers/${serverId}/network-probes/summary`),
    enabled: serverId.length > 0,
  })
}

export function useNetworkRecords(serverId: string, hours: number, options?: { targetId?: string; enabled?: boolean }) {
  return useQuery<NetworkProbeRecord[]>({
    queryKey: ['servers', serverId, 'network-probes', 'records', hours, options?.targetId],
    queryFn: () => {
      const now = new Date()
      const from = new Date(now.getTime() - hours * 3600 * 1000).toISOString()
      const to = now.toISOString()
      let url = `/api/servers/${serverId}/network-probes/records?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`
      if (options?.targetId) url += `&target_id=${encodeURIComponent(options.targetId)}`
      return api.get(url)
    },
    enabled: serverId.length > 0 && (options?.enabled ?? true),
    refetchInterval: 60_000,
  })
}

export function useNetworkAnomalies(serverId: string, hours: number) {
  return useQuery<NetworkProbeAnomaly[]>({
    queryKey: ['servers', serverId, 'network-probes', 'anomalies', hours],
    queryFn: () => {
      const now = new Date()
      const from = new Date(now.getTime() - hours * 3600 * 1000).toISOString()
      const to = now.toISOString()
      return api.get(`/api/servers/${serverId}/network-probes/anomalies?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`)
    },
    enabled: serverId.length > 0,
  })
}

// Mutation hooks for admin operations — example pattern for all mutations:
export function useCreateTarget() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: { name: string; provider: string; location: string; target: string; probe_type: string }) =>
      api.post('/api/network-probes/targets', input),
    onSuccess: () => { queryClient.invalidateQueries({ queryKey: ['network-probes', 'targets'] }) },
  })
}

// Follow the same pattern for remaining mutations:
export function useUpdateTarget() { /* PUT /api/network-probes/targets/{id}, invalidate targets */ }
export function useDeleteTarget() { /* DELETE /api/network-probes/targets/{id}, invalidate targets */ }
export function useUpdateSetting() { /* PUT /api/network-probes/setting, invalidate setting */ }
export function useSetServerTargets(serverId: string) { /* PUT /api/servers/{id}/network-probes/targets, invalidate server targets */ }
```

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/lib/network-types.ts apps/web/src/hooks/use-network-api.ts
git commit -m "feat(web): add network probe API hooks and types"
```

---

### Task 14: Add WebSocket integration for real-time updates

**Files:**
- Modify: `apps/web/src/hooks/use-servers-ws.ts`
- Create: `apps/web/src/hooks/use-network-realtime.ts`

- [ ] **Step 1: Handle `NetworkProbeUpdate` in existing WS hook**

In `use-servers-ws.ts`:
1. Add `| { type: 'network_probe_update'; server_id: string; results: NetworkProbeResultData[] }` to the `WsMessage` type union (import `NetworkProbeResultData` from `@/lib/network-types`)
2. Add a case for `network_probe_update` in the message handler switch. When received, update React Query cache for the affected server's network data and dispatch a custom event for the realtime hook.

- [ ] **Step 2: Create `use-network-realtime.ts`**

Implement a hook that maintains a ring buffer (200 points, trims at 250) of real-time network probe data per server, similar to `use-realtime-metrics.ts`.

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/hooks/
git commit -m "feat(web): add real-time network probe WebSocket integration"
```

---

### Task 15: Create i18n translation files

**Files:**
- Create: `apps/web/src/locales/en/network.json`
- Create: `apps/web/src/locales/zh/network.json`
- Modify: `apps/web/src/lib/i18n.ts` (register namespace)

- [ ] **Step 1: Create English translations**

- [ ] **Step 2: Create Chinese translations**

- [ ] **Step 3: Register `network` namespace in i18n config**

In `apps/web/src/lib/i18n.ts`:
- Add imports: `import enNetwork from '@/locales/en/network.json'` and `import zhNetwork from '@/locales/zh/network.json'`
- Add to `resources.en`: `network: enNetwork`
- Add to `resources.zh`: `network: zhNetwork`
- Follow the exact pattern used for existing namespaces (e.g., `dashboard`, `servers`)

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/locales/ apps/web/src/lib/i18n.ts
git commit -m "feat(web): add network quality i18n translations"
```

---

### Task 16: Create Network Overview page

**Files:**
- Create: `apps/web/src/routes/_authed/network/index.tsx`

- [ ] **Step 1: Create overview page with stats bar, anomaly banner, and VPS card list**

Page layout:
- Top stats bar: total VPS, online, anomaly count
- Anomaly banner (if any anomalies)
- Searchable VPS card list, each showing: name, online status, avg latency, availability %, target count
- Click card → navigate to `/network/${serverId}`

Uses `useNetworkOverview()` hook + WebSocket real-time updates.

- [ ] **Step 2: Verify dev server renders the page**

Run: `cd apps/web && bun run dev`
Navigate to `/network`, verify page renders.

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/routes/_authed/network/
git commit -m "feat(web): add network quality overview page"
```

---

### Task 17: Create Network Detail page

**Files:**
- Create: `apps/web/src/routes/_authed/network/$serverId.tsx`
- Create: `apps/web/src/components/network/target-card.tsx`
- Create: `apps/web/src/components/network/latency-chart.tsx`
- Create: `apps/web/src/components/network/anomaly-table.tsx`

- [ ] **Step 1: Create `target-card.tsx` component**

Card showing: target name (color-coded), latency value, packet loss %, eye toggle icon.

- [ ] **Step 2: Create `latency-chart.tsx` component**

Recharts multi-line AreaChart:
- One Line per visible target, color-coded
- NULL latency renders as gap (use `connectNulls={false}`)
- Tooltip with timestamp + all target values
- Supports both realtime (ring buffer) and historical data

- [ ] **Step 3: Create `anomaly-table.tsx` component**

Table with columns: time, target, type (badge), value.

- [ ] **Step 4: Create detail page `$serverId.tsx`**

Compose the page:
- Header with VPS name + status + time range selector
- VPS info bar (IPv4, region, etc.)
- Target cards row
- Latency chart
- Bottom stats bar
- Anomaly table
- Admin "Manage Targets" dialog
- CSV export button (frontend-only, Blob-based, exports current time range data)

Uses `useNetworkServerSummary()`, `useNetworkRecords()`, `useNetworkAnomalies()`, `useNetworkRealtime()`.

- [ ] **Step 5: Verify dev server renders**

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/routes/_authed/network/ apps/web/src/components/network/
git commit -m "feat(web): add network quality detail page with charts"
```

---

### Task 18: Create Settings page for network probes

**Files:**
- Create: `apps/web/src/routes/_authed/settings/network-probes.tsx`

- [ ] **Step 1: Create settings page with two tabs**

Tab 1: Target Management — table of targets, add/edit/delete dialogs
Tab 2: Global Settings — interval, packet count, default targets multi-select

- [ ] **Step 2: Commit**

```bash
git add apps/web/src/routes/_authed/settings/network-probes.tsx
git commit -m "feat(web): add network probe settings page"
```

---

### Task 19: Add sidebar navigation and final wiring

**Files:**
- Modify: `apps/web/src/components/layout/sidebar.tsx`
- Modify: `apps/web/src/locales/en/common.json`
- Modify: `apps/web/src/locales/zh/common.json`

- [ ] **Step 1: Add nav items to sidebar**

Add after the "Servers" item:
```typescript
{ to: '/network', labelKey: 'nav_network' as const, icon: Wifi },
```

Add in settings section:
```typescript
{ to: '/settings/network-probes', labelKey: 'nav_network_probes' as const, icon: Globe, adminOnly: true },
```

Import `Wifi` and `Globe` from `lucide-react`. (Don't use `Radar` — it's already used for the sidebar logo.)

- [ ] **Step 2: Add translation keys**

In `common.json` (en): `"nav_network": "Network", "nav_network_probes": "Network Probes"`
In `common.json` (zh): `"nav_network": "网络质量", "nav_network_probes": "网络探测"`

- [ ] **Step 3: Run frontend type check and lint**

Run: `cd apps/web && bun run typecheck && bun x ultracite check`
Expected: 0 errors

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/components/layout/sidebar.tsx apps/web/src/locales/
git commit -m "feat(web): add network quality navigation and translations"
```

---

### Task 20: Frontend tests

**Files:**
- Create: `apps/web/src/hooks/use-network-api.test.ts`

- [ ] **Step 1: Write tests for API hooks**

Test query key generation, enabled flag behavior, URL construction with from/to params.

- [ ] **Step 2: Run frontend tests**

Run: `cd apps/web && bun run test`
Expected: all tests pass

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/hooks/use-network-api.test.ts
git commit -m "test(web): add network probe API hook tests"
```

---

### Task 21: Full-stack verification

- [ ] **Step 1: Build and run full stack**

```bash
cargo build --workspace
cd apps/web && bun run build
cargo run -p serverbee-server
```

- [ ] **Step 2: Verify migration runs on startup**

Check server logs for successful migration.

- [ ] **Step 3: Verify builtin targets are seeded**

```bash
curl http://localhost:9527/api/network-probes/targets | jq
```

Expected: 12 builtin targets returned.

- [ ] **Step 4: Run all tests**

```bash
cargo test --workspace
cd apps/web && bun run test
```

- [ ] **Step 5: Run lints**

```bash
cargo clippy --workspace -- -D warnings
cd apps/web && bun x ultracite check && bun run typecheck
```

- [ ] **Step 6: Final commit**

```bash
git add -A
git commit -m "feat: complete network quality monitoring implementation"
```

---

## Update Documentation

### Task 22: Update project documentation

**Files:**
- Modify: `TESTING.md` — add network probe test counts and commands
- Modify: `ENV.md` — add `SERVERBEE_RETENTION__NETWORK_PROBE_DAYS` and `SERVERBEE_RETENTION__NETWORK_PROBE_HOURLY_DAYS`
- Modify: `apps/docs/content/docs/en/configuration.mdx` — add new env vars
- Modify: `apps/docs/content/docs/cn/configuration.mdx` — add new env vars (Chinese)

- [ ] **Step 1: Update TESTING.md**
- [ ] **Step 2: Update ENV.md**
- [ ] **Step 3: Update Fumadocs configuration pages**

Per CLAUDE.md requirement: "When adding/changing env vars, update `ENV.md` and `apps/docs/content/docs/{en,cn}/configuration.mdx` simultaneously."

- [ ] **Step 4: Commit**

```bash
git add TESTING.md ENV.md
git commit -m "docs: update testing and env docs for network quality monitoring"
```
