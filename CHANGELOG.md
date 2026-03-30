# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.8.0] - 2026-03-31

### Added

- **iOS Mobile Companion** -- Full-featured iOS app for ServerBee with QR code pairing, Bearer token authentication, and APNs push notifications. Supports real-time server metrics, alerts, and WebSocket-based live updates
- **Mobile Authentication API** -- New `/api/mobile/auth/*` endpoints for mobile device registration, access token refresh, and logout. Opaque access tokens (15-min TTL) with refresh token rotation (30-day TTL). New `mobile_sessions` and `device_tokens` tables with 3 database migrations
- **QR Pairing** -- New `/api/mobile/auth/qr/pair` flow for secure iOS app pairing. Pending pair state stored in-memory with 5-minute expiry. QR code displayed in web app settings; iOS app scans to complete authentication without manual token entry
- **APNs Push Notifications** -- Apple Push Notification service integration for alert delivery to iOS devices. New APNs notification channel type alongside existing Webhook/Telegram/Bark/Email. Configurable in Settings > Notifications with team ID, key ID, and private key
- **Server Card Grid Redesign** -- Server cards now display 3 ring charts (CPU, Memory, Disk) using new `RingChart` SVG component, plus compact metric rows for load, processes, TCP/UDP/UDP connections, swap, and network speeds. New `UptimeBar` vertical bar charts for network quality (latency and packet loss trends)
- **RingChart Component** -- New SVG donut chart component for visualizing resource utilization with percentage text overlay, color-coded thresholds (green <70%, yellow 70-90%, red >90%), and full accessibility support
- **UptimeBar Component** -- New vertical bar chart component for displaying time-series data (network quality metrics). Supports null values (100% height for failures), minimum 10% height for visibility, and color callbacks per bar
- **Mobile Devices Management** -- New Settings page `/settings/mobile-devices` for viewing and revoking connected iOS devices. Displays device name, paired date, and last active timestamp with revoke action
- **Install Script Self-Deploy** -- `deploy/install.sh` can now install itself as `/usr/local/bin/serverbee` CLI when run without the `server` or `agent` subcommand. Supports `--cli-only` and `--version` flags for CLI-only installation and version pinning

### Changed

- **Server card layout** -- Complete rewrite of `ServerCard` component replacing progress bars with ring charts and sparklines with `UptimeBar` charts. 9 metrics now visible per card (3 rings + 5 system metrics + 4 network metrics) with improved information density
- **Network data merge** -- Network quality data from multiple targets now sorted chronologically before display, ensuring accurate temporal trends across all probe targets
- **Test counts** -- Frontend tests increased from 222 to 248 across 32 test files. Added 24 new component tests (9 ServerCard, 8 RingChart, 9 UptimeBar)

### Fixed

- **UptimeBar zero values** -- Zero values now render at minimum 10% height per spec instead of disappearing (was incorrectly optimized for positive values only)
- **RingChart SVG sizing** -- SVG element now has explicit `width` and `height` attributes matching the `size` prop, preventing rendering issues at non-default sizes
- **Toggle group namespace import** -- Added `biome-ignore` comment for `import * as React` pattern required by React context APIs

### Testing

- 395 Rust tests: 245 server unit + 42 server integration + 4 Docker integration + 43 common + 61 agent
- 248 frontend Vitest tests across 32 test files (was 222 across 29)
- 9 new ServerCard component tests (rendering, ring charts, metrics rows, network quality)
- 8 new RingChart component tests (percentage display, clamping, custom size, SVG sizing)
- 9 new UptimeBar component tests (bar heights, colors, null handling, zero values)

## [0.7.5] - 2026-03-29

### Fixed

- **Linux agent TLS startup** -- Agent release binaries now install the `rustls` ring `CryptoProvider` explicitly during startup, preventing process panics before HTTPS registration or WebSocket TLS handshakes on some Linux builds
- **Reverse-proxy agent WebSocket auth** -- Agent WebSocket handshakes now carry the token in both the query string and `Authorization: Bearer` header, and the server logs whether agent auth failed because the token was missing or invalid

