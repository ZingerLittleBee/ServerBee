# ServerBee

Language: English | [简体中文](./README.zh-CN.md)

A lightweight, self-hosted VPS monitoring system built with Rust and React.

## Features

- **Real-time Dashboard** -- Server status, CPU/memory/disk/network metrics with live WebSocket updates
- **Server Card Ring Grid** -- Four ring charts per server (CPU, Memory, Disk, Traffic quota) plus inline disk I/O throughput, load trend, and billing-cycle "days remaining" hint
- **Server Groups** -- Organize servers by group with country flag display
- **iOS Mobile Companion** -- Native iOS app with QR pairing, push notifications, and real-time metrics
- **Detailed Metrics** -- Real-time streaming charts + historical views (1h/6h/24h/7d/30d) for CPU, memory, disk, network, load, temperature, GPU, disk I/O
- **Alert System** -- 14+ metric types, threshold/offline/traffic/expiration rules, AND logic, 70% sampling
- **Notifications** -- Webhook, Telegram, Bark, Email (via Resend) channels with notification groups
- **Network Quality Monitoring** -- Multi-target network probing (96 preset China 3-ISP + international nodes), real-time/historical latency charts, anomaly detection, per-server target assignment
- **Ping Monitoring** -- ICMP, TCP, HTTP probes with latency charts and success rate
- **Safer Agent Registration** -- Stable machine fingerprints reuse existing server records, `auth.max_servers` soft-caps auto-discovery growth, discovery keys can be regenerated from Settings, and unconnected placeholders can be cleaned up
- **Web Terminal** -- Browser-based PTY terminal via WebSocket proxy
- **GPU Monitoring** -- NVIDIA GPU usage/temperature/memory (via nvml-wrapper, feature-gated)
- **Disk I/O Monitoring** -- Per-disk read/write throughput charts with merged and per-disk views. Linux via `/proc/diskstats`, macOS/Windows via sysinfo
- **GeoIP** -- Automatic region/country detection from agent IP with in-app database download/update
- **Custom Dashboard** -- Drag-and-drop dashboard with 13 widget types, multiple dashboards, editor mode
- **OAuth & 2FA** -- GitHub/Google/OIDC login, TOTP two-factor authentication
- **Multi-user** -- Admin/Member roles, audit logging, rate limiting
- **File Management** -- Remote file browser with Monaco Editor, upload/download with progress, path sandbox security (`root_paths` + `deny_patterns`)
- **Docker Monitoring** -- Real-time Docker container monitoring with stats (CPU/memory/network/block I/O), container log streaming (stdout/stderr color-coded), events timeline, networks and volumes overview
- **Capability Toggles** -- Per-server feature controls (terminal, exec, upgrade, ping, file manager) with defense-in-depth enforcement
- **Uptime Timeline** -- 90-day uptime visualization with per-day color-coded bars on server detail, public status pages, and customizable dashboard widgets
- **Public Status Page** -- Unauthenticated status page with server groups, live metrics, and 90-day uptime timelines with configurable thresholds
- **Monthly Traffic Statistics** -- Billing cycle-aware traffic tracking with daily/hourly breakdowns, usage progress bars, and end-of-cycle prediction
- **Service Monitors (SSL/WHOIS/HTTP/Ping/TCP)** -- Scheduled service checks with normalized WHOIS hostnames, unsupported-TLD hints (e.g. `.app` / `.dev` → use SSL monitor), and reliable edit-form prefill
- **Billing Tracking** -- Price, billing cycle, expiration alerts, traffic limits per server
- **Backup & Restore** -- SQLite database backup/restore via admin API
- **Agent Auto-update** -- Remote binary upgrade with SHA-256 verification
- **Guided Deployment Management** -- `serverbee` CLI installs, upgrades, inspects, reconfigures, and uninstalls server and agent deployments in interactive or unattended mode
- **OpenAPI Documentation** -- Swagger UI at `/swagger-ui` with 50+ documented endpoints

