# Environment Variables Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 4 code/doc inconsistencies and reorganize env var documentation into layered categories across 10 files.

**Architecture:** Documentation-only changes. No Rust code. Three primary files (ENV.md, en/cn configuration.mdx) get full restructuring. Seven secondary files get surgical `GEOIP__ENABLED` removal.

**Tech Stack:** Markdown, MDX

---

### Task 1: Remove `GEOIP__ENABLED` from 7 secondary files

**Files:**
- Modify: `apps/docs/content/docs/en/server.mdx:125-126`
- Modify: `apps/docs/content/docs/cn/server.mdx:163-164`
- Modify: `apps/docs/content/docs/cn/deployment.mdx:63`
- Modify: `deploy/railway/README.md:96-101`
- Modify: `deploy/railway/Dockerfile:80-81`
- Modify: `README.md:175`
- Modify: `README.zh-CN.md:175`

- [ ] **Step 1: Fix en/server.mdx**

Replace:
```
# geoip.enabled
export SERVERBEE_GEOIP__ENABLED=true
```
With:
```
# geoip.mmdb_path (non-empty path enables GeoIP)
export SERVERBEE_GEOIP__MMDB_PATH="/path/to/GeoLite2-City.mmdb"
```

- [ ] **Step 2: Fix cn/server.mdx**

Replace:
```
# 等同于 geoip.enabled = true
export SERVERBEE_GEOIP__ENABLED=true
```
With:
```
# geoip.mmdb_path（路径非空即启用 GeoIP）
export SERVERBEE_GEOIP__MMDB_PATH="/path/to/GeoLite2-City.mmdb"
```

- [ ] **Step 3: Fix cn/deployment.mdx**

Remove this line from the docker-compose environment block:
```
      - SERVERBEE_GEOIP__ENABLED=true
```

The `SERVERBEE_GEOIP__MMDB_PATH=/data/GeoLite2-City.mmdb` line on line 64 already implicitly enables GeoIP.

- [ ] **Step 4: Fix deploy/railway/README.md**

Replace the GeoIP section:
```markdown
### GeoIP (Optional)

| Variable | Default | Description |
|----------|---------|-------------|
| `SERVERBEE_GEOIP__ENABLED` | `false` | Enable GeoIP lookup for agent IP addresses |
| `SERVERBEE_GEOIP__MMDB_PATH` | — | Path to MaxMind GeoLite2-City.mmdb file |
```
With:
```markdown
### GeoIP (Optional)

| Variable | Default | Description |
|----------|---------|-------------|
| `SERVERBEE_GEOIP__MMDB_PATH` | `""` | Path to MaxMind GeoLite2-City.mmdb file. Non-empty path enables GeoIP |
```

- [ ] **Step 5: Fix deploy/railway/Dockerfile**

Remove this line:
```dockerfile
# Enable GeoIP lookup for agent IP addresses
# ENV SERVERBEE_GEOIP__ENABLED=false
```

Update the adjacent mmdb_path comment:
```dockerfile
# Path to MaxMind GeoLite2-City.mmdb file (non-empty path enables GeoIP)
# ENV SERVERBEE_GEOIP__MMDB_PATH=
```

- [ ] **Step 6: Fix README.md**

Replace:
```bash
export SERVERBEE_GEOIP__ENABLED=true
```
With:
```bash
export SERVERBEE_GEOIP__MMDB_PATH="/path/to/GeoLite2-City.mmdb"
```

- [ ] **Step 7: Fix README.zh-CN.md**

Replace:
```bash
export SERVERBEE_GEOIP__ENABLED=true
```
With:
```bash
export SERVERBEE_GEOIP__MMDB_PATH="/path/to/GeoLite2-City.mmdb"
```

- [ ] **Step 8: Commit**

```bash
git add apps/docs/content/docs/en/server.mdx apps/docs/content/docs/cn/server.mdx \
  apps/docs/content/docs/cn/deployment.mdx deploy/railway/README.md deploy/railway/Dockerfile \
  README.md README.zh-CN.md
git commit -m "docs: remove phantom GEOIP__ENABLED from secondary files"
```

---

### Task 2: Rewrite ENV.md with layered structure

**Files:**
- Modify: `ENV.md` (full rewrite)

- [ ] **Step 1: Rewrite ENV.md**

Replace the entire file content with the layered structure below. Key changes:
- Remove `SERVERBEE_GEOIP__ENABLED` row
- Add `SERVERBEE_RETENTION__SERVICE_MONITOR_DAYS` to retention section
- Add `SERVERBEE_OAUTH__OIDC__SCOPES` to OAuth section
- Reorganize from flat tables into: Essential → Common → OAuth → GeoIP → Retention → Internal (for Server) and Essential → Common → Internal (for Agent)
- Add `IP_CHANGE__ENABLED` and `IP_CHANGE__CHECK_EXTERNAL_IP` to Agent Common
- Add `IP_CHANGE__EXTERNAL_IP_URL` and `IP_CHANGE__INTERVAL_SECS` to Agent Internal

Full content:

