# Docker Monitoring Design

## Overview

Add Docker container monitoring and management to ServerBee. The Agent connects to the local Docker daemon via bollard, reports container status, stats, logs, and events through the existing WebSocket channel to the Server, which caches data in memory and broadcasts to browser clients.

## Scope

### Phase 1 (this spec)

- Container list with real-time status
- Container management: start / stop / restart / remove
- Container real-time stats: CPU%, memory usage/limit, network rx/tx, block I/O
- Container log streaming
- Docker event stream
- Network list (read-only)
- Volume list (read-only)
- Docker system info (version, container counts, etc.)

### Future phases

- Image management (list / pull / remove / prune)
- Compose project management
- Container creation
- Network / volume write operations (create, delete)

## Architecture

### Agent: DockerManager

Independent manager at the same level as PingManager and TerminalManager. Not part of the Collector.

```
crates/agent/src/docker/
├── mod.rs              — DockerManager lifecycle, Docker client init
├── containers.rs       — Container list, stats polling, container actions
├── logs.rs             — Log stream session management
├── events.rs           — Docker event stream
├── networks.rs         — Network list query
├── volumes.rs          — Volume list query
└── types.rs            — Agent-side internal types (if needed)
```

#### DockerManager struct

```rust
pub struct DockerManager {
    docker: Docker,                              // bollard client (local socket)
    agent_tx: mpsc::Sender<AgentMessage>,        // sends back to Reporter main loop
    capabilities: Arc<AtomicU32>,                // shared capability bitmask

    // Container stats polling
    stats_interval: Option<Interval>,            // configurable interval

    // Active log stream sessions
    log_sessions: HashMap<String, JoinHandle<()>>,  // container_id → task

    // Docker event stream
    event_stream_handle: Option<JoinHandle<()>>,
}
```

#### Lifecycle

1. `DockerManager::new()` — attempts to connect to local Docker socket; returns `None` if Docker is not installed or not running.
2. Reporter initializes `Option<DockerManager>` in `connect_and_report()`.
3. Integrated into Reporter's `tokio::select!` loop via `docker_rx` channel.
4. On WebSocket disconnect, DockerManager cleans up all active sessions.

#### Reporter integration

```
Reporter main loop tokio::select! {
    report_interval  => collector.collect()         // system metrics
    ping_rx          => AgentMessage::PingResult     // ping
    terminal_rx      => AgentMessage::TerminalXxx    // terminal
    docker_rx        => AgentMessage::DockerXxx      // docker (new)
    server_msg       => handle_server_message()      // dispatch Docker commands
}
```

### Docker connection

Agent connects to the local Docker daemon via Unix socket (`/var/run/docker.sock` on Linux/macOS, named pipe on Windows). Uses the bollard crate (same as the existing dockerman project).

### Capability

```rust
// crates/common/src/constants.rs
pub const CAP_DOCKER: u32 = 1 << 7;  // 128

CapabilityMeta {
    bit: CAP_DOCKER,
    key: "docker",
    display_name: "Docker Management",
    default_enabled: false,
    risk_level: "high",
}

// Update CAP_VALID_MASK to 0b1111_1111 (255)
```

Default off. Agent checks `has_capability(CAP_DOCKER)` before processing any `ServerMessage::Docker*`. Returns `CapabilityDenied` if not granted.

### Graceful degradation

- **Docker available** → `DockerManager::new()` returns `Some(manager)`, reports `DockerInfo`
- **Docker unavailable** → returns `None`, Agent runs normally without Docker data
- **Docker stops at runtime** → DockerManager detects disconnect, sends notification, retries periodically

Server/frontend conditionally shows Docker Tab based on whether `DockerInfo` has been received for that server.

## Protocol Messages

### AgentMessage (Agent → Server)

