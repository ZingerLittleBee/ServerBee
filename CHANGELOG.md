# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.0.0-alpha.8] - 2026-06-19

### Security

- **SSRF guard on ping / network-probe targets** -- Probe targets are free-form strings pushed from the server to every agent and were executed after only a capability check, with no target validation -- a vector for turning the agent fleet into a distributed internal-network / cloud-metadata scanner driven by a compromised server or rogue admin. The agent now runs the monitor-grade SSRF guard on every ICMP/TCP/HTTP probe: RFC1918 internal monitoring stays allowed, but loopback, link-local (incl. the `169.254.169.254` cloud-metadata endpoint), NAT64, and broadcast are blocked. TCP probes connect to the validated address with no re-resolve, and HTTP probes disable redirect following (blocking a `302` bounce into metadata) and pin the host to the validated address. As defense-in-depth, `PingService` and `NetworkProbeService` also reject literal loopback/metadata targets at create/update time. Internal RFC1918 monitoring and domain-name targets are unaffected

## [1.0.0-alpha.7] - 2026-06-14

### Added

- **POSIX install script with OpenRC support** -- `deploy/install.sh` was rewritten in portable POSIX sh and now manages services under both systemd and OpenRC, making Alpine and other non-systemd hosts first-class. Releases publish `sha256sums` and the installer verifies every download against them, and a CI job gates the script with shellcheck and dash
- **Connection-lost banner** -- The layout shows a persistent banner when the browser loses its WebSocket link to the server (after a short grace period to ignore brief blips) and clears it automatically on reconnect, so a dropped connection no longer leaves the dashboard silently stale
- **Shareable tab state in the URL** -- The server detail tabs (metrics / traffic / security / IP quality) and the status-page settings tabs (config / incidents / maintenance) are persisted in the URL, so reload, browser back/forward, and shared links keep the selected tab instead of resetting to the first one; an invalid tab value falls back to the default

### Changed

- **Capability-gated terminal and file routes** -- Opening the web terminal or file manager for a server whose `CAP_TERMINAL` / `CAP_FILE` bit is disabled now renders an explanatory notice instead of a dead shell, and skips the doomed WebSocket connect and file-list request

### Fixed

- **Custom dashboard widgets** -- The backend accepts `metric-card` widgets, module widgets load reliably, the grid is more responsive, and the default layout no longer leaves an empty band
- **Service monitors** -- HTTP-keyword checks accept custom ports, and the service-monitor detail page no longer crashes on SSL certificate dates
- **Installer robustness** -- `purge` removes the base directory even with orphaned files and the snap Docker config directory; `toml_set` preserves the section separator when appending a key; `SERVERBEE_*` env is forwarded across `doas` elevation; `sha256sums` are matched on the exact filename; OpenRC agent respawn is bounded on permanent enrollment failure; and status/log commands print a clear message instead of nothing when there is no output
- **Web reliability** -- Failed data queries and dashboard create/update errors surface a toast instead of an empty view; destructive actions (monitors, ping tasks, notifications, incidents, maintenance windows, OAuth unlink, password change) require confirmation; the terminal clears its stale WS error on reconnect; capability toggles reflect immediately; `localStorage` access is guarded against unavailable storage; realtime state is no longer mutated in place; numeric `&&` guards no longer render a literal `0`; and alert-rule and scheduled-task validation is surfaced
- **Web layout & accessibility** -- Wide tables scroll instead of overflowing the viewport, modal dialogs stay centered on narrow screens, the main scroll area is constrained to the viewport width, the traffic table scrolls on narrow screens with a clamped usage label, latency chart series stay mounted when toggling targets, network overview cards deep-link to a valid time range, settings breadcrumbs are complete, terminal and file breadcrumb labels resolve, the security and IP-quality pages get a11y and narrow-screen fixes, data-table empty states are localized, the public traffic bar uses cumulative totals, and the API-key dialog gains a copy button
- **Agent** -- SSH security events are detected on OpenSSH 9.8+, which logs authentication through the new `sshd-session` process

### Security

- **OAuth login hardening** -- The OAuth sign-in flow adopts PKCE (S256), binds the login state to the initiating browser via a pre-auth cookie (closing a CSRF / session-fixation gap), and redacts OAuth secrets from `Debug` output
- **SSRF guards for service monitors** -- HTTP-keyword and other checkers route through a shared SSRF guard that rejects loopback, cloud-metadata, and private targets at create time and re-validates on every HTTP redirect hop
- **Agent token transport** -- The agent WebSocket prefers the `Authorization` header over the query string when sending its token
- **Expanded audit logging** -- Database backup/restore, user-management actions, and API-key creation/deletion now emit audit-log entries, and restore audit entries persist into the restored database
- **Fail-closed admin route gating** -- Every `/settings/*` route is admin-only by default; members reach only the self-service mobile-devices, API-keys, and security pages and are redirected away from any other settings route, so a new settings page is admin-only unless explicitly allow-listed

### Performance

- **Smaller initial bundle** -- Routes and vendor libraries are code-split into separate chunks
- **Hot-path cleanups** -- Linear scans were removed from the data-transform hot paths, and several components derive state during render instead of in effects

## [1.0.0-alpha.6] - 2026-05-31

### Added

- **Network quality dashboard widgets** -- New network overview, network latency chart, and network quality summary widgets, complete with per-widget config forms, i18n strings, picker icons, and registered widget types. Detail-page and widget chart records now share a single pure merge function and records hook. The dashboard save path whitelists the new network quality widget types on the backend
- **Server-cards widget layout controls** -- The dashboard server-cards widget gains a grid/list layout toggle, sizes itself to its content height (applied instantly to avoid overlap), and reveals additional rows on scroll instead of paginating, with a load-more spinner while fetching

### Changed

- **Documentation overhaul** -- The Chinese docs locale was renamed from `cn` to `zh` and README links now point to the docs site. The configuration reference was restructured into tables, and the agent, deployment, monitoring, terminal, ping, alerts, architecture, and index guides were expanded and corrected (JSON terminal transport with base64 data field, single status page, retention tiers, reverse proxy and OAuth/mobile coverage). `ENV.md` and the config docs were synced with the code

### Security

- **RBAC hardening** -- The Docker container logs WebSocket and the file read/download endpoints are now restricted to admins. The password policy is unified across change flows, and all active sessions are revoked when a user changes their password

## [1.0.0-alpha.5] - 2026-05-28

### Added