```markdown
# ServerBee Environment Variables

All environment variables use the `SERVERBEE_` prefix. Nested config keys use `__` (double underscore) as separator.

Example: TOML `admin.password` → env var `SERVERBEE_ADMIN__PASSWORD`

> **Maintainer Note**: When adding or modifying environment variables, update both this file and `apps/docs/content/docs/{en,cn}/configuration.mdx`.

## Server

### Essential

| Environment Variable | TOML Key | Type | Default | Description |
|---------------------|----------|------|---------|-------------|
| `SERVERBEE_ADMIN__PASSWORD` | `admin.password` | string | `""` (auto-generated) | Default admin password. Leave empty to auto-generate and print to startup log |
| `SERVERBEE_SERVER__LISTEN` | `server.listen` | string | `0.0.0.0:9527` | Listen address and port |

### Common

| Environment Variable | TOML Key | Type | Default | Description |
|---------------------|----------|------|---------|-------------|
| `SERVERBEE_SERVER__DATA_DIR` | `server.data_dir` | string | `./data` | Data directory for SQLite and backups |
| `SERVERBEE_AUTH__AUTO_DISCOVERY_KEY` | `auth.auto_discovery_key` | string | `""` (auto-generated) | Key for agent auto-registration. Leave empty to auto-generate on first startup |
| `SERVERBEE_ADMIN__USERNAME` | `admin.username` | string | `admin` | Default admin username (created on first startup if no users exist) |
| `SERVERBEE_SERVER__TRUSTED_PROXIES` | `server.trusted_proxies` | string[] | `[]` | CIDR list of trusted reverse proxies (e.g. `["127.0.0.1/32", "10.0.0.0/8"]`) |
| `SERVERBEE_SCHEDULER__TIMEZONE` | `scheduler.timezone` | string | `UTC` | Timezone for daily traffic aggregation and cron scheduling (e.g. `Asia/Shanghai`) |
| `SERVERBEE_LOG__LEVEL` | `log.level` | string | `info` | Log level: `trace`, `debug`, `info`, `warn`, `error` |
| `SERVERBEE_LOG__FILE` | `log.file` | string | `""` | Log file path. Empty means stdout only |

### OAuth (Optional)

| Environment Variable | TOML Key | Type | Default | Description |
|---------------------|----------|------|---------|-------------|
| `SERVERBEE_OAUTH__BASE_URL` | `oauth.base_url` | string | `""` | Public base URL for constructing OAuth callback URLs (e.g. `https://monitor.example.com`) |
| `SERVERBEE_OAUTH__ALLOW_REGISTRATION` | `oauth.allow_registration` | bool | `false` | Auto-create user accounts on first OAuth login |
| `SERVERBEE_OAUTH__GITHUB__CLIENT_ID` | `oauth.github.client_id` | string | - | GitHub OAuth App client ID |
| `SERVERBEE_OAUTH__GITHUB__CLIENT_SECRET` | `oauth.github.client_secret` | string | - | GitHub OAuth App client secret |
| `SERVERBEE_OAUTH__GOOGLE__CLIENT_ID` | `oauth.google.client_id` | string | - | Google OAuth client ID |
| `SERVERBEE_OAUTH__GOOGLE__CLIENT_SECRET` | `oauth.google.client_secret` | string | - | Google OAuth client secret |
| `SERVERBEE_OAUTH__OIDC__ISSUER_URL` | `oauth.oidc.issuer_url` | string | - | OIDC provider issuer URL |
| `SERVERBEE_OAUTH__OIDC__CLIENT_ID` | `oauth.oidc.client_id` | string | - | OIDC client ID |
| `SERVERBEE_OAUTH__OIDC__CLIENT_SECRET` | `oauth.oidc.client_secret` | string | - | OIDC client secret |
| `SERVERBEE_OAUTH__OIDC__SCOPES` | `oauth.oidc.scopes` | string[] | `["openid", "email", "profile"]` | OAuth scopes to request |

### GeoIP (Optional)

| Environment Variable | TOML Key | Type | Default | Description |
|---------------------|----------|------|---------|-------------|
| `SERVERBEE_GEOIP__MMDB_PATH` | `geoip.mmdb_path` | string | `""` | Path to MaxMind GeoLite2-City.mmdb file. Non-empty path enables GeoIP |

### Data Retention (Tuning)

| Environment Variable | TOML Key | Type | Default | Description |
|---------------------|----------|------|---------|-------------|
| `SERVERBEE_RETENTION__RECORDS_DAYS` | `retention.records_days` | u32 | `7` | Raw metric records retention in days |
| `SERVERBEE_RETENTION__RECORDS_HOURLY_DAYS` | `retention.records_hourly_days` | u32 | `90` | Hourly aggregated records retention in days |
| `SERVERBEE_RETENTION__GPU_RECORDS_DAYS` | `retention.gpu_records_days` | u32 | `7` | GPU metric records retention in days |
| `SERVERBEE_RETENTION__PING_RECORDS_DAYS` | `retention.ping_records_days` | u32 | `7` | Ping probe records retention in days |
| `SERVERBEE_RETENTION__NETWORK_PROBE_DAYS` | `retention.network_probe_days` | u32 | `7` | Raw network probe records retention in days |
| `SERVERBEE_RETENTION__NETWORK_PROBE_HOURLY_DAYS` | `retention.network_probe_hourly_days` | u32 | `90` | Hourly network probe aggregates retention in days |
| `SERVERBEE_RETENTION__AUDIT_LOGS_DAYS` | `retention.audit_logs_days` | u32 | `180` | Audit log retention in days |
| `SERVERBEE_RETENTION__TRAFFIC_HOURLY_DAYS` | `retention.traffic_hourly_days` | u32 | `7` | Traffic hourly records retention in days |
| `SERVERBEE_RETENTION__TRAFFIC_DAILY_DAYS` | `retention.traffic_daily_days` | u32 | `400` | Traffic daily records retention in days |
| `SERVERBEE_RETENTION__TASK_RESULTS_DAYS` | `retention.task_results_days` | u32 | `7` | Task results retention in days |
| `SERVERBEE_RETENTION__DOCKER_EVENTS_DAYS` | `retention.docker_events_days` | u32 | `7` | Docker event records retention in days |
| `SERVERBEE_RETENTION__SERVICE_MONITOR_DAYS` | `retention.service_monitor_days` | u32 | `30` | Service monitor records retention in days |