## Tech Stack

| Component | Technology |
|-----------|-----------|
| Server | Rust, Axum 0.8, sea-orm, SQLite (WAL) |
| Agent | Rust, sysinfo 0.33, tokio-tungstenite |
| Frontend | React 19, Vite 7, TanStack Router/Query, Recharts, shadcn/ui, Tailwind CSS v4 |
| Auth | argon2 password hashing, session cookies, API keys, OAuth2, TOTP |
| Docs | Fumadocs MDX, TanStack Start, CN+EN bilingual |

## Quick Start

### Prerequisites

- Rust 1.85+ (with cargo)
- Bun 1.x (for frontend build)

### Build from Source

```bash
# Clone
git clone https://github.com/ZingerLittleBee/ServerBee.git
cd ServerBee

# Build frontend
cd apps/web && bun install && bun run build && cd ../..

# Build server and agent
cargo build --release

# Binaries are at:
# target/release/serverbee-server
# target/release/serverbee-agent
```

### Run the Server

```bash
./serverbee-server
# Default: http://localhost:9527
# Admin password is auto-generated and printed to startup log
# Auto-discovery key is also printed on first startup
```

### Run the Agent

```bash
# Set server URL and discovery key via environment variables
SERVERBEE_SERVER_URL=http://your-server:9527 \
SERVERBEE_AUTO_DISCOVERY_KEY=YOUR_KEY \
./serverbee-agent

# Or create /etc/serverbee/agent.toml:
# server_url = "http://your-server:9527"
# auto_discovery_key = "YOUR_KEY"
```

After registration, the agent saves its token to config and reconnects automatically on restart.

### Docker

```bash
docker compose up -d
```

### Development (Make)

```bash
# Start server (port 9527) + Vite dev server (port 5173) concurrently
make dev-full
# Visit http://localhost:5173, login with admin / admin123

# Or step by step:
make server-dev                                           # Terminal 1: server on :9527
SERVERBEE_AUTO_DISCOVERY_KEY="<key>" make agent-dev       # Terminal 2: agent

# Testing & code quality:
make cargo-test        # Run all Rust tests (395)
make test              # Run frontend tests (248)
make cargo-clippy      # Lint Rust code
make                   # Interactive menu (requires fzf)
```

Manual browser verification checklists are indexed in `tests/README.md`.

The server prints the full auto-discovery key on startup. Copy it to start the agent, or regenerate it later from Settings if needed.

> **Note**: `make dev-full` starts a Vite dev server with HMR at `http://localhost:5173` (proxies `/api/*` to the Rust server at `:9527`). For production builds, use `make build` then `make server-run`.

## Configuration

All config options can be set via TOML files or environment variables with `SERVERBEE_` prefix and `__` (double underscore) as nested separator. See [ENV.md](ENV.md) for the complete environment variable reference.

### Server (`/etc/serverbee/server.toml`)

```toml
[server]
listen = "0.0.0.0:9527"
data_dir = "/var/lib/serverbee"
trusted_proxies = []              # Defaults to private/loopback CIDRs; set to [] to disable

[database]
path = "serverbee.db"
max_connections = 10

[auth]
session_ttl = 86400           # 24 hours
secure_cookie = true          # Set false for HTTP-only dev
auto_discovery_key = ""       # Leave empty to auto-generate
max_servers = 0               # Soft limit for new auto-registered servers

[admin]
username = "admin"
password = ""                 # Leave empty to auto-generate

[rate_limit]
login_max = 5                 # Max login attempts per 15min window
register_max = 3              # Max agent registrations per 15min window

[retention]
records_days = 7              # Raw metrics retention
records_hourly_days = 90      # Hourly aggregates retention
audit_logs_days = 180         # Audit log retention
network_probe_days = 7        # Network probe raw records retention
network_probe_hourly_days = 90 # Network probe hourly aggregates retention
traffic_hourly_days = 7        # Traffic hourly records retention
traffic_daily_days = 400       # Traffic daily records retention

[scheduler]
timezone = "UTC"               # Timezone for daily traffic aggregation (e.g. Asia/Shanghai)

[geoip]
mmdb_path = "/var/lib/serverbee/GeoLite2-City.mmdb"  # Non-empty path enables GeoIP

[upgrade]
release_base_url = "https://github.com/ZingerLittleBee/ServerBee/releases"
```