- **Custom widget system (B/C method)** -- New `@serverbee/widget-sdk` workspace package exposes `defineWidget`, a bundled `z` schema validator, and a typed hook surface (live: `useServers/useServer/useMetric/useCapability` via `useSyncExternalStore`; domain: `useHistory/useTraffic/useAlerts/useServiceMonitors/useUptime/useGeoIp`; host: `useTheme/useConfigUpdate`; escape hatches: `useApiQuery/useApiMutation`). Widgets are authored as a single ESM file with a top-of-file `@serverbee-widget` JSDoc manifest, statically extractable, no `eval`. Admins install via `POST /api/widget-modules` (URL or multipart upload) for single `.js`/`.mjs` files or `.zip` collection bundles with a `collection.json` index. Built-in widgets are emitted by a new Vite nested-build plugin to `apps/web/dist/builtin-widgets/`, embedded into the server binary via rust-embed, and registered at boot
- **Dashboard module rendering** -- `dashboard_widget` gained a `module_id` column; `widget_type='module'` widgets dispatch through the widget registry and render via the SDK component contract. The picker surfaces installed modules under a "Custom Widgets" section; the config dialog renders the module's `configSchema` via the SDK form renderer (real renderers for `z.metricPath`/`z.color`/`z.duration`) with friendly placeholders for missing modules or empty schemas. `ActionButton` from the SDK ships with confirm dialog, pending state, and success/error toast wiring
- **Bilingual widget docs** -- `apps/docs/content/docs/{en,cn}/custom-widgets.mdx` covers method B (single file) and method C (zip bundle) end-to-end: manifest fields, build pipeline with React/SDK externals, install flows, asset resolution rules, SDK surface summary, and the full safety/limits table

### Changed

- **Widget install hardening** -- SSRF guard now resolves DNS and rejects any host whose IP falls in a reserved/private range (loopback, RFC 1918, CGNAT, link-local incl. cloud metadata, IPv6 ULA/link-local, benchmarking, documentation, multicast, reserved); HTTP redirects are disabled; uploads enforce a per-route 1 MiB body limit with streaming size accounting; zip extraction caps total uncompressed size at 32 MiB across at most 64 entries; manifest extractor rejects sources > 1 MiB up front; id conflicts across `source_type` (e.g. upload trying to overwrite a builtin) return `409 Conflict`; `dashboard_widget.module_id` is validated against installed modules; SDK declarations carry a `sdkVersion` semver range checked at load time. Install and uninstall events emit audit log entries
- **Runtime bridge** -- `mountRuntimeBridge` now wires the SDK runtime to the live React Query servers cache and the host theme provider via `useSyncExternalStore`, surfaces sonner toasts, and exposes a confirm-dialog request channel. `/runtime/*` import-map shims are served with `Cache-Control: no-cache` to avoid stale shim drift across SPA upgrades. `defineWidget` rejects duplicate action ids

### Removed

- **Legacy SPA theme + custom CSS theme system** -- The `spa_theme` package upload feature, `custom_theme` CSS variable system, and seven preset themes are deleted in their entirety (backend service/router/entity, migrations dropping `spa_themes` and `custom_theme`, the appearance settings UI, preset CSS files, `theme_ref` from public status pages, and the `SERVERBEE_FEATURE__CUSTOM_THEMES` config field). The theme provider is collapsed to light/dark/system. The old `custom-themes.mdx` and `custom-frontend.mdx` doc pages are replaced by `custom-widgets.mdx`

## [1.0.0-alpha.4] - 2026-05-27

### Added

- **Public status page** -- New `/api/status/*` public surface with field redaction and a singleton `is_public` toggle exposes a curated public dashboard; the web app gained public variants of the server list, server detail, network overview, network detail, and IP quality cards with grid/list toggles and i18n coverage
- **Custom SPA themes** -- Operators can upload zip-packaged frontend themes from Settings → Appearance. The server validates the manifest, runs zip-bomb and symlink security checks, supports preview/activate flows with cookie-based theme selection, exposes `/__system/clear-recovery` and `/__system/clear-preview` recovery endpoints, and ships a starter template under `templates/` with a `pack.ts` helper
- **Agent registration redesign** -- "Add Server" now creates a pending server with full metadata up front; admins can recover offline agents via a dedicated dialog, regenerate enrollment codes with optimistic CAS, and bound enrollments tie tokens to specific servers. The UI surfaces pending status indicators and disables rotate-token on pending rows
- **Rate limit management page** -- New admin page and API to inspect and reset per-IP rate limits, with separate scopes for login, registration, and the public status surface; admin rate_limit endpoint reports the public scope alongside the existing ones
- **Network anomaly KPI card** -- The anomaly count is surfaced as a clickable KPI tile with a dialog containing the table, count, and window selector; latency anomaly detection thresholds are now configurable via env vars
- **ipapi.is IP quality provider** -- Default IP risk provider switched to ipapi.is with ip-api as automatic fallback orchestrated by `IpRiskService`; the snapshot entity and DTOs gained the new abuser_score and related fields, and the IP quality card renders them

### Changed

- **Settings forms moved into dialogs** -- Notification channels and groups, user creation, and ping task creation were converted to dialogs; outer card wrappers were dropped on alerts, notifications, and api-keys pages in favour of inline layouts, and redundant page titles were removed from settings routes
- **Service monitors and notifications polished** -- Service monitors gained a header description and an inline add button within the table column; notification sections wrap in cards for clearer grouping
- **Latency charts use 24-hour time** -- Avoids am/pm ambiguity on dense timelines
- **Agent registration error handling** -- Agent categorizes registration errors and backs off in-process instead of looping fast on permanent failures; registration is transactional with no implicit server creation
- **Default register rate-limit raised** -- From 3 to 10 attempts per 15-minute window so legitimate batch installs do not lock themselves out
- **Status page collapsed to singleton** -- Admin status-page router and service collapsed to `GET`/`PUT` on a single row, with new `is_public` columns and DTO/field cleanup
- **Canonical 429 error** -- OpenAPI rate-limit responses standardised and the in-band sweep tightened

### Fixed

- **IP quality provider edge cases** -- `abuser_score` clamps to `0..=100`; `risk_provider=none` suppresses fallback; misconfigured provider names warn at startup; new ipapi.is fields persist correctly in `save_ip_quality_snapshot`
- **Status page migration safety** -- Dropped a broken manual transaction from `simplify_status_page` and parameterised migration `LIKE` clauses inside a transaction; servers migration now uses explicit column names to avoid positional drift
- **SPA theme handler** -- Tightened the serve handler for spec compliance with cookie precedence, preview banner, and review fixes; the theme extractor was hardened with additional zip-bomb and symlink coverage
- **Agent token validation** -- `validate_agent_token` filters `NULL token_hash` at the query layer with a half-bound row regression test
- **Latency chart and UI polish** -- Anomaly count card gained `cursor-pointer`; tag chips test alignment with placeholder behaviour
- **Web bun.lock** -- Synced with the v1.0.0-alpha.3 web version bump that landed without lockfile refresh

### Removed

