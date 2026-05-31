<div align="center">

<img src="assets/logo/logo.svg" width="96" alt="ServerBee logo" />

# ServerBee

**Lightweight, self-hosted VPS monitoring — one Rust binary, real-time everything.**

[![CI](https://github.com/ZingerLittleBee/ServerBee/actions/workflows/ci.yml/badge.svg)](https://github.com/ZingerLittleBee/ServerBee/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/ZingerLittleBee/ServerBee?include_prereleases&sort=semver)](https://github.com/ZingerLittleBee/ServerBee/releases)
[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](LICENSE)
[![GitHub stars](https://img.shields.io/github/stars/ZingerLittleBee/ServerBee?style=flat)](https://github.com/ZingerLittleBee/ServerBee/stargazers)
[![Rust](https://img.shields.io/badge/Rust-2024-000000?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![React](https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=black)](https://react.dev)

English | [简体中文](./README.zh-CN.md)

</div>

---

ServerBee watches all your servers from one place. A central **server** receives metrics from lightweight **agents** over WebSocket, stores them in embedded SQLite, and serves a real-time React dashboard — no external database, no heavy runtime.

- 🪶 **Tiny footprint** — agents typically use only ~5–15 MB of RAM, and the server stays lightweight as your fleet grows.
- ⚡ **Real-time** — live WebSocket dashboard for CPU, memory, disk, network, load, temperature, GPU, and disk I/O.
- 📦 **Single binary** — server + embedded web UI in one file. Deploy with Docker, a one-line script, or Railway.
- 🔋 **Batteries included** — alerts, notifications, web terminal, file manager, Docker, firewall, status pages, and more.
- 🔒 **Secure by default** — OAuth + 2FA, RBAC, audit logs, one-time agent enrollment, per-server capability gates.

> [!NOTE]
> ServerBee is in active development (`v1.0.0-alpha`). Expect rapid iteration.

## Quick Start

### 1. Install the server

```bash
curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh | sudo bash -s -- server --method docker
```

Open `http://your-server:9527`. The admin password is auto-generated and printed to the startup log — change it on first login.

> The install script supports both **Docker** and **native binary** installs via `--method docker|binary`. **Docker is recommended for the server**; omit the flag to choose interactively. Prefer the cloud? Use the [Railway one-click deploy](#railway-one-click) below.

### 2. Enroll an agent

Sign in as admin → **Settings** → generate a one-time **enrollment code** (single-use, expires in ~10 min). Then on each node:

```bash
curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh | sudo bash -s -- agent --method binary \
  --server-url http://YOUR_SERVER:9527 --enrollment-code YOUR_ONE_TIME_CODE
```

> A **native binary is recommended for agents** — smallest footprint and full host-level metrics. Pass `--method docker` to run the agent in a container instead.

The agent saves a per-server token on first connect and reconnects automatically afterwards — the code is only needed once. That's it. 🎉

## Features

| | |
|---|---|
| **📊 Monitoring** | Real-time metrics (CPU/mem/disk/network/load/temp/GPU/disk I/O) · historical charts (1h–30d) · Docker container stats, logs & events · monthly traffic statistics with billing-cycle prediction |
| **🔔 Alerts** | 14+ metric types · threshold / offline / traffic / expiration rules · Webhook, Telegram, Bark & Email channels with notification groups |
| **🌐 Network** | Ping monitoring (ICMP/TCP/HTTP) · network-quality probing (96 China 3-ISP + international presets) · service monitors (SSL/WHOIS/HTTP/Ping/TCP) · IP-quality & streaming-unlock detection with fraud scoring |
| **🛠️ Remote management** | Browser web terminal (PTY over WS) · sandboxed file manager with Monaco editor · firewall blocklist via nftables · per-server capability toggles · agent auto-update |
| **🔐 Security & access** | SSH login / brute-force / port-scan detection · OAuth (GitHub/Google/OIDC) + TOTP 2FA · Admin/Member RBAC · audit logs · one-time agent enrollment codes |
| **🖥️ Dashboards & sharing** | Drag-and-drop custom dashboards (13 widget types) · public status pages with 90-day uptime timelines · custom OKLCH themes · server groups with country flags · native iOS companion app |
| **⚙️ Ops** | `serverbee` management CLI · backup & restore · GeoIP region detection · OpenAPI/Swagger docs (50+ endpoints) |

## Configuration

Configure via TOML files or `SERVERBEE_`-prefixed environment variables (`__` is the nested separator, e.g. `SERVERBEE_AUTH__MAX_SERVERS`). The minimum to get going:

```toml
# /etc/serverbee/server.toml
[server]
listen = "0.0.0.0:9527"
data_dir = "/var/lib/serverbee"

[admin]
password = ""   # leave empty to auto-generate
```

```toml
# /etc/serverbee/agent.toml
server_url = "http://your-server:9527"
enrollment_code = ""   # one-time code from Settings; only used for first registration

[collector]
interval = 3           # seconds between reports
```

📖 Full reference: **[ENV.md](ENV.md)** · OAuth, retention, rate limiting, GeoIP, and more in the [documentation](apps/docs).

## Deployment

### Railway (one-click)

[![Deploy on Railway](https://railway.com/button.svg)](https://railway.com/deploy/serverbee-server)

Add a volume mounted at `/data` to persist data. The server auto-creates an admin account on first start — check the deploy logs for the credentials banner.

### Management CLI

The installer drops a `serverbee` CLI at `/usr/local/bin/serverbee`:

```bash
sudo serverbee status         # status of all components
sudo serverbee upgrade -y     # upgrade to latest
sudo serverbee restart        # restart services
sudo serverbee config         # view / set config
sudo serverbee uninstall agent -y
```

### Reverse proxy

Behind Nginx/Caddy, proxy `/` to `127.0.0.1:9527` and make sure the WebSocket routes `/api/ws/` and `/api/agent/ws` forward the `Upgrade`/`Connection` headers with a long read timeout. See the [deployment docs](apps/docs) for a ready-to-use Nginx config.

## Development

```bash
git clone https://github.com/ZingerLittleBee/ServerBee.git
cd ServerBee

make dev-full         # server (:9527) + Vite dev server (:5173) — login admin / admin123
make cargo-test       # Rust tests
make test             # frontend tests
make cargo-clippy     # Rust lint
```

> `make dev-full` runs Vite with HMR at `http://localhost:5173` and proxies `/api/*` to the Rust server at `:9527`. Generate a one-time enrollment code in **Settings** to connect a dev agent.

**Stack:** Rust (Axum 0.8 · sea-orm · SQLite WAL) · React 19 (Vite 7 · TanStack Router/Query · Recharts · shadcn/ui · Tailwind CSS v4) · Rust agents (sysinfo · tokio-tungstenite).

## API

Interactive OpenAPI docs are served at `/swagger-ui` while the server runs.

## License

[AGPL-3.0](LICENSE)