### Internal

> The following variables have sensible defaults and rarely need modification. Only adjust when you have a specific requirement.

| Environment Variable | TOML Key | Type | Default | Description |
|---------------------|----------|------|---------|-------------|
| `SERVERBEE_DATABASE__PATH` | `database.path` | string | `serverbee.db` | SQLite database file path (relative to `data_dir`) |
| `SERVERBEE_DATABASE__MAX_CONNECTIONS` | `database.max_connections` | u32 | `10` | Maximum database connection pool size |
| `SERVERBEE_AUTH__SESSION_TTL` | `auth.session_ttl` | i64 | `86400` | Session token TTL in seconds (default 24h) |
| `SERVERBEE_AUTH__SECURE_COOKIE` | `auth.secure_cookie` | bool | `true` | Set `Secure` flag on session cookies. Set `false` only for development without HTTPS |
| `SERVERBEE_RATE_LIMIT__LOGIN_MAX` | `rate_limit.login_max` | u32 | `5` | Maximum login attempts per IP within 15-minute window |
| `SERVERBEE_RATE_LIMIT__REGISTER_MAX` | `rate_limit.register_max` | u32 | `3` | Maximum agent registrations per IP within 15-minute window |
| `SERVERBEE_UPGRADE__RELEASE_BASE_URL` | `upgrade.release_base_url` | string | `https://github.com/ZingerLittleBee/ServerBee/releases` | Base URL for agent upgrade release assets |

## Agent

Agent top-level keys use single underscore. Nested keys use `__` (double underscore).

### Essential

| Environment Variable | TOML Key | Type | Default | Description |
|---------------------|----------|------|---------|-------------|
| `SERVERBEE_SERVER_URL` | `server_url` | string | - (required) | Server HTTP base URL (e.g. `http://your-server:9527`). Agent appends API paths automatically |
| `SERVERBEE_AUTO_DISCOVERY_KEY` | `auto_discovery_key` | string | `""` | Discovery key for first-time agent registration. Only used when `token` is empty |

### Common

| Environment Variable | TOML Key | Type | Default | Description |
|---------------------|----------|------|---------|-------------|
| `SERVERBEE_COLLECTOR__INTERVAL` | `collector.interval` | u32 | `3` | Metric report interval in seconds |
| `SERVERBEE_COLLECTOR__ENABLE_GPU` | `collector.enable_gpu` | bool | `false` | Enable NVIDIA GPU monitoring (requires nvml) |
| `SERVERBEE_COLLECTOR__ENABLE_TEMPERATURE` | `collector.enable_temperature` | bool | `true` | Enable CPU temperature monitoring |
| `SERVERBEE_FILE__ENABLED` | `file.enabled` | bool | `false` | Enable file management capability on this agent |
| `SERVERBEE_FILE__ROOT_PATHS` | `file.root_paths` | string[] | `[]` | Allowed root paths for file browsing (e.g. `/home,/var/log`). Empty rejects all file operations |
| `SERVERBEE_IP_CHANGE__ENABLED` | `ip_change.enabled` | bool | `true` | Enable periodic IP change detection |
| `SERVERBEE_IP_CHANGE__CHECK_EXTERNAL_IP` | `ip_change.check_external_ip` | bool | `false` | Also query an external URL to detect public/NAT IP changes |
| `SERVERBEE_LOG__LEVEL` | `log.level` | string | `info` | Log level: `trace`, `debug`, `info`, `warn`, `error` |
| `SERVERBEE_LOG__FILE` | `log.file` | string | `""` | Log file path. Empty means stdout only |

### Internal

> The following variables have sensible defaults and rarely need modification. Only adjust when you have a specific requirement.

| Environment Variable | TOML Key | Type | Default | Description |
|---------------------|----------|------|---------|-------------|
| `SERVERBEE_TOKEN` | `token` | string | `""` | Agent authentication token. Auto-populated after registration, do not set manually |
| `SERVERBEE_FILE__MAX_FILE_SIZE` | `file.max_file_size` | u64 | `1073741824` | Maximum file size in bytes for read/download (default 1GB) |
| `SERVERBEE_FILE__DENY_PATTERNS` | `file.deny_patterns` | string[] | `*.key,*.pem,id_rsa*,.env*,shadow,passwd` | Glob patterns for files the agent will refuse to access |
| `SERVERBEE_IP_CHANGE__EXTERNAL_IP_URL` | `ip_change.external_ip_url` | string | `https://api.ipify.org` | URL that returns the agent's external IP as plain text |
| `SERVERBEE_IP_CHANGE__INTERVAL_SECS` | `ip_change.interval_secs` | u64 | `300` | IP check interval in seconds (default 5 minutes) |
```

- [ ] **Step 2: Commit**

```bash
git add ENV.md
git commit -m "docs: reorganize ENV.md into layered categories"
```

---

### Task 3: Restructure en/configuration.mdx

**Files:**
- Modify: `apps/docs/content/docs/en/configuration.mdx:21-82` (env var tables)
- Modify: `apps/docs/content/docs/en/configuration.mdx:157-162` (geoip TOML reference)
- Modify: `apps/docs/content/docs/en/configuration.mdx:229-234` (agent TOML reference — add ip_change after file)
- Modify: `apps/docs/content/docs/en/configuration.mdx:265-266` (production example — geoip.enabled)

- [ ] **Step 1: Replace Server env var table (lines 21-62)**

Replace the flat `### Server Environment Variables` section (from `### Server Environment Variables` through the last row before `### Agent Environment Variables`) with layered subsections.