- **Recovery-merge subsystem** -- Replaced by the new agent recovery flow; the legacy server and web code, including orphaned type re-exports, were removed
- **Legacy paid IP risk providers** -- Removed in favour of the ipapi.is + ip-api stack
- **RebindIdentity protocol** -- Handler and protocol removed as part of the registration redesign
- **Dead `ServerMessageOutcome` enum** -- Removed along with legacy test helpers
- **Legacy slug-based status page surface** -- The `/status/$slug` route was dropped after the singleton refactor

### Documentation

- **Custom frontend theme guide** -- New EN/CN docs cover `pack.ts`, manifest fields, and the upload flow; the SPA theme manual E2E checklist was added under `tests/`
- **Network probe anomaly thresholds** -- Configurable latency anomaly env vars are documented in ENV.md and the bilingual configuration pages
- **Design specs and plans** -- Added specs and plans for the public status page refactor, custom SPA themes, agent registration redesign, and ipapi.is provider refactor, with multiple review-driven revisions
- **IP quality docs replaced** -- The IP quality provider docs were rewritten around ipapi.is with the ip-api fallback story
- **Removed fabricated env vars** -- Stripped references to `SERVERBEE_FEATURE__SPA_THEMES` that never existed in the codebase

## [1.0.0-alpha.3] - 2026-05-25

### Added

- **ASN database for traceroute enrichment** -- A new ASN MMDB service labels every traceroute hop with its autonomous system number; the settings page exposes a download/update card mirroring the existing GeoIP control, and `SERVERBEE_ASN__MMDB_PATH` / the `[asn]` config section let operators bring a custom file
- **Server version on the settings page** -- A public `/api/about` endpoint reports the running build's `CARGO_PKG_VERSION` and the settings page renders it in an About row so operators can confirm the version at a glance
- **Manual audit log clear** -- Admins can wipe the audit log table from the audit logs page via a destructive button with a confirmation dialog; the clear itself is recorded as an `audit_log_clear` entry afterward so the operator who triggered it remains auditable

### Changed

- **Settings page redesigned** -- The standalone GeoIP/ASN/About cards were replaced with a unified `SettingsSection`/`SettingsRow` primitive grouped into "Data sources" and "About" panels in a macOS System Settings style; the DB-IP attribution moved into the section footer
- **Traceroute dialog UX** -- The dialog was rebuilt around a quick-pick "Recent" chips row backed by a full-history popover, grew to 92vh, pins the all-history button to the right of the chips, renders the protocol select's uppercase label, and shows a loading spinner while a selected history snapshot is being fetched
- **ScrollArea adoption on network surfaces** -- The traceroute result table, traceroute history list, and manage-targets dialog list all migrated from native `overflow-auto` to shadcn's `ScrollArea`; the manage-targets list also grew to 70vh so far fewer targets sit below the fold

### Fixed

- **Agent file receive flush** -- `receive_chunk` now flushes the file handle before returning so the last chunk reliably lands on disk
- **Select trigger label vs. value** -- Several admin selects (users page role picker, traceroute protocol picker) were rendering the raw value instead of the label; the `items` prop is now passed through so the trigger shows the display string
- **Self-delete button on users page** -- The current user's own row no longer shows a delete button that would have failed server-side

### Documentation

- **ASN configuration documented** -- `SERVERBEE_ASN__MMDB_PATH` and the `[asn]` config section are documented in ENV.md and the bilingual configuration MDX pages, and the GeoIP/ASN endpoints are now annotated with `utoipa::path` so they appear in `/swagger-ui/`

## [1.0.0-alpha.2] - 2026-05-24

### Added

- **Embedded traceroute with history and protocol selection** -- The shell `traceroute` invocation is replaced by an embedded `trippy-core` engine that runs per-hop probes over ICMP, UDP, or TCP and streams round updates back to the browser. Results are persisted to a new `traceroute_record` table with admin delete and clear controls, hops are enriched with PTR (reverse DNS) data via a server-side LRU cache, and the network detail page renders a 10-column streaming hop table inside a header dialog backed by a history list. New protocol enums (`TraceProtocol`, `RecordedProtocol`), `TracerouteRoundUpdate` agent messages, and a `TracerouteEnricher` on `AppState` make round-by-round streaming defense-in-depth safe -- updates from a mismatched `server_id` are rejected and each traceroute is bounded by a 60s wall-clock timeout
- **Capability picker during agent install** -- When adding a server from the web UI, admins can now pick exactly which agent capabilities to enable instead of accepting the default set, and the install script (`deploy/install.sh`) gained a matching interactive capability picker so the choice flows through to the new agent on first run. Capability toggles are also disabled for offline servers in the capabilities settings to avoid silent drift
- **Audit log filtering** -- The audit log page renders full-width and supports filtering by action and by user, so security reviews on long histories no longer require scrolling through every entry
- **Railway pre-release pinning** -- The Railway deployment template now accepts a `SERVERBEE_IMAGE_TAG` build argument so operators can pin a specific pre-release image (e.g. `1.0.0-alpha.2`) without forking the template, and the deployment docs describe the override

### Changed

- **Default capabilities include firewall and IP quality** -- `CAP_DEFAULT` now grants `CAP_FIREWALL_BLOCK` and `CAP_IP_QUALITY` out of the box so new agents get the full operational toolkit without manual toggling
- **IP quality blocked-state explanation** -- When an IP quality check is denied, the server reports which side blocked the request and the web UI surfaces the explanation inline on the server detail tab instead of showing an opaque failure
- **Network anomaly window alignment** -- The network detail anomaly window now matches the overview badge, and recent anomalies are surfaced regardless of the active window size so a short window no longer hides events the overview is highlighting
- **Capabilities page toolbar** -- The capabilities settings page was streamlined with a tighter toolbar and batch actions, and security preset cards now have consistent button alignment with reserved space for two-line descriptions
- **i18n coverage** -- The server detail tab labels are now translated, a dedicated i18n namespace was added for the IP Quality feature, and the security page filter dropdowns show their localized labels instead of raw keys

### Performance

- **Server detail CLS reduction** -- Cumulative Layout Shift on the server detail page dropped from 0.48 to 0.04 by deferring offscreen content, reserving space for late-loading widgets, and disabling Recharts animations on every chart in the route
- **Uptime timeline rewrite** -- The 90-day uptime timeline now paints as a single pixel-snapped CSS gradient on its own compositor layer with one shared tooltip popup across all segments, eliminating per-segment React nodes and the gradient seams that appeared on subpixel widths
- **Dashboard widget lazy loading** -- Dashboard widgets are now viewport-gated and chart animations are disabled by default, so a dashboard with many widgets no longer stalls the initial paint; a new docs page records the recommended widget capacity limits
- **Route-level code splitting** -- The server detail and terminal routes are now lazy-loaded, and the route generator ignores `-page.tsx` lazy modules so the `/servers` list page ships a noticeably smaller initial bundle

### Fixed