### Testing

- 395 Rust tests: 245 server unit + 42 server integration + 4 Docker integration + 43 common + 61 agent
- 222 frontend Vitest tests across 29 test files
- 1 new agent startup unit test covering idempotent `rustls` provider installation
- 1 new agent WebSocket handshake unit test covering query-token + `Authorization` header construction

## [0.7.4] - 2026-03-29

### Added

- **Guided deployment manager** -- New `serverbee` CLI one-stop manager for `install`, `uninstall`, `upgrade`, `status`, `start`/`stop`/`restart`, `config`, and `env`, with interactive mode plus binary and Docker flows for both server and agent
- **Orphan server cleanup** -- New admin `DELETE /api/servers/cleanup` endpoint and `/servers` toolbar action to remove offline `New Server` placeholders that never completed initialization, while preserving online-but-uninitialized agents
- **Discovery key rotation UI** -- Settings page now exposes a confirmed "Regenerate" action for the auto-discovery key instead of requiring a direct API call

### Changed

- **Fingerprint-based agent re-registration** -- Agents now send a stable machine fingerprint during `POST /api/agent/register`, so repeated registration from the same host reuses the existing `server_id` and rotates its token instead of creating duplicate rows
- **Auto-discovery soft cap** -- New `auth.max_servers` / `SERVERBEE_AUTH__MAX_SERVERS` setting limits how many new servers auto-discovery can create. Fingerprint reuse does not count against the cap
- **Docker agent fingerprint stability** -- Agent Docker install docs and compose output now mount `/etc/machine-id` so container recreation keeps the same fingerprint and reconnects to the existing server record

### Fixed

- **Registration race recovery** -- Concurrent same-fingerprint registrations now recover from unique-index races by falling back to token rotation on the existing server row and refreshing `last_remote_addr`
- **Installer robustness** -- `deploy/install.sh` now handles piped/non-interactive stdin, auto-installs missing dependencies in unattended mode, starts the agent service after install, and keeps agent WebSocket auth working behind reverse proxies
- **Cleanup online protection** -- Cleanup candidate logic now excludes placeholder agents that are already online but have not yet sent `SystemInfo`, preventing accidental deletion during slow or partial initialization

### Testing

- 393 Rust tests: 245 server unit + 42 server integration + 4 Docker integration + 43 common + 59 agent
- 222 frontend Vitest tests across 29 test files
- 3 new agent fingerprint unit tests (`machine-id` hashing, deterministic output, 64-char validation)
- 7 new orphan-cleanup helper unit tests (`remove_ids_from_json`, online placeholder filtering)
- 2 new server integration tests (`POST /api/agent/register` fingerprint reuse, `DELETE /api/servers/cleanup` online placeholder protection)
- 2 new frontend utility tests for cleanup candidate counting on the Servers page
- New `tests/registration-hardening.md` checklist plus updated `/servers` and `/settings` manual verification coverage

## [0.7.3] - 2026-03-28

### Changed

- **Auto-trust private/loopback proxies** -- `server.trusted_proxies` now defaults to RFC 1918 + loopback CIDRs (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `127.0.0.0/8`, `::1/128`) instead of an empty list. This means `X-Forwarded-For` is trusted automatically behind common reverse proxies (Nginx, Docker, Railway) without manual CIDR configuration. Set `trusted_proxies = []` to disable
- **Prominent startup credentials** -- Auto-generated admin password and auto-discovery key are now grouped in a visually distinct block in the startup log, making them easy to spot on first launch
- **Environment variable docs restructured** -- ENV.md and Fumadocs configuration pages reorganized into layered categories (Quick Start / Common / OAuth / GeoIP / Retention / Internal) for easier onboarding

### Added

