# Docker Monitoring Design

## Overview

Add Docker container monitoring and management to ServerBee. The Agent connects to the local Docker daemon via bollard, reports container status, stats, logs, and events through the existing WebSocket channel to the Server, which caches data in memory and broadcasts to browser clients. Log streaming uses a dedicated WebSocket endpoint (same pattern as Terminal) to avoid polluting the global broadcast channel.

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
├── mod.rs              — DockerManager lifecycle, Docker client init, reconnect
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
    stats_interval: Option<Interval>,            // set by DockerStartStats, None when idle

    // Active log stream sessions — keyed by session_id (not container_id)
    // Multiple sessions can exist for the same container (different viewers)
    log_sessions: HashMap<String, JoinHandle<()>>,  // session_id → task

    // Docker event stream
    event_stream_handle: Option<JoinHandle<()>>,
}
```

#### Lifecycle and reconnection

1. `DockerManager::try_new()` — attempts to connect to local Docker socket. Returns `Ok(manager)` on success, `Err` on failure.
2. Reporter initializes Docker support in `connect_and_report()`:
   - On success: stores `DockerManager`, sends `AgentMessage::DockerInfo` proactively.
   - On failure: stores `None`, starts a background retry timer (30s interval).
3. **Retry loop**: When `docker_manager` is `None`, Reporter periodically calls `DockerManager::try_new()`. On success, sends `DockerInfo` and begins accepting Docker commands. This runs as an arm of the existing `tokio::select!` loop (a `docker_retry_interval` ticker).
4. **Runtime disconnect**: If the bollard client encounters a connection error during stats/events/logs, DockerManager sends `AgentMessage::DockerUnavailable { server_id }` and transitions to retry mode. All active log sessions and event streams are aborted.
5. On WebSocket disconnect (Agent ↔ Server), DockerManager cleans up all active sessions.

#### Reporter integration

```
Reporter main loop tokio::select! {
    report_interval       => collector.collect()         // system metrics
    ping_rx               => AgentMessage::PingResult     // ping
    terminal_rx           => AgentMessage::TerminalXxx    // terminal
    docker_rx             => AgentMessage::DockerXxx      // docker (new)
    docker_retry_interval => DockerManager::try_new()     // reconnect (only when None)
    server_msg            => handle_server_message()      // dispatch Docker commands
}
```

### Docker connection

Agent connects to the local Docker daemon via Unix socket (`/var/run/docker.sock` on Linux/macOS, named pipe on Windows). Uses the bollard crate (same as the existing dockerman project).

### Protocol version and feature negotiation

Bump `PROTOCOL_VERSION` from 2 to 3. Add a `features` field to `SystemInfo`:

```rust
pub struct SystemInfo {
    // ... existing fields ...
    pub protocol_version: u32,        // now 3
    pub features: Vec<String>,        // e.g. ["docker"]
}
```

Server uses `features` to determine Agent capabilities beyond the bitmask:
- `features` contains `"docker"` → Agent has Docker support compiled in and Docker daemon is reachable.
- Server persists `features` to the `servers` table (new `features TEXT` column, JSON array).
- Frontend reads `features` from server data to conditionally show Docker Tab.

This decouples "Agent binary supports Docker" (feature negotiation) from "admin allows Docker operations" (CAP_DOCKER capability). Both must be true for Docker to function.

Old agents (protocol_version < 3) will not send `features` and will not understand `ServerMessage::Docker*` — Server simply never sends Docker commands to them. The capabilities dialog shows a notice: "Agent upgrade required for Docker support".

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

**Two gates for Docker to work:**
1. Agent reports `"docker"` in `features` (binary supports it + daemon reachable)
2. Admin enables `CAP_DOCKER` for the server

### Graceful degradation

- **Docker available** → `DockerManager::try_new()` succeeds, `features` includes `"docker"`, sends `DockerInfo` proactively on connect
- **Docker unavailable at startup** → `DockerManager` is `None`, `features` does not include `"docker"`, retry timer active (30s)
- **Docker becomes available later** → retry succeeds, sends `DockerInfo`, `features` updated via new `AgentMessage::FeaturesUpdate`
- **Docker stops at runtime** → sends `AgentMessage::DockerUnavailable`, transitions to retry mode, `features` updated

## Protocol Messages

### AgentMessage (Agent → Server)

```rust
// Sent proactively on Agent connect when Docker is available.
// Also sent in response to DockerGetInfo (with msg_id).
AgentMessage::DockerInfo {
    msg_id: Option<String>,   // None when proactive, Some when request-response
    info: DockerSystemInfo,
}

// Container list snapshot (sent in two cases):
// 1. In response to ServerMessage::DockerListContainers (includes msg_id)
// 2. Periodically alongside stats when stats streaming is active (msg_id is None)
AgentMessage::DockerContainers {
    msg_id: Option<String>,
    containers: Vec<DockerContainer>,
}