- **Traceroute correctness** -- Traceroute updates from a mismatched `server_id` are rejected at the server, the agent bounds each traceroute with a 60s wall-clock timeout, the PTR cache evicts by `inserted_at` instead of by IP ordering, and the `traceroute_record` foreign key was corrected to reference the `servers` table
- **Server card layout stability** -- Server card height now stays consistent whether the card has tags or not, and the route generator no longer treats `-page.tsx` lazy modules as routable, preventing accidental layout shifts on first paint
- **DataTable width blowup** -- Removed `table-fixed` from the shared `DataTable` so wide cells no longer force the whole grid to overflow horizontally
- **Add-server install command host** -- The install command shown in the add-server dialog now points at `raw.githubusercontent.com` so the copy-pasted one-liner actually fetches the script

### Documentation

- **Traceroute design and operations** -- New design spec (with four review passes), a matching implementation plan, and a manual E2E checklist describe how the embedded trippy-core flow replaced the prior shell-based path
- **Cost insights reference** -- A new dedicated docs page covers the cost insights and value-score feature so the configuration is no longer buried inside the alerts docs
- **Dashboard widget capacity limits** -- A new docs page records the recommended upper bound on widgets per dashboard, derived from the viewport-gating performance work in this release

## [1.0.0-alpha.1] - 2026-05-23

### Added

- **IP quality & streaming unlock checks** -- Agents probe a configurable catalog of streaming and AI services (Netflix, ChatGPT, Spotify, and more) and score their public IP for risk. Results appear on a dedicated overview page, a server-detail tab, and optionally on public status pages. Gated behind the new `CAP_IP_QUALITY` capability and protected by an SSRF guard that blocks internal ranges, IPv4-mapped IPv6 bypasses, and embedded credentials
- **Firewall blocklist management** -- Servers can block abusive IPs through an nftables-backed firewall manager, with a one-click block action, an auto-block toggle that reacts to security events, and a three-tier guardrail that protects the operator's own IPs from accidental lockout. Gated behind the new `CAP_FIREWALL_BLOCK` capability
- **Security event detection** -- Agents detect SSH logins, SSH brute-force attempts, and port scans via journal, conntrack, and kernel-firewall watchers, reporting them as security events with severity escalation. A new Security overview page and per-server Security tab visualize events, and alert rules can match on them. Gated behind the new `CAP_SECURITY_EVENTS` capability
- **Hardened Agent self-upgrade** -- A new `[upgrade]` config section pins release download and checksum URLs, supports a `--release-repo` CLI override, and verifies the release-signing certificate via SPKI SHA-256 pinning
- **iOS mobile client** -- The iOS app gains push-notification deep linking, an actor-based WebSocket layer with heartbeat and exponential-backoff reconnect, full VoiceOver and Dynamic Type accessibility, an editable device name, insecure-URL banners, and SwiftLint/swift-format tooling

### Changed

- **Dashboard rendering** -- The service-status widget was redesigned with a summary and richer rows, the gauge widget locks to a square aspect on resize, and dashboard WebSocket re-renders were reduced
- **Release CI** -- The cross-platform release build matrix no longer runs on pull requests; pre-release tags are flagged as pre-releases and no longer move the Docker `:latest` tag

### Fixed

- **NAT and Docker country detection** -- `country_code` is now populated for agents behind Docker bridges and NAT
- **Uptime timeline** -- Fixed tooltip clipping and a stray horizontal scrollbar
- **IP quality robustness** -- Response body streaming is capped, `0.0.0.0/8` is blocked, ambiguous unlock probes report as Failed rather than Blocked, and stale foreign keys and missing capabilities are guarded

## [0.9.4] - 2026-05-20

### Added

- **Servers table redesign with bulk actions** -- The Servers page now uses a fill-height table with a sticky header, a single consolidated toolbar row, an optional bulk-actions select column gated behind a toggle, a 2x2 disk/network cell layout, and pagination that hides itself when only one page exists
- **Per-target network breakdown** -- Server card tooltips show a per-target network traffic breakdown
- **Offline server cards are dimmed** -- Offline servers are visually de-emphasized with an overlay
- **Resource usage documentation** -- A new docs page reports the measured Agent and Server footprint, including cold-start vs 8h steady-state memory

### Changed

- **Stronger password policy** -- Login and registration now enforce a minimum password strength policy
- **SQLite connection hardening** -- Pragmas are applied to every pooled connection and statement logging stays disabled
- **Agent reaps terminal child processes** -- The Agent reaps the terminal child process on session close

### Fixed

- **Server deletion fully cleans up scoped data** -- Removing a server now cascades server-scoped data, `recovery_job` rows, and `device_tokens`; deletion is blocked while a recovery is running, and orphan-server cleanup routes through the shared deletion helpers
- **Push registration is scoped to the caller** -- `push_register` and `push_unregister` reject cross-user overwrite and delete attempts; mobile refresh cascades `device_tokens` when it rotates the session
- **Dashboard and chart rendering** -- Fixed multi-line chart rendering, performance and tooltips; dashboard jank during dialog and drag; server edit wiping the server list; grid layout sync while idle; widget alignment and overlap; and 24-hour time on chart axes and tooltips
- **Alert recovery notifications** -- A recovery notification is now dispatched when an alert resolves
- **Deployment and dependency advisories** -- The Caddy state dir is created when missing, mapped AAAA records are ignored, and `aws-lc-sys`/`rustls-webpki` are bumped to patch security advisories

## [0.9.3] - 2026-05-18

### Fixed

- **Installer prints the correct first-run admin password on reused hosts** -- The bootstrap installer scoped the first-run credential lookup to the current service instance and now reads the most recent banner instead of the oldest, so hosts with prior install history no longer surface a stale, invalid password

### Changed

- **Documentation reflects the `/opt/serverbee` install layout** -- Quick-start and deployment docs were corrected from the legacy `/etc/serverbee`, `/usr/local/bin`, `/var/lib/serverbee` paths to the actual bootstrap layout (`/opt/serverbee/{bin,etc,data}`), including the secure-cookie and backup/restore instructions

- **Agent upgrade source is now pinned on the Agent** -- The Agent downloads upgrade binaries from a locally-configured release source (`[upgrade] release_repo_url` in `agent.toml`, env `SERVERBEE_UPGRADE__RELEASE_REPO_URL`, or `--release-repo` CLI flag) instead of following a download URL supplied by the Server. An optional TLS SPKI pin (`release_cert_spki_sha256`) lets the Agent additionally validate the leaf certificate of the release host after standard chain validation.

### Breaking Changes

> **BREAKING: Agents older than this version cannot auto-upgrade after the Server is updated.**
>
> The updated Server sends only a target version number in the upgrade signal; it no longer includes a `download_url`. Pre-feature Agents expect a `download_url` and will fail silently when the field is absent, leaving them unable to self-upgrade.
>
> **Required one-time manual action:** After upgrading the Server, any Agent still running a version older than this release must be **manually reinstalled** (re-run the install script or redeploy the new binary). Once the new Agent binary is running, it will self-upgrade normally from its locally-pinned release source for all future upgrades.
>
> Agents already running this version or newer are unaffected.