- **Railway `.env.example`** -- New `deploy/railway/.env.example` with recommended environment variables and Chinese comments for quick Railway deployment configuration

## [0.7.2] - 2026-03-27

### Changed

- **Version-driven agent upgrade** -- Admin now only submits a version string to trigger agent upgrades. Server automatically resolves the download URL and SHA-256 checksum from a configurable release source (`upgrade.release_base_url`), with platform detection based on agent-reported OS/arch. Eliminates the previous arbitrary `download_url` input
- **Agent token via Authorization header** -- Agent WebSocket authentication moved from URL query parameter (`?token=`) to `Authorization: Bearer` header, preventing token exposure in server/proxy logs. Query param still accepted with deprecation warning for backward compatibility
- **Hourly aggregation rewrite** -- `aggregate_hourly` now truncates timestamps to hour boundaries and uses SQL `INSERT...ON CONFLICT` upsert for idempotency. Numeric column aggregation pushed to SQLite (no more full in-memory load). `UNIQUE(server_id, time)` index enforced via migration

### Fixed

- **CORS policy removed** -- Removed overly permissive `allow_origin(Any)` CORS configuration. SPA is served from the same origin via rust-embed, so CORS is unnecessary
- **WebSocket message size enforced** -- `MAX_WS_MESSAGE_SIZE` (1MB) now configured on all 4 WebSocket upgrade points (agent, browser, terminal, docker_logs), preventing OOM from oversized messages
- **Rate limit bypass via X-Forwarded-For** -- Unified `extract_client_ip` across 5 files into shared `router/utils.rs`. New `server.trusted_proxies` CIDR config controls when XFF is trusted. Removed X-Real-IP fallback to close spoofing vector. DashMap expired entries now cleaned probabilistically
- **Agent getpwuid/getgrgid thread safety** -- Replaced non-reentrant `getpwuid`/`getgrgid` with `getpwuid_r`/`getgrgid_r` to eliminate data races in async/multi-thread file listing
- **Mandatory SHA-256 in agent upgrade** -- Agent now requires HTTPS and mandatory SHA-256 checksum verification for upgrade downloads. Removed optional `x-checksum-sha256` header fallback. Added 10-minute download timeout
- **Frontend WebSocket robustness** -- Terminal WS: added try/catch on JSON.parse and atob, typeof guard on data field. Main WS: replaced `as WsMessage` cast with two-layer runtime validation (structural guard + per-variant field checks including null element filtering)

### Testing

- 379 Rust tests: 237 unit + 39 integration + 4 Docker integration + 43 common + 56 agent
- 220 frontend Vitest tests across 28 test files
- 6 new `extract_client_ip` unit tests (trusted proxy, XFF parsing, spoofing prevention)
- 3 new `map_os`/`map_arch`/`normalize_version` unit tests
- 1 new DashMap expiry cleanup test
- 1 new `aggregate_hourly` idempotency test (disk I/O device aggregation)

## [0.7.1] - 2026-03-25

### Fixed

- **Network quality target visibility** -- Network overview and detail summaries now include newly assigned probe targets before their first probe result arrives, rendering an empty state instead of omitting the target
- **Network overview localization** -- The `/network` search field now uses the translated placeholder text instead of a hardcoded English string
- **Ping task action feedback** -- Ping task create/delete/enable/disable flows now use localized success and error toasts in both English and Chinese, and the enable/disable button is disabled while an update is pending

### Testing

- **Manual verification index** -- Replaced the monolithic `TESTING.md` with `tests/README.md` plus feature-scoped browser verification checklists for auth, dashboard, network quality, ping tasks, Docker, traffic, uptime, and related pages

## [0.7.0] - 2026-03-23

### Added