Replace:
```markdown
### Server Environment Variables

| Environment Variable | Default | Description |
|---------------------|---------|-------------|
| `SERVERBEE_SERVER__LISTEN` | `0.0.0.0:9527` | Listen address and port |
...entire flat table through...
| `SERVERBEE_UPGRADE__RELEASE_BASE_URL` | `https://github.com/ZingerLittleBee/ServerBee/releases` | Base URL for agent upgrade release assets |
```

With:
```markdown
### Server Environment Variables

#### Essential

| Environment Variable | Default | Description |
|---------------------|---------|-------------|
| `SERVERBEE_ADMIN__PASSWORD` | auto-generated | Admin password. Leave empty to auto-generate and print to log |
| `SERVERBEE_SERVER__LISTEN` | `0.0.0.0:9527` | Listen address and port |

#### Common

| Environment Variable | Default | Description |
|---------------------|---------|-------------|
| `SERVERBEE_SERVER__DATA_DIR` | `./data` | Data directory for database and backups |
| `SERVERBEE_AUTH__AUTO_DISCOVERY_KEY` | auto-generated | Agent discovery key. Leave empty to auto-generate on first startup |
| `SERVERBEE_ADMIN__USERNAME` | `admin` | Initial admin username (only used when no users exist) |
| `SERVERBEE_SERVER__TRUSTED_PROXIES` | `[]` | CIDR list of trusted reverse proxies (e.g. `["127.0.0.1/32", "10.0.0.0/8"]`) |
| `SERVERBEE_SCHEDULER__TIMEZONE` | `UTC` | Timezone for daily traffic aggregation (e.g. `Asia/Shanghai`) |
| `SERVERBEE_LOG__LEVEL` | `info` | Log level: `trace`, `debug`, `info`, `warn`, `error` |
| `SERVERBEE_LOG__FILE` | `""` | Log file path. Empty means stdout only |

#### OAuth (Optional)

| Environment Variable | Default | Description |
|---------------------|---------|-------------|
| `SERVERBEE_OAUTH__BASE_URL` | `""` | Public server URL for constructing OAuth callback URLs |
| `SERVERBEE_OAUTH__ALLOW_REGISTRATION` | `false` | Auto-create user accounts on first OAuth login |
| `SERVERBEE_OAUTH__GITHUB__CLIENT_ID` | -- | GitHub OAuth App client ID |
| `SERVERBEE_OAUTH__GITHUB__CLIENT_SECRET` | -- | GitHub OAuth App client secret |
| `SERVERBEE_OAUTH__GOOGLE__CLIENT_ID` | -- | Google OAuth client ID |
| `SERVERBEE_OAUTH__GOOGLE__CLIENT_SECRET` | -- | Google OAuth client secret |
| `SERVERBEE_OAUTH__OIDC__ISSUER_URL` | -- | OIDC provider issuer URL |
| `SERVERBEE_OAUTH__OIDC__CLIENT_ID` | -- | OIDC client ID |
| `SERVERBEE_OAUTH__OIDC__CLIENT_SECRET` | -- | OIDC client secret |
| `SERVERBEE_OAUTH__OIDC__SCOPES` | `["openid", "email", "profile"]` | OAuth scopes to request |

#### GeoIP (Optional)

| Environment Variable | Default | Description |
|---------------------|---------|-------------|
| `SERVERBEE_GEOIP__MMDB_PATH` | `""` | Path to MaxMind GeoLite2-City.mmdb file. Non-empty path enables GeoIP |

#### Data Retention (Tuning)

| Environment Variable | Default | Description |
|---------------------|---------|-------------|
| `SERVERBEE_RETENTION__RECORDS_DAYS` | `7` | Raw metric records retention in days |
| `SERVERBEE_RETENTION__RECORDS_HOURLY_DAYS` | `90` | Hourly aggregated records retention in days |
| `SERVERBEE_RETENTION__GPU_RECORDS_DAYS` | `7` | GPU metric records retention in days |
| `SERVERBEE_RETENTION__PING_RECORDS_DAYS` | `7` | Ping probe records retention in days |
| `SERVERBEE_RETENTION__NETWORK_PROBE_DAYS` | `7` | Raw network probe records retention in days |
| `SERVERBEE_RETENTION__NETWORK_PROBE_HOURLY_DAYS` | `90` | Hourly network probe aggregates retention in days |
| `SERVERBEE_RETENTION__AUDIT_LOGS_DAYS` | `180` | Audit log retention in days |
| `SERVERBEE_RETENTION__TRAFFIC_HOURLY_DAYS` | `7` | Traffic hourly records retention in days |
| `SERVERBEE_RETENTION__TRAFFIC_DAILY_DAYS` | `400` | Traffic daily records retention in days |
| `SERVERBEE_RETENTION__TASK_RESULTS_DAYS` | `7` | Task results retention in days |
| `SERVERBEE_RETENTION__DOCKER_EVENTS_DAYS` | `7` | Docker event records retention in days |
| `SERVERBEE_RETENTION__SERVICE_MONITOR_DAYS` | `30` | Service monitor records retention in days |

#### Internal

> The following variables have sensible defaults and rarely need modification. Only adjust when you have a specific requirement.

