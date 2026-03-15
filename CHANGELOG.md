# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-03-15

### Added

- **Real-time Metrics Charts** -- Server detail page now defaults to real-time mode, streaming live CPU, memory, disk, network, and load data from WebSocket updates at ~3s intervals. Data is accumulated in a 10-minute ring buffer (~200 data points). Users can switch between real-time and historical views (1h/6h/24h/7d/30d). Time axis shows `mm:ss` format with `HH:mm:ss` on the first data point
- **`useRealtimeMetrics` hook** -- New React hook that subscribes to TanStack Query cache updates, deduplicates via server-side `last_active` timestamp, and manages a ring buffer with automatic trimming
- **`useServerRecords` enabled option** -- Added optional `{ enabled }` parameter to disable REST API queries when in real-time mode

### Changed

- Server detail page defaults to "Real-time" mode instead of "1h" historical view
- Temperature and GPU charts are hidden in real-time mode (data not available in WebSocket stream)
- REST API queries for historical records and GPU records are disabled when real-time mode is active

### Fixed

- Query cache subscription now handles TanStack Query v5 event types correctly (removed incorrect `event.type === 'updated'` filter)
- Ring buffer uses spread operator instead of `push` to ensure new array references for React dependency tracking

### Testing

- 86 frontend Vitest tests across 9 test files (was 72 across 8)
- 13 new tests for `useRealtimeMetrics`: pure function conversion (4) + hook integration via `renderHook` (9)
- 1 new test for `useServerRecords` with `enabled: false`
- 8 new E2E verification scenarios for real-time mode (4a-4h)

## [0.1.0] - 2026-03-14

First release of ServerBee — a lightweight, self-hosted VPS monitoring system.

### Server

- Axum 0.8 HTTP/WebSocket server with SQLite (WAL mode, sea-orm)
- Session cookie + API key (`serverbee_` prefix) + Agent token authentication
- Admin/Member RBAC with `require_admin` middleware
- Figment configuration: TOML files + `SERVERBEE_` environment variables
- 21 database tables with automatic migrations on startup
- Rate limiting for login (5/15min) and agent registration (3/15min)
- OpenAPI documentation with Swagger UI at `/swagger-ui/` (50+ endpoints)
- Static file embedding via rust-embed (serves React SPA)

### Agent

- System metrics collection via sysinfo: CPU, memory, disk, network, load, processes, uptime
- Temperature monitoring (Linux thermal sensors)
- GPU monitoring via nvml-wrapper (NVIDIA, feature-gated)
- Virtualization detection (KVM, Docker, LXC, etc.)
- WebSocket reporter with exponential backoff reconnect (1s-30s + jitter)
- Auto-registration with server via discovery key
- ICMP/TCP/HTTP ping probe execution
- PTY terminal sessions via portable-pty (max 3 concurrent)
- Remote command execution with timeout (max 5 concurrent, 512KB output limit)
- macOS APFS disk deduplication to prevent double-counting volumes

### Real-time

- Agent WebSocket handler with SystemInfo/Report/PingResult/TaskResult routing
- Browser WebSocket with FullSync + live Update/ServerOnline/ServerOffline broadcasts
- Terminal WebSocket proxy (browser <-> server <-> agent PTY)
- Background tasks: metric recording (60s), hourly aggregation, data cleanup, offline detection (30s), session cleanup (12h)

### Alert & Notification

- 14+ alert metric types: CPU, memory, disk, swap, network, load, temperature, GPU, connections, processes, traffic, offline, expiration
- Threshold rules with min/max bounds, sampling duration, AND logic, 70% majority trigger
- Alert state tracking with 300s debounce
- 4 notification channels: Webhook, Telegram, Bark, Email (SMTP)
- Notification groups with multi-channel routing
- Template variable substitution in notification messages

### Frontend

- React 19 SPA with TanStack Router (file-based routing) and TanStack Query
- Real-time dashboard with server cards, group filtering, statistics summary
- Server detail page with historical charts (1h/6h/24h/7d/30d) — CPU, memory, disk, network in/out, load, temperature, GPU
- Charts auto-refresh every 60 seconds
- Server list with search, multi-column sorting, batch delete
- Web terminal via xterm.js with FitAddon and WebLinksAddon
- Settings pages: users, notifications, alerts, ping tasks, commands, capabilities, API keys, security (2FA + password), audit logs
- Public status page (unauthenticated)
- Dark/light theme with system detection
- shadcn/ui components with Tailwind CSS v4
- Code splitting for xterm and recharts bundles

### Security

- Per-server capability toggles: Web Terminal, Remote Exec, Auto Upgrade (high risk, off by default), ICMP/TCP/HTTP Ping (low risk, on by default)
- Defense-in-depth: capabilities enforced on both server and agent side
- OAuth login: GitHub, Google, OIDC providers
- TOTP two-factor authentication with QR code setup
- Audit logging for all mutations
- argon2 password hashing with random salts

### Operations

- GeoIP region/country detection from agent IP (MaxMind MMDB)
- Billing info tracking: price, cycle, expiration, traffic limits per server
- SQLite backup/restore via admin API (VACUUM INTO + upload)
- Agent auto-upgrade with SHA-256 binary verification
- Systemd service files and install script
- Docker Compose support
- Nginx reverse proxy configuration (HTTP + WebSocket)
- GitHub Actions CI (clippy, test, build)
- Fumadocs documentation site (Chinese + English, 32 MDX pages)

### Testing

- 121 Rust tests: 110 unit + 11 integration (real SQLite, no mocks)
- 72 frontend Vitest tests across 8 test files
- 31 manual E2E browser verification scenarios
- cargo clippy with zero warnings enforced
- Ultracite (Biome) frontend linting