- **GeoIP Database Download** -- New `GET /api/geoip/status` and `POST /api/geoip/download` endpoints for managing the GeoIP database. Settings page includes a GeoIP card showing installation status with download/update button. Server Map widget prompts to download GeoIP when not installed
- **Cross-platform Disk I/O** -- Disk I/O collection now works on macOS and Windows via sysinfo `Disk::usage()` API, using mount point paths as keys. Linux `/proc/diskstats` implementation unchanged. Per-mount-path semantics on non-Linux (known APFS overcounting limitation documented)
- **Custom Dashboard** -- Fully customizable dashboard with drag-and-drop grid layout, 13 widget types (stat-number, server-cards, gauge, top-n, line-chart, multi-line, traffic-bar, disk-io, alert-list, service-status, server-map, markdown, uptime-timeline), dashboard switcher, editor mode with add/edit/delete widgets
- **Uptime Timeline** -- 90-day uptime visualization with per-day online/offline breakdown. New `UptimeTimeline` component renders a color-coded bar for each day (green = 100%, yellow = degraded, red = major outage, gray = no data) with hover tooltips showing date, uptime percentage, and online/total minutes
- **Uptime Daily API** -- New `GET /api/servers/{server_id}/uptime-daily` endpoint returning per-day uptime entries with configurable day range (default 90). Gap-filling logic ensures missing dates are represented with zero values for continuous timeline display
- **Uptime on Status Page** -- Public status pages now display a 90-day uptime timeline per server, replacing the previous simple uptime bar. Each server row shows the timeline alongside overall uptime percentage computed from daily data
- **Uptime on Server Detail** -- Server detail page now includes an uptime card showing the 90-day timeline for quick availability assessment
- **Uptime Timeline Dashboard Widget** -- New `uptime-timeline` widget type available in the customizable dashboard, allowing users to add uptime timelines to their dashboard layout
- **Uptime Threshold Configuration** -- Status page admin settings now include configurable uptime thresholds (`uptime_yellow_threshold` and `uptime_red_threshold`) that control the color breakpoints on the uptime timeline bars
- **Sidebar State Persistence** -- Sidebar open/collapsed state persisted to localStorage via zustand store, replacing previous cookie-based persistence
- **Agent IPv4/IPv6 Reporting** -- Agent now populates `ipv4` and `ipv6` fields in SystemInfo from detected network interfaces

### Changed

- Dashboard widget system refactored to extract shared logic into `widget-helpers.ts`, reducing code duplication across 13 widget components
- Status page API response fields renamed for consistency: `id` → `server_id`, `name` → `server_name`, `uptime_percentage` changed from `f64` to `Option<f64>` (returns `null` when no uptime data exists)
- GeoIP settings merged into the main Settings page (removed standalone `/settings/geoip` route and sidebar entry)
- Add Widget button moved to editor toolbar for better discoverability

### Fixed

- **Dashboard resize handles** -- Themed resize handles with proper border/background colors matching the current theme
- **TypeScript strict mode build errors** -- Resolved strict `tsc` build errors in `dashboard-grid.tsx` and `widget-picker.tsx` related to optional chaining and type narrowing
- **Server Map useQueryClient** -- Added missing import causing runtime error
- **Header separator alignment** -- Vertical separator between sidebar toggle and breadcrumb now properly centered

### Testing

- 362 Rust tests: 223 unit + 39 integration + 4 Docker integration + 43 common + 56 agent (was 361 in 0.7.0-pre)
- 220 frontend Vitest tests across 28 test files (was 186 across 23)
- 1 new `compute_disk_io` unit test for mount-path key rate calculation
- 3 new `uptime-timeline.test.tsx` tests (renders days, tooltip on hover, empty state)
- 3 new `UptimeService` unit tests (gap-filling empty returns zeros, gap-filling with data, date range boundaries)
- 7 new integration tests (uptime-daily, status page uptime, GeoIP status/download auth)
- 1 new `widget-config-dialog.test.tsx` test (uptime-timeline widget config)
- 9 new `dashboard-grid.test.tsx` tests (drag/resize local layout, mobile single-column)
- 8 new `use-dashboard.test.tsx` tests (CRUD hooks, cache sync)
- 7 new `dashboard-editor-view.test.tsx` tests (save/cancel, widget add/edit/delete)