| Environment Variable | Default | Description |
|---------------------|---------|-------------|
| `SERVERBEE_DATABASE__PATH` | `serverbee.db` | SQLite database file path (relative to data_dir) |
| `SERVERBEE_DATABASE__MAX_CONNECTIONS` | `10` | Maximum database connection pool size |
| `SERVERBEE_AUTH__SESSION_TTL` | `86400` | Session token TTL in seconds (default 24h) |
| `SERVERBEE_AUTH__SECURE_COOKIE` | `true` | Set Secure flag on session cookies. Set `false` for HTTP-only dev |
| `SERVERBEE_RATE_LIMIT__LOGIN_MAX` | `5` | Max login attempts per IP within 15-minute window |
| `SERVERBEE_RATE_LIMIT__REGISTER_MAX` | `3` | Max agent registrations per IP within 15-minute window |
| `SERVERBEE_UPGRADE__RELEASE_BASE_URL` | `https://github.com/ZingerLittleBee/ServerBee/releases` | Base URL for agent upgrade release assets |
```

- [ ] **Step 2: Replace Agent env var table (lines 64-81)**

Replace the flat `### Agent Environment Variables` section with layered subsections.

Replace:
```markdown
### Agent Environment Variables

Agent top-level keys use single underscore. Nested keys use `__` (double underscore).

| Environment Variable | Default | Description |
|---------------------|---------|-------------|
| `SERVERBEE_SERVER_URL` | -- (required) | Server HTTP base URL (e.g. `http://your-server:9527`). Agent appends API paths automatically |
| `SERVERBEE_TOKEN` | auto-populated | Agent auth token. Auto-populated after registration, do not set manually |
| `SERVERBEE_AUTO_DISCOVERY_KEY` | `""` | Discovery key for first-time registration. Only used when token is empty |
| `SERVERBEE_COLLECTOR__INTERVAL` | `3` | Metric report interval in seconds |
| `SERVERBEE_COLLECTOR__ENABLE_GPU` | `false` | Enable NVIDIA GPU monitoring (requires nvml) |
| `SERVERBEE_COLLECTOR__ENABLE_TEMPERATURE` | `true` | Enable CPU temperature monitoring |
| `SERVERBEE_FILE__ENABLED` | `false` | Enable file management on this agent |
| `SERVERBEE_FILE__ROOT_PATHS` | `[]` | Allowed root paths (comma-separated, e.g. `/home,/var/log`). Empty rejects all file operations |
| `SERVERBEE_FILE__MAX_FILE_SIZE` | `1073741824` | Max file size in bytes for read/download (default 1GB) |
| `SERVERBEE_FILE__DENY_PATTERNS` | `*.key,*.pem,...` | Glob patterns for files the agent refuses to access |
| `SERVERBEE_LOG__LEVEL` | `info` | Log level: `trace`, `debug`, `info`, `warn`, `error` |
| `SERVERBEE_LOG__FILE` | `""` | Log file path. Empty means stdout only |
```

With:
```markdown
### Agent Environment Variables

Agent top-level keys use single underscore. Nested keys use `__` (double underscore).

#### Essential

| Environment Variable | Default | Description |
|---------------------|---------|-------------|
| `SERVERBEE_SERVER_URL` | -- (required) | Server HTTP base URL (e.g. `http://your-server:9527`). Agent appends API paths automatically |
| `SERVERBEE_AUTO_DISCOVERY_KEY` | `""` | Discovery key for first-time registration. Only used when token is empty |

#### Common

| Environment Variable | Default | Description |
|---------------------|---------|-------------|
| `SERVERBEE_COLLECTOR__INTERVAL` | `3` | Metric report interval in seconds |
| `SERVERBEE_COLLECTOR__ENABLE_GPU` | `false` | Enable NVIDIA GPU monitoring (requires nvml) |
| `SERVERBEE_COLLECTOR__ENABLE_TEMPERATURE` | `true` | Enable CPU temperature monitoring |
| `SERVERBEE_FILE__ENABLED` | `false` | Enable file management on this agent |
| `SERVERBEE_FILE__ROOT_PATHS` | `[]` | Allowed root paths (comma-separated, e.g. `/home,/var/log`). Empty rejects all file operations |
| `SERVERBEE_IP_CHANGE__ENABLED` | `true` | Enable periodic IP change detection |
| `SERVERBEE_IP_CHANGE__CHECK_EXTERNAL_IP` | `false` | Also query an external URL to detect public/NAT IP changes |
| `SERVERBEE_LOG__LEVEL` | `info` | Log level: `trace`, `debug`, `info`, `warn`, `error` |
| `SERVERBEE_LOG__FILE` | `""` | Log file path. Empty means stdout only |

#### Internal

> The following variables have sensible defaults and rarely need modification. Only adjust when you have a specific requirement.

| Environment Variable | Default | Description |
|---------------------|---------|-------------|
| `SERVERBEE_TOKEN` | auto-populated | Agent auth token. Auto-populated after registration, do not set manually |
| `SERVERBEE_FILE__MAX_FILE_SIZE` | `1073741824` | Max file size in bytes for read/download (default 1GB) |
| `SERVERBEE_FILE__DENY_PATTERNS` | `*.key,*.pem,...` | Glob patterns for files the agent refuses to access |
| `SERVERBEE_IP_CHANGE__EXTERNAL_IP_URL` | `https://api.ipify.org` | URL that returns the agent's external IP as plain text |
| `SERVERBEE_IP_CHANGE__INTERVAL_SECS` | `300` | IP check interval in seconds (default 5 minutes) |
```

- [ ] **Step 3: Fix `[geoip]` TOML reference (lines 157-162)**

Replace:
```markdown
### `[geoip]` -- GeoIP Lookup

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `false` | Whether to perform GeoIP lookups on agent IP addresses |
| `mmdb_path` | string | `""` | Path to a MaxMind GeoLite2-City MMDB file |
```

With:
```markdown
### `[geoip]` -- GeoIP Lookup

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `mmdb_path` | string | `""` | Path to a MaxMind GeoLite2-City MMDB file. Non-empty path enables GeoIP |
```

- [ ] **Step 4: Add `[retention]` service_monitor_days row (around line 129)**

In the `[retention]` table, after the `docker_events_days` row, add:
```
| `service_monitor_days` | u32 | `30` | Days to keep service monitor check records |
```

- [ ] **Step 5: Add `[ip_change]` TOML reference section (after `[file]` section, around line 228)**

After the `### [file] -- File Management` section and before `### [log] -- Logging`, insert:
```markdown
### `[ip_change]` -- IP Change Detection

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `true` | Enable periodic IP change detection. Agent enumerates NIC addresses and reports changes |
| `check_external_ip` | bool | `false` | Also query an external URL to detect public/NAT IP changes |
| `external_ip_url` | string | `"https://api.ipify.org"` | URL that returns the agent's external IP as plain text (used when `check_external_ip` is true) |
| `interval_secs` | u64 | `300` | IP check interval in seconds (default 5 minutes) |
```