## [0.9.2] - 2026-05-18

### Added

- **Installer domain HTTPS setup with action preview** -- The interactive installer can configure a domain with automatic HTTPS, previews the exact actions before executing them, and shows the resolved release version (instead of `<latest>`) in the plan
- **Localized installer with cached language selection** -- The installer's interactive UI, plan, result and status output are fully localized; the language is chosen once and cached for subsequent runs
- **Smarter agent install defaults** -- The agent install prompt defaults the server URL to the detected local IP, lists Agent before Server in the component menu, and the install result prints the one-time first-run admin password
- **Self-updating management CLI** -- The running install script is installed as the management CLI and is refreshed automatically during `upgrade`
- **Add Server enrollment dialog** -- Servers can be enrolled directly from the Servers page via a dialog that mints a one-time code and shows the install command and steps

### Changed

- **Server card redesign** -- The card footer is a two-column stats grid; processes/TCP/UDP are merged into one line; the monthly-equivalent total cost replaces the value grade; disk read/write use circular R/W badges; load moved into the footer; days-left and cost slots stay reserved so card height is stable
- **Consistent grid and tooltips** -- The Servers card view uses the same auto-fill grid as the dashboard, tooltips follow the active theme, and the network square-grid tooltip styling is corrected
- **Consolidated install layout** -- The installation layout is consolidated under `/opt/serverbee`, with docker-mode and compose config placed in snap-accessible directories the server actually loads
- **Safer uninstall** -- Uninstall prints explicit `rm` commands instead of an opaque `--purge` hint, and docker is recommended for server installs

### Fixed

- **Network history rendering** -- Persisted network-quality history is colored on first paint (previously rendered gray until live samples arrived), and each square's tooltip shows that bucket's own values instead of a constant current snapshot
- **DataTable layout** -- Column widths no longer explode to extreme values and wide tables scroll horizontally instead of clipping content
- **Server config loading** -- The server also loads config from `/opt/serverbee/etc/server.toml` and the service workdir points at the config directory so `server.toml` is found
- **Robust domain/DNS flow** -- The installer waits for and rechecks domain DNS before confirmation and planning, warns on mismatched IPv6 DNS, makes domain verification idempotent, hardens the noninteractive flow, rejects an unsupported admin-password option, defers interactive dependency checks, and widens the docker first-run password poll budget

### Documentation

- Documented how to correct a wrong agent enrollment code, and clarified domain and IP access setup

## [0.9.1] - 2026-05-17

### Security

- **One-time agent enrollment codes replace the shared discovery key** -- The permanent, plaintext, globally-shared `auto_discovery_key` is gone. Agents now register with a single-use, short-lived (default 10 min), argon2-hashed enrollment code minted by an admin. Codes are verified in constant time, atomically consumed on first successful registration (race-safe), and pruned after expiry/consumption. This closes the prior takeover vector where a leaked discovery key allowed silent registration/impersonation of any agent
- **Forced first-login admin password change** -- The auto-provisioned admin account is created with a randomly generated password (printed once to the server logs in a high-visibility banner) and is hard-blocked from every authenticated surface -- REST (`auth_middleware`), the browser/terminal/docker-logs WebSockets, and mobile login/refresh -- until a new password is set. This removes the practice of supplying admin credentials in plaintext environment variables

### Added

- **Enrollment & token management API** -- New admin endpoints: `POST/GET/DELETE /api/agent/enrollments` to mint, list, and delete one-time enrollment codes (the list never returns the plaintext code or hash), and `POST /api/agent/{id}/rotate-token` to rotate and revoke a server's run token, force-disconnecting the live agent so it must reconnect with the new token. Enrollment create/delete, agent enrollment, and token rotation are written to the audit log
- **Settings UI for enrollment** -- The Settings page can mint a one-time code and shows a copyable one-line install command (carrying `--enrollment-code` and the current origin); existing codes are listed with prefix, status (active/consumed/expired), and a confirm-guarded delete
- **First-login onboarding flow** -- New `POST /api/auth/onboarding` endpoint and `/onboarding` web page (CN/EN) that forces the initial password change and optionally lets the admin rename the account. `must_change_password` is surfaced on the login and `me` responses, and a dedicated `MUST_CHANGE_PASSWORD` error code drives the frontend redirect

### Changed

- **Agent registration uses a Bearer enrollment code** -- `POST /api/agent/register` now authenticates via `Authorization: Bearer <enrollment_code>`. The agent config field is `enrollment_code` (env `SERVERBEE_ENROLLMENT_CODE`); the install script flag is `--enrollment-code`. Registration failures now surface the server's error body and log the bound `server_id`. Already-registered agents are unaffected -- the per-server run-token path is unchanged
- **Admin bootstrap is always a random password** -- On first start the server unconditionally generates the admin password instead of reading it from configuration. Existing deployments are unaffected: the new `must_change_password` column defaults off via an additive migration, so already-provisioned users are never forced to change

### Removed

- **`auto_discovery_key` fully removed** -- Server config `auth.auto_discovery_key`, env `SERVERBEE_AUTH__AUTO_DISCOVERY_KEY`, the `GET/PUT /api/settings/auto-discovery-key` endpoints, the startup-printed key, and the agent `auto_discovery_key` field are all deleted. A migration removes the stored key row so it no longer lingers in database backups
- **Admin credential environment variables removed** -- `SERVERBEE_ADMIN__USERNAME` / `SERVERBEE_ADMIN__PASSWORD` and the `AdminConfig` block are deleted; docs, README, and `docker-compose.yml` now describe the first-run log banner and forced onboarding instead

## [0.9.0] - 2026-05-16

### Added

- **Marketing landing page** -- The docs site now ships a dedicated landing page built from reusable primitives (section, gradient heading, code-copy, hex background) with a bilingual i18n layer. It opens on a hero with an animated mini-dashboard, then flows through a trust strip, a three-pillar value section, an eight-tile bento grid with per-feature animations, a how-it-works walkthrough, and a final call-to-action. The page is dark-only by design, so the theme switch is hidden there
- **ServerCard 2-column redesign** -- The server card was rebuilt around a 2x2 ring layout with side-by-side network grids. A new `NetworkSquareGrid` fits points dynamically to the available space, `RingChart` gained a compact mode, `TagChips` renders server tags, and new latency/loss color helpers drive the square coloring. The dashboard server-cards widget is now responsive via an auto-fill grid
- **Mobile viewport adaptation** -- The web UI now adapts to mobile viewports, including horizontally scrollable tables that clamp page width on small screens

### Changed