// Container real-time stats
AgentMessage::DockerStats {
    stats: Vec<DockerContainerStats>,
}

// Container log output (streaming, delivered via dedicated WS, not global broadcast)
AgentMessage::DockerLog {
    session_id: String,
    entries: Vec<DockerLogEntry>,
}

// Docker event (streaming)
AgentMessage::DockerEvent {
    event: DockerEventInfo,
}

// Feature update (when Docker becomes available/unavailable at runtime)
AgentMessage::FeaturesUpdate {
    features: Vec<String>,
}

// Docker became unavailable at runtime
AgentMessage::DockerUnavailable

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

// Stats streaming control.
// Server manages viewer refcount (see "Stats/Events subscription lifecycle" below).
// Sends DockerStartStats when first viewer subscribes, DockerStopStats when last unsubscribes.
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

// Event streaming control (same refcount model as stats).
ServerMessage::DockerEventsStart
ServerMessage::DockerEventsStop

ServerMessage::DockerGetInfo { msg_id: String }
ServerMessage::DockerListNetworks { msg_id: String }
ServerMessage::DockerListVolumes { msg_id: String }
```

### BrowserMessage (Server → Browser)

```rust
// Container list + stats updates (via global /ws/servers broadcast)
BrowserMessage::DockerUpdate {
    server_id: String,
    containers: Vec<DockerContainer>,
    stats: Option<Vec<DockerContainerStats>>,
}

// Docker events (via global /ws/servers broadcast — low frequency, safe for broadcast)
BrowserMessage::DockerEvent {
    server_id: String,
    event: DockerEventInfo,
}

// Docker availability change
BrowserMessage::DockerAvailabilityChanged {
    server_id: String,
    available: bool,
}
```

**Note:** `DockerLog` is NOT a BrowserMessage. Logs are delivered via a dedicated WebSocket (see below).

## Log Streaming: Dedicated WebSocket

Docker container logs are high-volume and session-specific. Sending them through the global `browser_tx` broadcast would cause:
1. **Data leakage** — all authenticated browser clients receive all log data
2. **Back-pressure** — high-frequency logs fill the 256-slot broadcast buffer, triggering `Lagged` for all subscribers

Instead, logs use a dedicated WebSocket endpoint, following the same pattern as Terminal:

### Endpoint

```
WebSocket /api/ws/docker/logs?server_id={id}&container_id={cid}&tail={n}&follow={bool}
```

### Server-side flow

```rust
// router/ws/docker_logs.rs
async fn handle_docker_logs_ws(
    socket: WebSocket,
    state: Arc<AppState>,
    server_id: String,
    container_id: String,
    tail: Option<u64>,
    follow: bool,
) {
    let session_id = generate_session_id();
    let (output_tx, mut output_rx) = mpsc::channel::<Vec<DockerLogEntry>>(256);

    // Register session in AgentManager
    state.agent_manager.add_docker_log_session(session_id.clone(), output_tx);

    // Tell Agent to start streaming
    state.agent_manager.send_to_agent(&server_id, ServerMessage::DockerLogsStart {
        session_id: session_id.clone(),
        container_id,
        tail,
        follow,
    });

    // Relay loop: output_rx → WebSocket
    let (mut ws_sink, mut ws_stream) = socket.split();
    loop {
        tokio::select! {
            Some(entries) = output_rx.recv() => {
                ws_sink.send(Message::Text(serde_json::to_string(&entries)?)).await?;
            }
            Some(msg) = ws_stream.next() => {
                // Browser can send control messages (e.g., pause/resume)
                // or just close the connection
                if msg.is_err() || matches!(msg, Ok(Message::Close(_))) {
                    break;
                }
            }
        }
    }

    // Cleanup
    state.agent_manager.remove_docker_log_session(&session_id);
    state.agent_manager.send_to_agent(&server_id, ServerMessage::DockerLogsStop {
        session_id,
    });
}
```

### Agent-side handling

When Agent receives `DockerLogsStart`, DockerManager spawns a tokio task that:
1. Calls `docker.logs()` with the specified options
2. Batches log entries (max 50 entries or 50ms interval, same as dockerman)
3. Sends `AgentMessage::DockerLog { session_id, entries }` via the main WebSocket

Server's `handle_agent_message()` routes `DockerLog` to the registered session channel (not to `browser_tx`):

```rust
AgentMessage::DockerLog { session_id, entries } => {
    // Route to specific log session, NOT broadcast
    if let Some(tx) = state.agent_manager.get_docker_log_session(&session_id) {
        let _ = tx.send(entries).await;
    }
}
```

### Frontend

```
apps/web/src/hooks/use-docker-logs.ts

