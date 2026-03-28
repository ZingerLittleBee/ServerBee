# ServerBee Environment Variables

All environment variables use the `SERVERBEE_` prefix. Nested config keys use `__` (double underscore) as separator.

Example: TOML `admin.password` → env var `SERVERBEE_ADMIN__PASSWORD`

> **Maintainer Note**: When adding or modifying environment variables, update both this file and `apps/docs/content/docs/{en,cn}/configuration.mdx`.

## Server

### Quick Start

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
| `SERVERBEE_SERVER__TRUSTED_PROXIES` | `server.trusted_proxies` | string[] | private/loopback CIDRs | CIDR list of trusted reverse proxies for X-Forwarded-For. Defaults to RFC 1918 + loopback. Set to `[]` to disable |
| `SERVERBEE_DATABASE__PATH` | `database.path` | string | `serverbee.db` | SQLite database file path (relative to `data_dir`) |
| `SERVERBEE_DATABASE__MAX_CONNECTIONS` | `database.max_connections` | u32 | `10` | Maximum database connection pool size |
| `SERVERBEE_AUTH__SESSION_TTL` | `auth.session_ttl` | i64 | `86400` | Session token TTL in seconds (default 24h) |
| `SERVERBEE_AUTH__SECURE_COOKIE` | `auth.secure_cookie` | bool | `true` | Set `Secure` flag on session cookies. Set `false` only for development without HTTPS |
| `SERVERBEE_RATE_LIMIT__LOGIN_MAX` | `rate_limit.login_max` | u32 | `5` | Maximum login attempts per IP within 15-minute window |
| `SERVERBEE_RATE_LIMIT__REGISTER_MAX` | `rate_limit.register_max` | u32 | `3` | Maximum agent registrations per IP within 15-minute window |
| `SERVERBEE_UPGRADE__RELEASE_BASE_URL` | `upgrade.release_base_url` | string | `https://github.com/ZingerLittleBee/ServerBee/releases` | Base URL for agent upgrade release assets |

## Agent

Agent top-level keys use single underscore. Nested keys use `__` (double underscore).

### Quick Start

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
