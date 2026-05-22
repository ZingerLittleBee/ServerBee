# IP Quality Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add agent-side service-unlock detection plus server-side IP risk scoring, surfaced on a dedicated route, a per-server tab, and the public status page.

**Architecture:** Each capable agent runs HTTP unlock probes against streaming/AI services and reports results + its egress IP. The server stores results, derives IP metadata from its local GeoIP MMDB, optionally scores fraud risk via third-party APIs in a non-blocking background task, and broadcasts updates. Built-in detectors are hardcoded Rust; custom services use a declarative single-request rule engine.

**Tech Stack:** Rust (Axum 0.8, sea-orm, reqwest+rustls), React 19 (TanStack Router/Query, shadcn/ui), SQLite.

**Spec:** `docs/superpowers/specs/2026-05-22-ip-quality-design.md` — read it before starting any task.

**Conventions:** Conventional Commits. Migrations implement `up()` only. All REST endpoints carry `#[utoipa::path]`, all DTOs `#[derive(ToSchema)]`. Frontend uses `bun x ultracite fix` before commit. Run `cargo clippy --workspace -- -D warnings` clean.

---

## File Structure

**`crates/common/`**
- `src/constants.rs` — modify: add `CAP_IP_QUALITY`, `CapabilityKey::IpQuality`, extend `CAP_VALID_MASK`.
- `src/protocol.rs` — modify: add IP-quality message variants + DTOs.

**`crates/server/`**
- `src/entity/{unlock_service,unlock_result,unlock_event,ip_quality_snapshot,ip_risk_cache,ip_quality_setting}.rs` — create (6 entities).
- `src/entity/status_page.rs` — modify: add `show_ip_quality`.
- `src/migration/m20260522_000029_ip_quality.rs` — create (6 feature tables + inline catalog seed).
- `src/migration/m20260522_000030_status_page_show_ip_quality.rs` — create (column add).
- `src/migration/mod.rs` — modify: register both migrations.
- `src/service/ip_quality.rs` — create (catalog CRUD, settings, results, queries).
- `src/service/ip_risk.rs` — create (GeoIP baseline + provider trait + providers + cache).
- `src/router/api/ip_quality.rs` — create (REST endpoints).
- `src/router/api/status.rs` / `status_page.rs` + `src/service/status_page.rs` — modify: per-page IP quality + guest masking.
- `src/router/ws/agent.rs` — modify: handle `UnlockResults`, push `IpQualitySync`.
- `src/task/cleanup.rs` — modify: prune `unlock_event` + `ip_risk_cache`.
- `src/config.rs` — modify: `IpQualityConfig`, `retention.ip_quality_event_days`.

**`crates/agent/`**
- `src/ip_quality/{mod.rs,ssrf.rs,rule_engine.rs,http.rs}` + `src/ip_quality/detectors/{mod.rs,netflix.rs,...}` — create.
- `src/reporter.rs` — modify: wire `UnlockChecker` into the WS message loop.

**`apps/web/`**
- `src/lib/ip-quality-types.ts`, `src/hooks/use-ip-quality-api.ts` — create.
- `src/routes/_authed/ip-quality.tsx`, `src/routes/_authed/settings/ip-quality.tsx` — create.
- `src/components/ip-quality/*` — create (matrix, ip card, status badge, rule editor).
- `src/routes/_authed/servers/$id.tsx`, `settings/status-pages.tsx`, public status page, `hooks/use-servers-ws.ts`, sidebar nav — modify.

**Docs:** `ENV.md`, `apps/docs/content/docs/{en,cn}/configuration.mdx`, new `ip-quality.mdx` page, `README.md` / `README.zh-CN.md`.

---

# Phase 1 — Foundation (`common` crate)

### Task 1: Capability bit `CAP_IP_QUALITY`

**Files:**
- Modify: `crates/common/src/constants.rs`

- [ ] **Step 1: Write failing tests** — append to the `tests` module in `constants.rs`:

```rust
    #[test]
    fn cap_ip_quality_bit() {
        assert_eq!(CAP_IP_QUALITY, 1024);
        assert_eq!(CAP_VALID_MASK & CAP_IP_QUALITY, CAP_IP_QUALITY);
        assert_eq!(CAP_DEFAULT & CAP_IP_QUALITY, 0); // opt-in, not default
    }

    #[test]
    fn capability_key_ip_quality_round_trip() {
        let key: CapabilityKey = "ip_quality".parse().unwrap();
        assert_eq!(key.to_bit(), CAP_IP_QUALITY);
        assert_eq!(key.as_str(), "ip_quality");
    }

    #[test]
    fn all_capabilities_includes_ip_quality() {
        let entry = ALL_CAPABILITIES.iter().find(|m| m.bit == CAP_IP_QUALITY);
        assert!(entry.is_some());
        assert!(!entry.unwrap().default_enabled);
    }
```

- [ ] **Step 2: Run, verify fail** — `cargo test -p serverbee-common constants` → FAIL (`CAP_IP_QUALITY` undefined).

