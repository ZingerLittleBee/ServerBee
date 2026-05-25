# IP Quality Design

**Status:** Draft (revised after spec review)
**Date:** 2026-05-22
**Branch:** `charlottetown`

## Goal

Let each agent assess the quality of its VPS egress IP and report it to the server for display. Two things are checked:

1. **Service unlock detection** — the agent issues HTTP requests from its own egress IP to determine the unlock status of streaming / AI / social services (Netflix, ChatGPT, Disney+, etc.).
2. **IP metadata + fraud risk score** — the agent reports its egress IP; the **server** derives IP metadata from its local GeoIP database and, when configured, queries third-party APIs for a fraud risk score.

Results are shown on a dedicated `IP Quality` sidebar route (global overview), a per-server detail tab, and — when enabled per status page — the public status page. Reference inspiration: the `#ip-info` section of node status pages such as `status.eeee.ooo`.

## Non-goals (v1)

- Per-server selection of which services to check (the enabled service set applies uniformly to all capable servers).
- Periodic time-series snapshots of unlock status (history is a status-change event log only).
- IP quality history (only the latest snapshot per server is kept).
- Hourly aggregation of results (unlock status is not a continuous metric).
- Multi-step / scripted custom detectors (custom services are single-request only).
- A server-side scheduler task (the agent owns its own check schedule).

These constraints keep v1 focused. The data model does not preclude later extension.

## Decisions locked during brainstorming

| Topic | Decision |
|---|---|
| Scope | Full: service unlock + IP metadata + third-party fraud risk score. |
| Risk scoring location | Server-side (API keys centralized on the server). |
| Unlock detection location | Agent-side (the request must originate from the VPS egress IP). |
| Custom service model | Single request + ordered match rules (status code / body regex / redirect target → result). |
| Built-in detection logic | Hardcoded Rust detectors, dispatched by a `detector` key. |
| Check cadence | Scheduled (default every 12h, configurable) + manual trigger + automatic rerun on egress IP change. |
| History | Latest snapshot + a status-change event log ("small history"). |
| Display surfaces | Dedicated `IP Quality` sidebar route, per-server detail tab, public status page (per-page opt-in). |

### Open questions resolved during spec review

