# ServerBee Environment Variables

All server and agent runtime environment variables use the `SERVERBEE_` prefix. Nested config keys use `__` (double underscore) as separator.

Example: TOML `server.listen` → env var `SERVERBEE_SERVER__LISTEN`

> **First-run admin account**: There is no admin username/password env var. On first start (when no users exist) the server auto-creates an admin account with a randomly generated password and prints it once to the server/container logs as a highlighted credentials banner — capture it from the logs. You must change this password on first login, and may optionally choose a different username at that time.

> **Maintainer Note**: When adding or modifying environment variables, update both this file and `apps/docs/content/docs/{en,cn}/configuration.mdx`.

## Developer Workflow Env Vars

These variables are for local repo tooling and development workflows. They are not Figment-backed server or agent runtime config, and changing them does not change `server.toml` or `agent.toml`.

| Environment Variable | Used By | Type | Default | Description |
|---------------------|---------|------|---------|-------------|
| `SERVERBEE_PROD_URL` | `make db-pull`, `make web-dev-prod` | string | - | Production base URL used by the database pull script and the frontend prod-proxy workflow |
| `SERVERBEE_PROD_API_KEY` | `make db-pull` | string | - | Admin-scoped API key for the production backup API. Do not reuse this for `make web-dev-prod` |
| `SERVERBEE_PROD_READONLY_API_KEY` | `make web-dev-prod` | string | - | Member-scoped API key injected by the frontend dev proxy for live production browsing |
| `ALLOW_WRITES` | `make web-dev-prod` | string | unset | Local opt-in override. Set to `1` to disable the proxy's read-method-only block. The UI banner also changes from the normal read-only warning to a stronger write-enabled warning when this is set |

`ALLOW_WRITES` is intentionally a local developer override for `make web-dev-prod`, not a runtime feature flag for the Rust server or agent.

## Server

### Quick Start

| Environment Variable | TOML Key | Type | Default | Description |
|---------------------|----------|------|---------|-------------|
| `SERVERBEE_SERVER__LISTEN` | `server.listen` | string | `0.0.0.0:9527` | Listen address and port |

### Common

| Environment Variable | TOML Key | Type | Default | Description |
|---------------------|----------|------|---------|-------------|
| `SERVERBEE_SERVER__DATA_DIR` | `server.data_dir` | string | `./data` | Data directory for SQLite and backups |
| `SERVERBEE_AUTH__MAX_SERVERS` | `auth.max_servers` | u32 | `0` | Maximum servers allowed via enrollment (0 = no limit). Best-effort soft cap |
| `SERVERBEE_SCHEDULER__TIMEZONE` | `scheduler.timezone` | string | `UTC` | Timezone for daily traffic aggregation and cron scheduling (e.g. `Asia/Shanghai`) |
| `SERVERBEE_FEATURE__CUSTOM_THEMES` | `feature.custom_themes` | bool | `true` | Disable user-defined themes when false. Custom refs are read-coerced to `preset:default` |
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

### ASN (Optional)

| Environment Variable | TOML Key | Type | Default | Description |
|---------------------|----------|------|---------|-------------|
| `SERVERBEE_ASN__MMDB_PATH` | `asn.mmdb_path` | string | `""` | Path to a DB-IP Lite ASN / MaxMind GeoLite2-ASN MMDB file. Non-empty path enables traceroute ASN enrichment; otherwise admins can download DB-IP Lite ASN from Settings → ASN Database |

### Resend (Email Notifications)