- [ ] **Step 3: Implement** in `constants.rs`:
  - Add `pub const CAP_IP_QUALITY: u32 = 1 << 10; // 1024` after `CAP_FIREWALL_BLOCK`.
  - Change `CAP_VALID_MASK` to `0b111_1111_1111; // 2047 — bits 0..=10`.
  - Add `IpQuality` to `enum CapabilityKey`.
  - Add match arms in `as_str()` (`Self::IpQuality => "ip_quality"`), `to_bit()` (`Self::IpQuality => CAP_IP_QUALITY`), and `FromStr` (`"ip_quality" => Ok(Self::IpQuality)`).
  - Add an `ALL_CAPABILITIES` entry: `CapabilityMeta { bit: CAP_IP_QUALITY, key: "ip_quality", display_name: "IP Quality", default_enabled: false, risk_level: "medium" }`.

- [ ] **Step 4: Run, verify pass** — `cargo test -p serverbee-common constants` → PASS.

- [ ] **Step 5: Commit** — `git commit -am "feat(common): add CAP_IP_QUALITY capability bit"`

### Task 2: Protocol messages + DTOs

**Files:**
- Modify: `crates/common/src/protocol.rs`

Read the existing `NetworkProbe*` message variants and DTO structs in `protocol.rs` first to match style (serde tagging, `#[serde(rename_all)]`, derives).

