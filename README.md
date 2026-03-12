# ServerBee

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

## Quick Start

### Prerequisites

- Rust 1.75+ (with cargo)
- Bun or Node.js 18+ (for frontend build)

### Build from Source

```bash
# Clone
git clone https://github.com/user/serverbee.git
cd serverbee

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
# Default admin: admin / admin (change immediately!)
# Auto-discovery key is printed on startup
```

### Run the Agent

```bash
./serverbee-agent --server-url http://your-server:9527 --discovery-key YOUR_KEY
```

### Docker

```bash
docker compose up -d
```

## Configuration

### Server (`config.toml`)

```toml
[server]
listen = "0.0.0.0:9527"
data_dir = "./data"

[database]
path = "serverbee.db"
max_connections = 5

[auth]
session_ttl = 86400           # 24 hours
secure_cookie = false         # Set true behind HTTPS proxy
auto_discovery_key = ""       # Leave empty to auto-generate

[admin]
username = "admin"
password = "admin"            # Only used on first run

[rate_limit]
login_max = 5                 # Max login attempts per 15min window
register_max = 10             # Max agent registrations per 15min window

[geoip]
enabled = false
mmdb_path = "./data/GeoLite2-City.mmdb"
```

### Agent (`agent-config.toml`)

```toml
[agent]
server_url = "http://localhost:9527"
discovery_key = "YOUR_KEY"
report_interval = 5           # Seconds between reports

[collector]
enable_temperature = true
enable_gpu = false            # Requires NVIDIA GPU + nvml
```

### OAuth Setup

Set environment variables or add to `config.toml`:

```toml
[oauth.github]
client_id = "..."
client_secret = "..."

[oauth.google]
client_id = "..."
client_secret = "..."
```

## Deployment

### Systemd

```bash
# Install (downloads and sets up systemd services)
curl -fsSL https://raw.githubusercontent.com/user/serverbee/main/deploy/install.sh | bash
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

    location /api/ws/ {
        proxy_pass http://127.0.0.1:9527;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }
}
```

## API

Interactive API documentation is available at `/swagger-ui` when the server is running.

## License

MIT