- [ ] **Step 6: Fix production server example (lines 265-266)**

Replace:
```toml
[geoip]
enabled = true
mmdb_path = "/var/lib/serverbee/GeoLite2-City.mmdb"
```

With:
```toml
[geoip]
mmdb_path = "/var/lib/serverbee/GeoLite2-City.mmdb"
```

- [ ] **Step 7: Add `[ip_change]` to production agent example (after `[file]` section, before `[log]`)**

After the `deny_patterns` line in the agent example, add:
```toml

[ip_change]
enabled = true
check_external_ip = false
```

- [ ] **Step 8: Commit**

```bash
git add apps/docs/content/docs/en/configuration.mdx
git commit -m "docs(en): restructure env vars into layered categories"
```

---

### Task 4: Restructure cn/configuration.mdx

**Files:**
- Modify: `apps/docs/content/docs/cn/configuration.mdx:27-68` (env var tables)
- Modify: `apps/docs/content/docs/cn/configuration.mdx:209-217` (geoip TOML example)
- Modify: `apps/docs/content/docs/cn/configuration.mdx:155-201` (retention + geoip TOML reference in example)
- Modify: `apps/docs/content/docs/cn/configuration.mdx:254-313` (agent TOML example)

- [ ] **Step 1: Replace Server env var table (lines 27-68)**

Replace the flat `### Server 环境变量` section with layered subsections. Same structure as en version but with Chinese descriptions.

Replace:
```markdown
### Server 环境变量

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `SERVERBEE_SERVER__LISTEN` | `0.0.0.0:9527` | 监听地址和端口 |
...entire flat table through...
| `SERVERBEE_UPGRADE__RELEASE_BASE_URL` | `https://github.com/ZingerLittleBee/ServerBee/releases` | Agent 升级 Release 资产的基础 URL |
```

With:
```markdown
### Server 环境变量

#### 必须配置（Essential）

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `SERVERBEE_ADMIN__PASSWORD` | 自动生成 | 初始管理员密码，留空自动生成并打印到日志 |
| `SERVERBEE_SERVER__LISTEN` | `0.0.0.0:9527` | 监听地址和端口 |

#### 常用配置（Common）

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `SERVERBEE_SERVER__DATA_DIR` | `./data` | 数据目录（存放数据库和备份） |
| `SERVERBEE_AUTH__AUTO_DISCOVERY_KEY` | 自动生成 | Agent 自动注册密钥，留空首次启动自动生成 |
| `SERVERBEE_ADMIN__USERNAME` | `admin` | 初始管理员用户名（仅首次无用户时生效） |
| `SERVERBEE_SERVER__TRUSTED_PROXIES` | `[]` | 受信任的反向代理 CIDR 列表（如 `["127.0.0.1/32", "10.0.0.0/8"]`） |
| `SERVERBEE_SCHEDULER__TIMEZONE` | `UTC` | 流量日聚合时区（如 `Asia/Shanghai`） |
| `SERVERBEE_LOG__LEVEL` | `info` | 日志级别：`trace`/`debug`/`info`/`warn`/`error` |
| `SERVERBEE_LOG__FILE` | `""` | 日志文件路径，留空输出到 stdout |

#### OAuth（按需配置）

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `SERVERBEE_OAUTH__BASE_URL` | `""` | 服务器公网地址（用于构造 OAuth 回调 URL） |
| `SERVERBEE_OAUTH__ALLOW_REGISTRATION` | `false` | 首次 OAuth 登录时是否自动创建用户 |
| `SERVERBEE_OAUTH__GITHUB__CLIENT_ID` | -- | GitHub OAuth App Client ID |
| `SERVERBEE_OAUTH__GITHUB__CLIENT_SECRET` | -- | GitHub OAuth App Client Secret |
| `SERVERBEE_OAUTH__GOOGLE__CLIENT_ID` | -- | Google OAuth Client ID |
| `SERVERBEE_OAUTH__GOOGLE__CLIENT_SECRET` | -- | Google OAuth Client Secret |
| `SERVERBEE_OAUTH__OIDC__ISSUER_URL` | -- | OIDC Issuer URL |
| `SERVERBEE_OAUTH__OIDC__CLIENT_ID` | -- | OIDC Client ID |
| `SERVERBEE_OAUTH__OIDC__CLIENT_SECRET` | -- | OIDC Client Secret |
| `SERVERBEE_OAUTH__OIDC__SCOPES` | `["openid", "email", "profile"]` | OAuth 请求的 scope |

#### GeoIP（按需配置）

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `SERVERBEE_GEOIP__MMDB_PATH` | `""` | MaxMind GeoLite2-City.mmdb 文件路径，路径非空即启用 GeoIP |

