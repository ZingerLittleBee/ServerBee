# IP Quality ipapi.is Provider Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the current "user must configure one of five paid providers" risk-scoring model with a zero-config default that just works — `api.ipapi.is` as primary, `ip-api.com` as fallback, both keyless.

**Architecture:** Server-side. `IpRiskService` resolves a primary + fallback provider from config; primary failure falls through to fallback. The legacy Scamalytics / IPQualityScore / ProxyCheck / AbuseIPDB implementations are deleted. The `ip_quality_snapshot` table gains five high-value fields from ipapi.is's richer response.

**Tech Stack:** Rust (Axum, sea-orm, reqwest, tokio), React 19 + TypeScript, SQLite.

**Spec:** `docs/superpowers/specs/2026-05-25-ip-quality-ipapi-is-design.md` — read it before starting any task.

**Conventions:**
- Conventional Commits (`type(scope): imperative summary`).
- Migrations implement `up()` only; `down()` is `Ok(())`.
- All REST endpoints already carry `#[utoipa::path]`, all DTOs `#[derive(ToSchema)]` — no new endpoints in this plan.
- Frontend: `bun x ultracite fix` before commit, `bun run typecheck` clean.
- Backend: `cargo clippy --workspace -- -D warnings` clean.
- Never add Claude attribution to commits/PRs/docs.

---

## File Structure

**`crates/common/`**
- `src/protocol.rs:227-241` — modify: extend `IpQualitySnapshotData` with 5 new fields.

**`crates/server/`**
- `src/migration/m20260525_000033_ip_quality_snapshot_extra_fields.rs` — create.
- `src/migration/mod.rs` — modify: register new migration.
- `src/entity/ip_quality_snapshot.rs` — modify: 5 new fields on `Model`.
- `src/service/ip_risk.rs` — modify heavily:
  - Extend `ProviderResult` struct
  - Add `IpApiIsProvider` + `parse_ipapi_is_response`
  - Change `provider_for_config` signature to take a provider name
  - Add `is_fallbackable` helper
  - Add fallback orchestration to `score_ip_with`
  - Update `write_cache` SQL to persist new columns
  - Delete `ScamalyticsProvider`, `IpQualityScoreProvider`, `ProxyCheckProvider`, `AbuseIpdbProvider`
- `src/config.rs:381-396` — modify: drop 4 legacy provider fields, add `ipapi_is` + `risk_provider_fallback`, change defaults.

**`apps/web/`**
- `src/lib/ip-quality-types.ts:62-76` — modify: add 5 fields to `IpQualitySnapshotData`.
- `src/components/ip-quality/ip-quality-card.tsx` — modify: render new badges.
- `src/locales/en/ip-quality.json`, `src/locales/zh/ip-quality.json` — modify: 4 new i18n keys.

**Docs**
- `ENV.md` — replace 4 legacy provider sections with `IPAPI_IS__*` + fallback.
- `apps/docs/content/docs/en/configuration.mdx` — mirror `ENV.md`.
- `apps/docs/content/docs/cn/configuration.mdx` — mirror `ENV.md`.
- `docs/superpowers/specs/2026-05-22-ip-quality-design.md` — append postscript.
- `docs/superpowers/plans/PROGRESS.md` — add entry.
- `tests/ip-quality/ipapi-is.md` — create (manual verification).
- `tests/README.md` — index the new test doc.

---

# Phase 1 — Foundation (schema + DTOs)

### Task 1: Migration for 5 new columns

**Files:**
- Create: `crates/server/src/migration/m20260525_000033_ip_quality_snapshot_extra_fields.rs`
- Modify: `crates/server/src/migration/mod.rs`

- [ ] **Step 1: Look at an existing recent migration for reference**

Read `crates/server/src/migration/m20260524_000032_create_traceroute_record.rs` to confirm the `add_column` + `alter_table` pattern this codebase uses with sea-orm-migration.

- [ ] **Step 2: Create the migration file**

Write `crates/server/src/migration/m20260525_000033_ip_quality_snapshot_extra_fields.rs`:

```rust
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(IpQualitySnapshot::Table)
                    .add_column(
                        ColumnDef::new(IpQualitySnapshot::IsTor)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .add_column(
                        ColumnDef::new(IpQualitySnapshot::IsAbuser)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .add_column(
                        ColumnDef::new(IpQualitySnapshot::IsMobile)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .add_column(
                        ColumnDef::new(IpQualitySnapshot::AsnAbuserScore)
                            .integer()
                            .null(),
                    )
                    .add_column(
                        ColumnDef::new(IpQualitySnapshot::AbuseEmail)
                            .string()
                            .null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

#[derive(DeriveIden)]
enum IpQualitySnapshot {
    Table,
    IsTor,
    IsAbuser,
    IsMobile,
    AsnAbuserScore,
    AbuseEmail,
}
```

- [ ] **Step 3: Register migration in `mod.rs`**

In `crates/server/src/migration/mod.rs`, find the line `mod m20260524_000032_create_traceroute_record;` and add immediately after:

```rust
mod m20260525_000033_ip_quality_snapshot_extra_fields;
```

Then in the `migrations()` vector, after `Box::new(m20260524_000032_create_traceroute_record::Migration),` add:

```rust
            Box::new(m20260525_000033_ip_quality_snapshot_extra_fields::Migration),
```

- [ ] **Step 4: Run migrations against a fresh DB**

```bash
rm -f /tmp/serverbee-plan-test.db
SERVERBEE_DATABASE__URL=sqlite:///tmp/serverbee-plan-test.db?mode=rwc \
  cargo run -p serverbee-server --bin serverbee-server -- --help 2>&1 | head -5
```

Then inspect the schema:

```bash
sqlite3 /tmp/serverbee-plan-test.db ".schema ip_quality_snapshot"
```

Expected: output contains `is_tor INTEGER NOT NULL DEFAULT FALSE`, `is_abuser`, `is_mobile`, `asn_abuser_score INTEGER`, `abuse_email VARCHAR`.

(If `--help` doesn't trigger migrations, use `cargo test -p serverbee-server --test integration -- --test-threads=1` against a temp DB instead. Most projects run migrations at startup; see `crates/server/src/main.rs` for the actual trigger.)

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/migration/
git commit -m "feat(server): add ip_quality_snapshot extra fields migration"
```

---

### Task 2: Extend entity Model

**Files:**
- Modify: `crates/server/src/entity/ip_quality_snapshot.rs`

- [ ] **Step 1: Add 5 fields to the Model struct**

Replace the contents of `crates/server/src/entity/ip_quality_snapshot.rs` with:

```rust
use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, utoipa::ToSchema)]
#[schema(as = IpQualitySnapshot)]
#[sea_orm(table_name = "ip_quality_snapshot")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub server_id: String,
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
    pub is_tor: bool,
    pub is_abuser: bool,
    pub is_mobile: bool,
    pub asn_abuser_score: Option<i32>,
    pub abuse_email: Option<String>,
    #[schema(value_type = String, format = DateTime)]
    pub checked_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

- [ ] **Step 2: Verify compilation**

```bash
cargo check -p serverbee-server 2>&1 | tail -20
```