```rust
// Container list snapshot (sent in two cases):
// 1. In response to ServerMessage::DockerListContainers (includes msg_id for request-response)
// 2. Periodically alongside stats when stats streaming is active (msg_id is None)
AgentMessage::DockerContainers {
    msg_id: Option<String>,
    containers: Vec<DockerContainer>,
}

// Container real-time stats
AgentMessage::DockerStats {
    stats: Vec<DockerContainerStats>,
}

// Container log output (streaming)
AgentMessage::DockerLog {
    container_id: String,
    session_id: String,
    entries: Vec<DockerLogEntry>,
}

// Docker event (streaming)
AgentMessage::DockerEvent {
    event: DockerEventInfo,
}

// Docker system info
AgentMessage::DockerInfo {
    msg_id: String,
    info: DockerSystemInfo,
}

// Network list response
AgentMessage::DockerNetworks {
    msg_id: String,
    networks: Vec<DockerNetwork>,
}

// Volume list response
AgentMessage::DockerVolumes {
    msg_id: String,
    volumes: Vec<DockerVolume>,
}

// Container action result
AgentMessage::DockerActionResult {
    msg_id: String,
    success: bool,
    error: Option<String>,
}
```

### ServerMessage (Server → Agent)

```rust
// Request container list (request-response)
ServerMessage::DockerListContainers { msg_id: String }

// Stats streaming control (fire-and-forget)
// Agent sends DockerContainers + DockerStats on start; absence of data
// after DockerStartStats implies Docker is unavailable — browser infers
// this from lack of updates and shows a "Docker unavailable" indicator.
ServerMessage::DockerStartStats { interval_secs: u32 }
ServerMessage::DockerStopStats

ServerMessage::DockerContainerAction {
    msg_id: String,
    container_id: String,
    action: DockerAction,  // Start, Stop, Restart, Remove
}

ServerMessage::DockerLogsStart {
    session_id: String,
    container_id: String,
    tail: Option<u64>,
    follow: bool,
}
ServerMessage::DockerLogsStop {
    session_id: String,
}

// Event streaming control (fire-and-forget, same inference model as stats)
ServerMessage::DockerEventsStart
ServerMessage::DockerEventsStop

ServerMessage::DockerGetInfo { msg_id: String }
ServerMessage::DockerListNetworks { msg_id: String }
ServerMessage::DockerListVolumes { msg_id: String }
```

### BrowserMessage (Server → Browser)

```rust
BrowserMessage::DockerUpdate {
    server_id: String,
    containers: Vec<DockerContainer>,
    stats: Option<Vec<DockerContainerStats>>,
}

BrowserMessage::DockerLog {
    server_id: String,
    session_id: String,
    entries: Vec<DockerLogEntry>,
}

BrowserMessage::DockerEvent {
    server_id: String,
    event: DockerEventInfo,
}
```

## Data Structures

```rust
pub struct DockerContainer {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,          // running, exited, paused, ...
    pub status: String,         // "Up 3 hours", "Exited (0) 5 min ago"
    pub created: i64,
    pub ports: Vec<DockerPort>,
    pub labels: HashMap<String, String>,
}

pub struct DockerContainerStats {
    pub id: String,
    pub name: String,
    pub cpu_percent: f64,
    pub memory_usage: u64,
    pub memory_limit: u64,
    pub memory_percent: f64,
    pub network_rx: u64,
    pub network_tx: u64,
    pub block_read: u64,
    pub block_write: u64,
}

pub struct DockerLogEntry {
    pub timestamp: Option<String>,
    pub stream: String,         // "stdout" | "stderr"
    pub message: String,
}

pub struct DockerEventInfo {
    pub timestamp: i64,
    pub event_type: String,     // "container", "image", "network", "volume"
    pub action: String,         // "start", "stop", "die", "create", ...
    pub actor_id: String,
    pub actor_name: Option<String>,
    pub attributes: HashMap<String, String>,
}

pub struct DockerSystemInfo {
    pub docker_version: String,
    pub api_version: String,
    pub os: String,
    pub arch: String,
    pub containers_running: i64,
    pub containers_paused: i64,
    pub containers_stopped: i64,
    pub images: i64,
    pub memory_total: u64,
}

pub struct DockerPort {
    pub private_port: u16,
    pub public_port: Option<u16>,
    pub port_type: String,       // "tcp" | "udp"
    pub ip: Option<String>,
}

pub struct DockerNetwork {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub scope: String,
    pub containers: HashMap<String, String>,  // container_id → name
}

pub struct DockerVolume {
    pub name: String,
    pub driver: String,
    pub mountpoint: String,
    pub created_at: Option<String>,
    pub labels: HashMap<String, String>,
}

pub enum DockerAction {
    Start,
    Stop { timeout: Option<i64> },
    Restart { timeout: Option<i64> },
    Remove { force: bool },
}
```

