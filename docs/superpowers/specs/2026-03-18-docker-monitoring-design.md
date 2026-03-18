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

## Authentication and Authorization

Docker functionality follows a two-tier permission model:

### Read operations (all authenticated users — Member + Admin)

- View Docker Tab (if `features` includes `"docker"` and `CAP_DOCKER` is enabled)
- View container list, stats, events, networks, volumes
- Subscribe to real-time Docker updates (stats + events stream)
- View container logs via dedicated WebSocket
- View Docker system info

All Docker read routes are placed in the **authenticated (non-admin) router**, alongside existing read-only server routes.

### Write operations (Admin only)

- Container actions: start / stop / restart / remove
- `POST /api/servers/{id}/docker/containers/{cid}/action` is placed in the **admin router** (`require_admin` middleware).

### WebSocket auth

The dedicated log WebSocket (`/api/ws/docker/logs`) performs the same auth checks as the existing browser WebSocket:
1. Session cookie or API key validation
2. `CAP_DOCKER` capability check for the target server
3. No admin role required (logs are read-only)

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
4. **Runtime disconnect**: If the bollard client encounters a connection error during stats/events/logs, DockerManager sends `AgentMessage::DockerUnavailable` and transitions to retry mode. All active log sessions and event streams are aborted.
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

    // Defaults to empty vec when absent (old v2 agents won't send this field).
    #[serde(default)]
    pub features: Vec<String>,        // e.g. ["docker"]
}
```

**`features` is always persisted on every Agent connect.** When Server receives `AgentMessage::SystemInfo`, it writes `SystemInfo.features` to the `servers.features` column, replacing any previous value. This ensures:
- If Docker was available last time but not this time, `features` becomes `[]` and the stale `["docker"]` is cleared.
- No separate `FeaturesUpdate` is needed for the initial state — `SystemInfo` is the single source of truth at connection time.

`AgentMessage::FeaturesUpdate` is only sent for **runtime changes** (Docker becomes available/unavailable after initial connect).

Server uses `features` to determine Agent capabilities beyond the bitmask:
- `features` contains `"docker"` → Agent has Docker support compiled in and Docker daemon is reachable.
- Frontend reads `features` from server data to conditionally show Docker Tab.

This decouples "Agent binary supports Docker" (feature negotiation) from "admin allows Docker operations" (CAP_DOCKER capability). Both must be true for Docker to function.

Old agents (protocol_version < 3) will not send `features` (field absent or empty) and will not understand `ServerMessage::Docker*` — Server simply never sends Docker commands to them. The capabilities dialog shows a notice: "Agent upgrade required for Docker support".

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

### CAP_DOCKER runtime revocation

When an admin disables `CAP_DOCKER` for a server (via `PUT /api/servers/{id}/capabilities`), the Server must immediately tear down all active Docker streams for that server:

1. **Server-side** (in the existing capabilities update handler):
   - Remove all Docker viewer subscriptions for the server: `docker_viewers.remove_all_for_server(&server_id)`
   - Send `DockerStopStats` and `DockerEventsStop` to the Agent
   - Close all active Docker log sessions for the server: `remove_docker_log_sessions_for_server(&server_id)` drops channels (causing relay loops to break) and returns session_ids, then send `DockerLogsStop` for each
   - Clear Docker memory caches for the server (containers, stats, info)
   - Broadcast `BrowserMessage::CapabilitiesChanged { server_id, capabilities }` (the existing message type) so frontend updates the capabilities cache

2. **Agent-side** (existing `CapabilitiesSync` handler, enhanced):
   - Update capability bitmap (existing behavior)
   - If `CAP_DOCKER` was just removed: DockerManager aborts all active log sessions, stops stats polling, stops event stream. Same cleanup as runtime Docker disconnect.

3. **Frontend**: receives `capabilities_changed` (existing handler already updates the capabilities cache). Docker Tab visibility is derived from `hasCap(CAP_DOCKER)` only — when capabilities change, `hasCap(CAP_DOCKER)` becomes false and Docker Tab hides automatically (component unmounts, `useDockerSubscription` cleanup fires). Any open log WS connections receive a close frame from server-side cleanup.

**Important separation of concerns:**
- `DockerAvailabilityChanged` is used **only** for Docker daemon availability changes (features-level, from Agent). It updates `server.features`. When Docker becomes unavailable, the Docker Tab stays **mounted** (showing "unavailable" placeholder) so viewer subscriptions are preserved for auto-recovery.
- `CapabilitiesChanged` is used **only** for admin permission changes (authz-level). It updates `server.capabilities`. When CAP_DOCKER is disabled, the Docker Tab **unmounts** — this is intentional because admin explicitly revoked access and server-side already cleaned up all streams.
- The two unmount behaviors are different by design: daemon unavailability = keep mounted (temporary, auto-recoverable), capability revocation = unmount (deliberate admin action, server handles cleanup).

This ensures "capability off = immediate effect" with no stale streams, and no semantic pollution of the features cache.

### AgentManager capabilities cache

The existing `AgentManager` already stores per-server connection state. Add an in-memory capabilities cache:

```rust
// New field in AgentManager
capabilities: DashMap<String, u32>,  // server_id → capability bitmask
```

**Populated from:**
1. **Server startup**: All server capabilities are loaded from DB into cache during `AgentManager::new()` initialization. This eliminates the cold-start gap — capabilities are available immediately, before any Agent reconnects.
2. **Agent connect**: Refreshed from DB alongside other server state (in case DB was modified externally).
3. **Capability REST update**: `PUT /api/servers/{id}/capabilities` writes to both DB and this cache.

**`has_docker_capability()` implementation:**
```rust
impl AgentManager {
    pub fn has_docker_capability(&self, server_id: &str) -> bool {
        self.capabilities.get(server_id)
            .map_or(false, |cap| has_capability(*cap, CAP_DOCKER))
    }
}
```

### DockerViewerTracker additions

```rust
impl DockerViewerTracker {
    /// Remove all viewers for a specific server (used on capability revocation).
    /// Returns true if there were any viewers (streams need to be stopped).
    pub fn remove_all_for_server(&self, server_id: &str) -> bool {
        self.viewers.remove(server_id).map_or(false, |(_, set)| !set.is_empty())
    }
}
```

### Graceful degradation

- **Docker available** → `DockerManager::try_new()` succeeds, `SystemInfo.features` includes `"docker"`, sends `DockerInfo` proactively on connect
- **Docker unavailable at startup** → `DockerManager` is `None`, `SystemInfo.features` is `[]` (no `"docker"`), retry timer active (30s)
- **Docker becomes available later** → retry succeeds, sends `DockerInfo` + `FeaturesUpdate { features: ["docker"] }`
- **Docker stops at runtime** → sends `DockerUnavailable` + `FeaturesUpdate { features: [] }`, transitions to retry mode

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

### BrowserClientMessage (Browser → Server, via /ws/servers)

New message type for browser-to-server communication over the existing `/ws/servers` WebSocket. Currently the browser WS is read-only (server pushes to browser). This adds a lightweight upstream channel for Docker subscription control:

```rust
// Browser sends these JSON messages over the /ws/servers WebSocket.
// Uses #[serde(tag = "type", rename_all = "snake_case")] to match
// the existing protocol convention in crates/common/src/protocol.rs.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum BrowserClientMessage {
    // Subscribe to Docker stats + events for a server.
    // Server tracks subscriptions per WS connection (connection_id).
    // First subscriber for a server_id triggers DockerStartStats + DockerEventsStart.
    DockerSubscribe { server_id: String },

    // Unsubscribe from Docker updates.
    // Last subscriber for a server_id triggers DockerStopStats + DockerEventsStop.
    DockerUnsubscribe { server_id: String },
}
// Wire format examples:
//   {"type": "docker_subscribe", "server_id": "abc123"}
//   {"type": "docker_unsubscribe", "server_id": "abc123"}
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