- **Landing layout polish** -- Refined the landing page across viewports: wrapped install commands, richer bento tiles, balanced headings, full-width stream/light-band animations, orbit icons that ride the dashed ring while staying upright, and a file-tree that no longer overflows into the card heading
- **ServerCard trend window** -- Widened the ServerCard trend window from 12 to 30 points for a smoother signal, and switched the TooltipTrigger to the base-ui render prop

## [0.8.12] - 2026-05-05

### Added

- **VPS cost insights** -- A new cost analytics surface scores every server's price-to-resource value from validated billing inputs. The backend computes normalized cost metrics, scores VPS value with guarded helpers, and exposes a `/api/cost/*` insights API (overview + per-server insights) with full OpenAPI schema registration so clients get typed payloads end to end
- **Cost signal in the servers UI** -- The servers table now surfaces cost alongside resources, the dashboard server card shows a compact cost signal with a footnote, and a dedicated cost insights panel renders the per-server scoring detail so admins can spot poor-value VPS at a glance
- **Cost insight configuration validation** -- New server-side validation rejects non-finite prices, invalid billing cycles, and malformed cost resource inputs before they reach scoring, keeping the insights pipeline resilient to bad data

### Changed

- **Documentation refresh** -- Reorganized the docs site with a refreshed feature overview and navigation, plus new or expanded guides for alerts & APNs, scheduled tasks, dashboards, file manager, service monitors, status pages, branding/themes, capabilities enforcement, GeoIP/traceroute flows, and the API endpoint overview
- **Cost UI polish** -- Localized billing cycle labels, kept the card cost footnote compact, formatted generated cost API types, labeled invalid card cost configs, and preserved expiry context inside cost insights

### Testing

- Added backend integration coverage for the cost insights API error body contract, billing input validation, and cost aggregation guards
- Hardened frontend cost helper tests around scoring edges and insight derivation

## [0.8.11] - 2026-05-01

### Added

- **Custom theme system** -- Admins can now create, edit, and activate fully custom themes from the settings page. Each theme is a typed bundle of OKLCH-validated CSS variables, persisted as a `custom_theme` row, and addressable via a `theme://` URN scheme that decouples references from numeric IDs. Status pages can opt into a custom theme through `status_page.theme_ref`, gated by the new `feature.custom_themes` flag and exposed over a dedicated `/api/themes/*` HTTP surface
- **OKLCH-aware theme variable validator** -- A new server-side validator parses every theme variable, enforces OKLCH lightness/chroma/hue ranges, and rejects malformed payloads before they reach storage so invalid themes can never be activated or referenced from a status page
- **Frontend theme runtime rewrite** -- `ThemeProvider` now resolves a payload from the API, applies it directly to CSS variables, and caches the result in localStorage so theme switches feel instant and survive refreshes. OKLCH ⇄ hex conversion is provided through `culori`, and a shared preset variable map keeps every built-in preset in lockstep with the runtime via a CSS sync invariant test

### Changed

- **Status page theming** -- Public status pages now read their theme from `theme_ref` and fall back to the previous default behavior when the feature flag is off, so existing deployments see no visual change until they explicitly opt in

### Testing

- Added backend integration coverage for the `/api/themes/*` HTTP routing, custom theme service invariants, theme-ref URN parsing edges, theme variable validation edges, and reference integrity between status pages and custom themes
- Added frontend coverage for the theme-ref URN parser, the preset variable map ↔ CSS sync invariant, and updated capabilities-dialog mocks to match the new theme surface

## [0.8.10] - 2026-04-18

### Added

- **Server tags management** -- Admins can now assign validated tags to servers via the edit dialog, with a dedicated `/api/servers/{id}/tags` API on the backend. Tags are normalized, deduplicated, and pushed back into the live servers cache so the UI updates immediately after save
- **Tags in the servers list** -- The servers table now renders server tags inline with the name cell, and the backend includes tags in the shared server status payload so the list stays in sync without extra page reloads

### Changed

- **Servers list density redesign** -- The `/servers` table now uses dedicated CPU, memory, disk, network, uptime, and name cells with denser metric presentation. Disk I/O, live network speeds, traffic quota usage, status dots, and core/load context now live directly in the row instead of forcing users to bounce between pages
- **Compact latency hero on server cards** -- The dashboard server card now shows a tighter latency summary with a unified severity bar sparkline and inline packet-loss indicator, making network health readable at a glance without the old bulky header treatment
- **Table controls and dialogs polish** -- Shared data-table controls gained better i18n coverage, and the server edit, recovery merge, and capabilities dialogs were tightened up to fit the denser admin workflow more cleanly

### Fixed

- **Per-card latency severity rendering** -- Each server card now uses a unique SVG pattern ID for failed latency bars, preventing one card's striped failure state from leaking into another card's chart
- **Server edit dialog stability** -- Edit-dialog translation keys and date-picker layout were corrected so the form no longer shows stale copy or awkward layout shifts while editing billing fields

### Testing

- Expanded frontend coverage for the redesigned servers list, tag chips, status dots, scroll area behavior, and server edit dialog tag flow
- Added backend integration coverage for server tag CRUD, RBAC enforcement, and tag propagation through the shared server status payload

## [0.8.9] - 2026-04-16

### Added

- **Resend email notifications** -- Email alerts are now delivered through Resend's HTTP API. Configure once per deployment via `SERVERBEE_RESEND__API_KEY`; each channel defines `from` and a `to` array so a single channel can fan out to multiple recipients in one call. The rendered email uses an inline-styled HTML body with event-coded header colours (triggered / resolved / neutral) plus a plain-text fallback. Existing SMTP rows are migrated automatically on startup — convertable ones are rewritten to the new `{from, to:[...]}` shape, unconvertable ones are disabled and suffixed with `(needs reconfiguration)` for in-UI repair
- **Edit flow for notification channels and groups** -- The settings page now supports editing existing notification channels (all 5 types) and notification groups. Opening Edit prefills the form, locks the channel type to prevent accidental conversion, and exposes an `Enabled` switch so rows flagged by the migration can be re-enabled after reconfiguration
- **Email address format validation** -- Recipient inputs now reject values missing `@` or a domain dot on both the backend `parse_config` and the frontend tag-input, surfacing the error as a toast before the row is saved
- **Agent recovery merge workflow** -- Servers flagged as a recovered identity can now be merged into their original record from the UI. The merge is atomic, preserves server identity, folds traffic and disk-I/O history together, and gates concurrent writes with a recovery lock so partially-merged state is never observable. Traffic-cache updates continue during the freeze window

### Changed

- **Email channel schema** -- `ChannelConfig::Email` shrinks from the 6-field SMTP layout (`smtp_host`, `smtp_port`, `username`, `password`, `from`, `to`) to `{from, to: string[]}`. Storage is migrated automatically; the settings form collapses to a `from` field plus a tag-style recipients input
- **Auto-upgrade reclassified as default capability** -- New servers now receive the full default capability set including `CAP_UPGRADE`, via the shared `CAP_DEFAULT` constant — no more manual toggling during registration
- **Storage-sizing guide** -- Added a dedicated storage-sizing reference page (EN + CN) with a capacity planning calculator and retention guidance