- **Capability rollout is dual opt-in.** Enabling `CAP_IP_QUALITY` server-side has no effect unless the agent is also started with `--allow-cap ip_quality`. This is the *existing* model for every non-default high-risk capability (`file`, `exec`, `terminal`, `docker`, `firewall_block`) — see [Section 2](#2-capability). It is intentional, not a bug; the spec must document the agent-side step and update install docs.
- **Public-page egress IP is masked for guests.** On the public status page the egress IP is rendered as `*.*.*.*` for unauthenticated viewers; the full IP is returned only when the request carries a valid session. Authenticated surfaces always show the full IP.
- **Built-in detectors are independently implemented.** Detectors are written from scratch in Rust based on each service's observable region-gating behavior. Open-source unlock checks may be consulted only for factual inputs (URLs, expected responses), never copied as code; any reference consulted is recorded with its license in the detector module, to stay clean under the project's AGPL-3.0 license.

---

## 1. Architecture

```
┌────────────────────────────────────────────────────────────────┐
│                            SERVER                               │
│                                                                  │
│  unlock_service ─┐                                               │
│  unlock_result   │   IpQualityService  ── IpQualitySync (WS) ──► │
│  unlock_event    ├─◄ (catalog CRUD,        IpQualityRunNow (WS)──►│
│  ip_quality_     │    results, settings)                         │
│    snapshot      │                                               │
│  ip_quality_     │   IpRiskService                               │
│    setting       │     ├─ local GeoIP MMDB  (baseline metadata)  │
│  ip_risk_cache ──┘     └─ third-party APIs  (risk score, opt-in) │
│                          ▲ ip_risk_cache: TTL keyed by IP        │
│   REST: /api/ip-quality/*                                         │
└────────────────────────────────────────────────────────────────┘
        ▲ UnlockResults (WS)            │ broadcast IpQualityUpdate
        │                               ▼  (partial, then full)
┌────────────────────────────────────────────────────────────────┐
│                             AGENT                               │
│                                                                  │
│   ws handler ──► UnlockChecker (CAP_IP_QUALITY gated)            │
│                       │                                          │
│      triggers: ┌──────┴───────┐                                  │
│      • interval │ detectors/   │  built-in: hardcoded Rust fns    │
│      • IP change│ rule_engine  │  custom:   single request +      │
│      • RunNow   └──────────────┘            ordered match rules   │
│                       │                                          │
│              dedicated reqwest client (no auto-redirect)         │
│              SSRF guard: scheme/port + every resolved addr +     │
│                          every redirect hop + body cap           │
└────────────────────────────────────────────────────────────────┘
```

- New capability `CAP_IP_QUALITY = 1 << 10 = 1024`. **Disabled by default** and **dual opt-in** (server toggle AND agent `--allow-cap`).
- The agent owns the check schedule. The server only pushes the service catalog (`IpQualitySync`) and forwards manual triggers (`IpQualityRunNow`).
- IP risk scoring runs entirely on the server in a bounded background task, never inline in the agent-connection message handler.

---

## 2. Capability

`CAP_IP_QUALITY = 1 << 10` (value `1024`), `default_enabled: false`, `risk_level: "medium"`.

Files to change (mirrors how `CAP_FIREWALL_BLOCK` was added):

- `crates/common/src/constants.rs` — new `CAP_IP_QUALITY` const; `CapabilityKey::IpQuality` variant plus its `as_str()`, `to_bit()`, and `FromStr` match arms; new `ALL_CAPABILITIES` entry; extend `CAP_VALID_MASK` to `0b111_1111_1111` (bits 0..=10, value `2047`). `CAP_DEFAULT` is **not** changed.
- Server and agent capability-handling sites that already branch on `CAP_FIREWALL_BLOCK` / `CAP_SECURITY_EVENTS`.

### Dual opt-in (do not skip)

Agent local capabilities are computed by `compute_agent_local_capabilities` (`crates/agent/src/capability_policy.rs`): they start from `CAP_DEFAULT` and are adjusted by `--allow-cap` / `--deny-cap`. The effective capability is `server_caps & agent_local_caps`. Because `CAP_IP_QUALITY` is not in `CAP_DEFAULT`:

- Enabling the capability in the server UI alone produces `effective = 0` → the server shows as **client-locked** for IP Quality, and the agent runs nothing.
- The operator must additionally start the agent with `--allow-cap ip_quality` (or the equivalent agent config / install-script flag).

This is the same two-sided gate already required by `file`, `exec`, `terminal`, `docker`, and `firewall_block`. The implementation must therefore also:

- Add `ip_quality` to the agent install documentation and the `serverbee` CLI / install-script capability flags, alongside the existing high-risk capabilities.
- Ensure the UI's existing "client-locked" capability state renders correctly for the new bit (no new UI code expected — it reuses the capability framework).

When the capability is not effective, the agent does not run checks and the UI shows the standard "capability disabled / client-locked" placeholder.

---

## 3. Data model

One new migration for the feature tables, plus one small migration adding a column to `status_page`. All migrations implement `up()` only (`down()` is a no-op per project convention). Six feature tables.

### `unlock_service` — service catalog (built-in seeds + custom rows in one table)

| Column | Type | Notes |
|---|---|---|
| `id` | PK | |
| `key` | TEXT UNIQUE | stable identifier (`netflix`, `chatgpt`, …); generated for custom services |
| `name` | TEXT | display name |
| `category` | TEXT | `streaming` / `ai` / `social` / `gaming` / `other` |
| `popularity` | INTEGER | sort weight within a category (higher = more popular) |
| `is_builtin` | BOOLEAN | |
| `enabled` | BOOLEAN | whether this service is checked |
| `detector` | TEXT NULL | built-in only: hardcoded detector key |
| `request` | JSON NULL | custom only: `{ url, method, headers, timeout_ms }` |
| `rules` | JSON NULL | custom only: ordered match rules |
| `created_at` / `updated_at` | TIMESTAMP | |

Built-in rows have `detector` set and `request`/`rules` null. Custom rows are the inverse.

**Seeding:** the initial migration embeds an **inline snapshot** of the built-in catalog and inserts it directly. The migration must be deterministic and immutable — it never reads a runtime constant. Later built-in additions are delivered as separate, idempotent re-seed migrations (`INSERT … ON CONFLICT(key) DO …`). There is no shared `common` constant feeding migrations.

### `unlock_result` — latest result per (server, service)

`id` PK, `server_id` FK, `service_id` FK, `status` TEXT, `region` TEXT NULL, `latency_ms` INTEGER NULL, `detail` TEXT NULL, `checked_at` TIMESTAMP. `UNIQUE(server_id, service_id)`. Upserted each run. `service_id` FK is `ON DELETE CASCADE` so deleting a custom service drops its results.

`status` enum: `unlocked` / `restricted` (e.g. originals-only) / `blocked` / `failed` (network error/timeout) / `unsupported`.

### `unlock_event` — status-change log ("small history")

`id` PK, `server_id` FK, `service_id` FK, `old_status` TEXT, `new_status` TEXT, `changed_at` TIMESTAMP. One row appended only when a service's status differs from its previous `unlock_result`. Cleaned by the `retention.ip_quality_event_days` config (default `90`), added alongside the existing `retention.*` keys.

### `ip_quality_snapshot` — latest IP quality per server

`id` PK, `server_id` FK `UNIQUE`, `ip` TEXT, `asn` TEXT NULL, `as_org` TEXT NULL, `country` / `region` / `city` TEXT NULL, `ip_type` TEXT (`residential` / `datacenter` / `hosting` / `mobile` / `isp` / `unknown`), `is_proxy` / `is_vpn` / `is_hosting` BOOLEAN, `risk_score` INTEGER NULL (0-100), `risk_level` TEXT (`low` / `medium` / `high` / `unknown`), `checked_at` TIMESTAMP. Upserted; replaced when the egress IP changes. This is the per-server view; the raw per-provider payloads live in `ip_risk_cache`.

### `ip_risk_cache` — third-party lookup cache, keyed by IP

`ip` TEXT PRIMARY KEY, `asn` / `as_org` / `country` / `region` / `city` TEXT NULL, `ip_type` TEXT, `is_proxy` / `is_vpn` / `is_hosting` BOOLEAN, `risk_score` INTEGER NULL, `risk_level` TEXT, `providers` JSON (raw per-provider results), `checked_at` TIMESTAMP. The risk service reads this first; on a miss or expired TTL it performs the lookups and upserts. `ip_quality_snapshot` copies the resolved fields from here. Stale rows are pruned by the same cleanup step (TTL-based, e.g. 30 days).

### `ip_quality_setting` — global settings (single row)

`check_interval_hours` INTEGER (default `12`). (Public-page exposure is *not* a global flag — see `status_page` below.)

### `status_page` column add

A boolean `show_ip_quality` column (default `false`) is added to the existing `status_page` table via its own migration. Public exposure of IP quality is decided per status page.

`unlock_result` and `ip_quality_snapshot` are latest-only and not purged. `unlock_event` and `ip_risk_cache` are pruned by `task/cleanup.rs`.

---

## 4. Protocol

New variants in `crates/common/src/protocol.rs`, styled after the network-probe messages.

**Server → Agent**

- `ServerMessage::IpQualitySync { services: Vec<UnlockServiceDef>, interval_hours: u32 }` — sent on connect, on catalog/settings change, and on capability change.
  - `UnlockServiceDef = { id, key, detector: Option<String>, request: Option<UnlockRequest>, rules: Option<Vec<UnlockRule>> }`
  - `UnlockRequest = { url, method, headers: Vec<(String, String)>, timeout_ms: u32 }`
  - `UnlockRule = { match: UnlockMatch, result: UnlockStatus }` where `UnlockMatch` is one of: status-code equals/range, body regex, redirect-target pattern. Rules are evaluated in order; first match wins.
- `ServerMessage::IpQualityRunNow` — forwarded from the UI "Check now" button.

**Agent → Server**

- `AgentMessage::UnlockResults { egress_ip: String, results: Vec<UnlockResultData>, checked_at: DateTime<Utc> }`
  - `UnlockResultData = { service_id, status: UnlockStatus, region: Option<String>, latency_ms: Option<u32>, detail: Option<String> }`

**Server → Browser**

- `BrowserMessage::IpQualityUpdate { server_id, unlock_results: Vec<UnlockResultData>, ip_quality: Option<IpQualitySnapshotData> }` — broadcast **twice**: first immediately after unlock results are stored (`ip_quality: None`), then again once risk scoring finishes (`ip_quality: Some(..)`). See [Section 6](#websocket-handling).

`UnlockStatus` is a shared enum in `common`: `Unlocked` / `Restricted` / `Blocked` / `Failed` / `Unsupported`.

---

## 5. Agent design

New module `crates/agent/src/ip_quality/`, structured like `NetworkProber`.

- `mod.rs` — `UnlockChecker`: holds the service list + interval, runs a scheduler loop, gated by the effective `CAP_IP_QUALITY`. `sync()` ingests `IpQualitySync`; `resync_capabilities()` reacts to capability changes.
- **Triggers** (all owned by the agent):
  - interval: every `interval_hours`;
  - egress IP change: rerun immediately when the agent's egress IP changes (reuses the existing egress-IP discovery mechanism);
  - manual: `IpQualityRunNow`.
- `detectors/` — one async Rust fn per built-in `detector` key, dispatched by a `match` on the `detector` string. Each detector is independently implemented from the service's observable region-gating behavior (see the licensing note in *Open questions resolved*). Complex judgements (e.g. Netflix originals-only) live here.
- `rule_engine.rs` — generic engine for custom services: issue the configured request, evaluate `rules` in order against the response, return on first match.
- **Execution control:** a concurrency cap (new `MAX_UNLOCK_CONCURRENT` constant, e.g. `5`), per-service timeout. A single failing service is recorded as `Failed` and does not abort the run.

### HTTP client

`reqwest` (with `rustls-tls`) is **already** an agent dependency, as are `url` and `ipnet`. The unlock checker uses a **dedicated `reqwest::Client`** — separate from the agent's self-upgrade downloader, which has its own pinning/redirect behavior and must not be reused. The dedicated client:

- sets a realistic browser-like `User-Agent` (per-detector overridable);
- has **automatic redirects disabled** (`redirect::Policy::none()`); the checker follows redirects manually so each hop can be validated;
- has connect + total timeouts.

### SSRF guard (hardened)

Custom-service URLs are user-supplied and the request is issued by the agent. Before issuing a request, and again **before following each redirect hop**, the agent enforces:

- **Scheme allowlist:** `http` / `https` only.
- **Port allowlist:** `80` / `443` only.
- **Address validation:** resolve the host and reject if **any** resolved A/AAAA address is private, loopback, link-local, unique-local, or otherwise non-global (e.g. `169.254.169.254`, `127.0.0.0/8`, `10/8`, `fc00::/7`, `::1`). All resolved addresses must pass, not just the first.
- **Body cap:** the response body read is capped (e.g. 256 KiB); only the capped prefix is matched against rules.

The server applies the scheme/port checks and a syntactic URL check when a custom service is created; the agent's resolved-address checks are the authoritative defense.

---

## 6. Server design

### `service/ip_quality.rs` (mirrors `network_probe.rs`)

- Catalog CRUD: custom services can be created / updated / deleted; built-in services can only be enabled / disabled (detection logic is immutable, no deletion).
- Global settings read/write (`check_interval_hours`).
- `save_unlock_results(server_id, egress_ip, results)` — upsert `unlock_result`; append an `unlock_event` row when a service's status changed from its previous value.
- `get_server_summary` / `get_overview` / public-page query functions.

### `service/ip_risk.rs` — server-side IP scoring

- **Baseline metadata is local, no external call.** Country, region, city, ASN, and AS org are resolved from the server's **existing GeoIP MMDB** (the database already used by the GeoIP feature). This always works and never depends on a third party.
- **Risk score is opt-in.** A `IpRiskProvider` trait with implementations enabled only when the admin supplies an API key: Scamalytics, IPQualityScore, proxycheck.io, AbuseIPDB. These yield the 0-100 `risk_score`, `risk_level`, and proxy/VPN/hosting flags.
- **`ip-api.com` is an explicit opt-in provider, not a baseline.** Its free JSON endpoint is HTTP-only and its terms forbid commercial use; it is offered as one selectable provider with a terms note in the config docs, never enabled implicitly.
- If no risk provider is configured, `risk_score` / `risk_level` are `null`/`unknown` and the UI shows only the GeoIP-derived metadata (and proxy/hosting flags if a provider supplied them).
- **Cache:** all third-party results are written to `ip_risk_cache` keyed by IP. A lookup for an IP with a fresh `checked_at` (TTL, e.g. 24h) reuses the cache and performs no external call.

### Configuration

A `[ip_quality]` TOML section on the server (follows the `[oauth]` pattern):

```toml
[ip_quality]
risk_provider = "none"        # none / scamalytics / ipqs / proxycheck / abuseipdb / ip-api

[ip_quality.scamalytics]
api_key = "..."
```

Per project convention, new env vars are documented in `ENV.md` and `apps/docs/content/docs/{en,cn}/configuration.mdx` in the same change, including the `ip-api` non-commercial / HTTP-only caveat.

### REST router `router/api/ip_quality.rs`

All endpoints carry `#[utoipa::path]`; all DTOs derive `ToSchema`.

| Method + path | Purpose | Auth |
|---|---|---|
| `GET /api/ip-quality/services` | catalog list | logged-in |
| `POST /api/ip-quality/services` | create custom service | admin |
| `PUT /api/ip-quality/services/:id` | update (built-in: `enabled` only) | admin |
| `DELETE /api/ip-quality/services/:id` | delete custom service | admin |
| `GET` / `PUT /api/ip-quality/settings` | global settings | read = logged-in / write = admin |
| `GET /api/ip-quality/overview` | all-servers unlock matrix | logged-in |
| `GET /api/ip-quality/servers/:id` | one server's results + IP quality | logged-in |
| `POST /api/ip-quality/servers/:id/check` | manual trigger (sends `IpQualityRunNow`) | admin |
| `GET /api/ip-quality/events` | status-change history | logged-in |

**Public status page:** the per-page endpoint `GET /api/status/{slug}` includes IP quality **only when that page's `show_ip_quality` is on**, and **only for the servers in that page's `server_ids_json`**. The legacy `GET /api/status` endpoint never includes IP quality. For unauthenticated requests the egress IP field is masked to `*.*.*.*`; if the request carries a valid session the full IP is returned.

### WebSocket handling

`router/ws/agent.rs` handles `AgentMessage::UnlockResults` **without blocking the agent connection's message loop**:

1. Store `unlock_result` + `unlock_event` (fast, local DB).
2. Broadcast `BrowserMessage::IpQualityUpdate { ip_quality: None }` immediately so the UI shows fresh unlock results.
3. Spawn a **bounded background task** (wrapped in an overall timeout) that runs `ip_risk` scoring for the reported `egress_ip` (cache-first), upserts `ip_quality_snapshot`, and broadcasts a second `IpQualityUpdate { ip_quality: Some(..) }`.

A slow, rate-limited, or down third-party API therefore never delays `Report` / `PingResult` / other messages from that agent.

### Background tasks

No new scheduler task — the agent owns its schedule. `task/cleanup.rs` gains: purge `unlock_event` rows older than `retention.ip_quality_event_days` (default `90`), and purge `ip_risk_cache` rows whose `checked_at` is older than a retention window (e.g. 30 days — distinct from the 24h freshness TTL that decides cache reuse). `unlock_result` and `ip_quality_snapshot` are latest-only and not purged.

---

## 7. Frontend design

Four display surfaces.

1. **Sidebar route `/ip-quality`** (`apps/web/src/routes/_authed/ip-quality.tsx`) — global overview: an all-servers × services unlock matrix (rows = servers, columns = services grouped by `category`, sorted within a group by `popularity`); one IP-quality card per server (egress IP, ASN, IP type, risk score + risk-level badge, proxy/VPN markers). A new "IP Quality" entry is added to the sidebar nav config.
2. **Settings page `settings/ip-quality.tsx`** (mirrors `settings/network-probes.tsx`) — two tabs: *Service catalog* (built-in list, locked, enable/disable only; custom-service table + create dialog with a URL / method / headers / ordered-match-rule editor) and *Settings* (`check_interval_hours`).
3. **Server detail tab** — `servers/$id.tsx` gains an `<IpQualityTab>`: the server's IP-quality card + its unlock matrix + status-change history + a "Check now" button.
4. **Public status page** — a new unlock-matrix section component, rendered only when the page's `show_ip_quality` is on. The egress IP is shown as `*.*.*.*` to unauthenticated viewers.

The `show_ip_quality` toggle is added to the status-page editor (`settings/status-pages.tsx`).

Supporting code: new hook `use-ip-quality-api.ts` (mirrors `use-network-api.ts`); new types `lib/ip-quality-types.ts`; `use-servers-ws.ts` handles `IpQualityUpdate` (both the partial and full broadcasts) for realtime updates. A shared status-badge component: unlocked = green, restricted = amber, blocked = red, failed = grey, unsupported = muted.

---

## 8. Error handling & security

- **Agent:** per-service timeout; network errors → `Failed` + `detail`; one failing service does not abort the run; concurrency capped; response body capped.
- **SSRF:** see [Section 5](#ssrf-guard-hardened) — scheme/port allowlist, all-resolved-address validation, per-redirect-hop revalidation, body cap. Server-side syntactic check on custom-service creation.
- **Server:** third-party API failure or timeout → the bounded background task stores GeoIP-only metadata (`risk_score` null); unlock results already persisted and broadcast independently in step 2.
- **Capability not effective:** the agent does not run; the UI shows the standard "capability disabled / client-locked" placeholder.
- **Public exposure:** off by default per status page; egress IP masked for unauthenticated viewers.

---

## 9. Testing

- **Rust unit:** rule engine (match rules → result); built-in detector response parsing using recorded fixtures (no live network); capability bit + `CapabilityKey` round-trip; SSRF address validation (private/loopback/link-local/ULA across IPv4 and IPv6, plus redirect-hop cases); service CRUD; status-change event logging; `ip_risk_cache` TTL hit/miss.
- **Server integration:** each REST endpoint; `save_unlock_results` persistence + event log; the two-phase `IpQualityUpdate` broadcast; public-page masking and per-page `show_ip_quality` filtering.
- **Frontend vitest:** the hook (partial then full update); matrix rendering; status badges; the custom-rule editor.
- Built-in detectors that hit live external services are not unit-tested deterministically — only their parsing logic is, via fixtures.

---

## 10. Files touched (summary)

- `crates/common/` — `constants.rs` (capability), `protocol.rs` (messages + DTOs).
- `crates/server/` — new `entity` modules (6 feature tables) + `status_page` column; two migrations (feature tables; `status_page.show_ip_quality`); `service/ip_quality.rs`, `service/ip_risk.rs` (reuses the GeoIP MMDB); `router/api/ip_quality.rs`; `router/api/status*.rs` + `service/status_page.rs` (per-page IP quality, masking); `router/ws/agent.rs` (non-blocking handler + background scoring task); `task/cleanup.rs`; `config.rs` (`[ip_quality]`, `retention.ip_quality_event_days`).
- `crates/agent/` — new `ip_quality/` module (`mod.rs`, `detectors/`, `rule_engine.rs`); capability handling; `capability_policy.rs` / install-script / CLI gain the `ip_quality` flag.
- `apps/web/` — `routes/_authed/ip-quality.tsx`, `routes/_authed/settings/ip-quality.tsx`, `<IpQualityTab>` in `servers/$id.tsx`, public-status-page section, `settings/status-pages.tsx` (`show_ip_quality` toggle), `hooks/use-ip-quality-api.ts`, `lib/ip-quality-types.ts`, `use-servers-ws.ts`, sidebar nav config.
- Docs — `ENV.md`, `apps/docs/content/docs/{en,cn}/configuration.mdx`, agent install docs (`--allow-cap ip_quality`), a new feature doc page; `README.md` / `README.zh-CN.md` feature list.

---

## 2026-05-25 Update — Provider Refactor

The "Risk Scoring" provider model originally described here (Scamalytics / IPQS / ProxyCheck / AbuseIPDB / ip-api) has been superseded. See [`2026-05-25-ip-quality-ipapi-is-design.md`](./2026-05-25-ip-quality-ipapi-is-design.md) for the new ipapi.is-primary, ip-api-fallback design.