- [ ] **Step 1: Add shared DTOs** near the other protocol DTOs:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnlockStatus {
    Unlocked,
    Restricted,
    Blocked,
    Failed,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnlockRequest {
    pub url: String,
    pub method: String,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    pub timeout_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UnlockMatch {
    StatusEquals { code: u16 },
    StatusInRange { min: u16, max: u16 },
    BodyRegex { pattern: String },
    RedirectMatches { pattern: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnlockRule {
    #[serde(rename = "match")]
    pub match_: UnlockMatch,
    pub result: UnlockStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnlockServiceDef {
    pub id: String,
    pub key: String,
    pub detector: Option<String>,
    pub request: Option<UnlockRequest>,
    pub rules: Option<Vec<UnlockRule>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnlockResultData {
    pub service_id: String,
    pub status: UnlockStatus,
    pub region: Option<String>,
    pub latency_ms: Option<u32>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpQualitySnapshotData {
    pub ip: String,
    pub asn: Option<String>,
    pub as_org: Option<String>,
    pub country: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
    pub ip_type: String,
    pub is_proxy: bool,
    pub is_vpn: bool,
    pub is_hosting: bool,
    pub risk_score: Option<i32>,
    pub risk_level: String,
}
```

(Match the existing `Serialize`/`Deserialize` import path used in the file. Entity IDs are strings in this codebase — confirm against an existing entity.)

- [ ] **Step 2: Add message variants** to the existing enums:
  - `ServerMessage`: `IpQualitySync { services: Vec<UnlockServiceDef>, interval_hours: u32 }` and `IpQualityRunNow`.
  - `AgentMessage`: `UnlockResults { egress_ip: String, results: Vec<UnlockResultData>, checked_at: DateTime<Utc> }`.
  - `BrowserMessage`: `IpQualityUpdate { server_id: String, unlock_results: Vec<UnlockResultData>, ip_quality: Option<IpQualitySnapshotData> }`.

- [ ] **Step 3: Verify compile** — `cargo build -p serverbee-common` → success.

- [ ] **Step 4: Commit** — `git commit -am "feat(common): add IP quality protocol messages"`

---

# Phase 2 — Server data layer

### Task 3: Feature-table migration with inline catalog seed

**Files:**
- Create: `crates/server/src/migration/m20260522_000029_ip_quality.rs`
- Modify: `crates/server/src/migration/mod.rs`

Pattern: read `m20260315_000004_network_probe.rs` for a `CREATE TABLE`-style migration and `m20260321_000011_status_page_uptime_thresholds.rs` for the `execute_unprepared` style. Use raw SQL via `db.execute_unprepared`.

- [ ] **Step 1: Write the migration.** `MigrationName` = `"m20260522_000029_ip_quality"`. In `up()`, create six tables (SQL per the spec §3 — `unlock_service`, `unlock_result`, `unlock_event`, `ip_quality_snapshot`, `ip_risk_cache`, `ip_quality_setting`). Use `TEXT` PKs for `id` columns to match existing entities, `INTEGER` for booleans (0/1), `TEXT` for JSON columns, `TEXT` timestamps (ISO-8601, matching existing tables — confirm by reading `m20260315`). Add the unique constraints and indexes from the spec. Insert one `ip_quality_setting` row (`check_interval_hours = 12`). Then insert the **inline built-in catalog snapshot** (see Task 3a list below) — each row `is_builtin = 1`, `enabled = 1`, `detector` set, `request`/`rules` NULL. `down()` returns `Ok(())`.

- [ ] **Step 1a: Built-in catalog seed rows** — insert these exact rows (`key`, `name`, `category`, `popularity`, `detector` = same as `key`):

```
netflix       | Netflix              | streaming | 100 | netflix
disney_plus   | Disney+              | streaming | 95  | disney_plus
youtube_premium | YouTube Premium    | streaming | 90  | youtube_premium
amazon_prime  | Amazon Prime Video   | streaming | 80  | amazon_prime
hbo_max       | HBO Max              | streaming | 70  | hbo_max
chatgpt       | ChatGPT              | ai        | 100 | chatgpt
gemini        | Google Gemini        | ai        | 85  | gemini
spotify       | Spotify              | social    | 80  | spotify
tiktok        | TikTok               | social    | 85  | tiktok
```

- [ ] **Step 2: Register** — add `mod m20260522_000029_ip_quality;` and `Box::new(m20260522_000029_ip_quality::Migration)` to `migration/mod.rs` (after `m20260521_000028`).

- [ ] **Step 3: Verify** — `cargo build -p serverbee-server` → success. Then run the server once against a throwaway DB (`SERVERBEE_DATABASE__PATH=/tmp/ipq-test.db cargo run -p serverbee-server` — Ctrl-C after "listening") and confirm no migration error in the log. Inspect: `sqlite3 /tmp/ipq-test.db ".tables"` shows the 6 new tables; `sqlite3 /tmp/ipq-test.db "SELECT count(*) FROM unlock_service"` → `9`.

- [ ] **Step 4: Commit** — `git commit -am "feat(server): add IP quality migration and catalog seed"`

### Task 4: `status_page.show_ip_quality` migration

**Files:**
- Create: `crates/server/src/migration/m20260522_000030_status_page_show_ip_quality.rs`
- Modify: `crates/server/src/migration/mod.rs`

- [ ] **Step 1: Write migration** — copy the structure of `m20260321_000011`. `up()`:
  `db.execute_unprepared("ALTER TABLE status_page ADD COLUMN show_ip_quality INTEGER NOT NULL DEFAULT 0").await?;`

- [ ] **Step 2: Register** in `migration/mod.rs`.

- [ ] **Step 3: Verify** — `cargo build -p serverbee-server`; rerun the throwaway-DB migration check; `sqlite3 ... "PRAGMA table_info(status_page)"` shows `show_ip_quality`.

- [ ] **Step 4: Commit** — `git commit -am "feat(server): add status_page.show_ip_quality column"`

### Task 5: sea-orm entities

**Files:**
- Create: `crates/server/src/entity/{unlock_service,unlock_result,unlock_event,ip_quality_snapshot,ip_risk_cache,ip_quality_setting}.rs`
- Modify: `crates/server/src/entity/status_page.rs`, `crates/server/src/entity/mod.rs`

Pattern: read `crates/server/src/entity/network_probe_target.rs` (or any `network_probe_*` entity) for the exact `DeriveEntityModel` boilerplate, then read `entity/mod.rs` for how modules + re-exports are declared.

- [ ] **Step 1: Create the 6 entity files.** Each `Model` mirrors its table columns from Task 3 (column names → snake_case fields; SQLite booleans as `bool`; JSON columns as `String`; timestamps as `DateTimeUtc` or `String` — match what `network_probe_record` uses). Set `table_name`. No relations needed beyond what queries require; keep `Relation` empty (`enum Relation {}`) like simple existing entities.

- [ ] **Step 2: Modify `status_page.rs`** — add `pub show_ip_quality: bool,` to the `Model` (place after the uptime threshold fields).

- [ ] **Step 3: Register** all 6 modules + re-exports in `entity/mod.rs` following the existing pattern.

- [ ] **Step 4: Verify** — `cargo build -p serverbee-server` → success.

- [ ] **Step 5: Commit** — `git commit -am "feat(server): add IP quality entities"`

### Task 6: Config — `[ip_quality]` + retention key

**Files:**
- Modify: `crates/server/src/config.rs`

- [ ] **Step 1: Write failing test** — append to `config.rs` `tests`:

```rust
    #[test]
    fn ip_quality_config_from_env() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("SERVERBEE_IP_QUALITY__RISK_PROVIDER", "scamalytics");
            jail.set_env("SERVERBEE_IP_QUALITY__SCAMALYTICS__API_KEY", "k_test");
            let cfg: AppConfig = figment::Figment::new()
                .merge(figment::providers::Env::prefixed("SERVERBEE_").split("__"))
                .extract()?;
            assert_eq!(cfg.ip_quality.risk_provider, "scamalytics");
            assert_eq!(cfg.ip_quality.scamalytics.unwrap().api_key, "k_test");
            Ok(())
        });
    }

    #[test]
    fn ip_quality_event_retention_default() {
        assert_eq!(RetentionConfig::default().ip_quality_event_days, 90);
    }
```

- [ ] **Step 2: Run, verify fail** — `cargo test -p serverbee-server config` → FAIL.

- [ ] **Step 3: Implement:**
  - Add `ip_quality_event_days: u32` to `RetentionConfig` (`#[serde(default = "default_90")]`, default `90` in the `Default` impl).
  - Add `IpQualityConfig` struct: `risk_provider: String` (`#[serde(default = "default_risk_provider")]` → `"none"`), and `Option<ApiKeyConfig>` fields `scamalytics`, `ipqs`, `proxycheck`, `abuseipdb` where `ApiKeyConfig { api_key: String }`. Derive `Deserialize, Default, Clone, Debug`. Add `fn default_risk_provider() -> String { "none".into() }`.
  - Add `#[serde(default)] pub ip_quality: IpQualityConfig` to `AppConfig` + its `Default` impl.

- [ ] **Step 4: Run, verify pass** — `cargo test -p serverbee-server config` → PASS.

- [ ] **Step 5: Commit** — `git commit -am "feat(server): add ip_quality config section"`

---

# Phase 3 — Server services

### Task 7: `ip_quality` service — catalog CRUD

**Files:**
- Create: `crates/server/src/service/ip_quality.rs`
- Modify: `crates/server/src/service/mod.rs` (register `pub mod ip_quality;`)

Pattern: read `crates/server/src/service/network_probe.rs` for service struct style, `AppError` usage, and sea-orm query patterns.

- [ ] **Step 1: Write failing integration-style tests** (use the in-memory DB helper used by other service tests — check how `network_probe.rs` tests set up a DB). Cover: `list_services` returns the 9 seeded built-ins; `create_custom_service` rejects a built-in `key` collision and rejects a non-http(s) URL (SSRF syntactic check); `update_service` on a built-in changes only `enabled`; `delete_service` refuses built-ins, succeeds for custom.

- [ ] **Step 2: Run, verify fail.**

- [ ] **Step 3: Implement** `IpQualityService` with: `list_services`, `create_custom_service` (validates: unique key, `url` scheme in {http,https}, port in {80,443} or absent, rules non-empty), `update_service` (built-in → only `enabled`), `delete_service` (built-in → `AppError` 400). Generate custom `key` as `custom_<uuid-short>`.

- [ ] **Step 4: Run, verify pass.**

- [ ] **Step 5: Commit** — `git commit -am "feat(server): add IP quality service catalog CRUD"`

### Task 8: `ip_quality` service — settings, results, events

**Files:**
- Modify: `crates/server/src/service/ip_quality.rs`

- [ ] **Step 1: Write failing tests** — `get_setting`/`update_setting` round-trip `check_interval_hours`; `save_unlock_results` upserts `unlock_result` and appends an `unlock_event` row only when status changed (call twice: first call → no events; second call with a changed status → one event row).

- [ ] **Step 2: Run, verify fail.**

- [ ] **Step 3: Implement** `get_setting`, `update_setting`, `save_unlock_results(server_id, results)` (per result: read prior `unlock_result`; if `status` differs or no prior, after upsert insert an `unlock_event`; then upsert). Also `enabled_service_defs()` → `Vec<UnlockServiceDef>` for `IpQualitySync`.

- [ ] **Step 4: Run, verify pass.**

- [ ] **Step 5: Commit** — `git commit -am "feat(server): add IP quality result + event persistence"`

### Task 9: `ip_quality` service — query/summary functions

**Files:**
- Modify: `crates/server/src/service/ip_quality.rs`

- [ ] **Step 1: Write failing tests** — `get_server_summary(server_id)` returns the server's `unlock_result` rows + `ip_quality_snapshot`; `get_overview()` returns all servers' rows; `list_events(server_id, limit)` returns recent `unlock_event` rows newest-first.

- [ ] **Step 2: Run, verify fail.**

- [ ] **Step 3: Implement** the three functions returning DTO structs (define `ServerIpQualitySummary`, `IpQualityOverviewRow` etc. with `Serialize + ToSchema`).

- [ ] **Step 4: Run, verify pass.**

- [ ] **Step 5: Commit** — `git commit -am "feat(server): add IP quality query functions"`

### Task 10: `ip_risk` service — GeoIP baseline + provider trait

**Files:**
- Create: `crates/server/src/service/ip_risk.rs`
- Modify: `crates/server/src/service/mod.rs`

Pattern: find how the existing GeoIP feature reads the MMDB (grep for `mmdb` / `maxminddb` in `crates/server/src`) and reuse that reader.

- [ ] **Step 1: Write failing tests** — `derive_ip_type` maps ASN/flags to `ip_type` strings; `IpRiskProvider` trait object dispatch; `score_ip` with `risk_provider = "none"` returns a snapshot with GeoIP fields populated and `risk_score = None`, `risk_level = "unknown"`.

- [ ] **Step 2: Run, verify fail.**

- [ ] **Step 3: Implement:**
  - `trait IpRiskProvider { async fn lookup(&self, ip: &str) -> anyhow::Result<ProviderResult>; }` where `ProviderResult` carries optional `risk_score`, flags, `ip_type`.
  - GeoIP baseline lookup function (country/region/city/asn/as_org from the local MMDB; never an external call).
  - `IpRiskService::score_ip(ip)` — read GeoIP baseline, then if a keyed provider is configured call it, merge into an `IpQualitySnapshotData`-shaped result + a `providers` JSON map.

- [ ] **Step 4: Run, verify pass.**

- [ ] **Step 5: Commit** — `git commit -am "feat(server): add IP risk scoring service"`

### Task 11: `ip_risk` providers + IP cache

**Files:**
- Modify: `crates/server/src/service/ip_risk.rs`

- [ ] **Step 1: Write failing tests** — `ip_risk_cache` read-through: `score_ip` for a fresh-cached IP performs no provider call (inject a mock provider, assert call count 0 on cache hit); expired cache → provider called + cache upserted. Provider response parsing tests use recorded JSON fixtures (no live network).

- [ ] **Step 2: Run, verify fail.**

- [ ] **Step 3: Implement** providers `Scamalytics`, `IpQualityScore`, `ProxyCheck`, `AbuseIpdb`, `IpApi` (each parses its API JSON; reqwest is available — confirm `reqwest` is a server dep, add to `crates/server/Cargo.toml` if not). Add cache read/write around `score_ip`: look up `ip_risk_cache`; if `checked_at` within 24h use it; else call provider, upsert cache. `IpApi` documented HTTP-only.

- [ ] **Step 4: Run, verify pass.**

- [ ] **Step 5: Commit** — `git commit -am "feat(server): add IP risk providers and IP cache"`

---

# Phase 4 — Server API, WS, tasks

### Task 12: REST router — service catalog endpoints

**Files:**
- Create: `crates/server/src/router/api/ip_quality.rs`
- Modify: `crates/server/src/router/api/mod.rs` (register), wherever routers are nested into the app router.

Pattern: read `crates/server/src/router/api/network_probe.rs` for router construction, `ApiResponse<T>` wrapping, `#[utoipa::path]`, and admin-guard usage.

- [ ] **Step 1: Write failing tests** — extend or add an integration test (see `crates/server/tests/`): `GET /api/ip-quality/services` returns 9; `POST` as non-admin → 403; `POST` valid custom service as admin → 200; `DELETE` a built-in → 400.

- [ ] **Step 2: Run, verify fail.**

- [ ] **Step 3: Implement** the catalog endpoints from spec §6: `GET/POST /api/ip-quality/services`, `PUT/DELETE /api/ip-quality/services/:id`. Writes behind the `require_admin` layer. Register the OpenAPI paths.

- [ ] **Step 4: Run, verify pass.**

- [ ] **Step 5: Commit** — `git commit -am "feat(server): add IP quality catalog API"`

### Task 13: REST router — settings, overview, detail, check, events

**Files:**
- Modify: `crates/server/src/router/api/ip_quality.rs`

- [ ] **Step 1: Write failing tests** — `GET/PUT /api/ip-quality/settings`; `GET /api/ip-quality/overview`; `GET /api/ip-quality/servers/:id`; `GET /api/ip-quality/events`; `POST /api/ip-quality/servers/:id/check` returns 200 and (with a connected agent stub) reaches the agent — or asserts a 404/409 when the server is offline.

- [ ] **Step 2: Run, verify fail.**

- [ ] **Step 3: Implement** the remaining endpoints. `check` resolves the agent from `agent_manager` and sends `ServerMessage::IpQualityRunNow`; if the agent is offline return a clear `AppError`.

- [ ] **Step 4: Run, verify pass.**

- [ ] **Step 5: Commit** — `git commit -am "feat(server): add IP quality settings/overview/check API"`

### Task 14: WS agent handler — `UnlockResults` + `IpQualitySync`

**Files:**
- Modify: `crates/server/src/router/ws/agent.rs`

Pattern: read how `agent.rs` currently handles `AgentMessage::NetworkProbeResults` and how it sends `ServerMessage::NetworkProbeSync` on connect.

- [ ] **Step 1: Write failing test** — a WS integration test (mirror an existing one): on agent connect with `CAP_IP_QUALITY` effective, the agent receives `IpQualitySync`; sending `UnlockResults` persists `unlock_result` rows and broadcasts `IpQualityUpdate`.

- [ ] **Step 2: Run, verify fail.**

- [ ] **Step 3: Implement:**
  - On connect / capability sync, if `CAP_IP_QUALITY` is effective, send `IpQualitySync { services: enabled_service_defs(), interval_hours }`.
  - On `AgentMessage::UnlockResults`: (1) `save_unlock_results`; (2) broadcast `IpQualityUpdate { ip_quality: None }`; (3) `tokio::spawn` a task wrapped in `tokio::time::timeout` that runs `IpRiskService::score_ip(egress_ip)`, upserts `ip_quality_snapshot`, broadcasts `IpQualityUpdate { ip_quality: Some(..) }`. The handler must not `.await` the scoring inline.

- [ ] **Step 4: Run, verify pass.**

- [ ] **Step 5: Commit** — `git commit -am "feat(server): handle UnlockResults and push IpQualitySync"`

### Task 15: Public status page — IP quality, guest masking

**Files:**
- Modify: `crates/server/src/service/status_page.rs`, `crates/server/src/router/api/status_page.rs`, `crates/server/src/router/api/status.rs`

- [ ] **Step 1: Write failing tests** — `GET /api/status/{slug}` with `show_ip_quality = 0` omits IP quality; with `show_ip_quality = 1` includes it but only for the page's `server_ids_json` servers; an unauthenticated request returns `ip` as `*.*.*.*`; a request with a valid session returns the full IP. Legacy `GET /api/status` never includes IP quality.

- [ ] **Step 2: Run, verify fail.**

- [ ] **Step 3: Implement** — extend the per-slug response with an optional `ip_quality` block gated by `show_ip_quality`, scoped to the page's servers; mask `ip` → `*.*.*.*` unless the request carries a valid session (reuse the existing optional-auth extractor pattern). `status_page` create/update accept `show_ip_quality`.

- [ ] **Step 4: Run, verify pass.**

- [ ] **Step 5: Commit** — `git commit -am "feat(server): expose IP quality on public status pages"`

### Task 16: Cleanup task

**Files:**
- Modify: `crates/server/src/task/cleanup.rs`

- [ ] **Step 1: Write failing test** — insert old `unlock_event` + `ip_risk_cache` rows, run the cleanup function, assert old rows gone and recent rows kept.

- [ ] **Step 2: Run, verify fail.**

- [ ] **Step 3: Implement** — add two delete steps: `unlock_event` older than `retention.ip_quality_event_days`; `ip_risk_cache` older than 30 days.

- [ ] **Step 4: Run, verify pass.**

- [ ] **Step 5: Commit** — `git commit -am "feat(server): prune IP quality history in cleanup task"`

### Task 17: Full server verification

- [ ] **Step 1:** `cargo test -p serverbee-server` → all pass.
- [ ] **Step 2:** `cargo clippy --workspace -- -D warnings` → 0 warnings.
- [ ] **Step 3:** Commit any clippy fixes — `git commit -am "chore(server): clippy clean for IP quality"` (skip if nothing changed).

---

# Phase 5 — Agent

### Task 18: SSRF guard

**Files:**
- Create: `crates/agent/src/ip_quality/ssrf.rs`, `crates/agent/src/ip_quality/mod.rs` (module stub re-exporting submodules)

- [ ] **Step 1: Write failing tests** — `validate_url` rejects: non-http(s) scheme, port other than 80/443, and hosts. `resolve_and_check(host)` rejects when any resolved address is loopback/private/link-local/ULA (test with `localhost`, `127.0.0.1`, `10.0.0.1`, `169.254.169.254`, `::1`, `fc00::1`) and accepts a public address. Use `ipnet`/`std::net` (deps already present).

- [ ] **Step 2: Run, verify fail.**

- [ ] **Step 3: Implement** `validate_url(&str) -> Result<Url>` (scheme + port allowlist) and `resolve_and_check(host, port) -> Result<Vec<SocketAddr>>` (resolve, reject if ANY address is non-global). Add a helper `is_global_addr(IpAddr) -> bool` covering IPv4 (loopback, private, link-local, broadcast, documentation) and IPv6 (loopback, ULA `fc00::/7`, link-local `fe80::/10`).

- [ ] **Step 4: Run, verify pass** — `cargo test -p serverbee-agent ssrf`.

- [ ] **Step 5: Commit** — `git commit -am "feat(agent): add SSRF guard for IP quality checks"`

### Task 19: HTTP client + rule engine

**Files:**
- Create: `crates/agent/src/ip_quality/http.rs`, `crates/agent/src/ip_quality/rule_engine.rs`

- [ ] **Step 1: Write failing tests** for the rule engine — given a synthetic `HttpOutcome { status, body, final_url, redirects }` and a `Vec<UnlockRule>`, `apply_rules` returns the first matching rule's `UnlockStatus`; no match → `Failed`. Test each `UnlockMatch` variant.

- [ ] **Step 2: Run, verify fail.**

- [ ] **Step 3: Implement:**
  - `http.rs` — a `build_client()` returning a dedicated `reqwest::Client` (`redirect(Policy::none())`, browser `User-Agent`, connect+total timeouts). `fetch(client, request) -> HttpOutcome` follows redirects manually, calling `ssrf::resolve_and_check` on each hop's host, capping the body at 256 KiB.
  - `rule_engine.rs` — `apply_rules(outcome, rules) -> UnlockStatus` using `regex`.

- [ ] **Step 4: Run, verify pass.**

- [ ] **Step 5: Commit** — `git commit -am "feat(agent): add IP quality HTTP client and rule engine"`

### Task 20: Built-in detectors

**Files:**
- Create: `crates/agent/src/ip_quality/detectors/mod.rs` + one file per detector (`netflix.rs`, `disney_plus.rs`, `youtube_premium.rs`, `amazon_prime.rs`, `hbo_max.rs`, `chatgpt.rs`, `gemini.rs`, `spotify.rs`, `tiktok.rs`).

- [ ] **Step 1: Write fixture-based parsing tests** — for each detector, a `classify(&HttpOutcome) -> (UnlockStatus, Option<region>)` pure function tested against recorded fixture responses (unlocked / blocked / restricted samples). Detectors are independently implemented from each service's observable region-gating behavior; record any consulted open-source reference + its license in a comment.

- [ ] **Step 2: Run, verify fail.**

- [ ] **Step 3: Implement** each detector as: a request spec + a pure `classify` function. `detectors/mod.rs` exposes `dispatch(detector_key, client) -> async (UnlockStatus, region, latency)` matching on the key. Netflix `classify` distinguishes full vs originals-only → `Restricted`.

- [ ] **Step 4: Run, verify pass** — `cargo test -p serverbee-agent detectors`.

- [ ] **Step 5: Commit** — `git commit -am "feat(agent): add built-in unlock detectors"`

### Task 21: `UnlockChecker` scheduler

**Files:**
- Modify: `crates/agent/src/ip_quality/mod.rs`

Pattern: read `crates/agent/src/network_prober.rs` for `sync()`, the spawn-per-task loop, capability gating, and IP-change reaction.

- [ ] **Step 1: Write failing tests** — `UnlockChecker::sync` stores services + interval; `run_once` with a stub service set produces a `Vec<UnlockResultData>`; concurrency is capped at `MAX_UNLOCK_CONCURRENT`.

- [ ] **Step 2: Run, verify fail.**

- [ ] **Step 3: Implement** `UnlockChecker`: holds services + `interval_hours`, gated by effective `CAP_IP_QUALITY`. `run_once()` runs all enabled services (built-in via `detectors::dispatch`, custom via `http::fetch` + `rule_engine::apply_rules`) with a `Semaphore(MAX_UNLOCK_CONCURRENT)`, returns results + reads the egress IP. A scheduler loop fires every `interval_hours`, on `run_now()`, and on egress-IP change. Add `MAX_UNLOCK_CONCURRENT` to `common` constants if shared, else local.

- [ ] **Step 4: Run, verify pass.**

- [ ] **Step 5: Commit** — `git commit -am "feat(agent): add UnlockChecker scheduler"`

### Task 22: Wire `UnlockChecker` into the agent

**Files:**
- Modify: `crates/agent/src/reporter.rs` (and `crates/agent/src/lib.rs`/`main.rs` as needed)

Pattern: read how `reporter.rs` constructs and drives `NetworkProber` (handles `NetworkProbeSync`, sends `NetworkProbeResults`).

- [ ] **Step 1:** Construct `UnlockChecker` alongside `NetworkProber`. Handle `ServerMessage::IpQualitySync` → `checker.sync(...)`; `ServerMessage::IpQualityRunNow` → `checker.run_now()`. When a run completes, send `AgentMessage::UnlockResults { egress_ip, results, checked_at }`. Gate on effective `CAP_IP_QUALITY` (reuse the capability handling used for network probe / docker).

- [ ] **Step 2: Verify** — `cargo build -p serverbee-agent`; `cargo test -p serverbee-agent` → pass; `cargo clippy --workspace -- -D warnings` → clean.

- [ ] **Step 3: Commit** — `git commit -am "feat(agent): wire UnlockChecker into reporter"`

---

# Phase 6 — Frontend

### Task 23: Types + API hook

**Files:**
- Create: `apps/web/src/lib/ip-quality-types.ts`, `apps/web/src/hooks/use-ip-quality-api.ts`

Pattern: read `apps/web/src/lib/network-types.ts` and `apps/web/src/hooks/use-network-api.ts`.

- [ ] **Step 1: Write failing vitest** — `apps/web/src/hooks/use-ip-quality-api.test.ts`: mock the API client, assert `useIpQualityOverview` / `useIpQualityServer` / `useIpQualityServices` query the right endpoints; mutations (`useCreateService`, `useUpdateService`, `useDeleteService`, `useUpdateSetting`, `useCheckNow`) hit the right paths.

- [ ] **Step 2: Run, verify fail** — `bun run test ip-quality`.

- [ ] **Step 3: Implement** the TS types (mirror the Rust DTOs) and the hooks (mirror `use-network-api.ts`, including refetch intervals).

- [ ] **Step 4: Run, verify pass.**

- [ ] **Step 5: Commit** — `git commit -am "feat(web): add IP quality types and API hooks"`

### Task 24: Shared components

**Files:**
- Create: `apps/web/src/components/ip-quality/{unlock-status-badge,unlock-matrix,ip-quality-card,custom-service-dialog}.tsx`

- [ ] **Step 1: Write failing vitest** — `unlock-status-badge.test.tsx`: renders the right color/label per `UnlockStatus`; `unlock-matrix.test.tsx`: groups columns by category, sorts by popularity.

- [ ] **Step 2: Run, verify fail.**

- [ ] **Step 3: Implement** the components. Badge colors per spec §7. Matrix: rows = servers, columns = category-grouped services. `ip-quality-card`: IP/ASN/type/risk badges. `custom-service-dialog`: URL/method/headers + ordered rule editor.

- [ ] **Step 4: Run, verify pass.**

- [ ] **Step 5: Commit** — `git commit -am "feat(web): add IP quality components"`

### Task 25: `/ip-quality` route + sidebar nav

**Files:**
- Create: `apps/web/src/routes/_authed/ip-quality.tsx`
- Modify: sidebar nav config (grep for where `network` / existing nav items are registered)

- [ ] **Step 1: Implement** the global overview page: per-server `ip-quality-card` grid + the all-servers `unlock-matrix`. Add an "IP Quality" sidebar entry.

- [ ] **Step 2: Verify** — `bun run typecheck` clean; `bun x ultracite check` clean. Start `make dev-full`, open `/ip-quality`, confirm it renders without console errors (capability disabled state is acceptable with no agents).

- [ ] **Step 3: Commit** — `git commit -am "feat(web): add IP quality overview route"`

### Task 26: Settings route

**Files:**
- Create: `apps/web/src/routes/_authed/settings/ip-quality.tsx`

Pattern: `apps/web/src/routes/_authed/settings/network-probes.tsx`.

- [ ] **Step 1: Implement** two tabs — *Service catalog* (built-in enable/disable list + custom-service table + `custom-service-dialog`) and *Settings* (`check_interval_hours`).

- [ ] **Step 2: Verify** — `bun run typecheck` + `bun x ultracite check` clean; browser-check the page renders and the create dialog opens.

- [ ] **Step 3: Commit** — `git commit -am "feat(web): add IP quality settings route"`

### Task 27: Server detail tab

**Files:**
- Create: `apps/web/src/components/ip-quality/ip-quality-tab.tsx`
- Modify: `apps/web/src/routes/_authed/servers/$id.tsx`

- [ ] **Step 1: Implement** `<IpQualityTab>` (ip card + matrix + event history + "Check now" button) and add a `TabsTrigger`/`TabsContent` `value="ip-quality"` to `$id.tsx` (mirror the existing `network`/`security` tabs).

- [ ] **Step 2: Verify** — `bun run typecheck` + `bun x ultracite check` clean; browser-check the tab appears.

- [ ] **Step 3: Commit** — `git commit -am "feat(web): add IP quality server detail tab"`

### Task 28: Public status page + status-pages toggle + WS

**Files:**
- Modify: the public status page route/component, `apps/web/src/routes/_authed/settings/status-pages.tsx`, `apps/web/src/hooks/use-servers-ws.ts`

- [ ] **Step 1: Implement** — a public-page IP-quality section rendered when `show_ip_quality` is set; a `show_ip_quality` toggle in the status-page editor; handle `IpQualityUpdate` (partial + full) in `use-servers-ws.ts` to update cached query data.

- [ ] **Step 2: Verify** — `bun run typecheck` + `bun x ultracite check` clean; `bun run test` → all frontend tests pass.

- [ ] **Step 3: Commit** — `git commit -am "feat(web): expose IP quality on public status pages"`

---

# Phase 7 — Docs

### Task 29: Documentation

**Files:**
- Modify: `ENV.md`, `apps/docs/content/docs/{en,cn}/configuration.mdx`, `README.md`, `README.zh-CN.md`
- Create: `apps/docs/content/docs/{en,cn}/ip-quality.mdx` (+ `meta.json` entry)

- [ ] **Step 1:** Document the `[ip_quality]` config + `SERVERBEE_IP_QUALITY__*` env vars (incl. the `ip-api` HTTP-only / non-commercial caveat) and `retention.ip_quality_event_days` in `ENV.md` and both `configuration.mdx` files.
- [ ] **Step 2:** Add an `ip-quality.mdx` doc page (EN + CN) covering the feature, the `--allow-cap ip_quality` agent requirement, and custom services; register it in `meta.json`.
- [ ] **Step 3:** Add an "IP Quality" feature bullet to `README.md` + `README.zh-CN.md`; update the "Capability Toggles" list to include `ip quality`.
- [ ] **Step 4: Commit** — `git commit -am "docs: document IP quality feature"`

---

# Phase 8 — Integration verification (cross-compile + VPS)

### Task 30: Build, deploy, acceptance-test

- [ ] **Step 1:** `cargo test --workspace` and `bun run test` → all pass. `cargo clippy --workspace -- -D warnings` → clean.
- [ ] **Step 2:** Build the frontend (`cd apps/web && bun run build`) so it embeds into the server binary.
- [ ] **Step 3:** Cross-compile Linux x86_64 binaries on macOS for `serverbee-server` and `serverbee-agent` (use the project's existing cross-compile setup — check `Makefile` / CI for the target triple and linker).
- [ ] **Step 4:** `scp` both binaries to the VPS `207.241.173.217`. Run the server there (or run server locally and only deploy the agent — decide based on what is reachable). Start the agent with `--allow-cap ip_quality`.
- [ ] **Step 5: Acceptance checks:**
  - Server UI → enable the `ip_quality` capability for the test server.
  - Agent connects, receives `IpQualitySync`, runs checks; `/ip-quality` shows an unlock matrix and an IP-quality card with a real egress IP + GeoIP metadata.
  - "Check now" on the server detail tab triggers a fresh run.
  - Create a custom service; confirm it appears and runs.
  - Confirm an SSRF-unsafe custom URL (e.g. `http://169.254.169.254/`) is rejected at creation.
  - Enable `show_ip_quality` on a status page; confirm the public page shows the matrix with the IP masked as `*.*.*.*` when logged out.
- [ ] **Step 6:** Fix any issue found, re-test until all acceptance checks pass.
- [ ] **Step 7: Commit** any fixes.

---

## Self-review notes

- Spec coverage: capability (T1), protocol (T2), 6 tables + status_page column (T3-T5), config (T6), services (T7-T11), API (T12-T13), WS two-phase (T14), public page masking (T15), cleanup (T16), agent SSRF/HTTP/rules/detectors/scheduler/wiring (T18-T22), frontend 4 surfaces (T23-T28), docs (T29), cross-compile + VPS acceptance (T30). All spec sections mapped.
- Type consistency: `UnlockStatus`, `UnlockResultData`, `IpQualitySnapshotData`, `UnlockServiceDef` defined once in T2 and reused by server (T8/T14), agent (T19-T22), and frontend types (T23, mirrored).
- Known verification points for executors: confirm entity ID column type (TEXT) and timestamp representation against `network_probe_*` entities; confirm `reqwest` is a `serverbee-server` dependency (add if missing); confirm the GeoIP MMDB reader API; confirm the cross-compile target/linker from the existing build setup.