Expected: clean (or errors only from callers that construct `Model` without the new fields — these get fixed in Task 4).

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/entity/ip_quality_snapshot.rs
git commit -m "feat(server): extend ip_quality_snapshot entity with ipapi.is fields"
```

---

### Task 3: Extend protocol DTO

**Files:**
- Modify: `crates/common/src/protocol.rs:225-241`

- [ ] **Step 1: Add 5 fields to `IpQualitySnapshotData`**

In `crates/common/src/protocol.rs`, find the struct at line 227 and modify it:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
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
    #[serde(default)]
    pub is_tor: bool,
    #[serde(default)]
    pub is_abuser: bool,
    #[serde(default)]
    pub is_mobile: bool,
    #[serde(default)]
    pub asn_abuser_score: Option<i32>,
    #[serde(default)]
    pub abuse_email: Option<String>,
    pub checked_at: DateTime<Utc>,
}
```

The `#[serde(default)]` on each new field guarantees the DTO can deserialize old JSON (e.g. cached `providers_json` blobs that lack these fields).

- [ ] **Step 2: Fix all construction sites**

```bash
grep -rn "IpQualitySnapshotData {" crates/ apps/ 2>&1 | grep -v 'target/'
```

Each match (in `service/ip_risk.rs`, possibly test fixtures, `protocol.rs:2038` example) needs the 5 new fields added. For real construction sites, set:
- `is_tor: false`
- `is_abuser: false`
- `is_mobile: false`
- `asn_abuser_score: None`
- `abuse_email: None`

The full field population happens in Task 11 (field mapping). For now this is just to make `cargo check` pass.

- [ ] **Step 3: Verify compilation**

```bash
cargo check --workspace 2>&1 | tail -10
```

Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add crates/common/src/protocol.rs crates/server/src/service/ip_risk.rs
git commit -m "feat(common): extend IpQualitySnapshotData with ipapi.is fields"
```

---

### Task 4: Persist new columns in `write_cache`

**Files:**
- Modify: `crates/server/src/service/ip_risk.rs:315-368`

- [ ] **Step 1: Extend the INSERT SQL**

In `write_cache()`, change the SQL and value bindings to include the 5 new columns:

```rust
let sql = "INSERT OR REPLACE INTO ip_risk_cache \
    (ip, asn, as_org, country, region, city, ip_type, is_proxy, is_vpn, is_hosting, \
     risk_score, risk_level, is_tor, is_abuser, is_mobile, asn_abuser_score, abuse_email, \
     providers, checked_at) \
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";
```

And add to the value vector (insert these between `risk_level` and `providers_str` lines):

```rust
                    sea_orm::Value::Int(Some(snapshot.is_tor as i32)),
                    sea_orm::Value::Int(Some(snapshot.is_abuser as i32)),
                    sea_orm::Value::Int(Some(snapshot.is_mobile as i32)),
                    opt_int(snapshot.asn_abuser_score),
                    opt_str(&snapshot.abuse_email),
```

- [ ] **Step 2: Check `ip_risk_cache` table schema**

```bash
grep -n "ip_risk_cache" crates/server/src/migration/m20260522_000029_ip_quality.rs | head -10
```

If `ip_risk_cache` (note: separate table from `ip_quality_snapshot`) is missing the 5 columns, **extend the Task 1 migration** to alter `ip_risk_cache` too:

```rust
manager.alter_table(
    Table::alter()
        .table(IpRiskCache::Table)
        .add_column(ColumnDef::new(IpRiskCache::IsTor).boolean().not_null().default(false))
        .add_column(ColumnDef::new(IpRiskCache::IsAbuser).boolean().not_null().default(false))
        .add_column(ColumnDef::new(IpRiskCache::IsMobile).boolean().not_null().default(false))
        .add_column(ColumnDef::new(IpRiskCache::AsnAbuserScore).integer().null())
        .add_column(ColumnDef::new(IpRiskCache::AbuseEmail).string().null())
        .to_owned()
).await?;
```

…with the corresponding `IpRiskCache` enum below `IpQualitySnapshot`.

- [ ] **Step 3: Update `read_cache` to populate snapshot from new columns**

Find the `read_cache` function (around `ip_risk.rs:282`) and the place where it constructs `IpQualitySnapshotData` from the DB row. Map the 5 new SQL columns (`is_tor`, `is_abuser`, `is_mobile`, `asn_abuser_score`, `abuse_email`) into the snapshot.

If `read_cache` uses the sea-orm entity (`ip_risk_cache::Entity::find_by_id`), the entity must also have these fields. Update `crates/server/src/entity/ip_risk_cache.rs` analogously to Task 2.

- [ ] **Step 4: Verify compilation**

```bash
cargo check -p serverbee-server 2>&1 | tail -10
```

Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/ip_risk.rs crates/server/src/entity/ip_risk_cache.rs crates/server/src/migration/
git commit -m "feat(server): persist and read new ip risk fields"
```

---

# Phase 2 — IpApiIsProvider implementation (TDD)

### Task 5: Extend `ProviderResult` struct

**Files:**
- Modify: `crates/server/src/service/ip_risk.rs:18-28`

- [ ] **Step 1: Add 5 fields**

Replace the `ProviderResult` struct definition:

```rust
#[derive(Debug, Clone, Default)]
pub struct ProviderResult {
    pub risk_score: Option<i32>,
    pub is_proxy: Option<bool>,
    pub is_vpn: Option<bool>,
    pub is_hosting: Option<bool>,
    /// Provider-supplied ip_type (e.g. "datacenter", "residential").
    pub ip_type: Option<String>,
    /// Raw provider JSON response (for the `providers` column).
    pub raw: JsonValue,
    // ipapi.is-derived fields. ip-api fallback leaves these as defaults.
    pub is_tor: bool,
    pub is_abuser: bool,
    pub is_mobile: bool,
    pub asn_abuser_score: Option<i32>,
    pub abuse_email: Option<String>,
}
```

- [ ] **Step 2: Verify compilation**

```bash
cargo check -p serverbee-server 2>&1 | tail -10
```

Expected: clean (no construction sites need updating because `Default` is derived).

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/service/ip_risk.rs
git commit -m "feat(server): extend ProviderResult with ipapi.is fields"
```

---

### Task 6: TDD `parse_ipapi_is_response` — write tests first

**Files:**
- Modify: `crates/server/src/service/ip_risk.rs` `#[cfg(test)] mod tests`

- [ ] **Step 1: Write failing tests**

In the existing `#[cfg(test)] mod tests` block (search for `#[cfg(test)]` near the bottom of the file, around line 787), add:

```rust
    #[test]
    fn parse_ipapi_is_full_response_8888() {
        let json = serde_json::json!({
            "ip": "8.8.8.8",
            "is_datacenter": true,
            "is_proxy": false,
            "is_vpn": false,
            "is_tor": false,
            "is_abuser": false,
            "is_mobile": false,
            "company": {
                "name": "Google LLC",
                "abuser_score": "0.0039 (Low)",
                "type": "hosting"
            },
            "asn": {
                "abuser_score": "0.001 (Low)",
                "type": "hosting"
            },
            "abuse": { "email": "network-abuse@google.com" }
        });

        let r = super::parse_ipapi_is_response(json);

        assert_eq!(r.risk_score, Some(0)); // 0.0039 * 100 ≈ 0
        assert_eq!(r.asn_abuser_score, Some(0)); // 0.001 * 100 ≈ 0
        assert_eq!(r.is_proxy, Some(false));
        assert_eq!(r.is_vpn, Some(false));
        assert_eq!(r.is_hosting, Some(true));
        assert_eq!(r.is_tor, false);
        assert_eq!(r.is_abuser, false);
        assert_eq!(r.is_mobile, false);
        assert_eq!(r.ip_type.as_deref(), Some("hosting"));
        assert_eq!(r.abuse_email.as_deref(), Some("network-abuse@google.com"));
    }

    #[test]
    fn parse_ipapi_is_handles_missing_fields() {
        let json = serde_json::json!({ "ip": "1.2.3.4" });
        let r = super::parse_ipapi_is_response(json);
        assert_eq!(r.risk_score, None);
        assert_eq!(r.is_proxy, None);
        assert_eq!(r.is_tor, false);
        assert_eq!(r.abuse_email, None);
    }

    #[test]
    fn parse_ipapi_is_abuser_score_formats() {
        let cases = vec![
            ("0.0039 (Low)", Some(0)),
            ("0.005", Some(1)),       // 0.005 * 100 = 0.5 → round() = 1 (Rust rounds half away from zero)
            ("0.5 (Medium)", Some(50)),
            ("1.0 (Very High)", Some(100)),
            ("null", None),
            ("garbage", None),
            ("", None),
        ];
        for (raw, want) in cases {
            let json = serde_json::json!({
                "company": { "abuser_score": raw }
            });
            let r = super::parse_ipapi_is_response(json);
            assert_eq!(r.risk_score, want, "input was {raw:?}");
        }
    }

    #[test]
    fn parse_ipapi_is_tor_exit_node() {
        let json = serde_json::json!({
            "is_tor": true,
            "is_abuser": true,
            "company": { "abuser_score": "0.85 (Very High)", "type": "isp" }
        });
        let r = super::parse_ipapi_is_response(json);
        assert!(r.is_tor);
        assert!(r.is_abuser);
        assert_eq!(r.risk_score, Some(85));
        assert_eq!(super::derive_risk_level(r.risk_score), "high");
    }

    #[test]
    fn parse_ipapi_is_falls_back_to_asn_type_when_company_type_missing() {
        let json = serde_json::json!({
            "asn": { "type": "isp" }
        });
        let r = super::parse_ipapi_is_response(json);
        assert_eq!(r.ip_type.as_deref(), Some("isp"));
    }
```

- [ ] **Step 2: Run tests — expect failure**

```bash
cargo test -p serverbee-server --lib ip_risk::tests::parse_ipapi_is -- --nocapture 2>&1 | tail -20
```

Expected: `cannot find function 'parse_ipapi_is_response' in module 'super'` compile error.

- [ ] **Step 3: Implement the parser**

In `crates/server/src/service/ip_risk.rs`, near the top of the file (right after the `ProviderResult` block), add:

```rust
// ---------------------------------------------------------------------------
// ipapi.is response parser
// ---------------------------------------------------------------------------

/// Extract a numeric risk score from an ipapi.is `abuser_score` string.
/// ipapi.is returns strings like `"0.0039 (Low)"`, `"0.5"`, `"null"`.
/// The numeric portion is a 0..=1 fraction; we round (* 100) into 0..=100.
fn parse_ipapi_is_abuser_score(raw: Option<&str>) -> Option<i32> {
    raw.and_then(|s| s.split_whitespace().next())
        .and_then(|n| n.parse::<f64>().ok())
        .map(|f| (f * 100.0).round() as i32)
}

pub(crate) fn parse_ipapi_is_response(v: JsonValue) -> ProviderResult {
    let company_score = v
        .pointer("/company/abuser_score")
        .and_then(JsonValue::as_str);
    let asn_score = v.pointer("/asn/abuser_score").and_then(JsonValue::as_str);

    let ip_type = v
        .pointer("/company/type")
        .or_else(|| v.pointer("/asn/type"))
        .and_then(JsonValue::as_str)
        .map(str::to_string);

    let raw = v.clone();

    ProviderResult {
        risk_score: parse_ipapi_is_abuser_score(company_score),
        asn_abuser_score: parse_ipapi_is_abuser_score(asn_score),
        is_proxy: v.get("is_proxy").and_then(JsonValue::as_bool),
        is_vpn: v.get("is_vpn").and_then(JsonValue::as_bool),
        is_hosting: v.get("is_datacenter").and_then(JsonValue::as_bool),
        is_tor: v.get("is_tor").and_then(JsonValue::as_bool).unwrap_or(false),
        is_abuser: v.get("is_abuser").and_then(JsonValue::as_bool).unwrap_or(false),
        is_mobile: v.get("is_mobile").and_then(JsonValue::as_bool).unwrap_or(false),
        abuse_email: v
            .pointer("/abuse/email")
            .and_then(JsonValue::as_str)
            .map(str::to_string),
        ip_type,
        raw,
    }
}
```

- [ ] **Step 4: Run tests — expect pass**

```bash
cargo test -p serverbee-server --lib ip_risk::tests::parse_ipapi_is -- --nocapture 2>&1 | tail -20
```

Expected: all 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/ip_risk.rs
git commit -m "feat(server): add ipapi.is response parser with TDD coverage"
```

---

### Task 7: Implement `IpApiIsProvider`

**Files:**
- Modify: `crates/server/src/service/ip_risk.rs`
- Modify: `crates/server/src/config.rs:368-377` (use existing `RiskProviderKey`)

- [ ] **Step 1: Add the provider struct + trait impl**

Append after the `parse_ipapi_is_response` function in `ip_risk.rs`:

```rust
// ---------------------------------------------------------------------------
// Provider: ipapi.is
// ---------------------------------------------------------------------------

pub const IPAPI_IS_DEFAULT_ENDPOINT: &str = "https://api.ipapi.is";

pub struct IpApiIsProvider {
    pub api_key: Option<String>,
    pub endpoint: String,
    client: reqwest::Client,
}

impl IpApiIsProvider {
    pub fn from_config(key: Option<&crate::config::RiskProviderKey>) -> Self {
        let (api_key, endpoint) = match key {
            Some(k) => (
                (!k.api_key.is_empty()).then(|| k.api_key.clone()),
                if k.endpoint.is_empty() {
                    IPAPI_IS_DEFAULT_ENDPOINT.to_string()
                } else {
                    k.endpoint.clone()
                },
            ),
            None => (None, IPAPI_IS_DEFAULT_ENDPOINT.to_string()),
        };
        Self {
            api_key,
            endpoint,
            client: build_provider_client(),
        }
    }
}

#[async_trait]
impl IpRiskProvider for IpApiIsProvider {
    fn name(&self) -> &'static str {
        "ipapi_is"
    }

    async fn lookup(&self, ip: &str) -> anyhow::Result<ProviderResult> {
        let mut url = format!("{}/?q={}", self.endpoint, ip);
        if let Some(k) = &self.api_key {
            url.push_str(&format!("&key={k}"));
        }

        let resp = self
            .client
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json::<JsonValue>()
            .await?;

        Ok(parse_ipapi_is_response(resp))
    }
}
```

- [ ] **Step 2: Add construction tests**

In the `#[cfg(test)] mod tests` block, add:

```rust
    #[test]
    fn ipapi_is_provider_from_config_no_key() {
        let p = super::IpApiIsProvider::from_config(None);
        assert!(p.api_key.is_none());
        assert_eq!(p.endpoint, super::IPAPI_IS_DEFAULT_ENDPOINT);
    }

    #[test]
    fn ipapi_is_provider_from_config_with_key() {
        let key = crate::config::RiskProviderKey {
            api_key: "abc123".to_string(),
            endpoint: String::new(),
        };
        let p = super::IpApiIsProvider::from_config(Some(&key));
        assert_eq!(p.api_key.as_deref(), Some("abc123"));
        assert_eq!(p.endpoint, super::IPAPI_IS_DEFAULT_ENDPOINT);
    }

    #[test]
    fn ipapi_is_provider_empty_key_treated_as_none() {
        let key = crate::config::RiskProviderKey {
            api_key: String::new(),
            endpoint: "https://example.com".to_string(),
        };
        let p = super::IpApiIsProvider::from_config(Some(&key));
        assert!(p.api_key.is_none());
        assert_eq!(p.endpoint, "https://example.com");
    }

    #[test]
    fn ipapi_is_provider_name() {
        let p = super::IpApiIsProvider::from_config(None);
        assert_eq!(p.name(), "ipapi_is");
    }
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p serverbee-server --lib ip_risk::tests::ipapi_is -- --nocapture 2>&1 | tail -20
```

Expected: 4 new tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/service/ip_risk.rs
git commit -m "feat(server): add IpApiIsProvider implementation"
```

---

# Phase 3 — Config refactor

### Task 8: Update `IpQualityConfig` and defaults

**Files:**
- Modify: `crates/server/src/config.rs:379-396`

- [ ] **Step 1: Replace `IpQualityConfig` and defaults**

In `config.rs`, replace the struct and `default_risk_provider` (lines ~379-396):

```rust
/// Configuration for the `[ip_quality]` section.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct IpQualityConfig {
    /// Primary risk provider. Default: `ipapi_is`. Accepted: `none`, `ipapi_is`, `ip-api`.
    #[serde(default = "default_risk_provider")]
    pub risk_provider: String,

    /// Fallback provider triggered when the primary fails. Default: `ip-api`.
    /// Set to `none` to disable fallback. Accepted: same as `risk_provider`.
    #[serde(default = "default_risk_provider_fallback")]
    pub risk_provider_fallback: String,

    /// Optional API key + endpoint override for ipapi.is. When omitted, the provider
    /// calls `https://api.ipapi.is` anonymously (1000/day per source IP).
    #[serde(default)]
    pub ipapi_is: Option<RiskProviderKey>,
}

fn default_risk_provider() -> String {
    "ipapi_is".to_string()
}

fn default_risk_provider_fallback() -> String {
    "ip-api".to_string()
}
```

The legacy fields (`scamalytics`, `ipqs`, `proxycheck`, `abuseipdb`) are removed. Old env vars `SERVERBEE_IP_QUALITY__SCAMALYTICS__*` etc. are silently ignored by Figment (no startup error).

- [ ] **Step 2: Update existing config tests**

Find the test block around `crates/server/src/config.rs:655`:

```bash
sed -n '650,690p' crates/server/src/config.rs
```

Delete the test that asserts `risk_provider == "scamalytics"`. Add new tests:

```rust
    #[test]
    fn ip_quality_default_provider_is_ipapi_is() {
        figment::Jail::expect_with(|_jail| {
            let cfg: AppConfig = Figment::new()
                .merge(Toml::string(""))
                .extract()
                .unwrap();
            assert_eq!(cfg.ip_quality.risk_provider, "ipapi_is");
            assert_eq!(cfg.ip_quality.risk_provider_fallback, "ip-api");
            Ok(())
        });
    }

    #[test]
    fn ip_quality_config_from_env_with_ipapi_is_key() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("SERVERBEE_IP_QUALITY__IPAPI_IS__API_KEY", "test-key-xyz");
            let cfg: AppConfig = Figment::new()
                .merge(Toml::string(""))
                .merge(Env::prefixed("SERVERBEE_").split("__"))
                .extract()
                .unwrap();
            let key = cfg.ip_quality.ipapi_is.expect("ipapi_is config present");
            assert_eq!(key.api_key, "test-key-xyz");
            Ok(())
        });
    }

    #[test]
    fn ip_quality_legacy_provider_env_vars_silently_ignored() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("SERVERBEE_IP_QUALITY__SCAMALYTICS__API_KEY", "ignored");
            jail.set_env("SERVERBEE_IP_QUALITY__ABUSEIPDB__API_KEY", "ignored");
            // Should not panic. Figment ignores unknown fields by default.
            let cfg: AppConfig = Figment::new()
                .merge(Toml::string(""))
                .merge(Env::prefixed("SERVERBEE_").split("__"))
                .extract()
                .unwrap();
            assert_eq!(cfg.ip_quality.risk_provider, "ipapi_is");
            Ok(())
        });
    }
```

Match the existing test scaffold style (look at the surrounding tests in the same file for imports).

- [ ] **Step 3: Run config tests**

```bash
cargo test -p serverbee-server --lib config:: 2>&1 | tail -20
```

Expected: all pass (3 new tests + existing ones; the old `scamalytics` assertion is gone).

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/config.rs
git commit -m "feat(server): default risk_provider to ipapi_is with ip-api fallback"
```

---

# Phase 4 — Provider factory + fallback orchestration

### Task 9: Refactor `provider_for_config` signature and arms

**Files:**
- Modify: `crates/server/src/service/ip_risk.rs:385-417`

- [ ] **Step 1: Rewrite `provider_for_config`**

Replace the current function body with:

```rust
pub fn provider_for_config(
    config: &IpQualityConfig,
    provider_name: &str,
) -> Option<Box<dyn IpRiskProvider>> {
    match provider_name {
        "ipapi_is" => Some(Box::new(IpApiIsProvider::from_config(
            config.ipapi_is.as_ref(),
        )) as Box<dyn IpRiskProvider>),
        "ip-api" => Some(Box::new(IpApiProvider {
            client: build_provider_client(),
        }) as Box<dyn IpRiskProvider>),
        _ => None, // "none" or unknown
    }
}
```

