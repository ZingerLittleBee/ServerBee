# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

ServerBee is a lightweight VPS monitoring probe system built with Rust and React. It follows a hub-and-spoke architecture: a central **Server** receives metrics from distributed **Agents** over WebSocket, stores them in SQLite, and serves a React SPA dashboard.

## Build & Run Commands

```bash
# Build
cargo build --workspace                         # All Rust crates
cd apps/web && bun install && bun run build     # Frontend (embedded into server binary via rust-embed)

# Run
cargo run -p serverbee-server                   # Server on port 9527
cargo run -p serverbee-agent                    # Agent (needs server_url configured)

# Test
cargo test --workspace                          # Rust: 54 unit + 2 integration tests
bun run test                                    # Frontend: 11 vitest tests
cargo test -p serverbee-server --test integration  # Integration tests only
cargo test -p serverbee-server test_name        # Single Rust test

# Lint & Format
cargo clippy --workspace -- -D warnings         # Rust (CI enforced, 0 warnings)
bun x ultracite check                           # Frontend (Biome)
bun x ultracite fix                             # Frontend auto-fix
bun run typecheck                               # TypeScript (web + fumadocs)
```

## Architecture

```
crates/
  common/     — Protocol messages (ServerMessage/AgentMessage/BrowserMessage),
                capability bitmask constants, shared types
  server/     — Axum 0.8 HTTP/WS server
    entity/   — sea-orm entities (21 tables)
    service/  — Business logic (auth, alert, notification, record, ping, etc.)
    router/   — REST API (api/) + WebSocket handlers (ws/agent, ws/browser, ws/terminal)
    task/     — Background jobs (record_writer, aggregator, cleanup, alert_evaluator, etc.)
    migration/ — Database migrations
  agent/      — Lightweight system probe
    collector/ — CPU, memory, disk, network, load, process, temperature, GPU metrics
    reporter   — WS connection with exponential backoff reconnect
    pinger     — ICMP/TCP/HTTP probe execution
    terminal   — PTY session management (portable-pty)
apps/
  web/        — React 19 SPA (TanStack Router + Query, shadcn/ui, Recharts, xterm.js)
  fumadocs/   — Documentation site (TanStack Start + Fumadocs MDX, CN+EN bilingual)
```

### Data Flow

```
Agent → WebSocket (JSON) → Server → SQLite (sea-orm)
                                  → broadcast::Sender → Browser WebSocket → React SPA
```

- **Agent→Server**: `AgentMessage` variants (SystemInfo, Report, PingResult, TaskResult, CapabilityDenied)
- **Server→Agent**: `ServerMessage` variants (Welcome, Ack, Execute, TerminalOpen, CapabilitiesSync, Upgrade)
- **Server→Browser**: `BrowserMessage` variants (ServerUpdate, ServerOnline/Offline, CapabilitiesChanged)
- Terminal data uses Binary WebSocket frames (session_id prefix + payload)

### AppState

Shared state passed to all handlers via `Arc<AppState>`:
- `db: DatabaseConnection` — sea-orm SQLite pool
- `agent_manager: AgentManager` — DashMap of connected agents with WS senders
- `browser_tx: broadcast::Sender<BrowserMessage>` — fan-out to browser clients
- `config: AppConfig` — Figment-loaded configuration
- `login_rate_limit / register_rate_limit: DashMap` — IP-based rate limiting (15min window)

### Authentication Model

Three auth paths, all checked in `middleware/auth.rs`:
1. **Session cookie** — Browser login via `/api/auth/login`, argon2 password hash
2. **API key** — `X-API-Key` header, `sb_` prefix + argon2 hash stored
3. **Agent token** — WebSocket query param, per-server token from registration

RBAC: Admin (full access) vs Member (read-only). `require_admin` middleware on write routes.

## Key Conventions

### Rust

- **Errors**: `AppError` enum → automatic HTTP status code mapping via `IntoResponse`
- **API responses**: All endpoints return `Json<ApiResponse<T>>` wrapping data in `{ data: T }`
- **OpenAPI**: Every endpoint annotated with `#[utoipa::path]`, every DTO with `#[derive(ToSchema)]`. Swagger UI at `/swagger-ui/`
- **Config**: Figment loads TOML then env vars. Prefix `SB_`, nested separator `__` (double underscore). Example: `SB_ADMIN__PASSWORD` → `admin.password`
- **Capabilities**: u32 bitmask per server — `CAP_TERMINAL=1, CAP_EXEC=2, CAP_UPGRADE=4, CAP_PING_ICMP=8, CAP_PING_TCP=16, CAP_PING_HTTP=32`. Default `CAP_DEFAULT=56` (ping only). Defense-in-depth: validated on both server and agent side.
- **Migrations**: sea-orm migrations in `crates/server/src/migration/`. Run automatically on startup.

### Frontend

- **Routing**: TanStack Router file-based routing in `apps/web/src/routes/`. `_authed/` directory requires login.
- **API client**: `apps/web/src/lib/api-client.ts` auto-unwraps `{ data: T }` from responses
- **WebSocket**: Global WS connection in layout, hooks in `apps/web/src/hooks/use-servers-ws.ts`
- **UI components**: shadcn/ui in `apps/web/src/components/ui/`
- **Dev proxy**: Vite proxies `/api/*` to `http://localhost:9527` in dev mode

### Code Quality (Ultracite/Biome)

This project uses **Ultracite** (Biome-based) for frontend linting and formatting:
- `bun x ultracite check` — verify
- `bun x ultracite fix` — auto-fix
- Line width 120, single quotes (JS), double quotes (JSX), no trailing commas
- Organize imports automatically

## Testing

See `TESTING.md` for the full testing guide with commands, coverage tables, and manual verification checklist.

**Keep `TESTING.md` in sync with code changes.** When adding/removing tests or testable features, update test counts, file locations, and the verification checklist to reflect the current codebase.

## Documentation

- **Fumadocs site**: `apps/docs/content/docs/{cn,en}/` — 16 MDX pages per language
- **OpenAPI**: Auto-generated at `/swagger-ui/` and `/api-docs/openapi.json`
- **Architecture spec**: `docs/superpowers/specs/2026-03-12-serverbee-architecture-design.md`
- **Progress tracking**: `docs/superpowers/plans/PROGRESS.md`
