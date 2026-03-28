# Environment Variables Cleanup & Documentation Reorganization

**Date:** 2026-03-28
**Scope:** Documentation only. No Rust code changes.

## Problem

1. **Phantom variable**: `SERVERBEE_GEOIP__ENABLED` is documented but does not exist in code. `GeoIpConfig` only has `mmdb_path`; GeoIP is implicitly enabled when `mmdb_path` is non-empty. This phantom variable appears in **10 locations** across the repo:
   - `ENV.md` (env var table)
   - `apps/docs/content/docs/en/configuration.mdx` (env var table + TOML reference)
   - `apps/docs/content/docs/cn/configuration.mdx` (env var table + TOML reference + TOML example)
   - `apps/docs/content/docs/en/server.mdx` (env var example)
   - `apps/docs/content/docs/cn/server.mdx` (env var example)
   - `apps/docs/content/docs/cn/deployment.mdx` (docker-compose example)
   - `deploy/railway/README.md` (env var table)
   - `deploy/railway/Dockerfile` (commented-out ENV line)
   - `README.md` (env var example)
   - `README.zh-CN.md` (env var example)
2. **Missing documentation** for 6 variables that exist in code:
   - `SERVERBEE_RETENTION__SERVICE_MONITOR_DAYS` (default 30) — missing from all docs
   - `SERVERBEE_IP_CHANGE__ENABLED` (default true) — missing from en/cn configuration.mdx
   - `SERVERBEE_IP_CHANGE__CHECK_EXTERNAL_IP` (default false) — missing from en/cn configuration.mdx
   - `SERVERBEE_IP_CHANGE__EXTERNAL_IP_URL` (default `https://api.ipify.org`) — missing from en/cn configuration.mdx
   - `SERVERBEE_IP_CHANGE__INTERVAL_SECS` (default 300) — missing from en/cn configuration.mdx
   - `SERVERBEE_OAUTH__OIDC__SCOPES` (default `["openid","email","profile"]`) — missing from ENV.md
3. **Flat structure**: env vars are listed in a single flat table, making it hard for users to identify which ones they actually need to configure.

## Design

### Decision: Remove `SERVERBEE_GEOIP__ENABLED`

Remove from **all documentation** across the repo. The implicit behavior ("provide `mmdb_path` → GeoIP is enabled") is the cleanest UX. Update all `GEOIP__MMDB_PATH` descriptions to explicitly state "non-empty path enables GeoIP".

### Decision: `IP_CHANGE__*` Classification

`IP_CHANGE__ENABLED` and `IP_CHANGE__CHECK_EXTERNAL_IP` are user-facing feature toggles → classify as **Common**. `IP_CHANGE__EXTERNAL_IP_URL` and `IP_CHANGE__INTERVAL_SECS` are tuning details with sensible defaults → classify as **Internal**.

### Fix Checklist

| # | Issue | Action | Files |
|---|-------|--------|-------|
| 1 | `GEOIP__ENABLED` phantom (10 locations) | Delete all references; update `GEOIP__MMDB_PATH` description to "non-empty = enabled" | ENV.md, en/cn configuration.mdx, en/cn server.mdx, cn/deployment.mdx, deploy/railway/README.md, deploy/railway/Dockerfile, README.md, README.zh-CN.md |
| 2 | `RETENTION__SERVICE_MONITOR_DAYS` missing | Add to retention tables (default 30) | ENV.md, en/cn configuration.mdx |
| 3 | `IP_CHANGE__*` missing from mdx | Add 4 vars to Agent sections (2 as Common, 2 as Internal) and TOML reference | en/cn configuration.mdx |
| 4 | `OAUTH__OIDC__SCOPES` missing from ENV.md | Add to OAuth table | ENV.md |

### Layered Documentation Structure

All three primary docs (ENV.md, en configuration.mdx, cn configuration.mdx) adopt the same classification:

#### ENV.md Structure