## [0.6.0] - 2026-03-20

### Added

- **Historical Disk I/O Monitoring** -- Full-stack disk I/O monitoring: Agent reads `/proc/diskstats` on Linux to collect per-disk read/write throughput, Server persists data in `disk_io_json` JSON column on both `records` and `records_hourly` tables, hourly aggregation computes per-device averages. Frontend `DiskIoChart` component with Merged (total) and Per Disk tab views on the server detail page (historical mode only)
- **Conditional WebSocket connection** -- `useServersWs` hook gains `enabled` prop; `AuthedLayout` only establishes browser WebSocket after authentication completes, preventing premature connection attempts and handling 401 re-registration gracefully

### Changed

- Time range selector on server detail page now uses distinct `key` identifiers instead of matching by `interval`, fixing a bug where ranges with the same interval value (e.g., multiple `raw` ranges) could collide

### Fixed

- **Duplicate OpenAPI operation IDs** -- Ping task endpoints (`list_ping_tasks`, `update_ping_task`, `delete_ping_task`) now have explicit unique operation IDs, resolving OpenAPI spec generation errors and regenerated `openapi.json` / `api-types.ts`

### Testing

- 288 Rust tests: 288 unit + 29 integration + 4 Docker integration (unchanged from 0.5.0)
- 129 frontend Vitest tests across 16 test files (was 121 across 13)
- 3 new `disk-io-chart.test.tsx` tests (merged view, per-disk view, empty state)
- 3 new `disk-io.test.ts` tests (buildMergedDiskIoSeries, buildPerDiskIoSeries, null handling)
- 2 new `use-servers-ws-hook.test.tsx` tests (enabled prop, disabled state)
- 2 new Rust unit tests (DiskIo round-trip serialization, SystemReport backward compatibility)
- 2 new RecordService unit tests (disk_io_json persistence, hourly aggregation per-device averages)
- 1 new integration test (disk I/O records end-to-end)
- 1 new Agent collector test (disk I/O compute and device filtering)
- 10/10 Disk I/O E2E browser verification scenarios passed (DI1-DI10)

## [0.5.0] - 2026-03-18

### Added

- **Docker Container Monitoring** -- Full-stack Docker monitoring: real-time container list with CPU/memory/network/block I/O stats, container log streaming via WebSocket (stdout white + stderr red), Docker events timeline, networks and volumes dialogs, overview cards (running/stopped/total CPU/memory/Docker version)
- **CAP_DOCKER capability** -- New `CAP_DOCKER` (128) capability toggle for Docker monitoring, following the same defense-in-depth pattern as other capabilities. Low risk, disabled by default
- **Docker monitoring API** -- 7 new endpoints under `/api/docker/{server_id}/` for containers, stats, info, events, networks, volumes, and container actions (start/stop/restart/remove)
- **Docker log WebSocket** -- New `/api/ws/docker/logs/{server_id}` endpoint for real-time container log streaming with subscribe/unsubscribe protocol, tail parameter, and follow mode
- **Agent DockerManager** -- Docker monitoring via bollard client: container listing, stats polling, log streaming with batched delivery, event monitoring with auto-reconnect, networks and volumes enumeration
- **Docker events persistence** -- New `docker_event` database table for storing container lifecycle events (start/stop/die/create/destroy) with configurable retention (`docker_events_days`, default 7)
- **Docker viewer tracking** -- DockerViewerTracker service for ref-counted viewer management, ensuring Docker data is only polled when browsers are actively viewing the Docker page
- **Docker feature detection** -- Agent reports `features: ["docker"]` when Docker daemon is available; server stores features per server for frontend capability checks with REST API fallback

### Fixed