#### 数据保留（可选调优）

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `SERVERBEE_RETENTION__RECORDS_DAYS` | `7` | 原始指标记录保留天数 |
| `SERVERBEE_RETENTION__RECORDS_HOURLY_DAYS` | `90` | 小时聚合记录保留天数 |
| `SERVERBEE_RETENTION__GPU_RECORDS_DAYS` | `7` | GPU 指标记录保留天数 |
| `SERVERBEE_RETENTION__PING_RECORDS_DAYS` | `7` | Ping 探测记录保留天数 |
| `SERVERBEE_RETENTION__NETWORK_PROBE_DAYS` | `7` | 原始网络质量探测记录保留天数 |
| `SERVERBEE_RETENTION__NETWORK_PROBE_HOURLY_DAYS` | `90` | 小时聚合网络质量探测记录保留天数 |
| `SERVERBEE_RETENTION__AUDIT_LOGS_DAYS` | `180` | 审计日志保留天数 |
| `SERVERBEE_RETENTION__TRAFFIC_HOURLY_DAYS` | `7` | 流量小时记录保留天数 |
| `SERVERBEE_RETENTION__TRAFFIC_DAILY_DAYS` | `400` | 流量日记录保留天数 |
| `SERVERBEE_RETENTION__TASK_RESULTS_DAYS` | `7` | 任务执行结果保留天数 |
| `SERVERBEE_RETENTION__DOCKER_EVENTS_DAYS` | `7` | Docker 事件记录保留天数 |
| `SERVERBEE_RETENTION__SERVICE_MONITOR_DAYS` | `30` | 服务监控记录保留天数 |

#### 内部配置（Internal）

> 以下变量有合理默认值，绝大多数场景无需修改。仅在有明确需求时调整。

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `SERVERBEE_DATABASE__PATH` | `serverbee.db` | SQLite 数据库文件路径（相对于 data_dir） |
| `SERVERBEE_DATABASE__MAX_CONNECTIONS` | `10` | 数据库连接池最大连接数 |
| `SERVERBEE_AUTH__SESSION_TTL` | `86400` | Session 有效期（秒），默认 24 小时 |
| `SERVERBEE_AUTH__SECURE_COOKIE` | `true` | Cookie 的 Secure 标记，开发环境设为 `false` |
| `SERVERBEE_RATE_LIMIT__LOGIN_MAX` | `5` | 15 分钟内每 IP 最大登录尝试次数 |
| `SERVERBEE_RATE_LIMIT__REGISTER_MAX` | `3` | 15 分钟内每 IP 最大 Agent 注册次数 |
| `SERVERBEE_UPGRADE__RELEASE_BASE_URL` | `https://github.com/ZingerLittleBee/ServerBee/releases` | Agent 升级 Release 资产的基础 URL |
```

- [ ] **Step 2: Replace Agent env var table (lines 70-87)**

Replace:
```markdown
### Agent 环境变量

Agent 顶层键使用单下划线，嵌套键使用 `__`（双下划线）。

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `SERVERBEE_SERVER_URL` | --（必填） | Server 的 HTTP 地址（如 `http://your-server:9527`），Agent 自动拼接 API 路径 |
| `SERVERBEE_TOKEN` | 注册后自动填充 | Agent 认证 Token，无需手动设置 |
| `SERVERBEE_AUTO_DISCOVERY_KEY` | `""` | 自动注册密钥，仅在 Token 为空时使用 |
| `SERVERBEE_COLLECTOR__INTERVAL` | `3` | 指标采集和上报间隔（秒） |
| `SERVERBEE_COLLECTOR__ENABLE_GPU` | `false` | 启用 NVIDIA GPU 监控（需要 nvml 驱动） |
| `SERVERBEE_COLLECTOR__ENABLE_TEMPERATURE` | `true` | 启用 CPU 温度监控 |
| `SERVERBEE_FILE__ENABLED` | `false` | 启用文件管理功能 |
| `SERVERBEE_FILE__ROOT_PATHS` | `[]` | 允许浏览的根路径（逗号分隔，如 `/home,/var/log`），留空则拒绝所有文件操作 |
| `SERVERBEE_FILE__MAX_FILE_SIZE` | `1073741824` | 文件读取/下载的最大字节数（默认 1GB） |
| `SERVERBEE_FILE__DENY_PATTERNS` | `*.key,*.pem,...` | 拒绝访问的文件名 glob 模式 |
| `SERVERBEE_LOG__LEVEL` | `info` | 日志级别：`trace`/`debug`/`info`/`warn`/`error` |
| `SERVERBEE_LOG__FILE` | `""` | 日志文件路径，留空输出到 stdout |
```

With:
```markdown
### Agent 环境变量

Agent 顶层键使用单下划线，嵌套键使用 `__`（双下划线）。

#### 必须配置（Essential）

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `SERVERBEE_SERVER_URL` | --（必填） | Server 的 HTTP 地址（如 `http://your-server:9527`），Agent 自动拼接 API 路径 |
| `SERVERBEE_AUTO_DISCOVERY_KEY` | `""` | 自动注册密钥，仅在 Token 为空时使用 |

#### 常用配置（Common）

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `SERVERBEE_COLLECTOR__INTERVAL` | `3` | 指标采集和上报间隔（秒） |
| `SERVERBEE_COLLECTOR__ENABLE_GPU` | `false` | 启用 NVIDIA GPU 监控（需要 nvml 驱动） |
| `SERVERBEE_COLLECTOR__ENABLE_TEMPERATURE` | `true` | 启用 CPU 温度监控 |
| `SERVERBEE_FILE__ENABLED` | `false` | 启用文件管理功能 |
| `SERVERBEE_FILE__ROOT_PATHS` | `[]` | 允许浏览的根路径（逗号分隔，如 `/home,/var/log`），留空则拒绝所有文件操作 |
| `SERVERBEE_IP_CHANGE__ENABLED` | `true` | 启用周期性 IP 变更检测 |
| `SERVERBEE_IP_CHANGE__CHECK_EXTERNAL_IP` | `false` | 同时查询外部 URL 检测公网/NAT IP 变更 |
| `SERVERBEE_LOG__LEVEL` | `info` | 日志级别：`trace`/`debug`/`info`/`warn`/`error` |
| `SERVERBEE_LOG__FILE` | `""` | 日志文件路径，留空输出到 stdout |

