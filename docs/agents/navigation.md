# Source navigation

Choose the row matching the task, trace its source owners, and run the relevant checks. Paths below are relative to this file. Commands run from the repository root unless a working directory is stated.

## Authority

Current implementation and accepted [ADRs](../adr/) define current behavior. [CONTEXT.md](../../CONTEXT.md) names domain concepts. User documentation explains supported workflows. Report conflicts and verify the implementation before editing.

The [initial architecture design](../superpowers/specs/2026-03-12-serverbee-architecture-design.md) and [implementation progress log](../superpowers/plans/PROGRESS.md) are **historical snapshots**. Read them only for a specific change's background; their old discovery-key, fingerprint, capability-toggle, and routing descriptions do not describe current contracts. Search the relevant issue or changed area instead of reading every historical plan.

## Task map

| Task | Authoritative reference | Code starting points | Relevant checks |
| --- | --- | --- | --- |
| HTTP, browser WebSocket, or mobile authentication | Current shared credential policy; [API reference](../../apps/docs/content/docs/en/api-reference.mdx) | [auth policy](../../crates/server/src/middleware/auth.rs): `resolve_connection`, `resolve_ws_connection`, `AuthenticatedConnection::lease_is_valid`; [WS session](../../crates/server/src/router/ws/session.rs); [browser WS](../../crates/server/src/router/ws/browser.rs); [mobile auth](../../crates/server/src/service/mobile_auth.rs) | Auth unit tests; [browser_ws](../../crates/server/tests/browser_ws.rs), [router_auth_user](../../crates/server/tests/router_auth_user.rs), [ws_terminal_relay](../../crates/server/tests/ws_terminal_relay.rs) |
| Realtime catalog, missing fields, or stale list/detail views | [ADR-0002](../adr/0002-update-frames-carry-live-metrics.md) | [wire types](../../crates/common/src/types.rs): `LiveMetrics`, `ServerStatus`; [browser full sync](../../crates/server/src/router/ws/browser.rs); [server catalog](../../apps/web/src/lib/server-catalog.ts): `projectServerCatalog`; [WS hook](../../apps/web/src/hooks/use-servers-ws.ts); [iOS WS router](../../apps/ios/ServerBee/Services/WebSocketRouter.swift) | [catalog tests](../../apps/web/src/lib/server-catalog.test.ts), [WS-hook tests](../../apps/web/src/hooks/use-servers-ws-hook.test.tsx), [browser_ws](../../crates/server/tests/browser_ws.rs) |
| Onboarding, enrollment, re-enrollment, or connection fencing | [CONTEXT.md](../../CONTEXT.md), [ADR-0004](../adr/0004-agent-authority-owns-enrollment-lifecycle.md) | [ServerOnboarding](../../crates/server/src/service/server_onboarding.rs); [AgentAuthority](../../crates/server/src/service/agent_authority/mod.rs): `claim`, `preflight_connection`, `PendingAdmission::admit`; [Server API adapters](../../crates/server/src/router/api/server.rs); [Agent registration](../../crates/agent/src/register.rs): `stage_run_token`; [Agent WS](../../crates/server/src/router/ws/agent/mod.rs) | [agent_registration_integration](../../crates/server/tests/agent_registration_integration.rs), [enrollment smoke](../../tests/agent-enrollment-smoke.md), [real-host re-enrollment](../../tests/manual/agent-reenrollment-e2e.md) |
| Capability gates or metadata | [Capabilities guide](../../apps/docs/content/docs/en/capabilities.mdx) and current Agent config | [constants](../../crates/common/src/constants.rs); [Agent config](../../crates/agent/src/config.rs); [capability grants](../../crates/agent/src/capability_grants/authority.rs); [capability generator](../../crates/common/examples/dump_capabilities_ts.rs) | Relevant common/Agent tests; `bun --filter @serverbee/web generate:capabilities` and inspect the diff |
| Metrics, historical charts, rollups, or alert columns | [ADR-0003](../adr/0003-metric-columns-owned-by-rollup-descriptor.md) | [rollup](../../crates/server/src/service/rollup.rs): `METRIC_COLUMNS`; [entities](../../crates/server/src/entity/); [migrations](../../crates/server/src/migration/); [chart model](../../apps/web/src/lib/metric-chart-model.ts): `METRIC_CHART_SPECS` | Rollup unit tests, relevant [server integration suites](../../crates/server/tests/), chart tests via `make web-test` |
| Network detail routes or shared time ranges | [ADR-0001](../adr/0001-network-detail-as-server-tab.md) | [navigation policy](../../apps/web/src/lib/server-detail-nav.ts); [legacy redirect](../../apps/web/src/routes/_authed/network/$serverId.tsx); [Network tab](../../apps/web/src/components/network/network-tab.tsx); [public fallback](../../apps/web/src/routes/status.network.$serverId.tsx) | [legacy redirect tests](../../apps/web/src/routes/_authed/network/$server-id.test.tsx), [public fallback tests](../../apps/web/src/routes/public-network-detail.test.tsx), [current route checklist](../../tests/network-quality.md) |
| Missing CI coverage or choosing a test command | Current workflows; [Testing & Quality](../../apps/docs/content/docs/en/testing.mdx) | [main CI](../../.github/workflows/ci.yml); [docs CI](../../.github/workflows/docs.yml); [iOS CI](../../.github/workflows/ci-ios.yml); [integration-target guard](../../scripts/check-integration-targets.ts); [Make registry](../../scripts/make-menu.ts) | `make check-integration-targets` (or `bun run check:integration-targets`), `bun run test:tooling`; then the relevant Rust/web gates |
| Configuration, docs contracts, or bilingual content | [Docs contributor guide](../../apps/docs/README.md), [ENV.md](../../ENV.md) | [Server config](../../crates/server/src/config.rs); [Agent config](../../crates/agent/src/config.rs); [docs contracts](../../apps/docs/scripts/check-contracts.ts); [route smoke tests](../../apps/docs/scripts/check-routes.ts); [docs path filters](../../.github/workflows/docs.yml) | `bun --filter @serverbee/docs check:contracts`, `bun --filter @serverbee/docs types:check`; route/browser checks need a running production preview as described in the contributor guide |
| Release notes, packaging, upgrade validation, or memory growth | [Release docs workflow](../../.claude/commands/release-docs.md), [prerelease runbook](beta-release-validation.md), [memory soak](../../tests/manual/server-memory-soak.md) | [publish](../../scripts/publish.sh); [release toolchain](../../.github/workflows/release.yml); [installer](../../deploy/install.sh); [soak harness](../../scripts/memory-soak.sh); [embedded SPA](../../crates/server/src/router/static_files.rs) | Run the release runbook's applicable local/VPS/distribution checks; soak the actual Linux release-toolchain binary |
| Backup, restore, or installation transfer | [Backup asset inventory](../../apps/docs/content/docs/en/deployment.mdx#persistent-data) and current handlers | [setting.rs](../../crates/server/src/router/api/setting.rs): `create_backup`, `restore_backup`, `resolve_db_path`; [brand files](../../crates/server/src/router/api/brand.rs): `brand_dir`; [widget storage](../../crates/server/src/service/widget_module/service.rs); [database pull](../../scripts/db-pull.sh) | [router_misc_endpoints](../../crates/server/tests/router_misc_endpoints.rs), [router_content_admin](../../crates/server/tests/router_content_admin.rs); verify separate installation files when transferring |
| Docs site UI, search, or branding | [Docs contributor guide](../../apps/docs/README.md) and current site sources | [landing copy](../../apps/docs/src/components/landing/translations.ts); [landing header](../../apps/docs/src/components/landing/chrome/header.tsx); [favicon head](../../apps/docs/src/routes/__root.tsx); [public assets](../../apps/docs/public/); [SPA appearance](../../apps/web/src/routes/_authed/settings/appearance.tsx) | Docs contract/type/build/route checks; [appearance tests](../../apps/web/src/routes/_authed/settings/appearance.test.tsx) for SPA changes |

## Hidden dependencies

**Credential lifetime:** HTTP, optional-auth public routes, and browser/control WebSockets share `resolve_connection`. WebSockets also revalidate long-lived credentials through the session helper. Trace admission and later revocation/expiry together. Agent WS admission belongs to Agent Authority independently of user sessions.

**Catalog ownership:** `LiveMetrics` carries partial updates. Full sync and REST supply static facts; the web catalog projection owns their merge into list and detail caches. Follow the catalog before patching an individual component. iOS has its own wire models and WS router.

**Enrollment ownership:** onboarding creates a Server identity and its offer; Agent Authority owns offer transitions, run-token claims, and connection fencing. The Agent stages its proposed token durably before claiming. Consult the accepted ADR when changing these seams.

**Docs gates:** contracts read sources outside `apps/docs`, including configuration references, protocol constants, installer behavior, enrollment commands, release metadata, and deployment defaults. A new source dependency needs a matching entry in both docs workflow path filters. The current script and workflow are the dependency list; avoid copying their changing assertions into prose.

**Production artifacts:** the Server embeds the web build and built-in widgets. [build.rs](../../crates/server/build.rs) only creates a placeholder for a cold Rust build; build the web bundle before checking embedded assets. Local Cargo builds and the root [Dockerfile](../../Dockerfile) use different build paths from released `cargo zigbuild` Linux binaries. Allocator/RSS diagnosis uses the actual release toolchain and soak runbook; record the candidate commit and artifact digest.

**Backup boundary:** the API exports SQLite, including uploaded custom widget package blobs. Uploaded logos/favicons are files in the configured data directory's `brand` subdirectory. Downloaded GeoIP/ASN MMDB files, configured external `geoip.mmdb_path`/`asn.mmdb_path` files, configuration, remote Agent state, and executable-embedded SPA/built-in widgets are separate. A database download alone does not reproduce an installation. Restore requires a restart, as described by the handler.

**Brand sources:** the docs landing reads its own [logo asset](../../apps/docs/public/logo-icon.svg); the docs root declares its favicon. SPA uploaded branding uses the Server's brand routes and storage. Inspect these owners separately when changing a logo, favicon, or title.

## Checks and working directories

| Change | Entry point |
| --- | --- |
| One server integration suite | `cargo test -p serverbee-server --test <suite>`; shared harness: [tests/common](../../crates/server/tests/common/) |
| Rust behavior | `make cargo-test`; use the exact Clippy flags in [CI](../../.github/workflows/ci.yml) |
| Web behavior, types, or lint | `make web-test`, `make web-typecheck`, `make check`; [web scripts](../../apps/web/package.json) |
| Docs source/content | Commands and dev-server setup: [apps/docs/README.md](../../apps/docs/README.md) |
| API schemas | `bun --filter @serverbee/web generate:api-types`; [dump_openapi](../../crates/server/examples/dump_openapi.rs). Inspect generated diffs and run relevant client checks |
| Agent navigation and tooling | `bun run check:agent-navigation`, `bun run check:integration-targets`, `bun run test:tooling`; [navigation checker](../../scripts/check-agent-navigation.ts), [integration-target guard](../../scripts/check-integration-targets.ts) |

The Makefile dispatches through `scripts/make-menu.ts`. Rust owns Server data through SQLite/SeaORM.

## Frontend debugging with production data

- `make db-pull` then `make server-dev-prod` uses a frozen SQLite snapshot and a local Rust Server. It needs `SERVERBEE_PROD_URL` and the admin-scoped `SERVERBEE_PROD_API_KEY`; there are no live production Agent/browser WS updates.
- `make web-dev-prod` uses Vite's `prod-proxy` mode for live production HTTP and server-update WS data. It needs `SERVERBEE_PROD_URL` and the member-scoped `SERVERBEE_PROD_READONLY_API_KEY`.

Read the current [Vite proxy](../../apps/web/vite.config.ts) and [.env.example](../../.env.example) first. The proxy blocks non-read HTTP methods by default, strips browser credentials/session cookies, limits auth to `GET /api/auth/me`, and allows only the server-update WS. `ALLOW_WRITES=1` changes the proxy's HTTP method block while the key still determines authorization; the UI displays the matching persistent production warning.

Report the verification layer precisely: source inspection, automated tests, local UI, actual release artifact, live service, and published distribution establish different facts.
