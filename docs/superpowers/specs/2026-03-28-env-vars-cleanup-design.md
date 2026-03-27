# Environment Variables Cleanup & Documentation Reorganization

**Date:** 2026-03-28
**Scope:** Documentation only (ENV.md, en/cn configuration.mdx). No code changes.

## Problem

1. **Phantom variable**: `SERVERBEE_GEOIP__ENABLED` is documented in ENV.md and both configuration.mdx files but does not exist in code. `GeoIpConfig` only has `mmdb_path`; GeoIP is implicitly enabled when `mmdb_path` is non-empty.
2. **Missing documentation** for 6 variables that exist in code:
   - `SERVERBEE_RETENTION__SERVICE_MONITOR_DAYS` (default 30) — missing from all three docs
   - `SERVERBEE_IP_CHANGE__ENABLED` (default true) — missing from en/cn configuration.mdx
   - `SERVERBEE_IP_CHANGE__CHECK_EXTERNAL_IP` (default false) — missing from en/cn configuration.mdx
   - `SERVERBEE_IP_CHANGE__EXTERNAL_IP_URL` (default `https://api.ipify.org`) — missing from en/cn configuration.mdx
   - `SERVERBEE_IP_CHANGE__INTERVAL_SECS` (default 300) — missing from en/cn configuration.mdx
   - `SERVERBEE_OAUTH__OIDC__SCOPES` (default `["openid","email","profile"]`) — missing from ENV.md
3. **Stale TOML example**: cn configuration.mdx line 213 references `geoip.enabled = false` which has no corresponding code.
4. **Flat structure**: All 59 env vars are listed in a single flat table, making it hard for users to identify which ones they actually need to configure.

## Design

### Decision: Remove `SERVERBEE_GEOIP__ENABLED`

Remove from documentation only. The implicit behavior ("provide `mmdb_path` → GeoIP is enabled") is the cleanest UX. An extra boolean toggle is redundant.

### Fix Checklist

| # | Issue | Action | Files |
|---|-------|--------|-------|
| 1 | `GEOIP__ENABLED` phantom | Delete from env var tables; update description of `GEOIP__MMDB_PATH` to say "non-empty = enabled" | ENV.md, en/cn configuration.mdx |
| 2 | `RETENTION__SERVICE_MONITOR_DAYS` missing | Add to retention tables (default 30) | ENV.md, en/cn configuration.mdx |
| 3 | `IP_CHANGE__*` missing from mdx | Add 4 vars to Agent env var tables and TOML reference sections | en/cn configuration.mdx |
| 4 | `OAUTH__OIDC__SCOPES` missing from ENV.md | Add to OAuth table | ENV.md |
| 5 | cn mdx TOML example has `geoip.enabled` | Delete line, keep only `mmdb_path` | cn/configuration.mdx |

### Layered Documentation Structure

All three docs (ENV.md, en configuration.mdx, cn configuration.mdx) adopt the same classification:

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
  SERVERBEE_LOG__LEVEL
  SERVERBEE_LOG__FILE

### Internal
  SERVERBEE_TOKEN
  SERVERBEE_FILE__MAX_FILE_SIZE
  SERVERBEE_FILE__DENY_PATTERNS
  SERVERBEE_IP_CHANGE__ENABLED
  SERVERBEE_IP_CHANGE__CHECK_EXTERNAL_IP
  SERVERBEE_IP_CHANGE__EXTERNAL_IP_URL
  SERVERBEE_IP_CHANGE__INTERVAL_SECS
```

Each "Internal" section is prefixed with a note:

> The following variables have sensible defaults and rarely need modification. Only adjust when you have a specific requirement.

#### configuration.mdx Structure

**Environment Variable Quick Reference** section: same layered tables as ENV.md above.

**TOML Detailed Reference** section: keeps the existing per-`[section]` organization (`[server]`, `[database]`, `[auth]`, etc.) unchanged. No structural change here — this part is already well-organized by config section.

**TOML Example** updates:
- Add `[ip_change]` section to agent.toml examples
- Add `retention.service_monitor_days` to server.toml examples
- Remove `geoip.enabled` from cn server.toml example

### Variable Count Summary

After cleanup:

| Category | Server | Agent | Total |
|----------|--------|-------|-------|
| Essential | 2 | 2 | 4 |
| Common | 7 | 7 | 14 |
| OAuth (Optional) | 10 | — | 10 |
| GeoIP (Optional) | 1 | — | 1 |
| Retention (Tuning) | 12 | — | 12 |
| Internal | 7 | 7 | 14 |
| **Total** | **39** | **16** | **55** |

Note: Total is 55 (down from 59 documented previously) because:
- Removed 1 phantom variable (`GEOIP__ENABLED`)
- Added 1 missing variable (`RETENTION__SERVICE_MONITOR_DAYS`)
- Added 1 missing variable (`OAUTH__OIDC__SCOPES`)
- The `IP_CHANGE__*` 4 vars were already in ENV.md, just missing from configuration.mdx
- Net: 59 - 1 + 1 + 1 = 60 actual env vars across server + agent (some like LOG__* are shared names but independent configs)

Precise count: **39 server + 16 agent = 55 unique env vars**.

### Files to Modify

1. `ENV.md` — restructure into layered sections, fix issues #1/#2/#4
2. `apps/docs/content/docs/en/configuration.mdx` — restructure env var tables, fix issues #1/#2/#3, update TOML reference
3. `apps/docs/content/docs/cn/configuration.mdx` — same as en, plus fix issue #5

### Out of Scope

- No Rust code changes (the `GeoIpConfig` implicit behavior is intentional)
- No changes to Dockerfile or docker-compose env var examples
- No changes to the "内部默认值（不可配置）" section in cn configuration.mdx (hardcoded constants table is already good)
