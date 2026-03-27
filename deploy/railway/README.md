# ServerBee Server — Railway Template

[![Deploy on Railway](https://railway.com/button.svg)](https://railway.com/template/YOUR_TEMPLATE_ID)

Lightweight, self-hosted VPS monitoring server. Receives metrics from distributed agents over WebSocket, stores in SQLite, and serves a React dashboard.

## Quick Start

1. Click the **Deploy on Railway** button above
2. Set `SERVERBEE_ADMIN__PASSWORD` (required)
3. Deploy — the server will be live in ~30 seconds

## Volume

ServerBee stores data in SQLite. Add a Railway volume mounted at `/data` to persist data across deploys.

| Mount Path | Recommended Size |
|-----------|-----------------|
| `/data` | 1 GB |

## Environment Variables

### Server

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | _auto_ | Assigned by Railway automatically. Do not set manually. |
| `SERVERBEE_SERVER__DATA_DIR` | `/data` | Data directory for SQLite and backups |
| `SERVERBEE_SERVER__TRUSTED_PROXIES` | `["10.0.0.0/8",...]` | Pre-configured to trust Railway's internal proxy. Override only if needed |

### Admin (Required)

| Variable | Default | Description |
|----------|---------|-------------|
| `SERVERBEE_ADMIN__USERNAME` | `admin` | Admin username (created on first startup) |
| `SERVERBEE_ADMIN__PASSWORD` | **Required** | **Admin password. Must be set before first deploy.** |

### Database

| Variable | Default | Description |
|----------|---------|-------------|
| `SERVERBEE_DATABASE__PATH` | `serverbee.db` | SQLite file path (relative to `data_dir`) |
| `SERVERBEE_DATABASE__MAX_CONNECTIONS` | `10` | Maximum database connection pool size |

### Authentication

| Variable | Default | Description |
|----------|---------|-------------|
| `SERVERBEE_AUTH__SESSION_TTL` | `86400` | Session token TTL in seconds (24h) |
| `SERVERBEE_AUTH__AUTO_DISCOVERY_KEY` | Auto-generated | Key for agent auto-registration. Leave empty to auto-generate |
| `SERVERBEE_AUTH__SECURE_COOKIE` | `true` | Set `Secure` flag on session cookies. Set `false` for HTTP-only |

### Data Retention

| Variable | Default | Description |
|----------|---------|-------------|
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

### Scheduler

| Variable | Default | Description |
|----------|---------|-------------|
| `SERVERBEE_SCHEDULER__TIMEZONE` | `UTC` | Timezone for daily traffic aggregation and cron scheduling (e.g. `Asia/Shanghai`) |

### Rate Limiting

| Variable | Default | Description |
|----------|---------|-------------|
| `SERVERBEE_RATE_LIMIT__LOGIN_MAX` | `5` | Maximum login attempts per IP within 15-minute window |
| `SERVERBEE_RATE_LIMIT__REGISTER_MAX` | `3` | Maximum agent registrations per IP within 15-minute window |

### OAuth (Optional)

| Variable | Default | Description |
|----------|---------|-------------|
| `SERVERBEE_OAUTH__BASE_URL` | — | Public base URL for OAuth callbacks (e.g. `https://monitor.example.com`) |
| `SERVERBEE_OAUTH__ALLOW_REGISTRATION` | `false` | Auto-create user accounts on first OAuth login |
| `SERVERBEE_OAUTH__GITHUB__CLIENT_ID` | — | GitHub OAuth App client ID |
| `SERVERBEE_OAUTH__GITHUB__CLIENT_SECRET` | — | GitHub OAuth App client secret |
| `SERVERBEE_OAUTH__GOOGLE__CLIENT_ID` | — | Google OAuth client ID |
| `SERVERBEE_OAUTH__GOOGLE__CLIENT_SECRET` | — | Google OAuth client secret |
| `SERVERBEE_OAUTH__OIDC__ISSUER_URL` | — | OIDC provider issuer URL |
| `SERVERBEE_OAUTH__OIDC__CLIENT_ID` | — | OIDC client ID |
| `SERVERBEE_OAUTH__OIDC__CLIENT_SECRET` | — | OIDC client secret |

### GeoIP (Optional)

| Variable | Default | Description |
|----------|---------|-------------|
| `SERVERBEE_GEOIP__ENABLED` | `false` | Enable GeoIP lookup for agent IP addresses |
| `SERVERBEE_GEOIP__MMDB_PATH` | — | Path to MaxMind GeoLite2-City.mmdb file |

### Logging

| Variable | Default | Description |
|----------|---------|-------------|
| `SERVERBEE_LOG__LEVEL` | `info` | Log level: `trace`, `debug`, `info`, `warn`, `error` |
| `SERVERBEE_LOG__FILE` | — | Log file path. Empty means stdout only |

### Upgrade

| Variable | Default | Description |
|----------|---------|-------------|
| `SERVERBEE_UPGRADE__RELEASE_BASE_URL` | `https://github.com/ZingerLittleBee/ServerBee/releases` | Base URL for agent upgrade release assets |

## Connecting Agents

After deployment, configure your agents to connect:

```bash
SERVERBEE_SERVER_URL=https://your-railway-app.up.railway.app
SERVERBEE_AUTO_DISCOVERY_KEY=<your-discovery-key>
```

The discovery key is shown in the server startup logs if auto-generated, or set via `SERVERBEE_AUTH__AUTO_DISCOVERY_KEY`.
