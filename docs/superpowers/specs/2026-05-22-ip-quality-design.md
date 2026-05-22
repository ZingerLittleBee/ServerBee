# IP Quality Design

**Status:** Draft
**Date:** 2026-05-22
**Branch:** `charlottetown`

## Goal

Let each agent assess the quality of its VPS egress IP and report it to the server for display. Two things are checked:

1. **Service unlock detection** — the agent issues HTTP requests from its own egress IP to determine the unlock status of streaming / AI / social services (Netflix, ChatGPT, Disney+, etc.).
2. **IP metadata + fraud risk score** — the agent reports its egress IP; the **server** queries third-party APIs to obtain IP type, ASN, and a fraud risk score.

Results are shown on a dedicated `IP Quality` sidebar route (global overview), a per-server detail tab, and — when enabled — the public status page. Reference inspiration: the `#ip-info` section of node status pages such as `status.eeee.ooo`.

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
| Built-in detection logic | Hardcoded Rust detectors ported from open-source scripts, dispatched by a `detector` key. |
| Check cadence | Scheduled (default every 12h, configurable) + manual trigger + automatic rerun on egress IP change. |
| History | Latest snapshot + a status-change event log ("small history"). |
| Display surfaces | Dedicated `IP Quality` sidebar route, per-server detail tab, public status page. |
| Catalog source of truth | Built-in service catalog defined as a `const` in the `common` crate; migration seeds the DB from it. |

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
│  ip_quality_     │   IpRiskService ── third-party APIs ──►       │
│    setting     ──┘   (ip-api / scamalytics / ipqs / proxycheck)  │
│                          ▲ cache by IP (TTL)                     │
│   REST: /api/ip-quality/*                                         │
└────────────────────────────────────────────────────────────────┘
        ▲ UnlockResults (WS)            │ broadcast IpQualityUpdate
        │                               ▼
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
│                  HTTP client (TLS + redirects + UA)              │
│                  SSRF guard on custom URLs                        │
└────────────────────────────────────────────────────────────────┘
```

- New capability `CAP_IP_QUALITY = 1 << 10 = 1024`. **Disabled by default** (opt-in: it makes the VPS actively reach out to third-party services).
- The agent owns the check schedule. The server only pushes the service catalog (`IpQualitySync`) and forwards manual triggers (`IpQualityRunNow`).
- IP risk scoring runs entirely on the server using the egress IP the agent reports with its results.

---

## 2. Capability

`CAP_IP_QUALITY = 1 << 10` (value `1024`), `default_enabled: false`, `risk_level: "low"`.

Files to change (mirrors how `CAP_FIREWALL_BLOCK` was added):

- `crates/common/src/constants.rs` — new `CAP_IP_QUALITY` const; `CapabilityKey::IpQuality` variant plus its `as_str()`, `to_bit()`, and `FromStr` match arms; new `ALL_CAPABILITIES` entry; extend `CAP_VALID_MASK` to `0b111_1111_1111` (bits 0..=10, value `2047`). `CAP_DEFAULT` is **not** changed.
- Server and agent capability-handling sites that already branch on `CAP_FIREWALL_BLOCK` / `CAP_SECURITY_EVENTS`.

When the capability is disabled, the agent does not run checks and the UI shows a "capability disabled" placeholder.

---

## 3. Data model

One new migration (`up()` only; `down()` is a no-op per project convention). Five tables.

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

### `unlock_result` — latest result per (server, service)

`id` PK, `server_id` FK, `service_id` FK, `status` TEXT, `region` TEXT NULL, `latency_ms` INTEGER NULL, `detail` TEXT NULL, `checked_at` TIMESTAMP. `UNIQUE(server_id, service_id)`. Upserted each run.

`status` enum: `unlocked` / `restricted` (e.g. originals-only) / `blocked` / `failed` (network error/timeout) / `unsupported`.

### `unlock_event` — status-change log ("small history")

`id` PK, `server_id` FK, `service_id` FK, `old_status` TEXT, `new_status` TEXT, `changed_at` TIMESTAMP. One row appended only when a service's status differs from its previous `unlock_result`. Cleaned up by retention.

### `ip_quality_snapshot` — latest IP quality per server

`id` PK, `server_id` FK `UNIQUE`, `ip` TEXT, `asn` TEXT NULL, `as_org` TEXT NULL, `country` / `region` / `city` TEXT NULL, `ip_type` TEXT (`residential` / `datacenter` / `hosting` / `mobile` / `isp` / `unknown`), `is_proxy` / `is_vpn` / `is_hosting` BOOLEAN, `risk_score` INTEGER NULL (0-100), `risk_level` TEXT (`low` / `medium` / `high` / `unknown`), `providers` JSON (raw per-provider results), `checked_at` TIMESTAMP. Upserted; replaced when the egress IP changes.

### `ip_quality_setting` — global settings (single row)

`check_interval_hours` INTEGER (default `12`), `public_page_enabled` BOOLEAN (default `false`).

`unlock_result` and `ip_quality_snapshot` hold only the latest value and are not subject to cleanup. `unlock_event` is cleaned by the `retention.ip_quality_event_days` config (default `90`), added alongside the existing `retention.*` keys.

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

- `BrowserMessage::IpQualityUpdate { server_id, unlock_results: Vec<UnlockResultData>, ip_quality: IpQualitySnapshotData }` — broadcast after results are stored and risk scoring completes.

`UnlockStatus` is a shared enum in `common`: `Unlocked` / `Restricted` / `Blocked` / `Failed` / `Unsupported`.

---

## 5. Agent design

New module `crates/agent/src/ip_quality/`, structured like `NetworkProber`.

- `mod.rs` — `UnlockChecker`: holds the service list + interval, runs a scheduler loop, gated by `CAP_IP_QUALITY`. `sync()` ingests `IpQualitySync`; `resync_capabilities()` reacts to capability changes.
- **Triggers** (all owned by the agent):
  - interval: every `interval_hours`;
  - egress IP change: rerun immediately when the agent's egress IP changes (reuses the existing egress-IP discovery mechanism);
  - manual: `IpQualityRunNow`.
- `detectors/` — one async Rust fn per built-in `detector` key, ported from open-source unlock scripts (`netflix`, `disney_plus`, `chatgpt`, `youtube_premium`, `spotify`, `tiktok`, …). Complex judgements (e.g. Netflix originals-only) live here.
- `rule_engine.rs` — generic engine for custom services: issue the configured request, evaluate `rules` in order against the response, return on first match.
- **Execution control:** a concurrency cap (new `MAX_UNLOCK_CONCURRENT` constant, e.g. `5`), per-service timeout. A single failing service is recorded as `Failed` and does not abort the run.
- **HTTP client:** unlock detection needs full TLS + redirect handling + custom User-Agent/headers, which the standard-library HTTP in `pinger.rs` does not provide. Implementation should first reuse the agent's existing HTTP download client (the self-upgrade downloader); if none is suitable, add `reqwest` with `rustls-tls` and a minimal feature set. The implementation plan verifies the agent's current dependencies before deciding.
- **SSRF guard:** before issuing a custom-service request, the agent rejects targets that resolve to private / loopback / link-local addresses, so the agent cannot be used to probe internal networks.

---

## 6. Server design

### `service/ip_quality.rs` (mirrors `network_probe.rs`)

- Catalog CRUD: custom services can be created / updated / deleted; built-in services can only be enabled / disabled (detection logic is immutable, no deletion).
- Global settings read/write (`check_interval_hours`, `public_page_enabled`).
- `save_unlock_results(server_id, egress_ip, results)` — upsert `unlock_result`; append an `unlock_event` row when a service's status changed from its previous value.
- `get_server_summary` / `get_overview` / public-page query functions.

### `service/ip_risk.rs` — server-side IP risk scoring

- `IpRiskProvider` trait with multiple implementations.
- **Always-on baseline: `ip-api.com`** — free, no key, 45 req/min; provides ASN/org/country plus `proxy` / `hosting` / `mobile` flags → derives `ip_type` and the proxy/VPN/hosting markers.
- **Optional providers (enabled only when the admin supplies an API key):** Scamalytics, IPQualityScore, proxycheck.io, AbuseIPDB — these provide the 0-100 `risk_score`.
- All provider outputs are stored in the `providers` JSON. `risk_score` / `risk_level` are taken from the configured primary provider; if no keyed provider is configured, they are left null and only the ip-api flags are shown.
- **Cache by IP:** the same egress IP is not re-queried within a TTL (e.g. 24h) — saves third-party quota and avoids rate limiting.

### Configuration

A `[ip_quality]` TOML section on the server (follows the `[oauth]` pattern):

```toml
[ip_quality]
risk_provider = "ip-api"          # ip-api / scamalytics / ipqs / proxycheck

[ip_quality.scamalytics]
api_key = "..."
```

Per project convention, new env vars are documented in `ENV.md` and `apps/docs/content/docs/{en,cn}/configuration.mdx` in the same change.

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

The existing public-status-page API is extended to include unlock data when `public_page_enabled` is on.

### WebSocket handling

`router/ws/agent.rs` handles `AgentMessage::UnlockResults`: store `unlock_result` + `unlock_event`, trigger `ip_risk` scoring with the reported `egress_ip` (cached), then broadcast `BrowserMessage::IpQualityUpdate`.

### Background tasks

No new scheduler task — the agent owns its schedule. `task/cleanup.rs` gains one step: purge `unlock_event` rows older than the `retention.ip_quality_event_days` config (default `90`). `unlock_result` and `ip_quality_snapshot` are latest-only and not purged.

### Built-in catalog source of truth

`BUILTIN_UNLOCK_SERVICES` is a `const` array in the `common` crate (each entry: `key`, `name`, `category`, `popularity`, `detector`). The migration seeds `unlock_service` from it. Adding a built-in service later = edit the const + a re-seed migration. Initial coverage: Netflix, Disney+, YouTube Premium, ChatGPT, Spotify, TikTok, Amazon Prime Video, HBO Max (categories `streaming` / `ai` / `social`).

---

## 7. Frontend design

Four display surfaces.

1. **Sidebar route `/ip-quality`** (`apps/web/src/routes/_authed/ip-quality.tsx`) — global overview: an all-servers × services unlock matrix (rows = servers, columns = services grouped by `category`, sorted within a group by `popularity`); one IP-quality card per server (egress IP, ASN, IP type, risk score + risk-level badge, proxy/VPN markers). A new "IP Quality" entry is added to the sidebar nav config.
2. **Settings page `settings/ip-quality.tsx`** (mirrors `settings/network-probes.tsx`) — two tabs: *Service catalog* (built-in list, locked, enable/disable only; custom-service table + create dialog with a URL / method / headers / ordered-match-rule editor) and *Settings* (`check_interval_hours`, `public_page_enabled`).
3. **Server detail tab** — `servers/$id.tsx` gains an `<IpQualityTab>`: the server's IP-quality card + its unlock matrix + status-change history + a "Check now" button.
4. **Public status page** — a new unlock-matrix section component, gated by `public_page_enabled`.

Supporting code: new hook `use-ip-quality-api.ts` (mirrors `use-network-api.ts`); new types `lib/ip-quality-types.ts`; `use-servers-ws.ts` handles `IpQualityUpdate` for realtime updates. A shared status-badge component: unlocked = green, restricted = amber, blocked = red, failed = grey, unsupported = muted.

---

## 8. Error handling & security

- **Agent:** per-service timeout; network errors → `Failed` + `detail`; one failing service does not abort the run; concurrency capped.
- **Server:** third-party API failure → store a partial `ip_quality_snapshot` (`risk_score` null); unlock results still persist independently.
- **Capability disabled:** the agent does not run; the UI shows a "capability disabled" placeholder.
- **SSRF:** custom-service URLs are validated on the server at creation time and re-validated by the agent (against the resolved address) before each request, rejecting private / loopback / link-local targets.

---

## 9. Testing

- **Rust unit:** rule engine (match rules → result); built-in detector response parsing using recorded fixtures (no live network); capability bit; service CRUD; status-change event logging; SSRF address validation.
- **Server integration:** each REST endpoint; `save_unlock_results` persistence + event log; IP-cache hit behavior.
- **Frontend vitest:** the hook; matrix rendering; status badges; the custom-rule editor.
- Built-in detectors that hit live external services are not unit-tested deterministically — only their parsing logic is, via fixtures.

---

## 10. Files touched (summary)

- `crates/common/` — `constants.rs` (capability), `protocol.rs` (messages + DTOs), new `BUILTIN_UNLOCK_SERVICES` const.
- `crates/server/` — new `entity` modules (5 tables), one migration, `service/ip_quality.rs`, `service/ip_risk.rs`, `router/api/ip_quality.rs`, `router/ws/agent.rs` (handler), `task/cleanup.rs` (one step), `config.rs` (`[ip_quality]`).
- `crates/agent/` — new `ip_quality/` module (`mod.rs`, `detectors/`, `rule_engine.rs`), capability handling, possibly an HTTP-client dependency.
- `apps/web/` — `routes/_authed/ip-quality.tsx`, `routes/_authed/settings/ip-quality.tsx`, `<IpQualityTab>` in `servers/$id.tsx`, public-status-page section, `hooks/use-ip-quality-api.ts`, `lib/ip-quality-types.ts`, `use-servers-ws.ts`, sidebar nav config.
- Docs — `ENV.md`, `apps/docs/content/docs/{en,cn}/configuration.mdx`, plus a new feature doc page; `README.md` / `README.zh-CN.md` feature list.