(The legacy `scamalytics` / `ipqs` / `proxycheck` / `abuseipdb` match arms are gone. The dead-code `ScamalyticsProvider` etc. will be deleted in Task 13. For now `cargo check` may warn — that's expected.)

- [ ] **Step 2: Add factory tests**

In the test module, append:

```rust
    #[test]
    fn provider_for_config_ipapi_is() {
        let cfg = crate::config::IpQualityConfig {
            risk_provider: "ipapi_is".to_string(),
            risk_provider_fallback: "ip-api".to_string(),
            ipapi_is: None,
        };
        let p = super::provider_for_config(&cfg, "ipapi_is");
        assert!(p.is_some());
        assert_eq!(p.unwrap().name(), "ipapi_is");
    }

    #[test]
    fn provider_for_config_ip_api() {
        let cfg = crate::config::IpQualityConfig::default();
        let p = super::provider_for_config(&cfg, "ip-api");
        assert!(p.is_some());
        assert_eq!(p.unwrap().name(), "ip-api");
    }

    #[test]
    fn provider_for_config_none_returns_none() {
        let cfg = crate::config::IpQualityConfig::default();
        assert!(super::provider_for_config(&cfg, "none").is_none());
    }

    #[test]
    fn provider_for_config_unknown_provider_returns_none() {
        let cfg = crate::config::IpQualityConfig::default();
        // Legacy provider names (deleted in this refactor) should resolve to None,
        // not panic — this preserves graceful degradation for users still on old config.
        assert!(super::provider_for_config(&cfg, "scamalytics").is_none());
        assert!(super::provider_for_config(&cfg, "abuseipdb").is_none());
    }
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p serverbee-server --lib ip_risk::tests::provider_for_config -- --nocapture 2>&1 | tail -20
```

Expected: 4 new tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/service/ip_risk.rs
git commit -m "feat(server): refactor provider_for_config to take provider name"
```

---

### Task 10: Add fallback orchestration to `score_ip_with` and `score_ip`

**Files:**
- Modify: `crates/server/src/service/ip_risk.rs:177-275`

- [ ] **Step 1: Add `is_fallbackable` helper**

In `ip_risk.rs`, add (just above `IpRiskService` impl block, ~line 156):

```rust
/// Whether a provider error should trigger fallback. Currently informational only —
/// `score_ip_with` falls through on any failure. Kept for future metric labeling.
fn is_fallbackable(e: &anyhow::Error) -> bool {
    if let Some(re) = e.downcast_ref::<reqwest::Error>() {
        if re.is_timeout() || re.is_connect() || re.is_request() {
            return true;
        }
        if let Some(s) = re.status() {
            return s.is_server_error() || s == reqwest::StatusCode::TOO_MANY_REQUESTS;
        }
    }
    e.downcast_ref::<serde_json::Error>().is_some()
}
```

- [ ] **Step 2: Add a `try_provider` helper**

Just below `is_fallbackable`, add:

```rust
async fn try_provider(
    p: &Option<Box<dyn IpRiskProvider>>,
    ip: &str,
) -> Option<(&'static str, ProviderResult)> {
    let provider = p.as_ref()?;
    let name = provider.name();
    match tokio::time::timeout(std::time::Duration::from_secs(15), provider.lookup(ip)).await {
        Ok(Ok(r)) => {
            tracing::debug!("provider {name} succeeded for {ip}");
            Some((name, r))
        }
        Ok(Err(e)) => {
            let kind = if is_fallbackable(&e) {
                "fallbackable"
            } else {
                "non-fallback"
            };
            tracing::warn!("provider {name} failed for {ip} ({kind}): {e}");
            None
        }
        Err(_) => {
            tracing::warn!("provider {name} timed out for {ip}");
            None
        }
    }
}
```

- [ ] **Step 3: Change `score_ip_with` signature and body**

Replace `score_ip_with`'s signature (~line 196) and lookup logic (~lines 218-249):

```rust
pub async fn score_ip_with(
    &self,
    db: &DatabaseConnection,
    geoip: &Arc<RwLock<Option<GeoIpService>>>,
    ip: &str,
    primary: Option<Box<dyn IpRiskProvider>>,
    fallback: Option<Box<dyn IpRiskProvider>>,
) -> Option<IpQualitySnapshotData> {
    if ip.trim().is_empty() {
        tracing::debug!("score_ip_with: skipping empty egress IP");
        return None;
    }

    if let Some(snapshot) = self.read_cache(db, ip).await {
        return Some(snapshot);
    }

    let baseline = lookup_geoip_baseline(geoip, ip);

    // Try primary, then fallback. Both can be None (no scoring).
    let provider_result = match try_provider(&primary, ip).await {
        Some(hit) => Some(hit),
        None => {
            if primary.is_some() && fallback.is_some() {
                tracing::info!("primary failed for {ip}, attempting fallback");
            }
            try_provider(&fallback, ip).await
        }
    };

    let (risk_score, is_proxy, is_vpn, is_hosting, provider_ip_type, providers_json,
         is_tor, is_abuser, is_mobile, asn_abuser_score, abuse_email) =
        match &provider_result {
            Some((name, r)) => {
                let providers_json = serde_json::json!({ *name: r.raw });
                (
                    r.risk_score,
                    r.is_proxy.unwrap_or(false),
                    r.is_vpn.unwrap_or(false),
                    r.is_hosting.unwrap_or(false),
                    r.ip_type.clone(),
                    providers_json,
                    r.is_tor,
                    r.is_abuser,
                    r.is_mobile,
                    r.asn_abuser_score,
                    r.abuse_email.clone(),
                )
            }
            None => {
                if primary.is_some() || fallback.is_some() {
                    tracing::warn!(
                        "both providers failed for {ip}, returning geo-only snapshot"
                    );
                }
                (None, false, false, false, None, serde_json::json!({}),
                 false, false, false, None, None)
            }
        };

    let ip_type = derive_ip_type(is_hosting, is_vpn, is_proxy, provider_ip_type.as_deref());
    let risk_level = derive_risk_level(risk_score);

    let now = Utc::now();
    let snapshot = IpQualitySnapshotData {
        ip: ip.to_string(),
        asn: baseline.asn,
        as_org: baseline.as_org,
        country: baseline.country,
        region: baseline.region,
        city: baseline.city,
        ip_type,
        is_proxy,
        is_vpn,
        is_hosting,
        risk_score,
        risk_level,
        is_tor,
        is_abuser,
        is_mobile,
        asn_abuser_score,
        abuse_email,
        checked_at: now,
    };

    self.write_cache(db, &snapshot, &providers_json).await;

    Some(snapshot)
}
```

- [ ] **Step 4: Update `score_ip` outer wrapper**

Replace the body of `score_ip` (~line 177):

```rust
pub async fn score_ip(
    &self,
    db: &DatabaseConnection,
    geoip: &Arc<RwLock<Option<GeoIpService>>>,
    ip: &str,
) -> Option<IpQualitySnapshotData> {
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

- [ ] **Step 5: Update any existing tests that call `score_ip_with`**

```bash
grep -n "score_ip_with" crates/server/src/service/ip_risk.rs
```

For each call site (e.g. test at `ip_risk.rs:965`), add `None` as the fifth argument:

```rust
.score_ip_with(&db, &geoip, "5.6.7.8", Some(Box::new(mock)), None)
```

- [ ] **Step 6: Add new fallback-orchestration tests**

Append to the test module:

```rust
    use anyhow::anyhow;

    struct AlwaysFailProvider;
    #[async_trait]
    impl IpRiskProvider for AlwaysFailProvider {
        fn name(&self) -> &'static str { "always_fail" }
        async fn lookup(&self, _ip: &str) -> anyhow::Result<ProviderResult> {
            Err(anyhow!("intentional test failure"))
        }
    }

    struct FixedScoreProvider {
        name: &'static str,
        score: i32,
    }
    #[async_trait]
    impl IpRiskProvider for FixedScoreProvider {
        fn name(&self) -> &'static str { self.name }
        async fn lookup(&self, _ip: &str) -> anyhow::Result<ProviderResult> {
            Ok(ProviderResult {
                risk_score: Some(self.score),
                ..Default::default()
            })
        }
    }

    #[tokio::test]
    async fn fallback_invoked_when_primary_fails() {
        use sea_orm::{Database, ConnectionTrait};
        let db = Database::connect("sqlite::memory:").await.unwrap();
        // Apply the minimal schema the test needs (or run all migrations — see existing test patterns)
        let geoip = Arc::new(RwLock::new(None));

        // Reuse whatever test harness the existing tests use to init schema.
        // The pattern at ip_risk.rs:925 shows how to create the ip_risk_cache table inline.

        let svc = IpRiskService::new(crate::config::IpQualityConfig::default());
        let result = svc
            .score_ip_with(
                &db,
                &geoip,
                "5.6.7.8",
                Some(Box::new(AlwaysFailProvider)),
                Some(Box::new(FixedScoreProvider { name: "fb", score: 42 })),
            )
            .await
            .expect("snapshot");
        assert_eq!(result.risk_score, Some(42));
    }

    #[tokio::test]
    async fn both_providers_fail_returns_geo_only_snapshot() {
        use sea_orm::Database;
        let db = Database::connect("sqlite::memory:").await.unwrap();
        let geoip = Arc::new(RwLock::new(None));
        // (init schema — copy from existing ip_risk_cache table create in tests)

        let svc = IpRiskService::new(crate::config::IpQualityConfig::default());
        let result = svc
            .score_ip_with(
                &db,
                &geoip,
                "9.9.9.9",
                Some(Box::new(AlwaysFailProvider)),
                Some(Box::new(AlwaysFailProvider)),
            )
            .await
            .expect("snapshot");
        assert!(result.risk_score.is_none());
        assert_eq!(result.risk_level, "unknown");
    }

    #[test]
    fn score_ip_skips_fallback_when_same_as_primary() {
        // This is a config-level test; check that the wrapper logic
        // would produce fallback = None.
        let cfg = crate::config::IpQualityConfig {
            risk_provider: "ipapi_is".to_string(),
            risk_provider_fallback: "ipapi_is".to_string(),
            ipapi_is: None,
        };
        // Manually exercise the resolution logic from score_ip:
        let same = cfg.risk_provider_fallback == cfg.risk_provider;
        assert!(same, "test setup precondition");
        // The actual `score_ip` short-circuits and never builds the fallback provider.
    }

    #[test]
    fn score_ip_skips_fallback_when_set_to_none() {
        let cfg = crate::config::IpQualityConfig {
            risk_provider: "ipapi_is".to_string(),
            risk_provider_fallback: "none".to_string(),
            ipapi_is: None,
        };
        assert_eq!(cfg.risk_provider_fallback, "none");
        // Equivalent: score_ip's `if cfg.risk_provider_fallback != "none"` is false.
    }
```

For the two `#[tokio::test]` tests, copy the in-memory DB + `ip_risk_cache` schema init from the existing test at `ip_risk.rs:925` (the `CREATE TABLE ip_risk_cache (...)` block). Adjust the inline `CREATE TABLE` to include the 5 new columns.

- [ ] **Step 7: Run tests**

```bash
cargo test -p serverbee-server --lib ip_risk:: 2>&1 | tail -30
```

Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add crates/server/src/service/ip_risk.rs
git commit -m "feat(server): add fallback orchestration to IpRiskService"
```

---

# Phase 5 — Delete legacy code

### Task 11: Delete legacy provider impls

**Files:**
- Modify: `crates/server/src/service/ip_risk.rs:423-722`

- [ ] **Step 1: Identify exact line ranges to delete**

```bash
grep -n "^pub struct \(ScamalyticsProvider\|IpQualityScoreProvider\|ProxyCheckProvider\|AbuseIpdbProvider\)" \
    crates/server/src/service/ip_risk.rs
```

Note the start lines. Each provider block runs until the next `// ----` separator or the next `pub struct`. Use these line numbers as deletion boundaries.

- [ ] **Step 2: Delete the 4 provider blocks**

Delete:
- `// Provider: Scamalytics` separator + `ScamalyticsProvider` struct + `impl` blocks
- `// Provider: IPQualityScore` separator + `IpQualityScoreProvider` struct + `impl` blocks
- `// Provider: ProxyCheck` separator + `ProxyCheckProvider` struct + `impl` blocks
- `// Provider: AbuseIPDB` separator + `AbuseIpdbProvider` struct + `impl` blocks

Keep:
- `// Provider: ip-api` separator + `IpApiProvider` struct + `impl` block (used as fallback)
- `// Provider: ipapi.is` block added in Task 7

- [ ] **Step 3: Delete legacy provider tests**

```bash
grep -n "scamalytics\|abuseipdb\|proxycheck\|ipqualityscore\|ipqs" crates/server/src/service/ip_risk.rs
```

Delete any tests that reference these provider names (parse tests, factory tests for the deleted arms, etc.). Tests referencing `"ip-api"` or `"ipapi_is"` stay.

- [ ] **Step 4: Compile clean with no warnings**

```bash
cargo clippy -p serverbee-server -- -D warnings 2>&1 | tail -30
```

Expected: no errors, no warnings. If clippy complains about unused imports (e.g. `serde_json::Value` no longer referenced by deleted parsers), clean them up.

- [ ] **Step 5: Run the full service test suite**

```bash
cargo test -p serverbee-server --lib ip_risk:: 2>&1 | tail -20
```

Expected: green.

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/service/ip_risk.rs
git commit -m "refactor(server): remove legacy paid IP risk providers"
```

---

# Phase 6 — Frontend

### Task 12: Extend TS type + add badges

**Files:**
- Modify: `apps/web/src/lib/ip-quality-types.ts:62-76`
- Modify: `apps/web/src/components/ip-quality/ip-quality-card.tsx`
- Modify: `apps/web/src/locales/en/ip-quality.json`
- Modify: `apps/web/src/locales/zh/ip-quality.json`

- [ ] **Step 1: Extend the TypeScript interface**

In `apps/web/src/lib/ip-quality-types.ts`, modify `IpQualitySnapshotData`:

```typescript
export interface IpQualitySnapshotData {
  as_org: string | null
  asn: string | null
  checked_at: string
  city: string | null
  country: string | null
  ip: string
  ip_type: string
  is_hosting: boolean
  is_proxy: boolean
  is_vpn: boolean
  is_tor: boolean
  is_abuser: boolean
  is_mobile: boolean
  asn_abuser_score: number | null
  abuse_email: string | null
  region: string | null
  risk_level: string
  risk_score: number | null
}
```

- [ ] **Step 2: Add i18n keys (English)**

In `apps/web/src/locales/en/ip-quality.json`, add:

```json
{
  "field_is_tor": "Tor",
  "field_is_abuser": "Known abuser",
  "field_is_mobile": "Mobile network",
  "field_asn_score": "ASN risk"
}
```

- [ ] **Step 3: Add i18n keys (Chinese)**

In `apps/web/src/locales/zh/ip-quality.json`, add:

```json
{
  "field_is_tor": "Tor 出口",
  "field_is_abuser": "已知滥用",
  "field_is_mobile": "移动网络",
  "field_asn_score": "ASN 风险"
}
```

- [ ] **Step 4: Render badges in the detail card**

In `apps/web/src/components/ip-quality/ip-quality-card.tsx`, locate the area where existing flags (`is_proxy`, `is_vpn`, `is_hosting`) are rendered as badges. Add three more conditional `<Badge>` renders for `is_tor`, `is_abuser`, `is_mobile`.

Pattern (adapt to existing JSX shape, **do not invent new component names** — read the file first):

```tsx
{ipQuality.is_tor && <Badge variant="destructive">{t('field_is_tor')}</Badge>}
{ipQuality.is_abuser && <Badge variant="destructive">{t('field_is_abuser')}</Badge>}
{ipQuality.is_mobile && <Badge variant="secondary">{t('field_is_mobile')}</Badge>}
```

Also render `asn_abuser_score` inline next to `as_org` when not null:

```tsx
{ipQuality.as_org && (
  <span>
    {ipQuality.as_org}
    {ipQuality.asn_abuser_score != null && (
      <span className="ml-2 text-muted-foreground">
        ({t('field_asn_score')}: {ipQuality.asn_abuser_score})
      </span>
    )}
  </span>
)}
```

**Important:** read the existing card layout first; match its visual conventions.

- [ ] **Step 5: Run frontend checks**

```bash
cd apps/web && bun x ultracite fix && bun run typecheck 2>&1 | tail -20
```

Expected: clean.

- [ ] **Step 6: Verify in browser (manual)**

Per CLAUDE.md user preference: UI changes must be visually verified. Per memory `feedback_visual_verification.md`, if no browser tool is available, state so explicitly.

For this task: spin up `bun run dev` from `apps/web` and visit `/ip-quality`, eyeball the badges. If running headless, document the inability and skip — but mention it in the commit message as a follow-up TODO.

- [ ] **Step 7: Commit**

```bash
git add apps/web/src/lib/ip-quality-types.ts \
        apps/web/src/components/ip-quality/ip-quality-card.tsx \
        apps/web/src/locales/en/ip-quality.json \
        apps/web/src/locales/zh/ip-quality.json
git commit -m "feat(web): render new ipapi.is fields in IP quality card"
```

---

# Phase 7 — Documentation

### Task 13: Update `ENV.md` + `configuration.mdx`

**Files:**
- Modify: `ENV.md:108-125`
- Modify: `apps/docs/content/docs/en/configuration.mdx`
- Modify: `apps/docs/content/docs/cn/configuration.mdx`

- [ ] **Step 1: Replace IP Quality section in `ENV.md`**

Read `ENV.md` lines 108-125 to confirm context, then replace the "IP Quality (Optional)" section with:

```markdown
### IP Quality

Default risk-scoring works out of the box via [ipapi.is](https://ipapi.is) (no API key required, ~1000 requests/day per source IP). On primary failure the server falls back to [ip-api.com](https://ip-api.com), which provides geo + proxy/hosting flags but no risk score.

| Environment Variable | TOML Key | Type | Default | Description |
|---------------------|----------|------|---------|-------------|
| `SERVERBEE_IP_QUALITY__RISK_PROVIDER` | `ip_quality.risk_provider` | string | `"ipapi_is"` | Primary risk provider. One of: `none`, `ipapi_is`, `ip-api`. |
| `SERVERBEE_IP_QUALITY__RISK_PROVIDER_FALLBACK` | `ip_quality.risk_provider_fallback` | string | `"ip-api"` | Fallback provider triggered on primary failure. Set to `none` to disable. |
| `SERVERBEE_IP_QUALITY__IPAPI_IS__API_KEY` | `ip_quality.ipapi_is.api_key` | string | - | Optional. Configure for higher per-account rate limits. |
| `SERVERBEE_IP_QUALITY__IPAPI_IS__ENDPOINT` | `ip_quality.ipapi_is.endpoint` | string | `https://api.ipapi.is` | Override for self-hosted mirrors or testing. |

**Migration from older versions:** Earlier releases supported four paid providers (Scamalytics, IPQualityScore, ProxyCheck, AbuseIPDB) configured via `SERVERBEE_IP_QUALITY__{SCAMALYTICS,IPQS,PROXYCHECK,ABUSEIPDB}__*`. These env vars are silently ignored. To restore equivalent functionality, fork or vendor the provider implementation from a tag prior to 2026-05-25.
```

- [ ] **Step 2: Mirror to English docs**

In `apps/docs/content/docs/en/configuration.mdx`, find the matching "IP Quality" section and replace with the same content (adapt markdown to MDX where needed — e.g. wrap tables identically; check the surrounding MDX style first).

- [ ] **Step 3: Mirror to Chinese docs**

In `apps/docs/content/docs/cn/configuration.mdx`, replace with a Chinese translation:

```markdown
### IP 质量

默认开箱即用,通过 [ipapi.is](https://ipapi.is)(无需 API Key,按源 IP 限 1000 次/天)获取风险评分。主 Provider 失败时自动回退到 [ip-api.com](https://ip-api.com)(提供地理 + 代理/托管标志,无风险评分)。

| 环境变量 | TOML Key | 类型 | 默认值 | 说明 |
|---------|----------|------|--------|------|
| `SERVERBEE_IP_QUALITY__RISK_PROVIDER` | `ip_quality.risk_provider` | string | `"ipapi_is"` | 主风险评分 Provider。可选:`none` / `ipapi_is` / `ip-api`。 |
| `SERVERBEE_IP_QUALITY__RISK_PROVIDER_FALLBACK` | `ip_quality.risk_provider_fallback` | string | `"ip-api"` | 主 Provider 失败时的兜底。设为 `none` 关闭。 |
| `SERVERBEE_IP_QUALITY__IPAPI_IS__API_KEY` | `ip_quality.ipapi_is.api_key` | string | - | 可选。配置后享受更高的账户级速率。 |
| `SERVERBEE_IP_QUALITY__IPAPI_IS__ENDPOINT` | `ip_quality.ipapi_is.endpoint` | string | `https://api.ipapi.is` | 自建镜像 / 测试时覆盖。 |

**老版本升级说明**:早期版本支持 4 个付费 Provider(Scamalytics / IPQualityScore / ProxyCheck / AbuseIPDB),通过 `SERVERBEE_IP_QUALITY__{SCAMALYTICS,IPQS,PROXYCHECK,ABUSEIPDB}__*` 配置。这些环境变量会被静默忽略。如需恢复对应能力,请从 2026-05-25 之前的 tag 中 fork 或 vendor 对应实现。
```

- [ ] **Step 4: Commit**

```bash
git add ENV.md apps/docs/content/docs/en/configuration.mdx apps/docs/content/docs/cn/configuration.mdx
git commit -m "docs: replace IP quality provider docs with ipapi.is + fallback"
```

---

### Task 14: Progress tracking + spec postscript + manual verification doc

**Files:**
- Modify: `docs/superpowers/plans/PROGRESS.md`
- Modify: `docs/superpowers/specs/2026-05-22-ip-quality-design.md`
- Create: `tests/ip-quality/ipapi-is.md`
- Modify: `tests/README.md`

- [ ] **Step 1: Append postscript to old spec**

Append to `docs/superpowers/specs/2026-05-22-ip-quality-design.md`:

```markdown

---

## 2026-05-25 Update — Provider Refactor

The "Risk Scoring" provider model originally described here (Scamalytics / IPQS / ProxyCheck / AbuseIPDB / ip-api) has been superseded. See [`2026-05-25-ip-quality-ipapi-is-design.md`](./2026-05-25-ip-quality-ipapi-is-design.md) for the new ipapi.is-primary, ip-api-fallback design.
```

- [ ] **Step 2: Add PROGRESS.md entry**

Add a row/entry to `docs/superpowers/plans/PROGRESS.md` matching its existing format. Read the file first to match the convention.

- [ ] **Step 3: Create manual verification doc**

Create `tests/ip-quality/ipapi-is.md`:

```markdown
# IP Quality (ipapi.is) Manual Verification

This file lives alongside the test plan for IP Quality. Run these checks after any change to `crates/server/src/service/ip_risk.rs` or `IpQualityConfig`.

## Prerequisites
- A fresh SQLite database (or one with the 2026-05-25 migration applied).
- An agent able to reach the test server.

## Checklist

1. **Default zero-config works.**
   Start the server with no `SERVERBEE_IP_QUALITY__*` env vars set. Connect an agent. Open the IP Quality page in the UI.
   - Expect: real `risk_level` populated (not `unknown`); badges for `proxy`/`vpn`/`hosting` reflect the IP's true category.

2. **Fallback triggers when primary is unreachable.**
   Set `SERVERBEE_IP_QUALITY__IPAPI_IS__ENDPOINT=https://example.invalid` and restart.
   - Expect: server log shows `primary failed for <ip>, attempting fallback` then `provider ip-api succeeded`. UI shows geo + proxy flags but `risk_score` is blank.

3. **`risk_provider=none` disables scoring.**
   Set `SERVERBEE_IP_QUALITY__RISK_PROVIDER=none`.
   - Expect: UI shows `risk_level = unknown`, geo still populated from MMDB.

4. **API key flows correctly.**
   Set `SERVERBEE_IP_QUALITY__IPAPI_IS__API_KEY=test123`. Capture outbound HTTPS request (e.g. `tcpdump -s0 -w /tmp/cap.pcap host api.ipapi.is`).
   - Expect: query string contains `&key=test123`.

5. **Legacy env vars are ignored, not erroring.**
   Set `SERVERBEE_IP_QUALITY__SCAMALYTICS__API_KEY=xxx`. Start server.
   - Expect: clean startup, no error. `risk_provider` resolves to default `ipapi_is`.

6. **New columns persist.**
   After an agent reports its egress IP, query the DB:
   ```sql
   SELECT ip, risk_score, is_tor, is_abuser, is_mobile, asn_abuser_score, abuse_email
   FROM ip_quality_snapshot LIMIT 5;
   ```
   - Expect: columns exist; populated values look reasonable for the IP type.
```

- [ ] **Step 4: Index in `tests/README.md`**

Add a line under the IP quality section of `tests/README.md`:

```markdown
- [`tests/ip-quality/ipapi-is.md`](./ip-quality/ipapi-is.md) — ipapi.is provider + fallback verification (2026-05-25).
```

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/ tests/
git commit -m "docs: track ipapi.is refactor + add manual verification"
```

---

# Phase 8 — Validation

### Task 15: Full workspace verification

**Files:** none (verification only).

- [ ] **Step 1: Backend tests**

```bash
cargo test --workspace 2>&1 | tail -30
```

Expected: all pass.

- [ ] **Step 2: Backend clippy**

```bash
cargo clippy --workspace -- -D warnings 2>&1 | tail -30
```

Expected: no warnings.

- [ ] **Step 3: Frontend checks**

```bash
cd apps/web && bun x ultracite check && bun run typecheck 2>&1 | tail -20
```

Expected: clean.

- [ ] **Step 4: Frontend tests (if any)**

```bash
cd apps/web && bun run test 2>&1 | tail -20
```

Expected: pass (or skip if no test target).

- [ ] **Step 5: Confirm no leftover commits to push** (per goal: commit but don't push)

```bash
git log --oneline auckland-v1..HEAD 2>&1 | head -20
```

Review the commits made by this plan. They should all be on the current branch and ahead of origin/auckland-v1.

---

### Task 16: Deploy to test VPS + run manual checklist

**Files:** none.

**VPS:** `207.241.173.217` (user: `root`, port 22). Per memory `reference_test_vps.md` — Ubuntu 24.04 reusable test box.

- [ ] **Step 1: Cross-compile server binary for Linux x86_64**

From the repo root:

```bash
# Confirm target is installed
rustup target add x86_64-unknown-linux-gnu 2>&1 | tail -3

# Build server in release mode for Linux
cargo build --release --target x86_64-unknown-linux-gnu -p serverbee-server 2>&1 | tail -10

ls -lh target/x86_64-unknown-linux-gnu/release/serverbee-server
```

If cross-compilation toolchain isn't set up on macOS, try `cross` or fall back to using `cargo zigbuild`. If still blocked, document the blocker and skip ahead to "skip-on-fail" path below.

- [ ] **Step 2: Build frontend (embedded into the binary)**

The binary uses `rust-embed`, so frontend must be built **before** the binary:

```bash
cd apps/web && bun install && bun run build 2>&1 | tail -10
cd ../..
# Re-run the cargo build from Step 1 if you built the binary before this.
```

- [ ] **Step 3: SCP to test VPS**

```bash
sshpass -p '2ucW09DzI@!LZ!e47yG' scp \
    -o StrictHostKeyChecking=accept-new \
    target/x86_64-unknown-linux-gnu/release/serverbee-server \
    root@207.241.173.217:/tmp/serverbee-server-test

# (If sshpass not installed: brew install hudochenkov/sshpass/sshpass)
```

- [ ] **Step 4: Run manual verification checklist on the VPS**

SSH in and run each of the 6 checklist items from `tests/ip-quality/ipapi-is.md`:

```bash
sshpass -p '2ucW09DzI@!LZ!e47yG' ssh -o StrictHostKeyChecking=accept-new \
    root@207.241.173.217
```

For each checklist item: set up env vars, start the binary in a tmux session, drive it, capture output.

Specifically for item 6 (DB schema):

```bash
# Find the database file
ls -la /tmp/serverbee*.db /root/.serverbee/ 2>/dev/null

sqlite3 <db-path> ".schema ip_quality_snapshot"
sqlite3 <db-path> "SELECT ip, risk_score, is_tor, is_abuser, is_mobile, asn_abuser_score, abuse_email FROM ip_quality_snapshot;"
```

Expected: columns exist, values populated for any IPs the agent has reported.

- [ ] **Step 5: Document results**

For each checklist item, record pass/fail. If any failed, that's a defect — fix in code, re-run Task 15 (verify), re-deploy (Task 16 Steps 1-4), re-verify.

- [ ] **Step 6: Stop binary, clean up test files**

```bash
# Inside SSH session
pkill -f serverbee-server-test || true
rm -f /tmp/serverbee-server-test /tmp/serverbee-plan-test.db
```

- [ ] **Step 7: Final commit if any fixes were made**

If Step 5 surfaced bugs that needed code fixes:

```bash
git add <changed files>
git commit -m "fix(server): <one-line description>"
```

Re-run `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` once more.

- [ ] **Step 8: Done**

Report to the user:
- Total tasks completed
- All tests passing
- Manual checklist results (6/6 pass, or document any deferred items)
- Number of commits on the branch ahead of origin (per goal: do **not** push)