## Server

### Message handling

In `handle_agent_message()`:

- `DockerContainers` → cache in AgentManager + broadcast `BrowserMessage::DockerUpdate`. If `msg_id` is present, also dispatch via `pending_requests` for the initial REST request.
- `DockerStats` → cache in AgentManager + broadcast `BrowserMessage::DockerUpdate`
- `DockerLog` → forward directly via `BrowserMessage::DockerLog` (no storage)
- `DockerEvent` → save to `docker_event` table + broadcast `BrowserMessage::DockerEvent`
- `DockerInfo` → cache in AgentManager + send Ack
- `DockerNetworks` / `DockerVolumes` → dispatch via `pending_requests` (request-response pattern)
- `DockerActionResult` → dispatch via `pending_requests`

### AgentManager memory cache

```rust
// New fields
docker_containers: DashMap<String, Vec<DockerContainer>>,
docker_stats: DashMap<String, Vec<DockerContainerStats>>,
docker_info: DashMap<String, DockerSystemInfo>,
```

### REST API

```
GET    /api/servers/{id}/docker/containers              — container list (from cache)
GET    /api/servers/{id}/docker/stats                    — container stats (from cache)
GET    /api/servers/{id}/docker/info                     — Docker system info (from cache)
GET    /api/servers/{id}/docker/networks                 — network list (request-response via Agent)
GET    /api/servers/{id}/docker/volumes                  — volume list (request-response via Agent)
GET    /api/servers/{id}/docker/events                   — historical events (from database)
POST   /api/servers/{id}/docker/containers/{cid}/action  — container action (start/stop/restart/remove)
POST   /api/servers/{id}/docker/containers/{cid}/logs/start — start log stream (returns session_id)
DELETE /api/servers/{id}/docker/containers/{cid}/logs/{session_id} — stop log stream
```

Log streaming flow: browser calls `POST .../logs/start` with `{ tail, follow }` body, Server generates a `session_id`, sends `DockerLogsStart` to Agent, and returns `{ session_id }` to browser. Browser then receives log entries via the existing BrowserMessage WebSocket (`BrowserMessage::DockerLog` filtered by `session_id`). On close, browser calls `DELETE .../logs/{session_id}`, Server sends `DockerLogsStop` to Agent.

### Database

One new table for event history:

```sql
CREATE TABLE docker_event (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    server_id   TEXT NOT NULL,
    timestamp   INTEGER NOT NULL,
    event_type  TEXT NOT NULL,
    action      TEXT NOT NULL,
    actor_id    TEXT NOT NULL,
    actor_name  TEXT,
    attributes  TEXT,              -- JSON
    created_at  TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_docker_event_server_time ON docker_event(server_id, timestamp DESC);
```

Retention: 7 days, cleaned by the existing cleanup task.

### Data flow