Environment variable examples:
```bash
export SERVERBEE_ADMIN__PASSWORD="my-secure-password"
export SERVERBEE_AUTH__MAX_SERVERS="50"
export SERVERBEE_GEOIP__MMDB_PATH="/path/to/GeoLite2-City.mmdb"
export SERVERBEE_OAUTH__GITHUB__CLIENT_ID="..."
```

### Agent (`/etc/serverbee/agent.toml`)

```toml
server_url = "http://your-server:9527"
token = ""                    # Auto-populated after registration
auto_discovery_key = ""       # Used only for first registration

[collector]
interval = 3                  # Seconds between metric reports
enable_temperature = true
enable_gpu = false            # Requires NVIDIA GPU + nvml

[log]
level = "info"
```

Agent environment variables use the `SERVERBEE_` prefix without nesting (top-level keys):
```bash
export SERVERBEE_SERVER_URL="http://your-server:9527"
export SERVERBEE_AUTO_DISCOVERY_KEY="YOUR_KEY"
```

### OAuth Setup

```toml
[oauth]
base_url = "https://monitor.example.com"
allow_registration = false    # Auto-create users on first OAuth login

[oauth.github]
client_id = "..."
client_secret = "..."

[oauth.google]
client_id = "..."
client_secret = "..."
```

Callback URL format: `https://your-domain/api/auth/oauth/{provider}/callback`

## Deployment

### Railway (One-Click)

[![Deploy on Railway](https://railway.com/button.svg)](https://railway.com/deploy/serverbee-server)

1. Click the button above, set `SERVERBEE_ADMIN__PASSWORD`, and deploy
2. Add a volume mounted at `/data` to persist data across deploys
3. Configure your agents to connect to the Railway URL

### Install Script

Install via curl (one-liner):

```bash
# Server
curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh | sudo bash -s -- server

# Agent (replace with your server URL and discovery key)
curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh | sudo bash -s -- agent \
  --server-url http://YOUR_SERVER:9527 --discovery-key YOUR_KEY
```

The installer automatically places a `serverbee` management CLI at `/usr/local/bin/serverbee`.

> **Note**: Re-running `install agent` adopts an existing `/usr/local/bin/serverbee-agent` instead of replacing it. Use `sudo serverbee upgrade agent -y` (or replace the binary manually) when you need to refresh an existing installation.

### Management

```bash
sudo serverbee status              # View status of all components
sudo serverbee upgrade -y           # Upgrade all to latest version
sudo serverbee restart              # Restart all services
sudo serverbee config               # View current config
sudo serverbee config set <key> <value>  # Update config
sudo serverbee uninstall agent -y   # Uninstall agent
sudo serverbee uninstall server --purge  # Uninstall server + remove data
```

### Reverse Proxy (Nginx)

```nginx
server {
    listen 443 ssl;
    server_name monitor.example.com;

    location / {
        proxy_pass http://127.0.0.1:9527;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    # WebSocket (browser + agent + terminal)
    location /api/ws/ {
        proxy_pass http://127.0.0.1:9527;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_read_timeout 86400s;
        proxy_send_timeout 86400s;
    }

    location /api/agent/ws {
        proxy_pass http://127.0.0.1:9527;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_read_timeout 86400s;
        proxy_send_timeout 86400s;
    }
}
```

## API

Interactive API documentation is available at `/swagger-ui` when the server is running.

## License

[AGPL-3.0](LICENSE)