#### 内部配置（Internal）

> 以下变量有合理默认值，绝大多数场景无需修改。仅在有明确需求时调整。

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `SERVERBEE_TOKEN` | 注册后自动填充 | Agent 认证 Token，无需手动设置 |
| `SERVERBEE_FILE__MAX_FILE_SIZE` | `1073741824` | 文件读取/下载的最大字节数（默认 1GB） |
| `SERVERBEE_FILE__DENY_PATTERNS` | `*.key,*.pem,...` | 拒绝访问的文件名 glob 模式 |
| `SERVERBEE_IP_CHANGE__EXTERNAL_IP_URL` | `https://api.ipify.org` | 返回外部 IP 的查询 URL（纯文本格式） |
| `SERVERBEE_IP_CHANGE__INTERVAL_SECS` | `300` | IP 检测间隔（秒），默认 5 分钟 |
```

- [ ] **Step 3: Fix `[geoip]` in server.toml TOML example (lines 209-217)**

Replace:
```toml
# --- GeoIP 地理位置 ---
[geoip]
# 是否启用 IP 地理位置查询
# 默认: false
enabled = false

# MaxMind MMDB 数据库文件路径
# 默认: ""
mmdb_path = ""
```

With:
```toml
# --- GeoIP 地理位置 ---
[geoip]
# MaxMind MMDB 数据库文件路径，路径非空即启用 GeoIP
# 默认: ""
mmdb_path = ""
```

- [ ] **Step 4: Add `service_monitor_days` to retention section in TOML example (after `docker_events_days`)**

After the `docker_events_days = 7` line in the server.toml example, add:
```toml

# 服务监控记录保留天数
# 默认: 30
service_monitor_days = 30
```

- [ ] **Step 5: Add `[ip_change]` section to agent.toml example (after `[file]` section, before `[log]`)**

After the `deny_patterns` line in the agent.toml example, insert:
```toml

# --- IP 变更检测 ---
[ip_change]
# 是否启用周期性 IP 变更检测
# Agent 定期枚举网络接口地址并上报变更
# 默认: true
enabled = true

# 是否同时查询外部 URL 检测公网/NAT IP 变更
# 默认: false
check_external_ip = false

# 返回外部 IP 的查询 URL（纯文本格式）
# 仅在 check_external_ip = true 时使用
# 默认: "https://api.ipify.org"
external_ip_url = "https://api.ipify.org"

# IP 检测间隔（秒）
# 默认: 300 (5 分钟)
interval_secs = 300
```

- [ ] **Step 6: Add `[ip_change]` to Agent defaults table (around line 349-355)**

In the `### Agent 默认值` table, after the `文件大小限制` row, add:
```
| IP 变更检测 | 开启 | 默认检测网络接口 IP 变更 |
| 外部 IP 检测 | 关闭 | 需手动启用 |
| IP 检测间隔 | `300` 秒（5 分钟） | 定期检查间隔 |
```

- [ ] **Step 7: Remove `geoip.enabled` from `### Server 默认值` table**

In the Server defaults table, replace the GeoIP row:
```
| GeoIP | 关闭 | 需手动配置 MMDB 文件 |
```
With:
```
| GeoIP | 关闭 | 提供 MMDB 文件路径即启用 |
```

- [ ] **Step 8: Commit**

```bash
git add apps/docs/content/docs/cn/configuration.mdx
git commit -m "docs(cn): restructure env vars into layered categories"
```

---

### Task 5: Verification

**Files:** All 10 modified files

- [ ] **Step 1: Verify no GEOIP__ENABLED remains in user-facing docs**

Run:
```bash
grep -r "GEOIP__ENABLED\|geoip.*enabled" --include="*.md" --include="*.mdx" --include="Dockerfile" . \
  | grep -v "docs/superpowers/" \
  | grep -v "node_modules/"
```

Expected: **zero matches** (docs/superpowers/ files are internal design docs, excluded intentionally).

- [ ] **Step 2: Verify SERVICE_MONITOR_DAYS is documented**

Run:
```bash
grep -r "SERVICE_MONITOR_DAYS\|service_monitor_days" ENV.md apps/docs/content/docs/
```

Expected: matches in ENV.md, en/configuration.mdx, cn/configuration.mdx (3 files minimum).

- [ ] **Step 3: Verify IP_CHANGE vars are documented**

Run:
```bash
grep -r "IP_CHANGE__\|ip_change\." apps/docs/content/docs/en/configuration.mdx apps/docs/content/docs/cn/configuration.mdx ENV.md
```

Expected: matches for all 4 `IP_CHANGE__*` vars in configuration.mdx files, and in ENV.md.

- [ ] **Step 4: Verify OIDC__SCOPES is in ENV.md**

Run:
```bash
grep "OIDC__SCOPES\|oidc.*scopes" ENV.md
```

Expected: 1 match.

- [ ] **Step 5: Commit all files together as final verification commit (if any missed fixes)**

If all checks pass and no further changes needed:
```bash
echo "All verification checks passed. No additional changes needed."
```

If any fixes are needed, apply them and commit:
```bash
git add -A
git commit -m "docs: fix remaining env var documentation issues"
```