```
# ServerBee Environment Variables

## Server

### Essential
  SERVERBEE_ADMIN__PASSWORD
  SERVERBEE_SERVER__LISTEN

### Common
  SERVERBEE_SERVER__DATA_DIR
  SERVERBEE_AUTH__AUTO_DISCOVERY_KEY
  SERVERBEE_ADMIN__USERNAME
  SERVERBEE_SERVER__TRUSTED_PROXIES
  SERVERBEE_SCHEDULER__TIMEZONE
  SERVERBEE_LOG__LEVEL
  SERVERBEE_LOG__FILE

### OAuth (Optional)
  SERVERBEE_OAUTH__BASE_URL
  SERVERBEE_OAUTH__ALLOW_REGISTRATION
  SERVERBEE_OAUTH__GITHUB__CLIENT_ID
  SERVERBEE_OAUTH__GITHUB__CLIENT_SECRET
  SERVERBEE_OAUTH__GOOGLE__CLIENT_ID
  SERVERBEE_OAUTH__GOOGLE__CLIENT_SECRET
  SERVERBEE_OAUTH__OIDC__ISSUER_URL
  SERVERBEE_OAUTH__OIDC__CLIENT_ID
  SERVERBEE_OAUTH__OIDC__CLIENT_SECRET
  SERVERBEE_OAUTH__OIDC__SCOPES

### GeoIP (Optional)
  SERVERBEE_GEOIP__MMDB_PATH

### Data Retention (Tuning)
  12 RETENTION__* variables (including SERVICE_MONITOR_DAYS)

### Internal
  SERVERBEE_DATABASE__PATH
  SERVERBEE_DATABASE__MAX_CONNECTIONS
  SERVERBEE_AUTH__SESSION_TTL
  SERVERBEE_AUTH__SECURE_COOKIE
  SERVERBEE_RATE_LIMIT__LOGIN_MAX
  SERVERBEE_RATE_LIMIT__REGISTER_MAX
  SERVERBEE_UPGRADE__RELEASE_BASE_URL

## Agent

### Essential
  SERVERBEE_SERVER_URL
  SERVERBEE_AUTO_DISCOVERY_KEY

### Common
  SERVERBEE_COLLECTOR__INTERVAL
  SERVERBEE_COLLECTOR__ENABLE_GPU
  SERVERBEE_COLLECTOR__ENABLE_TEMPERATURE
  SERVERBEE_FILE__ENABLED
  SERVERBEE_FILE__ROOT_PATHS
  SERVERBEE_IP_CHANGE__ENABLED
  SERVERBEE_IP_CHANGE__CHECK_EXTERNAL_IP
  SERVERBEE_LOG__LEVEL
  SERVERBEE_LOG__FILE

### Internal
  SERVERBEE_TOKEN
  SERVERBEE_FILE__MAX_FILE_SIZE
  SERVERBEE_FILE__DENY_PATTERNS
  SERVERBEE_IP_CHANGE__EXTERNAL_IP_URL
  SERVERBEE_IP_CHANGE__INTERVAL_SECS
```

Each "Internal" section is prefixed with a note:

> The following variables have sensible defaults and rarely need modification. Only adjust when you have a specific requirement.

#### configuration.mdx Structure

**Environment Variable Quick Reference** section: same layered tables as ENV.md above.

**TOML Detailed Reference** section: keeps the existing per-`[section]` organization (`[server]`, `[database]`, `[auth]`, etc.) but with these fixes:
- Remove `geoip.enabled` row from `[geoip]` tables in both en/cn
- Add `[ip_change]` section to agent TOML reference in both en/cn
- Add `service_monitor_days` row to `[retention]` tables in both en/cn

**TOML Example** updates:
- Add `[ip_change]` section to agent.toml examples (en/cn)
- Add `retention.service_monitor_days` to server.toml examples (cn)
- Remove `geoip.enabled` lines from server.toml examples (en and cn)

#### Other files — targeted fixes only

These files get surgical `GEOIP__ENABLED` removal, no structural reorganization:
- `apps/docs/content/docs/en/server.mdx` — delete env var example lines 125-126, replace with `GEOIP__MMDB_PATH` example
- `apps/docs/content/docs/cn/server.mdx` — delete env var example lines 163-164, replace with `GEOIP__MMDB_PATH` example
- `apps/docs/content/docs/cn/deployment.mdx` — remove `GEOIP__ENABLED` from docker-compose example (line 63)
- `deploy/railway/README.md` — remove `GEOIP__ENABLED` row from env var table, update `GEOIP__MMDB_PATH` description
- `deploy/railway/Dockerfile` — remove commented-out `GEOIP__ENABLED` line (line 81)
- `README.md` — replace `GEOIP__ENABLED` example with `GEOIP__MMDB_PATH`
- `README.zh-CN.md` — same as README.md

### Variable Count Summary

Counting method: unique `SERVERBEE_*` env var names, per binary.

| Category | Server | Agent |
|----------|--------|-------|
| Essential | 2 | 2 |
| Common | 7 | 9 |
| OAuth (Optional) | 10 | — |
| GeoIP (Optional) | 1 | — |
| Retention (Tuning) | 12 | — |
| Internal | 7 | 5 |
| **Subtotal** | **39** | **16** |

`LOG__LEVEL` and `LOG__FILE` appear in both Server and Agent (independent configs, same env var name). Deduplicating gives **53 unique env var names** across the project.

### Files to Modify

Primary (structural reorganization + fixes):

1. `ENV.md` — restructure into layered sections; fix #1, #2, #4
2. `apps/docs/content/docs/en/configuration.mdx` — restructure env var tables; fix #1, #2, #3; update TOML reference
3. `apps/docs/content/docs/cn/configuration.mdx` — same as en; also fix TOML example

Secondary (surgical `GEOIP__ENABLED` removal only):

4. `apps/docs/content/docs/en/server.mdx` — replace GEOIP__ENABLED example with GEOIP__MMDB_PATH
5. `apps/docs/content/docs/cn/server.mdx` — same as en
6. `apps/docs/content/docs/cn/deployment.mdx` — remove from docker-compose example
7. `deploy/railway/README.md` — remove from env var table
8. `deploy/railway/Dockerfile` — remove commented-out ENV line
9. `README.md` — replace example
10. `README.zh-CN.md` — replace example

### Out of Scope

- No Rust code changes (the `GeoIpConfig` implicit behavior is intentional)
- No changes to the "内部默认值（不可配置）" section in cn configuration.mdx (hardcoded constants table is already good)
- `docs/superpowers/specs/` and `docs/superpowers/plans/` — internal design docs, not user-facing; `GEOIP__ENABLED` references in these are historical and left as-is