```
Browser opens Docker tab
    ↓
React → GET /api/servers/{id}/docker/containers (initial load from cache)
React → subscribe WebSocket BrowserMessage::DockerUpdate (real-time updates)
    ↓
Server → ServerMessage::DockerStartStats { interval_secs: 3 }
Server → ServerMessage::DockerEventsStart
    ↓
Agent DockerManager:
    - polls container list + stats → AgentMessage::DockerContainers / DockerStats
    - Docker event stream → AgentMessage::DockerEvent
    ↓
Server receives → cache + broadcast
    ↓
Browser updates UI in real-time

User views logs:
    React → POST /api/servers/{id}/docker/containers/{cid}/logs/start { tail: 100, follow: true }
    Server generates session_id, returns { session_id } to browser
    Server → ServerMessage::DockerLogsStart { session_id, container_id, tail: 100, follow: true }
    Agent → docker.logs() stream → AgentMessage::DockerLog { session_id, entries }
    Server → BrowserMessage::DockerLog { session_id, entries } → browser terminal display
    User closes dialog → DELETE /api/servers/{id}/docker/containers/{cid}/logs/{session_id}
    Server → ServerMessage::DockerLogsStop { session_id }
```

## Frontend

### Entry point

Docker Tab inside the Server detail page, alongside Overview / Terminal / Files / Ping tabs. Only shown when the server has reported `DockerInfo`.

### Docker Tab layout (single-page overview)

Top to bottom:
1. **Overview cards** — Running count, Stopped count, Total CPU, Total Memory, Docker version
2. **Container list** — Table with Name, Image, Status, CPU, Memory, Net I/O, Actions. Search and filter (All/Running/Stopped). Click row to open detail Dialog.
3. **Recent events timeline** — Chronological list with timestamp, action badge (start/stop/die/create), actor name.
4. **Networks & Volumes** — Accessed via buttons or links, displayed in Dialog.

### Container detail Dialog (sectioned layout)

No tabs. Vertically stacked sections:
1. **Top: Meta info + actions** — Image, status, ports. Stop / Restart / Remove buttons.
2. **Middle: Real-time stats** — 4 mini cards: CPU (with sparkline), Memory (with usage bar), Net I/O, Block I/O.
3. **Bottom: Log terminal** — Monospace log output with Follow toggle and tail control.

### New frontend files

```
apps/web/src/routes/_authed/servers/$serverId/docker/
├── index.tsx                — Docker Tab main page
├── components/
│   ├── docker-overview.tsx  — Overview cards
│   ├── container-list.tsx   — Container table
│   ├── container-detail-dialog.tsx — Detail Dialog
│   ├── container-logs.tsx   — Log terminal component
│   ├── container-stats.tsx  — Stats mini cards
│   ├── docker-events.tsx    — Events timeline
│   ├── docker-networks-dialog.tsx  — Networks list Dialog
│   └── docker-volumes-dialog.tsx   — Volumes list Dialog
├── hooks/
│   ├── use-docker-ws.ts     — WebSocket subscription for Docker data
│   └── use-docker-logs.ts   — Log stream session management
└── types.ts                 — Frontend Docker types
```

## Testing

### Rust unit tests

- DockerManager: connection init, graceful degradation when Docker unavailable
- Protocol serialization/deserialization for all new message types
- Container stats calculation (CPU%, memory%)
- DockerService: event save/query
- Capability check for `CAP_DOCKER`

### Frontend tests (vitest)

- Docker Tab conditional rendering based on DockerInfo availability
- Container list rendering, filtering, search
- Container detail Dialog sections
- Docker events timeline rendering
- WebSocket message handling for Docker data

### Integration tests

- Agent ↔ Server Docker message round-trip
- Container action request-response flow
- Log stream start/stop lifecycle
- Docker event persistence and retrieval

## Reference

The existing dockerman Tauri application (`/Users/zingerbee/Bee/dockerman/app.dockerman`) serves as implementation reference, particularly:
- `src-tauri/src/commands/container.rs` — container list and actions using bollard
- `src-tauri/src/commands/stats.rs` — CPU/memory/network stats calculation
- `src-tauri/src/commands/logs.rs` — log stream with batching
- `src-tauri/src/commands/events.rs` — event stream with auto-reconnect