| Environment Variable | TOML Key | Type | Default | Description |
|---------------------|----------|------|---------|-------------|
| `SERVERBEE_RESEND__API_KEY` | `resend.api_key` | string | `""` | Resend API key (https://resend.com/api-keys). Required to use the Email notification channel. The sender address (`from`) configured on each email channel must belong to a domain verified at https://resend.com/domains. |

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
| `SERVERBEE_RETENTION__SECURITY_EVENT_DAYS` | `retention.security_event_days` | u32 | `30` | Security event records (SSH login / brute force / port scan) retention in days |
| `SERVERBEE_RETENTION__IP_QUALITY_EVENT_DAYS` | `retention.ip_quality_event_days` | u32 | `90` | IP quality status-change event records retention in days |

### Mobile (Optional)

| Environment Variable | TOML Key | Type | Default | Description |
|---------------------|----------|------|---------|-------------|
| `SERVERBEE_MOBILE__ACCESS_TTL` | `mobile.access_ttl` | i64 | `900` | Mobile access token lifetime in seconds (15 min) |
| `SERVERBEE_MOBILE__REFRESH_TTL` | `mobile.refresh_ttl` | i64 | `2592000` | Mobile refresh token lifetime in seconds (30 days) |

### Firewall (Optional)

Tier-2 guardrail for the firewall blocklist feature. CIDRs / IPs listed here are refused by `POST /api/firewall/blocks` even if a server administrator tries to insert them. Tier 1 (hard-coded protected ranges: loopback, RFC 1918, link-local, multicast, unspecified) is always enforced inside `service::firewall`.

| Environment Variable | TOML Key | Type | Default | Description |
|---------------------|----------|------|---------|-------------|
| `SERVERBEE_FIREWALL__ALLOW_LIST` | `firewall.allow_list` | string[] | `[]` | CIDRs / IPs the server refuses to insert into `block_list`. Tier-2 guardrail. Tier 1 (loopback + RFC 1918 + link-local + multicast + unspecified) is hard-coded and always applied |

### IP Quality

Default risk-scoring works out of the box via [ipapi.is](https://ipapi.is) (no API key required, ~1000 requests/day per source IP). On primary failure the server falls back to [ip-api.com](https://ip-api.com), which provides geo + proxy/hosting flags but no risk score.

| Environment Variable | TOML Key | Type | Default | Description |
|---------------------|----------|------|---------|-------------|
| `SERVERBEE_IP_QUALITY__RISK_PROVIDER` | `ip_quality.risk_provider` | string | `"ipapi_is"` | Primary risk provider. One of: `none`, `ipapi_is`, `ip-api`. |
| `SERVERBEE_IP_QUALITY__RISK_PROVIDER_FALLBACK` | `ip_quality.risk_provider_fallback` | string | `"ip-api"` | Fallback provider triggered on primary failure. Set to `none` to disable. |
| `SERVERBEE_IP_QUALITY__IPAPI_IS__API_KEY` | `ip_quality.ipapi_is.api_key` | string | - | Optional. Configure for higher per-account rate limits. |
| `SERVERBEE_IP_QUALITY__IPAPI_IS__ENDPOINT` | `ip_quality.ipapi_is.endpoint` | string | `https://api.ipapi.is` | Override for self-hosted mirrors or testing. |

**Migration from older versions:** Earlier releases supported four paid providers (Scamalytics, IPQualityScore, ProxyCheck, AbuseIPDB) configured via `SERVERBEE_IP_QUALITY__{SCAMALYTICS,IPQS,PROXYCHECK,ABUSEIPDB}__*`. These env vars are silently ignored. To restore equivalent functionality, fork or vendor the provider implementation from a tag prior to 2026-05-25.

### Network Probe Anomaly Thresholds

Avg-latency cutoffs used to classify network-probe records as `high_latency` / `very_high_latency` in `/api/servers/{id}/network-probes/anomalies` and `anomaly_count` on the network-probe overview.

| Environment Variable | TOML Key | Type | Default | Description |
|---------------------|----------|------|---------|-------------|
| `SERVERBEE_NETWORK_PROBE__HIGH_LATENCY_MS` | `network_probe.high_latency_ms` | f64 | `500` | Records with `avg_latency` strictly greater than this are tagged `high_latency` |
| `SERVERBEE_NETWORK_PROBE__VERY_HIGH_LATENCY_MS` | `network_probe.very_high_latency_ms` | f64 | `800` | Records with `avg_latency` strictly greater than this are tagged `very_high_latency` (overrides the high tier) |

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
| `SERVERBEE_UPGRADE__LATEST_VERSION_URL` | `upgrade.latest_version_url` | string | `""` | Optional custom URL for latest version API. If empty, uses GitHub API |
| `SERVERBEE_FILE__MAX_UPLOAD_SIZE` | `file.max_upload_size` | u64 | `104857600` (100 MB) | Maximum file upload size in bytes |

## Agent

Agent top-level keys use single underscore. Nested keys use `__` (double underscore).

### Quick Start

| Environment Variable | TOML Key | Type | Default | Description |
|---------------------|----------|------|---------|-------------|
| `SERVERBEE_SERVER_URL` | `server_url` | string | - (required) | Server HTTP base URL (e.g. `http://your-server:9527`). Agent appends API paths automatically |
| `SERVERBEE_ENROLLMENT_CODE` | `enrollment_code` | string | `""` | One-time enrollment code for first-time agent registration. Generated by an admin in the server UI Settings (or `POST /api/agent/enrollments`). Single-use and short-lived (default 10 min); only needed until the agent has a persisted token |

### Common

| Environment Variable | TOML Key | Type | Default | Description |
|---------------------|----------|------|---------|-------------|
| `SERVERBEE_COLLECTOR__INTERVAL` | `collector.interval` | u32 | `3` | Metric report interval in seconds |
| `SERVERBEE_COLLECTOR__ENABLE_GPU` | `collector.enable_gpu` | bool | `false` | Enable NVIDIA GPU monitoring (requires nvml) |
| `SERVERBEE_COLLECTOR__ENABLE_TEMPERATURE` | `collector.enable_temperature` | bool | `true` | Enable CPU temperature monitoring |
| `SERVERBEE_FILE__ENABLED` | `file.enabled` | bool | `false` | Enable file management capability on this agent |
| `SERVERBEE_FILE__ROOT_PATHS` | `file.root_paths` | string[] | `[]` | Allowed root paths for file browsing (e.g. `/home,/var/log`). Empty rejects all file operations |
| `SERVERBEE_IP_CHANGE__ENABLED` | `ip_change.enabled` | bool | `true` | Enable periodic IP change detection |
| `SERVERBEE_LOG__LEVEL` | `log.level` | string | `info` | Log level: `trace`, `debug`, `info`, `warn`, `error` |
| `SERVERBEE_LOG__FILE` | `log.file` | string | `""` | Log file path. Empty means stdout only |

### Internal

> The following variables have sensible defaults and rarely need modification. Only adjust when you have a specific requirement.

| Environment Variable | TOML Key | Type | Default | Description |
|---------------------|----------|------|---------|-------------|
| `SERVERBEE_TOKEN` | `token` | string | `""` | Agent authentication token. Auto-populated after registration, do not set manually |
| `SERVERBEE_FILE__MAX_FILE_SIZE` | `file.max_file_size` | u64 | `1073741824` | Maximum file size in bytes for read/download (default 1GB) |
| `SERVERBEE_FILE__DENY_PATTERNS` | `file.deny_patterns` | string[] | `*.key,*.pem,id_rsa*,.env*,shadow,passwd` | Glob patterns for files the agent will refuse to access |
| `SERVERBEE_IP_CHANGE__EXTERNAL_IP_URLS` | `ip_change.external_ip_urls` | string[] | `["https://api.ipify.org","https://ifconfig.me/ip","https://icanhazip.com","https://checkip.amazonaws.com"]` | Ordered list of external IP services tried at startup and on every IP-change check. First success wins. Required for agents behind NAT, in containers, or anywhere interface enumeration can't see the routable public IP. Air-gapped deployments can set this to an empty list to skip external lookups entirely |
| `SERVERBEE_IP_CHANGE__INTERVAL_SECS` | `ip_change.interval_secs` | u64 | `300` | IP check interval in seconds (default 5 minutes) |

### Upgrade (Agent)

| Environment Variable | TOML Key | Type | Default | Description |
|---------------------|----------|------|---------|-------------|
| `SERVERBEE_UPGRADE__RELEASE_REPO_URL` | `upgrade.release_repo_url` | string | `https://github.com/ZingerLittleBee/ServerBee/releases` | Pinned release source base URL the Agent downloads upgrades from. Any HTTPS host mirroring the GitHub releases path layout `{base}/download/v{version}/{asset}` and `{base}/download/v{version}/checksums.txt` works. The compiled-in default can only be changed at build time via the `SERVERBEE_RELEASE_REPO` environment variable when compiling the agent (not a runtime setting). At runtime, override via this `SERVERBEE_UPGRADE__RELEASE_REPO_URL` env var, the `[upgrade] release_repo_url` config, or the `--release-repo` CLI flag |
| `SERVERBEE_UPGRADE__RELEASE_CERT_SPKI_SHA256` | `upgrade.release_cert_spki_sha256` | string | `""` | Optional TLS certificate SPKI pin for the release host. Must be 64 lowercase hex chars (SHA-256 of the leaf cert SubjectPublicKeyInfo DER). Empty = disabled. If set, the Agent additionally pins the leaf cert SPKI after standard chain validation. Invalid (non-64/non-hex) values are rejected at startup |

### Security (Agent)

Tunes the agent-side security event detectors (SSH login / brute force, port scan). Detection runs entirely on the agent; the server only stores events and evaluates alert rules. Configure per-host since traffic profiles differ.

| Environment Variable | TOML Key | Type | Default | Description |
|---------------------|----------|------|---------|-------------|
| `SERVERBEE_SECURITY__ENABLED` | `security.enabled` | bool | `true` | Master switch for all security detectors. When `false` the agent emits no `security_event` messages |
| `SERVERBEE_SECURITY__SSH__WINDOW_SECONDS` | `security.ssh.window_seconds` | u32 | `60` | Sliding window length (seconds) for SSH brute-force detection |
| `SERVERBEE_SECURITY__SSH__FAILED_THRESHOLD` | `security.ssh.failed_threshold` | u32 | `10` | Number of failed SSH attempts within the window that triggers an `ssh_brute_force` event. Queue clears after firing |
| `SERVERBEE_SECURITY__PORT_SCAN__ENABLED` | `security.port_scan.enabled` | bool | `false` | Enable port-scan detection. Requires `conntrack` CLI installed (Linux) |
| `SERVERBEE_SECURITY__PORT_SCAN__WINDOW_SECONDS` | `security.port_scan.window_seconds` | u32 | `30` | Sliding window length (seconds) for port-scan detection |
| `SERVERBEE_SECURITY__PORT_SCAN__DISTINCT_PORT_THRESHOLD` | `security.port_scan.distinct_port_threshold` | u32 | `20` | Distinct destination ports hit by a single source IP within the window that triggers a `port_scan` event |
| `SERVERBEE_SECURITY__DATA_DIR` | `security.data_dir` | string | `/var/lib/serverbee/security` | Directory for the persistent `first_seen` store used to mark `ssh_login` events as new (user, IP) combinations |