- **Docker log WebSocket protocol mismatch** -- Frontend sent `docker_logs_start` but server expected `subscribe`; frontend expected `docker_log` message type but server sent `logs`
- **ServerResponse missing features field** -- API could not return Docker feature information to the frontend
- **Agent poll_stats missing DockerContainers** -- Agent sent only DockerStats without DockerContainers, causing server cache to skip broadcasting
- **Docker availability timing** -- WS features data not reaching React components in time; added REST API fallback for Docker availability check

### Testing

- 226 Rust tests: 226 unit + 26 integration (was 210 unit + 26 integration)
- 3 new Docker types unit tests (container/action/log entry serialization)
- 21 new protocol serialization tests (Docker message variants)
- 5 new DockerViewerTracker unit tests
- 121 frontend Vitest tests across 13 test files (unchanged count)
- 23/24 Docker E2E browser verification scenarios passed

## [0.4.0] - 2026-03-18

### Added

- **Monthly Traffic Statistics** -- Full-stack traffic monitoring system: hourly and daily traffic aggregation from Agent network reports, billing cycle-aware queries with configurable `billing_start_day`, timezone-aware daily rollup via `scheduler.timezone`, and traffic prediction algorithm for cycle-end estimates
- **Traffic API** -- New `GET /api/servers/{id}/traffic` endpoint returning cycle totals, daily/hourly breakdowns, usage percentage against traffic limit, and end-of-cycle prediction
- **Traffic database tables** -- 3 new tables (`traffic_hourly`, `traffic_daily`, `traffic_state`) with delta-based calculation from cumulative counters, automatic hourly→daily aggregation, and configurable retention
- **Traffic frontend** -- Collapsible traffic detail card on server detail page with daily/hourly bar charts (shadcn Chart + Recharts), traffic progress bar showing usage against limit, `useTraffic` hook for traffic API integration
- **Billing start day** -- New `billing_start_day` field on server entity, configurable via server edit dialog, determines when monthly billing cycles begin
- **Scheduler timezone config** -- New `[scheduler]` config section with `timezone` setting for daily traffic aggregation (supports IANA timezone names like `Asia/Shanghai`)
- **shadcn Chart components** -- Added `apps/web/src/components/ui/chart.tsx` with `ChartContainer`, `ChartTooltip`, `ChartLegend` wrappers for Recharts integration following shadcn patterns
- **Shared chart color palette** -- `CHART_COLORS` constant in `apps/web/src/lib/chart-colors.ts` for consistent multi-series chart colors across all pages
- **Capabilities dialog** -- Server capabilities moved from inline section to a dedicated dialog with grouped high-risk/low-risk layout and per-toggle descriptions
- **Traffic retention config** -- New retention settings: `traffic_hourly_days` (default 7), `traffic_daily_days` (default 400), `task_results_days` (default 7)

### Changed

- All frontend charts migrated from raw Recharts to shadcn Chart components: MetricsChart, LatencyChart, PingResultsChart, and TrafficCard now use `ChartContainer`/`ChartTooltip`/`ChartLegend` wrappers
- Transfer cycle alerts (`transfer_in_cycle`, `transfer_out_cycle`, `transfer_all_cycle`) refactored to query `traffic_hourly` and `traffic_daily` tables instead of raw metric records, improving accuracy for billing cycle calculations
- Chart height increased from 200px to 260px for better readability
- Traffic card layout refined with tabs (daily/hourly), collapsible detail section, and persistent realtime buffer across route changes
- Chart tooltip improved with `valueFormatter` and Y-axis formatting for network speed display
- Diversified chart colors across multi-series charts for better visual distinction
- Install script (`deploy/install.sh`) significantly rewritten with improved error handling
- Docker Compose configuration updated with refined service definitions

### Fixed