### Removed

- **`lettre` SMTP dependency** -- The `lettre` crate is dropped from the server binary. Outbound email is now exclusively via Resend's REST API

### Fixed

- **Notification config re-validated on update** -- `PUT /api/notifications/{id}` now re-parses the effective `(notify_type, config_json)` pair, so partial updates that would produce an invalid shape (e.g. changing `notify_type` without supplying a matching `config_json`, or clearing `to` on an email row) are rejected with a 422

## [0.8.8] - 2026-04-15

### Added

- **Diceui data-table on /servers** -- Replaced the hand-rolled servers-list table with the `@diceui/data-table` registry. Adds URL-synced status and group filters, client-side pagination (pageSize 20), and stabilizes column layout (`table-fixed`, explicit widths, `tabular-nums`) so WebSocket metric updates no longer cause horizontal jitter. The upgrade badge is inlined next to the server name, dropping the blank upgrade column

### Changed

- **Auto-upgrade reclassified as low risk** -- `CAP_UPGRADE` is now treated as a low-risk capability and included in `CAP_DEFAULT`. The capabilities UI regroups and relabels the badge accordingly, with docs and QA notes updated to match
- **Agent version card in server header** -- The agent version card now lives in the server detail header beside metadata and actions, spanning its own full-width row. Header ordering is asserted in the server detail route test

### Fixed

- **Realtime chart stops updating** -- `useRealtimeMetrics` no longer mutates the sparkline buffer in place, so React detects buffer changes and realtime charts refresh continuously without requiring a tab switch to reveal accumulated points
- **Realtime chart axis labels** -- X-axis ticks now show `hh:mm` with consecutive duplicates hidden, the first data point is always labelled, and tooltips display full `hh:mm:ss`. `MetricsChart` exposes an `xAxisInterval` prop so the realtime path forces a tick per data point

## [0.8.6] - 2026-04-14

### Added

- **Traffic quota ring on server cards** -- Server cards now render a fourth ring chart showing monthly traffic-quota utilization, wired to `/api/traffic/overview`. Rings fall back to cumulative agent counters when no quota is configured, and a `days remaining` hint appears when a billing cycle is active
- **Disk I/O and load trend in server cards** -- Cards display current disk read/write throughput and a compact `load5 · load15` trend alongside network speed, replacing the old single "net total" cell
- **Aggregate disk I/O in ServerStatus** -- The server-to-browser `ServerStatus` WebSocket payload now includes `disk_read_bytes_per_sec` / `disk_write_bytes_per_sec`, summed across devices, so server cards can render realtime disk throughput without fetching historical data
- **Configurable anomaly threshold design** -- New spec `2026-04-13-configurable-anomaly-thresholds-design.md` defines how network-probe warning/critical thresholds for latency and packet loss will become user-configurable (spec only; implementation lands in a later release)

### Changed

- **Server card layout** -- Reworked into a 4-column ring grid (CPU, Memory, Disk, Traffic) with inline bytes/percent values, plus a condensed footer row summarising uptime, swap, processes, and TCP/UDP counts. Visual density increases without crowding the network sparklines below
- **WHOIS targets are normalized** -- Both the Rust service and the frontend form now normalize WHOIS inputs such as `https://example.com/path` or `example.com:8443` down to the bare hostname before looking up registry data, preventing spurious lookup errors caused by schemes, ports, or trailing dots
- **Localized preset network-probe metadata** -- Preset probe target names, providers, and locations are translated into Chinese when the UI language is `zh-*` (e.g. "China Telecom" → "电信", "Shanghai" → "上海"). English users continue to see the canonical names from the catalog
- **Service monitor form prefill** -- The Service Monitors create/edit dialog now resets from a `useEffect` whenever it opens, so editing an existing monitor reliably prefills name, type, target, interval, enabled flag, and parsed config instead of retaining stale values from the last open

### Fixed

- **Unsupported WHOIS TLD error** -- `.app`, `.dev`, and `.page` domains (Google Registry) now return a clear, actionable error ("Use an SSL monitor for `demo.example.app` instead.") from both the backend checker and the frontend form hint, instead of failing with an opaque lookup error
- **Service monitor detail JSON parsing** -- Monitor detail rendering now goes through a shared `parseMonitorDetail` helper that rejects non-object payloads and swallows malformed JSON, avoiding runtime errors when `detail_json` is `null`, an array, or invalid JSON
- **Capabilities settings navigation freeze** -- Stabilized the `_authed/settings/capabilities` route so navigating away no longer wedges the router in a loading state
- **Network probe i18n stability** -- Column headers for the network-probes settings table are now produced by lazy header functions, fixing stale-translation bugs after switching UI language. New language-switch tests guard the regression
- **Network probe target actions** -- Target-row actions in the settings table now render with clearer affordances and correct spacing on narrow widths
- **Capability headers and risk ordering** -- Restored the original capability column order on the settings page so high-risk toggles are grouped and labelled consistently with the backend catalog
- **Traffic overview empty state** -- The `/traffic` page now shows a clearer empty-state message when no servers have traffic quotas configured, instead of rendering an empty chart
- **Network detail and server detail spacing** -- Added bottom padding to `/servers/:id` and restored vertical spacing on `/network/:serverId` so the last card no longer sits flush against the viewport edge
- **CI typed route tests** -- Route components `ServiceMonitorDetailPage` and related detail routes are now exported so typed tests in CI can import them directly

### Testing

- 5 new frontend test files: `servers/$id.test.tsx`, `service-monitors/$id.test.tsx`, `settings/capabilities.test.tsx`, `settings/service-monitors.test.tsx`, `traffic/index.test.tsx` — covering ring layouts, WHOIS form validation, capability toggling, and traffic overview rendering
- Extended `server-card`, `network/$server-id`, and `settings/network-probes` test suites with coverage for disk I/O metrics, traffic ring fallbacks, preset-name localization, and language-switch rerenders
- New Rust coverage in `crates/server/src/service/checker/whois.rs` for target normalization (URL, host:port) and the unsupported-TLD error path

## [0.8.5] - 2026-04-12

### Added