const useDockerLogs = (serverId, containerId, options) => {
    // Opens dedicated WS to /api/ws/docker/logs?...
    // Returns log entries stream
    // Auto-closes WS on unmount (triggers server-side cleanup)
}
```

## Stats/Events Subscription Lifecycle

Stats and events use the global broadcast channel (they are low-frequency: stats every 3s, events are sparse). But they need a proper control plane to start/stop Agent-side streaming.

### Server-side viewer refcount

```rust
// In AgentManager or a dedicated DockerSubscriptionManager
pub struct DockerViewerTracker {
    // server_id → set of viewer_ids (browser session IDs)
    stats_viewers: DashMap<String, HashSet<String>>,
    events_viewers: DashMap<String, HashSet<String>>,
}

impl DockerViewerTracker {
    /// Returns true if this is the first viewer (should send Start to Agent)
    pub fn add_stats_viewer(&self, server_id: &str, viewer_id: &str) -> bool;

    /// Returns true if this was the last viewer (should send Stop to Agent)
    pub fn remove_stats_viewer(&self, server_id: &str, viewer_id: &str) -> bool;

    // Same for events
}
```

### REST endpoints for subscription control

```
POST   /api/servers/{id}/docker/subscribe      — { viewer_id } → starts stats+events if first viewer
DELETE /api/servers/{id}/docker/subscribe       — { viewer_id } → stops stats+events if last viewer
```

### Frontend integration

```typescript
// When Docker tab mounts:
const viewerId = useRef(crypto.randomUUID())
useEffect(() => {
    api.post(`/servers/${serverId}/docker/subscribe`, { viewer_id: viewerId.current })
    return () => {
        api.delete(`/servers/${serverId}/docker/subscribe`, { viewer_id: viewerId.current })
    }
}, [serverId])
```

### Server-side cleanup

- When a browser WebSocket disconnects, Server removes all viewer registrations for that session.
- A periodic sweep (60s) removes stale viewers whose browser WS is no longer connected.
- `BrowserMessage::DockerUpdate` and `BrowserMessage::DockerEvent` go through the existing global broadcast — they are low-frequency and small payload, safe for the 256-slot buffer.

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

- `DockerInfo` → cache in AgentManager, persist `features` to `servers` table. If `msg_id` is present, dispatch via `pending_requests`. Broadcast `BrowserMessage::DockerAvailabilityChanged { available: true }`.
- `DockerContainers` → cache in AgentManager + broadcast `BrowserMessage::DockerUpdate`. If `msg_id` is present, also dispatch via `pending_requests`.
- `DockerStats` → cache in AgentManager + broadcast `BrowserMessage::DockerUpdate`
- `DockerLog` → route to specific log session channel via `agent_manager.get_docker_log_session()` (**not** broadcast)
- `DockerEvent` → save to `docker_event` table + broadcast `BrowserMessage::DockerEvent`
- `DockerUnavailable` → clear Docker caches for this server, broadcast `BrowserMessage::DockerAvailabilityChanged { available: false }`
- `FeaturesUpdate` → update `servers.features` in database, broadcast availability change
- `DockerNetworks` / `DockerVolumes` → dispatch via `pending_requests` (request-response pattern)
- `DockerActionResult` → dispatch via `pending_requests`

### AgentManager memory cache

```rust
// New fields
docker_containers: DashMap<String, Vec<DockerContainer>>,
docker_stats: DashMap<String, Vec<DockerContainerStats>>,
docker_info: DashMap<String, DockerSystemInfo>,

// Log session routing (session_id → channel, NOT container_id)
docker_log_sessions: DashMap<String, mpsc::Sender<Vec<DockerLogEntry>>>,

// Viewer tracking for stats/events subscription lifecycle
docker_viewers: DockerViewerTracker,
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
POST   /api/servers/{id}/docker/subscribe                — register viewer (start stats+events if first)
DELETE /api/servers/{id}/docker/subscribe                — unregister viewer (stop if last)
```

### WebSocket endpoints

```
WS /api/ws/servers                                      — existing, now also carries DockerUpdate + DockerEvent
WS /api/ws/docker/logs?server_id={id}&container_id={cid}&tail={n}&follow={bool}  — dedicated log stream (new)
```

### Database changes

**New table** for event history:

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

**Existing table modification** — add `features` column to `servers`:

```sql
ALTER TABLE servers ADD COLUMN features TEXT DEFAULT '[]';  -- JSON array of strings
```

Retention for `docker_event`: 7 days. **Requires explicit addition** to the existing cleanup task (`crates/server/src/task/cleanup.rs`) — add a `docker_event` cleanup branch alongside the existing `records`, `ping_record`, etc.

### Data flow

```
Agent connects to Server:
    Agent → DockerManager::try_new()
    If Docker available:
        Agent → AgentMessage::DockerInfo { msg_id: None, info }
        Server → cache DockerInfo, persist features=["docker"] to servers table
        Server → broadcast BrowserMessage::DockerAvailabilityChanged { available: true }
    If Docker unavailable:
        Agent starts 30s retry timer, features=[] (no "docker")