- Recharts Tooltip type errors resolved with proper TypeScript types
- `Option<Option<T>>` deserialization: custom deserializer to distinguish `null` from absent fields (fixes `billing_start_day` updates)
- Clippy `collapsible-if` warnings resolved across the codebase
- Removed Recharts outline CSS hack (no longer needed with shadcn Chart)

### Testing

- 236 Rust tests: 210 unit + 26 integration (was 192 unit + 23 integration)
- 21 new traffic service unit tests (delta calculation, cycle range computation, prediction algorithm, aggregation, cleanup)
- 3 new traffic integration tests (API response, billing cycle query, one-shot regression)
- 1 new config unit test (timezone parsing validation)
- 121 frontend Vitest tests across 13 test files (was 116 across 10)
- 3 new traffic card tests (render, loading state, empty state)
- 2 new use-traffic hook tests (data fetching, error handling)
- shadcn chart verification checklist added to TESTING.md

## [0.3.0] - 2026-03-16

### Added

- **Network Quality Monitoring** -- Full-stack network quality monitoring system: Agent sends ICMP/TCP/HTTP probes to configured targets, Server records results and aggregates hourly, Frontend displays multi-line latency charts with real-time and historical views
- **96 preset probe targets** -- Built-in network probe targets loaded from embedded TOML config: 31 Chinese provinces × 3 ISPs (Telecom/Unicom/Mobile) using Zstatic CDN TCP nodes + 3 international ICMP targets (Cloudflare, Google DNS, AWS Tokyo)
- **Network quality overview page** (`/network`) -- Server-level network quality cards with target count, average latency, availability, and anomaly indicators
- **Network quality detail page** (`/network/:id`) -- Per-server multi-line latency chart (Recharts), target cards with toggle visibility, anomaly summary table, CSV export, real-time + historical time ranges (1h/6h/24h/7d/30d)
- **Network probe settings page** (`/settings/network-probes`) -- Target management tab (96 presets + custom CRUD) and global settings tab (probe interval, packet count, default targets)
- **Per-server target management** -- Assign up to 20 probe targets per server via manage dialog, with validation
- **Network probe alert types** -- `network_latency` and `network_packet_loss` alert rule types integrated into the existing alert system
- **Real-time network probe WebSocket** -- Live probe results streamed via existing browser WebSocket, with seed data from last hour for immediate chart display
- **Preset target architecture** -- Presets defined in `crates/server/src/presets/targets.toml`, embedded via `include_str!`, parsed at startup with `LazyLock` cache. DB stores only user-created targets. API returns unified `TargetDto` with `source`/`source_name` fields
- **File Management** -- Full-stack remote file manager: browse directories, view/edit text files with Monaco Editor (syntax highlighting for 15+ languages), create/rename/delete files and folders, upload/download with progress tracking. Path sandbox via `root_paths` and `deny_patterns` for security
- **CAP_FILE capability** -- New `CAP_FILE` (64) capability toggle for file management, following the same defense-in-depth pattern as other capabilities. High risk, disabled by default
- **File management API** -- 13 new endpoints under `/api/files/{server_id}/` for list, stat, read, write, delete, mkdir, move, download, upload, plus transfer management endpoints
- **Agent FileManager** -- Path validation with `root_paths` sandbox (prevents directory traversal), `deny_patterns` glob matching (blocks `.env`, `*.key`, `*.pem`, etc.), base64 content encoding, chunked download/upload support
- **FileTransferManager** -- Server-side transfer orchestration with concurrent transfer limiting (max 3), automatic expiry cleanup, temporary file management for downloads
- **Request-Response Relay** -- New `pending_requests` mechanism in AgentManager enabling synchronous request-response patterns over WebSocket (used by file operations)
- **Monaco Editor integration** -- Embedded Monaco editor with dark/light theme sync, Ctrl+S save shortcut, conflict detection (warns when file modified externally), language detection from file extension
- **File management i18n** -- Full Chinese and English translations for all file manager UI elements
- **shadcn/ui component migration** -- Replaced hand-rolled Dialog, Select, Switch, Tabs, Skeleton, Checkbox with shadcn/ui equivalents across 29+ files
- **DataTable (TanStack React Table)** -- Generic `DataTable` component with `DataTableColumnHeader`, `DataTablePagination`, `createSelectColumn`. Refactored 5 tables: servers, capabilities, audit-logs, network-probes, anomaly-table
- **Toast notifications (Sonner)** -- 40+ mutations across all CRUD operations now show success/error toasts