- **Agent local capability locks** -- Agents can now refuse high-risk capabilities locally via CLI flags (`--deny-terminal`, `--deny-exec`, `--deny-upgrade`, `--deny-file`, `--deny-docker`) regardless of what the server grants. Locks are reported back to the server and surfaced in the capabilities UI so admins can see which features are locked on each host
- **High-risk audit trail** -- New `high_risk_audit` service logs terminal sessions, exec invocations, file transfers, and Docker log/exec access with actor, target server, and session source. Audit rows are retained alongside admin audit logs and are visible in the admin audit view
- **Effective capability UI** -- The per-server capabilities dialog and `/settings/capabilities` page now show the *effective* capability (server grant AND agent lock) with tooltips explaining which side is denying each bit
- **Memory and frontend optimization pass** -- ServerCard uses `React.memo` with a tailored comparator, `Bar` animations are disabled, the grid uses `content-visibility: auto`, `QueryClient` now carries an explicit `gcTime`, and `useRealtimeMetrics` mutates the sparkline buffer in place to avoid per-tick allocation
- **Selective sysinfo refresh on agent** -- Agent metric collection switches from `refresh_all` to targeted `refresh_*` calls, reducing CPU usage on the agent for high-interval collectors

### Changed

- **Server ownership of SystemReport** -- Server `AgentManager` now stores `Arc<SystemReport>` instead of cloning full reports for every broadcast, eliminating allocation churn on every agent tick
- **Servers-list mutation** -- Frontend replaces `invalidateQueries` on each ws message with `setQueryData`, preventing unnecessary refetches when only a few server fields changed
- **Scheduled task audit lifecycle** -- Manual scheduled-task execution and scheduler-driven execution now flow through the same audit context, so the audit log always records who triggered an exec
- **Network probes i18n coverage** -- Remaining untranslated strings in the network-probes settings surfaces are now localized; scroll areas use shadcn `ScrollArea` consistently
- **Capabilities dialog layout** -- Capabilities dialog groups high-risk toggles together and adds inline descriptions for each bit

### Fixed

- **Stale agent connection teardown** -- Agent WS teardown on reconnect now serializes frame handling and disconnect cleanup per connection, preventing the server from applying offline-state cleanup belonging to a previous connection after a successful reconnect
- **Docker state cleanup on disconnect** -- On agent disconnect the server now fully clears Docker viewer state and container cache, avoiding stale Docker pages after the agent comes back online
- **Workspace CI type errors** -- Resolved residual TypeScript errors in the web app that only surfaced in CI
- **Ultracite lints** -- Removed an unused `capabilities` import and other fixes required to pass the frontend Biome check

### Testing

- New `crates/server/tests/docker_integration.rs` exercising Docker flow cleanup (213 lines)
- Significantly expanded `crates/server/tests/integration.rs` (+636 lines) covering capability enforcement, high-risk audit writes, and scheduled-task exec audit paths
- Frontend tests added for `use-servers-ws` setQueryData fast path and `capabilities` effective-grant logic

## [0.8.4] - 2026-04-12

### Added

- **Server card network redesign** -- The ServerCard network section is restructured with a dedicated data layer (`server-card-network-data.ts`) and clearer click-through behavior that takes the user into the per-server network detail page
- **Disk I/O chart polish** -- `disk-io-chart.tsx` gains a consistent legend and axis formatting to match other historical charts
- **Widget picker improvements** -- The dashboard widget-picker dialog is enlarged, scrollable via shadcn `ScrollArea`, and uses a more compact `stat-number` widget preset

### Changed

- **Chart time axes** -- All historical charts now use a 24-hour time format and show explicit date labels for 7d/30d ranges, so day boundaries are obvious at long ranges
- **Dashboard layout clamp** -- Dashboard layout constraints are now clamped so widgets cannot be resized below a usable minimum
- **Server card network click** -- Clicking the network section of a card now navigates to `/network/:serverId` instead of the generic detail page

### Fixed

- **RFC3339 time format in raw SQL** -- Raw SQL queries issued against sea-orm tables now emit timestamps in RFC3339, matching the storage format and avoiding zero-row results on time-range filters
- **TypeScript errors in network and sparkline tests** -- Resolved type drift in network data types and the sparkline test after refactoring
- **Server card latency fallback** -- Restored the graceful latency fallback when no probe data is available for a server
- **Missing network sparklines guard** -- Server card no longer crashes when the sparkline payload is absent for a newly registered server
- **Widget picker lint** -- Replaced a non-null assertion with a type cast to satisfy Biome

### Testing

- New `server-card-network-data.test.ts` (212 lines) covering network-data derivation and fallback ordering
- Dashboard layout tests updated for the new clamp rules

## [0.8.3] - 2026-04-12

### Added

- **Seeded server card sparklines** -- Server cards now render latency sparklines immediately on first render by seeding from the overview history payload, instead of waiting for the next realtime tick. Includes new `sparkline.ts` utilities and a 164-line test suite
- **Overview sparkline history** -- Server backend `network_probe` service now returns seeded sparkline data as part of the overview response, so the frontend no longer has to request per-server probe history just to populate the first paint
- **Design spec and plan** -- `2026-04-12-server-card-latency-sparkline-seed-design.md` and the matching 6-task implementation plan document the seed flow and rollout

### Fixed

- **Graceful sparkline fallback** -- When the seeded sparkline query fails the server now degrades gracefully and the card still renders the latency number without the trend line, instead of surfacing a JSON error to the browser

### Testing

- 164-line `sparkline.test.ts` covering downsampling, buffer trimming, and empty-history handling
- Extended `server-card.test.tsx` for seeded-vs-realtime precedence

## [0.8.2] - 2026-04-10

### Added

- **Production Proxy Dev Workflow** -- New `make web-dev-prod` and `bun run dev:prod` workflows let the Vite dev server proxy API requests and live `/api/ws/servers` updates to the production backend for frontend debugging against real traffic
- **Prod Proxy Warning Banner** -- The web app now shows a persistent banner in `prod-proxy` mode so you do not forget the UI is pointed at production, and whether writes are still blocked

### Changed

- **Prod Proxy Safety Model** -- The production dev proxy now uses a dedicated read-only member API key, blocks non-read HTTP methods by default, strips browser auth headers and cookies, and only allows `GET /api/auth/me` from the auth routes
- **Configuration Docs** -- `.env.example`, `ENV.md`, and the bilingual configuration docs now document the split production keys and the `ALLOW_WRITES=1` escape hatch

### Fixed

- **Production WebSocket Allow-List** -- The frontend dev proxy now forwards only `/api/ws/servers`, preventing terminal and other control-plane WebSocket routes from piggybacking through localhost into production

### Testing

- Added frontend Vitest coverage for prod-proxy request filtering, auth-path blocking, WebSocket allow-listing, and the persistent warning banner

## [0.8.1] - 2026-04-09

### Added

- **Database Pull Script** -- New `make db-pull` command to download production SQLite database via the backup API for local development. Validates SQLite file header and stores to `data/prod.db`
- **Production Data Dev Server** -- New `make server-dev-prod` command to run the local server against a pulled production database
- **Environment Example** -- `.env.example` with `SERVERBEE_PROD_URL` and `SERVERBEE_PROD_API_KEY` placeholders for database pull configuration

### Changed

- **Setup script** -- Conductor setup now copies `.env` from `CONDUCTOR_ROOT_PATH` for consistent environment configuration

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