Browser opens Docker tab (only visible if server.features includes "docker"):
    React → POST /api/servers/{id}/docker/subscribe { viewer_id }
    Server → if first viewer for this server:
        Server → ServerMessage::DockerStartStats { interval_secs: 3 }
        Server → ServerMessage::DockerEventsStart
    React → GET /api/servers/{id}/docker/containers (initial load from cache)
    React → subscribe /ws/servers for BrowserMessage::DockerUpdate + DockerEvent
    ↓
Agent DockerManager:
    - polls container list + stats → AgentMessage::DockerContainers / DockerStats
    - Docker event stream → AgentMessage::DockerEvent
    ↓
Server receives → cache + broadcast via browser_tx
    ↓
Browser updates UI in real-time

Browser closes Docker tab:
    React → DELETE /api/servers/{id}/docker/subscribe { viewer_id }
    Server → if last viewer for this server:
        Server → ServerMessage::DockerStopStats
        Server → ServerMessage::DockerEventsStop

User views container logs (opens detail Dialog):
    React → opens WebSocket to /api/ws/docker/logs?server_id=X&container_id=Y&tail=100&follow=true
    Server → generates session_id, registers log session channel
    Server → ServerMessage::DockerLogsStart { session_id, container_id, tail, follow }
    Agent → docker.logs() stream → batches → AgentMessage::DockerLog { session_id, entries }
    Server → routes to session channel → dedicated WS → browser terminal display
    User closes Dialog → WebSocket closes
    Server → cleanup: remove session, send ServerMessage::DockerLogsStop { session_id }
```

## Frontend

### Entry point

Docker Tab inside the Server detail page, alongside Overview / Terminal / Files / Ping tabs. Only shown when the server's `features` array includes `"docker"` AND `CAP_DOCKER` is enabled.

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
3. **Bottom: Log terminal** — Monospace log output with Follow toggle and tail control. Uses dedicated WebSocket via `useDockerLogs` hook.

### New frontend files

```
apps/web/src/routes/_authed/servers/$serverId/docker/
├── index.tsx                — Docker Tab main page
├── components/
│   ├── docker-overview.tsx  — Overview cards
│   ├── container-list.tsx   — Container table
│   ├── container-detail-dialog.tsx — Detail Dialog
│   ├── container-logs.tsx   — Log terminal component (dedicated WS)
│   ├── container-stats.tsx  — Stats mini cards
│   ├── docker-events.tsx    — Events timeline
│   ├── docker-networks-dialog.tsx  — Networks list Dialog
│   └── docker-volumes-dialog.tsx   — Volumes list Dialog
├── hooks/
│   ├── use-docker-ws.ts     — Subscribe/unsubscribe lifecycle + DockerUpdate from global WS
│   └── use-docker-logs.ts   — Dedicated WS for log streaming
└── types.ts                 — Frontend Docker types
```

## Testing

### Rust unit tests

- DockerManager: connection init, graceful degradation when Docker unavailable, reconnection
- Protocol serialization/deserialization for all new message types (including FeaturesUpdate)
- Container stats calculation (CPU%, memory%)
- DockerService: event save/query
- Capability check for `CAP_DOCKER`
- DockerViewerTracker: refcount add/remove, first/last detection
- Log session routing: session_id based dispatch

### Frontend tests (vitest)

- Docker Tab conditional rendering based on `features` + `CAP_DOCKER`
- Container list rendering, filtering, search
- Container detail Dialog sections
- Docker events timeline rendering
- Subscribe/unsubscribe lifecycle on tab mount/unmount
- Dedicated log WebSocket connection management

### Integration tests

- Agent ↔ Server Docker message round-trip
- Container action request-response flow
- Log stream via dedicated WS: start, receive entries, stop on disconnect
- Docker event persistence and retrieval
- Viewer refcount: multiple browsers subscribe/unsubscribe, stats start/stop correctly
- Feature negotiation: old agent (protocol v2) does not receive Docker commands
- Docker availability change: Agent sends DockerUnavailable, Server broadcasts, frontend hides tab

## Reference

The existing dockerman Tauri application (`/Users/zingerbee/Bee/dockerman/app.dockerman`) serves as implementation reference, particularly:
- `src-tauri/src/commands/container.rs` — container list and actions using bollard
- `src-tauri/src/commands/stats.rs` — CPU/memory/network stats calculation
- `src-tauri/src/commands/logs.rs` — log stream with batching (50 entries / 50ms)
- `src-tauri/src/commands/events.rs` — event stream with auto-reconnect and heartbeat
