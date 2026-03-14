# ServerBee

Language: English | [简体中文](./README.zh-CN.md)

A lightweight, self-hosted VPS monitoring system built with Rust and React.

## Features

- **Real-time Dashboard** -- Server status, CPU/memory/disk/network metrics with live WebSocket updates
- **Server Groups** -- Organize servers by group with country flag display
- **Detailed Metrics** -- Historical charts (1h/6h/24h/7d/30d) for CPU, memory, disk, network, load, temperature, GPU
- **Alert System** -- 14+ metric types, threshold/offline/traffic/expiration rules, AND logic, 70% sampling
- **Notifications** -- Webhook, Telegram, Bark, Email (SMTP) channels with notification groups
- **Ping Monitoring** -- ICMP, TCP, HTTP probes with latency charts and success rate
- **Web Terminal** -- Browser-based PTY terminal via WebSocket proxy
- **GPU Monitoring** -- NVIDIA GPU usage/temperature/memory (via nvml-wrapper, feature-gated)
- **GeoIP** -- Automatic region/country detection from agent IP (MaxMind MMDB)
- **OAuth & 2FA** -- GitHub/Google/OIDC login, TOTP two-factor authentication
- **Multi-user** -- Admin/Member roles, audit logging, rate limiting
- **Capability Toggles** -- Per-server feature controls (terminal, exec, upgrade, ping) with defense-in-depth enforcement
- **Public Status Page** -- Unauthenticated status page with server groups and live metrics
- **Billing Tracking** -- Price, billing cycle, expiration alerts, traffic limits per server
- **Backup & Restore** -- SQLite database backup/restore via admin API
- **Agent Auto-update** -- Remote binary upgrade with SHA-256 verification
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
make cargo-test        # Run all Rust tests (121)
make test              # Run frontend tests (72)
make cargo-clippy      # Lint Rust code
make                   # Interactive menu (requires fzf)
```

The server prints the full auto-discovery key on startup. Copy it to start the agent.

> **Note**: `make dev-full` starts a Vite dev server with HMR at `http://localhost:5173` (proxies `/api/*` to the Rust server at `:9527`). For production builds, use `make build` then `make server-run`.

## Configuration

All config options can be set via TOML files or environment variables with `SERVERBEE_` prefix and `__` (double underscore) as nested separator.

### Server (`/etc/serverbee/server.toml`)

```toml
[server]
listen = "0.0.0.0:9527"
data_dir = "/var/lib/serverbee"

[database]
path = "serverbee.db"
max_connections = 10

[auth]
session_ttl = 86400           # 24 hours
secure_cookie = true          # Set false for HTTP-only dev
auto_discovery_key = ""       # Leave empty to auto-generate

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

[geoip]
enabled = false
mmdb_path = "/var/lib/serverbee/GeoLite2-City.mmdb"
```

Environment variable examples:
```bash
export SERVERBEE_ADMIN__PASSWORD="my-secure-password"
export SERVERBEE_GEOIP__ENABLED=true
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

### Systemd

```bash
# Install server
sudo bash deploy/install.sh server

# Install agent
sudo bash deploy/install.sh agent
```

Service files are provided in `deploy/`:
- `serverbee-server.service`
- `serverbee-agent.service`

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
