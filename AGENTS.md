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
cargo test --workspace                          # Rust: 226 unit + 26 integration tests
bun run test                                    # Frontend: 121 vitest tests
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
2. **API key** — `X-API-Key` header, `serverbee_` prefix + argon2 hash stored
3. **Agent token** — WebSocket query param, per-server token from registration

RBAC: Admin (full access) vs Member (read-only). `require_admin` middleware on write routes.

## Key Conventions

### Rust

- **Errors**: `AppError` enum → automatic HTTP status code mapping via `IntoResponse`
- **API responses**: All endpoints return `Json<ApiResponse<T>>` wrapping data in `{ data: T }`
- **OpenAPI**: Every endpoint annotated with `#[utoipa::path]`, every DTO with `#[derive(ToSchema)]`. Swagger UI at `/swagger-ui/`
- **Config**: Figment loads TOML then env vars. Prefix `SERVERBEE_`, nested separator `__` (double underscore). Example: `SERVERBEE_ADMIN__PASSWORD` → `admin.password`. **When adding/changing env vars, update `ENV.md` and `apps/docs/content/docs/{en,cn}/configuration.mdx` simultaneously.**
- **Capabilities**: u32 bitmask per server — `CAP_TERMINAL=1, CAP_EXEC=2, CAP_UPGRADE=4, CAP_PING_ICMP=8, CAP_PING_TCP=16, CAP_PING_HTTP=32, CAP_FILE=64, CAP_DOCKER=128`. Default `CAP_DEFAULT=56` (ping only). Defense-in-depth: validated on both server and agent side.
- **Migrations**: sea-orm migrations in `crates/server/src/migration/`. Run automatically on startup. **Only implement `up()` — leave `down()` as a no-op (`Ok(())`).** Migrations are not reversible to avoid accidental data loss.

### Frontend

- **Routing**: TanStack Router file-based routing in `apps/web/src/routes/`. `_authed/` directory requires login.
- **API client**: `apps/web/src/lib/api-client.ts` auto-unwraps `{ data: T }` from responses
- **WebSocket**: Global WS connection in layout, hooks in `apps/web/src/hooks/use-servers-ws.ts`
- **UI components**: shadcn/ui in `apps/web/src/components/ui/`
- **Scroll areas**: All scrollable containers must use shadcn `<ScrollArea>` instead of native `overflow-y-auto` / `overflow-auto`
- **Dev proxy**: Vite proxies `/api/*` to `http://localhost:9527` in dev mode

### Code Quality (Ultracite/Biome)

This project uses **Ultracite** (Biome-based) for frontend linting and formatting:
- `bun x ultracite check` — verify
- `bun x ultracite fix` — auto-fix
- Line width 120, single quotes (JS), double quotes (JSX), no trailing commas
- Organize imports automatically

## Testing

For tiny CSS-only or presentational class changes with no logic or behavior changes, default to minimal verification instead of running tests. Prefer a targeted visual check, typecheck, or lint as appropriate. Only run tests if the user asks, the change affects behavior, or you add/update regression coverage.

E2E manual verification checklists are in `tests/` directory, organized by feature/page. See `tests/README.md` for the full index and local environment setup.

## Documentation

- **Fumadocs site**: `apps/docs/content/docs/{cn,en}/` — 16 MDX pages per language
- **OpenAPI**: Auto-generated at `/swagger-ui/` and `/api-docs/openapi.json`
- **Architecture spec**: `docs/superpowers/specs/2026-03-12-serverbee-architecture-design.md`
- **Progress tracking**: `docs/superpowers/plans/PROGRESS.md`

## Git

- **Commit messages**: Use Conventional Commits: `type(scope): imperative summary` or `type: imperative summary`
- **Types**: Use lowercase types such as `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, and `release`
- **Scope**: Add a scope when it clarifies the affected area, for example `agent`, `server`, `web`, `deploy`, or `register`
- **Summary**: Keep the summary imperative, concise, lowercase when practical, and without a trailing period
- **Examples**: `fix(deploy): handle piped install stdin`, `docs: update agent bootstrap command examples`

## Debugging the frontend with production data

Two workflows exist for seeing production data while developing:

- **`make db-pull && make server-dev-prod`** downloads a VACUUM'd SQLite snapshot from production via the backup API and runs the local Rust server against it. Use this when you need a full local backend stack for Rust handlers, migrations, or background-task debugging. Required env vars: `SERVERBEE_PROD_URL` and the admin-scoped `SERVERBEE_PROD_API_KEY`. Data is a frozen snapshot, so there are no live production WebSocket pushes or agent updates.
- **`make web-dev-prod`** runs the Vite dev server in `prod-proxy` mode. HTTP `/api/*` requests and the browser server-update WebSocket (`/api/ws/servers`) are forwarded to the production Railway backend so the browser sees live production data and realtime pushes without running the Rust server locally. Use this for frontend layout, styling, chart, and interaction debugging against real traffic. Required env vars: `SERVERBEE_PROD_URL` and the member-scoped `SERVERBEE_PROD_READONLY_API_KEY`.

Safety model for `make web-dev-prod`:

- Non-read HTTP methods (`POST`, `PUT`, `PATCH`, `DELETE`) are blocked with `403` at the proxy layer by default. `ALLOW_WRITES=1 make web-dev-prod` disables that proxy-side method block, but writes are still limited by whatever permissions the member key actually has.
- `Cookie` and `Authorization` request headers are stripped, and `Set-Cookie` response headers are stripped, so localhost cannot accidentally create or reuse a production browser session.
- `/api/auth/*` is blocked except exactly `GET /api/auth/me`, which the UI uses to display the current user.
- WebSocket proxying is allow-listed to `/api/ws/servers`. Control-plane WebSocket routes such as terminal and Docker log streaming stay blocked in prod-proxy mode to avoid exposing production control surfaces through a misconfigured key.
- The UI always shows a persistent warning banner in prod-proxy mode. In normal mode the banner says `⚠ Dev proxy → PROD (...) · read-only`. If `ALLOW_WRITES=1` is set, the banner switches to a stronger write-enabled warning instead of pretending the session is still read-only.

See `.env.example` for the split-key setup and the warning about member-accessible mobile write endpoints.