**Auth:** Same as existing browser WS — session cookie or API key. Additionally checks `CAP_DOCKER` capability for the target server. No admin role required (logs are read-only).

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
    // Auth + CAP_DOCKER validated by middleware before reaching this handler.
    // Additionally check that the Agent supports Docker (features guard):
    if !state.agent_manager.has_feature(&server_id, "docker") {
        let _ = socket.send(Message::Close(Some(CloseFrame {
            code: 4001,
            reason: "Agent does not support Docker".into(),
        }))).await;
        return;
    }

    let session_id = generate_session_id();
    let (output_tx, mut output_rx) = mpsc::channel::<Vec<DockerLogEntry>>(256);

    // Register session in AgentManager (composite key: "server_id:session_id")
    state.agent_manager.add_docker_log_session(&server_id, session_id.clone(), output_tx);

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
            entries = output_rx.recv() => {
                match entries {
                    Some(entries) => {
                        ws_sink.send(Message::Text(serde_json::to_string(&entries)?)).await?;
                    }
                    None => { break; } // Agent-side session closed (channel dropped)
                }
            }
            msg = ws_stream.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => { break; }
                    _ => {} // Ping/Pong/Text control messages — ignore
                }
            }
        }
    }

    // Cleanup (composite key: "server_id:session_id")
    state.agent_manager.remove_docker_log_session(&server_id, &session_id);
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
    // server_id is known from the agent connection context (available in handle_agent_message)
    if let Some(tx) = state.agent_manager.get_docker_log_session(&server_id, &session_id) {
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

### Connection-bound subscriptions via browser WebSocket

Instead of REST endpoints with ad-hoc viewer IDs, subscriptions are tied to the browser WebSocket connection itself. This provides automatic cleanup on disconnect — no orphan risk.

#### Server-side

Each browser WS connection is assigned a `connection_id` (UUID) when established. The browser handler now reads incoming messages (previously ignored) and processes `BrowserClientMessage`.

**Server-side guard for Docker commands**: Before sending any `ServerMessage::Docker*` to an Agent, the Server checks that the Agent's `features` includes `"docker"` (and protocol_version >= 3). This is enforced in a helper function `send_docker_command()` that wraps `send_to_agent()`. If the Agent does not support Docker, the command is silently dropped and the caller receives an error. This prevents sending Docker commands to old agents regardless of how the request originates (browser WS, REST API, or direct scripting).

```rust
// Helper: only send Docker commands to Docker-capable agents
fn send_docker_command(
    agent_manager: &AgentManager,
    server_id: &str,
    msg: ServerMessage,
) -> Result<(), AppError> {
    if !agent_manager.has_feature(server_id, "docker") {
        return Err(AppError::DockerNotSupported);
    }
    agent_manager.send_to_agent(server_id, msg)
}
```

```rust
// router/ws/browser.rs — enhanced to handle upstream messages
async fn handle_browser_ws(socket: WebSocket, state: Arc<AppState>) {
    let connection_id = Uuid::new_v4().to_string();
    let mut browser_rx = state.browser_tx.subscribe();
    let (mut ws_sink, mut ws_stream) = socket.split();

    loop {
        tokio::select! {
            // Existing: broadcast → browser (preserve Lagged recovery)
            msg = browser_rx.recv() => {
                match msg {
                    Ok(msg) => { ws_sink.send(serialize(msg)).await?; }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        // Slow client missed messages — send full sync to recover
                        let full_sync = build_full_sync(&state).await;
                        ws_sink.send(serialize(full_sync)).await?;
                    }
                    Err(broadcast::error::RecvError::Closed) => { break; }
                }
            }
            // Browser → server: single next() call, match internally
            // (same pattern as existing browser.rs to avoid multiple mutable borrows)
            msg = ws_stream.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(client_msg) = serde_json::from_str::<BrowserClientMessage>(&text) {
                            match client_msg {
                                BrowserClientMessage::DockerSubscribe { server_id } => {
                                    // Enforce CAP_DOCKER: reject subscription if capability is disabled.
                                    // Uses in-memory capability cache (pre-loaded on server startup from DB,
                                    // so always available — no cold-start gap).
                                    if !state.agent_manager.has_docker_capability(&server_id) {
                                        continue;
                                    }
                                    let is_first = state.docker_viewers
                                        .add_viewer(&server_id, &connection_id);
                                    if is_first {
                                        let _ = send_docker_command(&state.agent_manager, &server_id,
                                            ServerMessage::DockerStartStats { interval_secs: 3 });
                                        let _ = send_docker_command(&state.agent_manager, &server_id,
                                            ServerMessage::DockerEventsStart);
                                    }
                                }
                                BrowserClientMessage::DockerUnsubscribe { server_id } => {
                                    let is_last = state.docker_viewers
                                        .remove_viewer(&server_id, &connection_id);
                                    if is_last {
                                        let _ = send_docker_command(&state.agent_manager, &server_id,
                                            ServerMessage::DockerStopStats);
                                        let _ = send_docker_command(&state.agent_manager, &server_id,
                                            ServerMessage::DockerEventsStop);
                                    }
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => { break; }
                    _ => {} // Ping/Pong/Binary — ignore
                }
            }
        }
    }

    // Connection closed — remove ALL subscriptions for this connection
    let affected_servers = state.docker_viewers.remove_all_for_connection(&connection_id);
    for (server_id, is_last) in affected_servers {
        if is_last {
            let _ = send_docker_command(&state.agent_manager, &server_id,
                ServerMessage::DockerStopStats);
            let _ = send_docker_command(&state.agent_manager, &server_id,
                ServerMessage::DockerEventsStop);
        }
    }
}
```

#### DockerViewerTracker

```rust
pub struct DockerViewerTracker {
    // server_id → set of connection_ids (browser WS connections)
    viewers: DashMap<String, HashSet<String>>,
}

impl DockerViewerTracker {
    /// Add a viewer. Returns true if this is the first viewer (should start streaming).
    pub fn add_viewer(&self, server_id: &str, connection_id: &str) -> bool {
        let mut set = self.viewers.entry(server_id.to_string()).or_default();
        let was_empty = set.is_empty();
        set.insert(connection_id.to_string());
        was_empty
    }

    /// Remove a viewer. Returns true if this was the last viewer (should stop streaming).
    pub fn remove_viewer(&self, server_id: &str, connection_id: &str) -> bool {
        if let Some(mut set) = self.viewers.get_mut(server_id) {
            set.remove(connection_id);
            if set.is_empty() {
                drop(set);
                self.viewers.remove(server_id);
                return true;
            }
        }
        false
    }

    /// Check if any viewer is watching a server (used for Docker recovery).
    pub fn has_viewers(&self, server_id: &str) -> bool {
        self.viewers.get(server_id).map_or(false, |set| !set.is_empty())
    }

    /// Remove all subscriptions for a disconnected connection.
    /// Returns Vec<(server_id, was_last_viewer)>.
    pub fn remove_all_for_connection(&self, connection_id: &str) -> Vec<(String, bool)> {
        let mut results = Vec::new();
        let server_ids: Vec<String> = self.viewers.iter()
            .filter(|entry| entry.value().contains(connection_id))
            .map(|entry| entry.key().clone())
            .collect();
        for server_id in server_ids {
            let is_last = self.remove_viewer(&server_id, connection_id);
            results.push((server_id, is_last));
        }
        results
    }
}
```

#### Frontend integration

**Required WsClient enhancement**: The existing `WsClient` (`apps/web/src/lib/ws-client.ts`) is currently receive-only (no `send()` method). It must be extended to support upstream messages:

```typescript
// WsClient additions:
class WsClient {
    // ... existing fields ...

    /** Send a JSON message to the server over the existing WS connection. */
    send(data: unknown): void {
        if (this.ws?.readyState === WebSocket.OPEN) {
            this.ws.send(JSON.stringify(data))
        }
    }
}
```

**Required refactor: extend existing `useServersWs` to expose `send` and `connectionState`.** The current `useServersWs` hook in `_authed.tsx` creates the WS connection, handles incoming messages (full_sync, update, server_online/offline, capabilities_changed, agent_info_updated, etc.), and updates the React Query cache. This existing receive-side logic must be preserved exactly as-is.

The refactor adds two things without changing existing behavior:

1. **`WsClient.send()` method** — add to the existing WsClient class
2. **Expose `send` and `connectionState` to child components** — via React Context

```typescript
// Minimal changes to existing code:

// 1. WsClient: add send() and connection state tracking
class WsClient {
    // ... existing connect/onMessage/reconnect logic unchanged ...
    private connectionStateListeners: Set<(state: 'connected' | 'disconnected') => void> = new Set()
    private _connectionState: 'connected' | 'disconnected' = 'disconnected'

    // In existing onopen handler, add: this.setConnectionState('connected')
    // In existing onclose handler, add: this.setConnectionState('disconnected')

    private setConnectionState(state: 'connected' | 'disconnected') {
        this._connectionState = state
        for (const listener of this.connectionStateListeners) listener(state)
    }

    get connectionState() { return this._connectionState }

    onConnectionStateChange(listener: (state: 'connected' | 'disconnected') => void): () => void {
        this.connectionStateListeners.add(listener)
        return () => this.connectionStateListeners.delete(listener)
    }

    send(data: unknown): void {
        if (this.ws?.readyState === WebSocket.OPEN) {
            this.ws.send(JSON.stringify(data))
        }
    }
}

// 2. New context (apps/web/src/contexts/servers-ws-context.tsx)
interface ServersWsContextValue {
    send: (data: unknown) => void
    connectionState: 'connected' | 'disconnected'
}
const ServersWsContext = createContext<ServersWsContextValue | null>(null)

export const useServersWsSend = () => {
    const ctx = useContext(ServersWsContext)
    if (!ctx) throw new Error('useServersWsSend must be used within provider')
    return ctx
}

// 3. _authed.tsx: wrap existing useServersWs with context provider.
// The existing useServersWs() call stays in place — it still handles
// all incoming messages (full_sync, update, etc.) and updates React Query.
// The provider wraps the WsClient and exposes send + connectionState:
//
// Inside the provider, connectionState is derived from WsClient:
//   const [connectionState, setConnectionState] = useState(wsClient.connectionState)
//   useEffect(() => wsClient.onConnectionStateChange(setConnectionState), [wsClient])
//
// This ensures useDockerSubscription's effect re-fires on reconnect
// because connectionState changes: 'disconnected' → 'connected'.
```

This is strictly additive: existing message handling in `useServersWs` is untouched. The context only exposes `send` and `connectionState` for Docker subscription control. One WS connection per session, no duplicates.

**New WS message branches in `useServersWs`**: The existing `onMessage` switch in `use-servers-ws.ts` must be extended with three new cases to consume Docker downstream messages:

```typescript
// Added to the existing switch(msg.type) in useServersWs onMessage handler:
case 'docker_update': {
    // msg: { type, server_id, containers, stats? }
    // Update Docker-specific React Query cache
    queryClient.setQueryData(
        ['docker', 'containers', msg.server_id],
        msg.containers
    )
    if (msg.stats) {
        queryClient.setQueryData(
            ['docker', 'stats', msg.server_id],
            msg.stats
        )
    }
    break
}
case 'docker_event': {
    // msg: { type, server_id, event }
    // Append to Docker events query cache (ring buffer, keep last 100)
    queryClient.setQueryData(
        ['docker', 'events', msg.server_id],
        (prev: DockerEventInfo[] = []) => [...prev, msg.event].slice(-100)
    )
    break
}
case 'docker_availability_changed': {
    // msg: { type, server_id, available }
    // This message reflects Docker DAEMON availability (features-level),
    // NOT admin capability changes. Only updates server.features.
    // CAP_DOCKER changes arrive via the existing 'capabilities_changed' handler.
    // Docker Tab visibility = hasCap(CAP_DOCKER) only.
    // Docker Tab content = features.includes('docker') ? real data : "unavailable" placeholder.
    // Tab stays MOUNTED when Docker is unavailable — viewer subscriptions are preserved.
    const updateFeatures = (server: Server) => ({
        ...server,
        features: msg.available
            ? [...new Set([...server.features, 'docker'])]
            : server.features.filter((f: string) => f !== 'docker')
    })
    // List cache
    queryClient.setQueryData(['servers'], (prev: Server[]) =>
        prev?.map(s => s.id === msg.server_id ? updateFeatures(s) : s)
    )
    // Detail cache
    queryClient.setQueryData(['servers', msg.server_id], (prev: Server | undefined) =>
        prev ? updateFeatures(prev) : prev
    )
    break
}
```

Docker page components consume these caches via `useQuery(['docker', 'containers', serverId])` etc. Initial REST fetch populates the cache; WS messages update it in real-time.

**Docker subscription hook with auto-resubscribe on reconnect:**

```typescript
// apps/web/src/hooks/use-docker-subscription.ts
const useDockerSubscription = (serverId: string) => {
    const { send, connectionState } = useServersWsSend()  // consumes context from _authed provider

    useEffect(() => {
        // Subscribe on mount AND on every reconnect
        if (connectionState === 'connected') {
            send({ type: 'docker_subscribe', server_id: serverId })
        }
        return () => {
            // Unsubscribe on unmount (best-effort; server cleans up on disconnect anyway)
            send({ type: 'docker_unsubscribe', server_id: serverId })
        }
    }, [serverId, send, connectionState])
    // connectionState changes from 'disconnected' → 'connected' on reconnect,
    // re-triggering the effect and re-sending DockerSubscribe.
    // Server assigns a new connection_id on reconnect, so the subscription
    // is correctly registered against the new connection.
}
```

**Key behaviors:**
- On tab mount: sends `DockerSubscribe`
- On WS reconnect (auto-reconnect after disconnect): `connectionState` flips to `'connected'`, effect re-runs, sends `DockerSubscribe` again on the new connection
- On tab unmount: sends `DockerUnsubscribe` (best-effort)
- On browser crash/close: WS disconnects, server auto-cleans via `remove_all_for_connection`

No REST endpoints needed. No viewer_id management. Connection lifecycle = subscription lifecycle.

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

- `SystemInfo` → (existing handler, enhanced) persist `features` to `servers.features` column on **every connect**, replacing previous value. Also update `agent_manager.features` in-memory cache. This clears stale `["docker"]` when Docker is no longer available.
- `DockerInfo` → cache in AgentManager. If `msg_id` is present, dispatch via `pending_requests`. Broadcast `BrowserMessage::DockerAvailabilityChanged { available: true }`.
- `DockerContainers` → cache in AgentManager + broadcast `BrowserMessage::DockerUpdate`. If `msg_id` is present, also dispatch via `pending_requests`.
- `DockerStats` → cache in AgentManager + broadcast `BrowserMessage::DockerUpdate`
- `DockerLog` → route to specific log session channel via `agent_manager.get_docker_log_session()` (**not** broadcast)
- `DockerEvent` → save to `docker_event` table + broadcast `BrowserMessage::DockerEvent`
- `DockerUnavailable` → clear Docker caches for this server, broadcast `BrowserMessage::DockerAvailabilityChanged { available: false }`. Note: viewer subscriptions are NOT removed — they remain so that streams auto-resume when Docker recovers.
- `FeaturesUpdate` → update `servers.features` in database AND `agent_manager.features` in-memory cache. Broadcast `DockerAvailabilityChanged` accordingly. **If Docker becomes available** (features now includes `"docker"`) **and there are active viewers** for this server, Server automatically re-sends `DockerStartStats` + `DockerEventsStart` to the Agent to resume streaming. This handles runtime recovery without requiring browser-side re-subscription.
- `DockerNetworks` / `DockerVolumes` → dispatch via `pending_requests` (request-response pattern)
- `DockerActionResult` → dispatch via `pending_requests`

### AgentManager memory cache

```rust
// New fields
docker_containers: DashMap<String, Vec<DockerContainer>>,
docker_stats: DashMap<String, Vec<DockerContainerStats>>,
docker_info: DashMap<String, DockerSystemInfo>,

// Feature cache: server_id → Vec<String>. Updated on every SystemInfo and FeaturesUpdate.
// Used by send_docker_command() guard — no DB access needed at command dispatch time.
features: DashMap<String, Vec<String>>,

// Capabilities cache: server_id → u32 bitmask. Updated on Agent connect (from DB) and
// on capability update (PUT /api/servers/{id}/capabilities). Used by has_docker_capability().
capabilities: DashMap<String, u32>,

// Log session routing. Keyed as "server_id:session_id" composite key to support
// per-server cleanup on capability revocation (iterate and match by server_id prefix).
// All access methods use the composite key internally:
//   add_docker_log_session(server_id, session_id, tx)  → inserts "server_id:session_id"
//   get_docker_log_session(server_id, session_id)       → looks up "server_id:session_id"
//   remove_docker_log_session(server_id, session_id)    → removes "server_id:session_id"
//   remove_docker_log_sessions_for_server(server_id)    → removes all with matching prefix
docker_log_sessions: DashMap<String, mpsc::Sender<Vec<DockerLogEntry>>>,
```

### REST API

```
// Read routes (all authenticated users)
GET    /api/servers/{id}/docker/containers              — container list (from cache)
GET    /api/servers/{id}/docker/stats                    — container stats (from cache)
GET    /api/servers/{id}/docker/info                     — Docker system info (from cache)
GET    /api/servers/{id}/docker/networks                 — network list (request-response via Agent)
GET    /api/servers/{id}/docker/volumes                  — volume list (request-response via Agent)
GET    /api/servers/{id}/docker/events                   — historical events (from database)

// Write routes (admin only, behind require_admin middleware)
POST   /api/servers/{id}/docker/containers/{cid}/action  — container action (start/stop/restart/remove)

**All Docker REST endpoints** (read and write) check **both** gates before proceeding:
1. `has_capability(server.capabilities, CAP_DOCKER)` — returns 403 if CAP_DOCKER is disabled
2. `agent_manager.has_feature(server_id, "docker")` — returns 409 if Agent does not support Docker

This is enforced via a shared `require_docker` middleware/extractor that runs before each Docker route handler, similar to how `require_admin` works for write routes.
```

Note: stats/events subscription is handled via browser WS messages (DockerSubscribe/Unsubscribe), not REST. Log streaming uses a dedicated WS endpoint.

### WebSocket endpoints

```
WS /api/ws/servers              — existing, enhanced: now reads BrowserClientMessage upstream,
                                  also carries DockerUpdate + DockerEvent + DockerAvailabilityChanged downstream
WS /api/ws/docker/logs?...      — dedicated log stream (new, auth + CAP_DOCKER check, no admin required)
```

### DTO changes for `features` propagation

The `features` field must be present in all paths that deliver server data to the frontend:

1. **`ServerResponse`** (REST `GET /api/servers/{id}`) — add `features: Vec<String>`
2. **`ServerStatus`** (used in `BrowserMessage::FullSync` and `BrowserMessage::Update`) — add `features: Vec<String>`
3. **Frontend TypeScript types** — add `features: string[]` to the Server interface
4. **`useServersWs` hook** — propagate `features` from WS messages to React Query cache

This ensures Docker Tab visibility is consistent across:
- Initial page load (REST response)
- WebSocket full sync (on connect / lagged resync)
- Real-time updates (DockerAvailabilityChanged modifies the cached server's `features`)

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

**Cleanup task**: Add explicit `docker_event` cleanup branch to `crates/server/src/task/cleanup.rs` alongside existing `records`, `ping_record`, `network_probe_record`, etc. Retention: 7 days.

### Data flow

```
Agent connects to Server:
    Agent → SystemInfo { features: ["docker"], protocol_version: 3 }
        Server persists features=["docker"] to servers.features (always, replaces old value)
    Agent → DockerInfo { msg_id: None, info }
        Server caches DockerInfo in AgentManager
        Server broadcasts BrowserMessage::DockerAvailabilityChanged { available: true }

    (If Docker unavailable at startup:)
    Agent → SystemInfo { features: [], protocol_version: 3 }
        Server persists features=[] (clears any stale ["docker"] from previous session)
    Agent starts 30s retry timer

Browser loads page:
    React → GET /api/servers (response includes features for each server)
    React → connect /ws/servers, receive FullSync (includes features per server)
    Docker Tab visibility: hasCap(CAP_DOCKER) only (admin permission)
    Docker Tab content: features.includes("docker") ? real data : "unavailable" placeholder

Browser opens Docker tab:
    React → sends BrowserClientMessage::DockerSubscribe { server_id } over /ws/servers
    Server → if first viewer for this server:
        Server → ServerMessage::DockerStartStats { interval_secs: 3 }
        Server → ServerMessage::DockerEventsStart
    React → GET /api/servers/{id}/docker/containers (initial load from cache)
    React → listens for BrowserMessage::DockerUpdate + DockerEvent on /ws/servers
    ↓
Agent DockerManager:
    - polls container list + stats → AgentMessage::DockerContainers / DockerStats
    - Docker event stream → AgentMessage::DockerEvent
    ↓
Server receives → cache + broadcast via browser_tx
    ↓
Browser updates UI in real-time

Browser closes Docker tab:
    React → sends BrowserClientMessage::DockerUnsubscribe { server_id } over /ws/servers
    Server → if last viewer for this server:
        Server → ServerMessage::DockerStopStats
        Server → ServerMessage::DockerEventsStop

Browser crashes / network drops:
    /ws/servers disconnects
    Server → remove_all_for_connection(connection_id)
    Server → if any server lost last viewer: send DockerStopStats + DockerEventsStop

Admin disables CAP_DOCKER:
    Server → capabilities update handler:
        1. docker_viewers.remove_all_for_server(&server_id)
        2. Send DockerStopStats + DockerEventsStop to Agent
        3. Close all docker_log_sessions for server_id (drop channels + DockerLogsStop)
        4. Clear Docker caches (containers, stats, info)
        5. Broadcast CapabilitiesChanged { server_id, capabilities } (existing message type)
    Agent → CapabilitiesSync handler: abort DockerManager sessions
    Browser → receives capabilities_changed (existing handler), hasCap(CAP_DOCKER) → false
        Docker Tab unmounts (intentional — admin revoked access), useDockerSubscription
        cleanup fires docker_unsubscribe (server already cleaned up, this is harmless)
        Open log WS connections receive close frame from server-side cleanup
    Note: server.features is NOT modified — Docker daemon is still available

Admin re-enables CAP_DOCKER:
    Server → capabilities update handler:
        1. Update capabilities cache and DB
        2. Broadcast CapabilitiesChanged { server_id, capabilities }
    Browser → receives capabilities_changed, hasCap(CAP_DOCKER) → true
        Docker Tab becomes visible again (features still includes "docker")
        User navigates to Docker Tab → useDockerSubscription sends DockerSubscribe
        Normal subscription flow resumes — no special recovery needed

Docker runtime recovery (Docker daemon restarts while viewers are active):
    Agent → DockerManager retry succeeds
    Agent → DockerInfo { msg_id: None, info }
    Agent → FeaturesUpdate { features: ["docker"] }
    Server → updates features in DB, broadcasts DockerAvailabilityChanged { available: true }
    Server → checks docker_viewers.has_viewers(&server_id)
    Server → if has viewers: re-sends DockerStartStats + DockerEventsStart to Agent
    Browser → receives DockerAvailabilityChanged, Docker Tab content switches from
        "unavailable" placeholder back to real data (tab was never unmounted, so
        useDockerSubscription was still active and viewer was retained)
    Browser → begins receiving DockerUpdate + DockerEvent again (no re-subscribe needed)

Browser WS reconnects (network blip while Docker tab is open):
    Server → old connection dropped, remove_all_for_connection cleans old subscriptions
    Browser → WsClient auto-reconnects, connectionState → 'connected'
    Browser → useDockerSubscription effect re-fires, sends DockerSubscribe on new connection
    Server → registers new connection_id, sends DockerStartStats if first viewer

User views container logs (opens detail Dialog):
    React → opens WebSocket to /api/ws/docker/logs?server_id=X&container_id=Y&tail=100&follow=true
    Server → validates auth + CAP_DOCKER
    Server → generates session_id, registers log session channel
    Server → ServerMessage::DockerLogsStart { session_id, container_id, tail, follow }
    Agent → docker.logs() stream → batches → AgentMessage::DockerLog { session_id, entries }
    Server → routes to session channel → dedicated WS → browser terminal display
    User closes Dialog → WebSocket closes
    Server → cleanup: remove session, send ServerMessage::DockerLogsStop { session_id }
```

## Frontend

### Entry point

Docker Tab inside the Server detail page, alongside Overview / Terminal / Files / Ping tabs.

**Tab visibility**: Only `hasCap(CAP_DOCKER)` — admin has granted Docker permission. The tab is shown regardless of current Docker daemon availability.

**Tab content**: Inside the tab, check `server.features.includes("docker")`:
- **true**: Show real-time Docker data (containers, stats, events)
- **false**: Show a "Docker unavailable" placeholder (e.g., "Docker daemon is not reachable on this server. Waiting for connection..."). The tab stays mounted, `useDockerSubscription` remains active, and viewer subscriptions are preserved so that when Docker recovers, streams auto-resume without re-subscription.

This separation is critical for the Docker runtime recovery flow: if Docker daemon restarts, the tab must NOT unmount (which would fire `docker_unsubscribe` and clear viewer tracking), otherwise server-side auto-recovery via `has_viewers()` would fail.

### Docker Tab layout (single-page overview)

Top to bottom:
1. **Overview cards** — Running count, Stopped count, Total CPU, Total Memory, Docker version
2. **Container list** — Table with Name, Image, Status, CPU, Memory, Net I/O, Actions (actions only visible to admin). Search and filter (All/Running/Stopped). Click row to open detail Dialog.
3. **Recent events timeline** — Chronological list with timestamp, action badge (start/stop/die/create), actor name.
4. **Networks & Volumes** — Accessed via buttons or links, displayed in Dialog.

### Container detail Dialog (sectioned layout)

No tabs. Vertically stacked sections:
1. **Top: Meta info + actions** — Image, status, ports. Stop / Restart / Remove buttons (admin only).
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
│   ├── use-docker-subscription.ts — DockerSubscribe/Unsubscribe via global WS (auto-resubscribe on reconnect)
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
- DockerViewerTracker: add/remove viewer, first/last detection, remove_all_for_connection, has_viewers
- Log session routing: session_id based dispatch
- SystemInfo.features persistence: overwrite on reconnect, stale value cleared
- BrowserClientMessage parsing in browser WS handler
- send_docker_command guard: rejects commands to agents without "docker" feature
- has_docker_capability guard: rejects when CAP_DOCKER is disabled
- FeaturesUpdate with active viewers: auto-resumes stats/events streaming
- CAP_DOCKER runtime revocation: tears down viewers, stops streams, clears caches, closes log sessions
- DockerViewerTracker.remove_all_for_server: clears all viewers for a server

### Frontend tests (vitest)

- Docker Tab conditional rendering based on `features` + `CAP_DOCKER`
- Container list rendering, filtering, search
- Container action buttons visible only for admin users
- Container detail Dialog sections
- Docker events timeline rendering
- DockerSubscribe/Unsubscribe sent on tab mount/unmount via WS
- Auto-resubscribe on WS reconnect (connectionState dependency)
- Dedicated log WebSocket connection management

### Integration tests

- Agent ↔ Server Docker message round-trip
- Container action request-response flow (admin succeeds, member gets 403)
- Log stream via dedicated WS: auth check, start, receive entries, stop on disconnect
- Docker event persistence and retrieval
- Viewer refcount via WS: multiple connections subscribe/unsubscribe, stats start/stop correctly
- WS disconnect triggers automatic cleanup of all subscriptions
- WS reconnect: browser re-sends DockerSubscribe, server resumes streaming
- Feature negotiation: old agent (protocol v2) does not receive Docker commands (send_docker_command guard)
- Docker availability change: Agent reconnects without Docker, features cleared, frontend hides tab
- Docker runtime recovery with active viewers: Server auto-resumes stats/events streaming
- SystemInfo.features overwrite: stale ["docker"] cleared on reconnect without Docker
- Log WS to non-Docker agent: connection closed with 4001 code
- CAP_DOCKER runtime revocation: active stats/events/log streams terminated immediately
- CAP_DOCKER revocation with open log WS: log session closed, DockerLogsStop sent to Agent

## Reference

The existing dockerman Tauri application (`/Users/zingerbee/Bee/dockerman/app.dockerman`) serves as implementation reference, particularly:
- `src-tauri/src/commands/container.rs` — container list and actions using bollard
- `src-tauri/src/commands/stats.rs` — CPU/memory/network stats calculation
- `src-tauri/src/commands/logs.rs` — log stream with batching (50 entries / 50ms)
- `src-tauri/src/commands/events.rs` — event stream with auto-reconnect and heartbeat
