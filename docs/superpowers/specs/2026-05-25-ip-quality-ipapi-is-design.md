# IP Quality: ipapi.is Provider Refactor

**Status:** Draft
**Date:** 2026-05-25
**Branch:** `auckland-v1`
**Supersedes:** Provider section of [`2026-05-22-ip-quality-design.md`](./2026-05-22-ip-quality-design.md)

## Goal

Replace the current "user must pick one of five risk-scoring providers and supply an API key" model with a zero-configuration default that works out of the box. The current default (`risk_provider = "none"`) produces a misleading `risk_level = "unknown"` for every deployment that has not configured a paid provider — which is the overwhelming majority of installations.

Concretely: make [api.ipapi.is](https://ipapi.is) the default risk provider, keep [ip-api.com](https://ip-api.com) as a built-in fallback, and remove the four commercial providers (Scamalytics, IPQualityScore, ProxyCheck, AbuseIPDB) from the codebase.

## Non-goals

- Re-locate risk scoring to the agent. (Considered and rejected during brainstorming — server-side is simpler, the 1000/day quota is far above what a typical deployment consumes, and the "distributed quota" argument is YAGNI.)
- Touch the local MaxMind MMDB pipeline. (It is used by `iptrace` and remains the baseline source of geographic data.)
- Change how `ip_quality_snapshot` rows are written, read, or cached. (Cache TTL, `ip_risk_cache` table, and the broadcast path stay untouched.)
- Change the unlock-detection subsystem on the agent side.
- Backfill historical snapshots. New columns are nullable / default-false and get populated on the next agent-driven refresh.
- Introduce active rate limiting against ipapi.is. (Warn on 429 in logs, alerting-driven.)
- Generate OpenAPI clients automatically. Frontend types are kept in manual sync as today.

## Decisions locked during brainstorming

| Topic | Decision |
|---|---|
| Where the call happens | **Server-side.** Agent unchanged. |
| Primary provider | **`ipapi_is`** (default). API key optional. |
| Fallback provider | **`ip-api`** (default, no key). User-disablable via `risk_provider_fallback = "none"`. |
| Legacy providers | **Delete all five** concrete implementations (Scamalytics, IPQS, ProxyCheck, AbuseIPDB, ip-api as a *primary* choice). The `ip-api` code is retained internally to back the fallback slot. |
| Extension architecture | Keep `trait IpRiskProvider` and `provider_for_config()` factory. Adding a future provider remains a single new `impl` + one match arm. |
| Schema | **Extend** `ip_quality_snapshot` with five high-value nullable / boolean columns from ipapi.is. Drop nothing. |
| MMDB | **Retain** (iptrace dependency). Continues to provide GeoIP baseline when the provider call fails. |
| Threshold mapping | Reuse existing `derive_risk_level(33/66)`. Re-evaluate only if real-world data shows systematic skew. |
| Breaking change | **Accepted.** Old `SERVERBEE_IP_QUALITY__SCAMALYTICS__*` etc. become silently ignored unknown fields (Figment does not error). Documented in migration notes. |

## Architecture

```
Server
  WS agent handler ── EgressIpReport ──► IpRiskService::score_ip
                                              │
                                              ├─► read_cache(ip)        ── HIT (24h TTL) ──► return
                                              │
                                              ├─► lookup_geoip_baseline (MMDB, local)
                                              │
                                              ├─► primary provider call (15s timeout)
                                              │     ├─ Ok                            ──┐
                                              │     └─ Err / timeout                ──┤
                                              │                                       │
                                              ├─► fallback provider (if primary failed
                                              │     and fallback != primary
                                              │     and fallback != "none")
                                              │     │                                  │
                                              │     └─ Ok / Err                     ──┤
                                              │                                       │
                                              ├─► merge: baseline + (primary ?? fallback)
                                              │
                                              ├─► write_cache(ip_risk_cache)
                                              └─► broadcast IpQualityUpdate to browsers
```

Agent, MMDB pipeline, and the unlock-detection subsystem are unchanged.

## Configuration

### Environment variables

**Added:**

| Variable | Default | Notes |
|---|---|---|
| `SERVERBEE_IP_QUALITY__RISK_PROVIDER` | `ipapi_is` | Was `none`. Accepted values: `none`, `ipapi_is`, `ip-api`. |
| `SERVERBEE_IP_QUALITY__RISK_PROVIDER_FALLBACK` | `ip-api` | New. Set to `none` to disable fallback. Same accepted values. |
| `SERVERBEE_IP_QUALITY__IPAPI_IS__API_KEY` | empty | Optional. When empty, the API is called anonymously (1000/day per source IP). |
| `SERVERBEE_IP_QUALITY__IPAPI_IS__ENDPOINT` | `https://api.ipapi.is` | Override for testing / private mirrors. |

**Removed:**

```
SERVERBEE_IP_QUALITY__SCAMALYTICS__API_KEY
SERVERBEE_IP_QUALITY__SCAMALYTICS__ENDPOINT
SERVERBEE_IP_QUALITY__IPQS__API_KEY
SERVERBEE_IP_QUALITY__IPQS__ENDPOINT
SERVERBEE_IP_QUALITY__PROXYCHECK__API_KEY
SERVERBEE_IP_QUALITY__PROXYCHECK__ENDPOINT
SERVERBEE_IP_QUALITY__ABUSEIPDB__API_KEY
SERVERBEE_IP_QUALITY__ABUSEIPDB__ENDPOINT
```

Figment will silently ignore these if still present in `serverbee.toml` or the environment. Documented in the release notes.

### Rust config struct (`crates/server/src/config.rs`)

```rust
pub struct IpQualityConfig {
    #[serde(default = "default_risk_provider")]
    pub risk_provider: String,                          // default "ipapi_is"

    #[serde(default = "default_risk_provider_fallback")]
    pub risk_provider_fallback: String,                 // default "ip-api"

    #[serde(default)]
    pub ipapi_is: Option<RiskProviderKey>,              // primary key + endpoint override
}

fn default_risk_provider() -> String { "ipapi_is".to_string() }
fn default_risk_provider_fallback() -> String { "ip-api".to_string() }
```

The four fields `scamalytics / ipqs / proxycheck / abuseipdb` on `IpQualityConfig` are removed. `RiskProviderKey` itself is retained.

## Database schema

### Migration `m20260525_000032_ip_quality_snapshot_extra_fields.rs`

```sql
ALTER TABLE ip_quality_snapshot
  ADD COLUMN is_tor              BOOLEAN  NOT NULL DEFAULT 0,
  ADD COLUMN is_abuser           BOOLEAN  NOT NULL DEFAULT 0,
  ADD COLUMN is_mobile           BOOLEAN  NOT NULL DEFAULT 0,
  ADD COLUMN asn_abuser_score    INTEGER  NULL,
  ADD COLUMN abuse_email         VARCHAR  NULL;
```

`up()` is implemented; `down()` is `Ok(())` per project convention. Registered at the end of `migration::migrations()`.

### Column rationale

| Column | Type | Why included |
|---|---|---|
| `is_tor` | bool | Core anonymity signal. Not equivalent to `is_proxy`. |
| `is_abuser` | bool | Composite flag the UI can use as a quick "risk" tag. |
| `is_mobile` | bool | Mobile networks carry elevated risk; useful for alert-weight tuning. |
| `asn_abuser_score` | int (0-100) | ASN-level reputation complements per-IP `risk_score`. A fresh IP inside a notorious ASN is still suspicious. |
| `abuse_email` | string | Direct value for the user: one-click `mailto:` to report attackers. |

### Columns deliberately excluded

| Field | Why skipped |
|---|---|
| `is_crawler`, `is_satellite` | Irrelevant for VPS monitoring; ipapi.is's `is_crawler` has known false positives. |
| `datacenter.{name,network}` | Overlap with existing `as_org` / `asn`. |
| `abuse.{name,phone}` | `name` duplicates `as_org`; one contact channel (`email`) is enough. |

### Entity

`crates/server/src/entity/ip_quality_snapshot.rs` gains the five matching fields. `is_tor / is_abuser / is_mobile` are `bool` (non-null with default); `asn_abuser_score: Option<i32>` and `abuse_email: Option<String>`.

## Core implementation

### `IpApiIsProvider` (new, ~120 LoC)

Lives in `crates/server/src/service/ip_risk.rs` alongside the existing provider implementations.

```rust
pub const IPAPI_IS_DEFAULT_ENDPOINT: &str = "https://api.ipapi.is";

pub struct IpApiIsProvider {
    pub api_key: Option<String>,
    pub endpoint: String,
    client: reqwest::Client,
}

impl IpApiIsProvider {
    pub fn from_config(key: Option<&RiskProviderKey>) -> Self { /* ... */ }
}

#[async_trait]
impl IpRiskProvider for IpApiIsProvider {
    fn name(&self) -> &'static str { "ipapi_is" }

    async fn lookup(&self, ip: &str) -> anyhow::Result<ProviderResult> {
        let mut url = format!("{}/?q={ip}", self.endpoint);
        if let Some(k) = &self.api_key {
            url.push_str(&format!("&key={k}"));
        }
        let resp = self.client.get(&url).send().await?
            .error_for_status()?
            .json::<JsonValue>().await?;
        Ok(parse_ipapi_is_response(resp))
    }
}
```

### Response parsing

ipapi.is returns `abuser_score` as a string like `"0.0039 (Low)"`. Extraction:

```rust
fn parse_score(raw: Option<&str>) -> Option<i32> {
    raw.and_then(|s| s.split_whitespace().next())
       .and_then(|n| n.parse::<f64>().ok())
       .map(|f| (f * 100.0).round() as i32)
}
```

Score multiplication (`* 100`) maps the 0–1 ipapi.is range onto the existing 0–100 `risk_score` integer. Most clean IPs end up at 0–1, which `derive_risk_level` correctly classifies as `low`. Real abusers (Tor exits, listed proxies) score 0.5+ → `50+` → `medium`/`high`. The current 33/66 thresholds stay; we revisit only if production data shows systematic skew.

Field mapping (raw JSON path → `ProviderResult`):

| JSON path | Field |
|---|---|
| `/company/abuser_score` | `risk_score` (parsed) |
| `/asn/abuser_score` | `asn_abuser_score` (parsed) |
| `/is_proxy` | `is_proxy` |
| `/is_vpn` | `is_vpn` |
| `/is_datacenter` | `is_hosting` |
| `/is_tor` | `is_tor` |
| `/is_abuser` | `is_abuser` |
| `/is_mobile` | `is_mobile` |
| `/abuse/email` | `abuse_email` |
| `/company/type` or `/asn/type` | `ip_type` |
| entire response | `raw` (kept for the `providers` debug column) |

### `ProviderResult` extension

```rust
pub struct ProviderResult {
    pub risk_score: Option<i32>,
    pub is_proxy: Option<bool>,
    pub is_vpn: Option<bool>,
    pub is_hosting: Option<bool>,
    pub ip_type: Option<String>,
    pub raw: JsonValue,

    // New:
    pub is_tor: bool,
    pub is_abuser: bool,
    pub is_mobile: bool,
    pub asn_abuser_score: Option<i32>,
    pub abuse_email: Option<String>,
}
```

`IpApiProvider` (ip-api.com, kept for fallback) fills the new fields with defaults (`false` / `None`), since ip-api.com does not return that data.

### Factory function

```rust
pub fn provider_for_config(
    config: &IpQualityConfig,
    provider_name: &str,
) -> Option<Box<dyn IpRiskProvider>> {
    match provider_name {
        "ipapi_is" => Some(Box::new(IpApiIsProvider::from_config(config.ipapi_is.as_ref()))),
        "ip-api"   => Some(Box::new(IpApiProvider { client: build_provider_client() })),
        _          => None,    // "none" or unknown
    }
}
```

The signature gains a `provider_name` argument so `IpRiskService` can construct the primary and fallback instances separately.

### Fallback orchestration

`IpRiskService::score_ip` resolves both providers from config and delegates to `score_ip_with`:

```rust
pub async fn score_ip(&self, db, geoip, ip) -> Option<IpQualitySnapshotData> {
    let primary = provider_for_config(&self.config, &self.config.risk_provider);
    let fallback = if self.config.risk_provider_fallback != "none"
        && self.config.risk_provider_fallback != self.config.risk_provider
    {
        provider_for_config(&self.config, &self.config.risk_provider_fallback)
    } else {
        None
    };
    self.score_ip_with(db, geoip, ip, primary, fallback).await
}
```

`score_ip_with` (signature extended with a `fallback` argument) tries `primary` first; on `Err` or timeout it falls through to `fallback`. Both failing returns a geo-only snapshot — the existing graceful-degradation behavior is preserved.

A small helper categorizes errors:

```rust
fn is_fallbackable(e: &anyhow::Error) -> bool {
    if let Some(re) = e.downcast_ref::<reqwest::Error>() {
        if re.is_timeout() || re.is_connect() || re.is_request() { return true; }
        if let Some(s) = re.status() {
            return s.is_server_error()
                || s == reqwest::StatusCode::TOO_MANY_REQUESTS;
        }
    }
    e.downcast_ref::<serde_json::Error>().is_some()
}
```

`is_fallbackable` currently only drives **log level**, not the fallback decision itself: every primary failure falls through. The categorization is in place so a future metric can split "expected transient failure" vs "configuration error".

### Deletions

| Location | Action |
|---|---|
| `ip_risk.rs` `ScamalyticsProvider` impl block | Delete |
| `ip_risk.rs` `IpQualityScoreProvider` impl block | Delete |
| `ip_risk.rs` `ProxyCheckProvider` impl block | Delete |
| `ip_risk.rs` `AbuseIpdbProvider` impl block | Delete |
| `ip_risk.rs` `IpApiProvider` impl block | **Keep** (drives the fallback) |
| `ip_risk.rs` `provider_for_config` four match arms | Replace per the factory above |
| `config.rs` four `Option<RiskProviderKey>` fields | Delete |
| `config.rs` legacy provider test cases | Rewrite to cover `ipapi_is` + `ip-api` |

## Logging

| Outcome | Level | Message shape |
|---|---|---|
| Primary success | DEBUG | `"primary provider ipapi_is succeeded for {ip}"` |
| Primary fail, fallback success | INFO | `"primary ipapi_is failed ({err}), fallback ip-api succeeded for {ip}"` |
| Both fail | WARN | `"both providers failed for {ip}, returning geo-only snapshot"` |
| Misconfig: `risk_provider == risk_provider_fallback` | WARN at startup | `"risk_provider_fallback matches risk_provider, ignoring fallback"` |
| Misconfig: unknown provider name | WARN at startup | `"unknown risk_provider '{x}', no provider will be used"` |

Metrics are deferred — no exporter is in place today. Logs are sufficient for alerting.

## Frontend changes

### Type sync — `apps/web/src/lib/ip-quality-types.ts`

Extend `IpQualitySnapshotData` with the five new fields:

```diff
 export interface IpQualitySnapshotData {
   // ...existing fields...
+  is_tor: boolean
+  is_abuser: boolean
+  is_mobile: boolean
+  asn_abuser_score: number | null
+  abuse_email: string | null
 }
```

### UI scope

**In this spec:**

- Render `is_tor`, `is_abuser`, `is_mobile` as `<Badge>` chips on the per-server IP quality detail panel when true.
- Show `asn_abuser_score` inline next to `as_org` when present.
- Add i18n keys: `field_is_tor`, `field_is_abuser`, `field_is_mobile`, `field_asn_score`. Update both `apps/web/src/locales/en/ip-quality.json` and `apps/web/src/locales/zh/ip-quality.json`.

**Deferred (future improvements section):**

- `mailto:` link for `abuse_email` with a pre-filled report template.
- Color-grading the `asn_abuser_score` badge by severity.
- Multi-provider overlay views (one row per provider, xykt-style).

`ip-quality-card.tsx:56` (`risk_level || 'unknown'`) is intentionally left alone. With `ipapi_is` as the default primary, `unknown` will appear only when the user has explicitly disabled scoring (`risk_provider = none`) or when both primary and fallback failed.

## Testing

### Unit tests (`crates/server/src/service/ip_risk.rs#[cfg(test)]`)

| Test | Covers |
|---|---|
| `test_ipapi_is_parse_minimal` | Sparse JSON parses without panic; missing fields → defaults. |
| `test_ipapi_is_parse_full` | Real 8.8.8.8 response → all `ProviderResult` fields populated correctly. |
| `test_ipapi_is_parse_abuser_score_formats` | `"0.0039 (Low)"`, `"0.5"`, `"null"`, missing field — all handled. |
| `test_ipapi_is_score_rounding` | `0.0039 → 0`, `0.005 → 1`, `0.5 → 50`, `1.0 → 100`. |
| `test_provider_for_config_ipapi_is` | Factory returns correct concrete type with and without API key. |
| `test_provider_for_config_unknown_returns_none` | `"scamalytics"` (deleted) → `None`, no panic. |
| `test_score_ip_fallback_invoked_on_primary_error` | Mock primary errors → mock fallback is called → result uses fallback data. |
| `test_score_ip_fallback_skipped_when_same_provider` | `primary = fallback = "ipapi_is"` → fallback not constructed. |
| `test_score_ip_fallback_skipped_when_fallback_none` | `risk_provider_fallback = "none"` → fallback not constructed. |
| `test_score_ip_both_fail_returns_geo_baseline` | Both fail → snapshot returned with `risk_score = None`, geo fields populated. |

Mocks reuse the existing `MockProvider` pattern in `ip_risk.rs#[cfg(test)]`.

### Integration tests (`crates/server/tests/integration/ip_quality.rs` or new file)

| Test | Covers |
|---|---|
| `test_egress_ip_report_writes_snapshot_with_new_fields` | Agent EgressIpReport → DB row contains `is_tor`, `is_abuser`, etc. |
| `test_default_config_uses_ipapi_is` | Empty env → `config.risk_provider == "ipapi_is"`, `risk_provider_fallback == "ip-api"`. |
| `test_legacy_provider_env_vars_silently_ignored` | Setting `SERVERBEE_IP_QUALITY__SCAMALYTICS__API_KEY` causes no startup error. |

### Manual verification

New file `tests/ip-quality/ipapi-is.md` indexed from `tests/README.md`:

1. Fresh deploy, no IP-quality env vars → IP quality page shows a real `risk_level` (not `unknown`).
2. Point primary endpoint at `https://example.invalid` → log shows fallback hit, UI shows partial data (geo + proxy flags, no `risk_score`).
3. Set `SERVERBEE_IP_QUALITY__RISK_PROVIDER=none` → UI shows `unknown` (user-opted-out, expected).
4. Set `SERVERBEE_IP_QUALITY__IPAPI_IS__API_KEY=xxx` → packet capture confirms `&key=xxx` in outgoing URL.
5. Upgrade an existing install that had `SCAMALYTICS__API_KEY=xxx` set → startup succeeds, IP quality functions via ipapi.is, legacy env var is silently ignored.

## Documentation sync

CLAUDE.md mandates documentation updates whenever env vars change. Files to update:

| File | Change |
|---|---|
| `ENV.md` | Remove four legacy provider sections; add `IPAPI_IS__*`, `RISK_PROVIDER_FALLBACK`; add migration note. |
| `apps/docs/content/docs/en/configuration.mdx` | Mirror `ENV.md`. |
| `apps/docs/content/docs/cn/configuration.mdx` | Mirror `ENV.md`. |
| `apps/docs/content/docs/en/ip-quality.mdx` (if exists) | Update "configure a provider" instructions to reflect zero-config default. |
| `apps/docs/content/docs/cn/ip-quality.mdx` (if exists) | Same. |
| `docs/superpowers/specs/2026-05-22-ip-quality-design.md` | Append a postscript pointing at this spec. |
| `docs/superpowers/plans/PROGRESS.md` | Add task entry. |
| `tests/README.md` | Index the new manual verification doc. |

## Release notes (excerpt)

```
IP quality refactor:
- Default risk provider changed from "none" to "ipapi_is" (api.ipapi.is).
- New env vars: SERVERBEE_IP_QUALITY__RISK_PROVIDER_FALLBACK (default "ip-api"),
  SERVERBEE_IP_QUALITY__IPAPI_IS__API_KEY (optional), SERVERBEE_IP_QUALITY__IPAPI_IS__ENDPOINT.
- Removed providers: Scamalytics, IPQualityScore, ProxyCheck, AbuseIPDB. Their env vars
  (SERVERBEE_IP_QUALITY__{SCAMALYTICS,IPQS,PROXYCHECK,ABUSEIPDB}__*) are silently ignored.
- Schema: ip_quality_snapshot gains is_tor, is_abuser, is_mobile, asn_abuser_score,
  abuse_email. Existing rows are unaffected; new fields populate on the next agent refresh.
- No action required for users running default config — risk scoring just starts working.
```

## Risks & open considerations

| Risk | Mitigation |
|---|---|
| ipapi.is becomes unreachable from a deployment region | Built-in `ip-api` fallback. Users can configure an HTTP proxy via standard `https_proxy` env var. |
| ipapi.is rate-limits the server's egress IP | 24h cache + ~one query per unique egress IP per day per deployment. Practical ceiling is far below 1000/day. Logged on 429. |
| `derive_risk_level(33/66)` thresholds are wrong for ipapi.is's distribution | Existing thresholds are reused initially; data-driven adjustment is a follow-up if reports come in. |
| Users with paid provider keys lose functionality | Documented in release notes. Re-adding a provider is mechanical (~80 LoC impl + one match arm). |
| Frontend type drift if OpenAPI generation is added later | Types are manually kept in sync, consistent with existing project practice. |

## Future improvements (out of scope here)

- One-click "report abuse" CTA wired to `abuse_email`.
- Severity color grading for `asn_abuser_score`.
- Multi-provider overlay (xykt-style multiple scores side by side) as an optional power-user mode.
- Prometheus exporter exposing `ip_risk_provider_calls_total{provider, status}` and `_latency_seconds{provider}`.
- Per-server cache freshness override (UI button to force-refresh a single server's IP quality).