### Changed

- Probe targets no longer stored as `is_builtin` rows in database — preset targets live in code, user targets in DB, merged at API level
- `network_probe_target` table: removed `is_builtin` column, removed FK constraints on `target_id` in config/record/record_hourly tables
- Deleted migration `m20260315_000005_update_builtin_targets` (replaced by embedded TOML presets)
- `list_targets` API returns `Vec<TargetDto>` with `source: "preset:china-telecom"` for presets and `source: null` for custom targets
- `update_setting` now validates `default_target_ids` against both presets and DB targets
- `set_server_targets` validates all target IDs before assignment
- All native `<table>`, `<select>`, `<input type="checkbox">` elements replaced with shadcn/ui components (zero native remnants)
- Default capabilities value updated from `56` to `56` (CAP_FILE=64 excluded by default, requires explicit opt-in)

### Fixed

- **24h time range crash** -- `Invalid time value` error when clicking 24h on network detail page (hourly records had `hour` field instead of `timestamp`, now unified via `ProbeRecordDto`)
- **File content base64 display** -- Frontend displayed raw base64 in Monaco editor instead of decoded text; added UTF-8 safe base64 encode/decode for file read/write
- **Root paths navigation** -- File manager showed empty directory on initial load when `root_paths` didn't include `/`; agent now returns root_paths as virtual entries when browsing ancestor directories
- Sonner theme integration (uses project's `useTheme`, removed `richColors` for neutral style)
- Removed undefined `cn-toast` CSS class from Sonner component

### Testing

- 215 Rust tests: 192 unit + 23 integration (was 132 unit + 15 integration)
- 8 new preset module tests (load, uniqueness, find, group metadata, probe type validation)
- 6 new service tests (preset protection, setting validation, server target assignment)
- 2 new integration tests (source field verification, preset update protection)
- 9 new file transfer service tests (concurrent limits, expiry cleanup, status transitions, progress updates)
- 24 new agent file manager tests (path validation, directory listing, file read/write, delete/mkdir/rename, upload/download)
- 6 new file management integration tests (offline handling, capability enforcement, transfer endpoints, admin-only write/delete/mkdir)
- 116 frontend Vitest tests across 10 test files (was 86 across 9)
- 30 new file-utils tests (extension-to-language mapping, text/image file detection)
- 22 new E2E verification scenarios for network quality monitoring (N1-N22)
- 36 new E2E verification scenarios for file management (F1-F36)

## [0.2.1] - 2026-03-15

### Added

- **Full-site i18n** -- Chinese + English internationalization for the entire web frontend (~250 translated strings across all pages)
- **react-i18next** integration with `i18next-browser-languagedetector` for automatic browser language detection and `localStorage` persistence
- **Language switcher** -- Toggle button in the header (and public status page) to switch between English and 中文
- **7 translation namespaces** -- `common`, `dashboard`, `servers`, `terminal`, `settings`, `login`, `status`, each with en/zh JSON files
- **TypeScript type-safe translation keys** -- Module augmentation ensures all `t()` calls reference valid keys at compile time
- **14 i18n E2E verification scenarios** added to TESTING.md

### Changed

- `capabilities.ts` uses `labelKey` (translation key) instead of hardcoded `label` string
- Sidebar `navItems` uses `labelKey` pattern for translatable navigation labels
- All 25 component/page files refactored to use `useTranslation()` hook instead of hardcoded English strings

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
