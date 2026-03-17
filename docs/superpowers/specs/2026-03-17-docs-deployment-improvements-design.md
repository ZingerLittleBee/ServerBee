# Docs & Deployment Improvements Design

**Date:** 2026-03-17
**Status:** Approved

## Problem

The documentation site (`apps/docs/`) has critical usability issues that prevent new users from navigating effectively:

1. **~99 broken internal links** — all Card `href` and Markdown links use `/docs/{lang}/{slug}` but the actual route structure is `/{lang}/docs/{slug}`
2. **Incorrect health check endpoints** — docs reference `/api/health` and `/api/status/health` but the actual endpoint is `/healthz`
3. **Wrong Docker image names** — docs use `ghcr.io/zingerbee/serverbee:latest` but actual images are `ghcr.io/zingerlittlebee/serverbee-server:latest` and `ghcr.io/zingerlittlebee/serverbee-agent:latest`
4. **Non-existent install script URLs** — `https://get.serverbee.io` is not configured; `deploy/install.sh` exists locally but docs reference a non-existent hosted URL
5. **Inconsistent GitHub repository URLs** — docs use `zingerbee/ServerBee` and `ZingerBee/ServerBee` but the actual repo is `ZingerLittleBee/ServerBee`
6. **Missing Traefik reverse proxy documentation**
7. **Missing Agent Docker deployment documentation**
8. **Inconsistent `server_url` format** — mix of `ws://` and `http://` across docs
9. **EN/CN Docker Compose inconsistencies** — EN version uses deprecated `version: "3.8"`, wrong volume path `/app/data`, `curl` in Alpine healthcheck

## Decisions

| Decision | Outcome |
|----------|---------|
| Install script strategy | Refactor `deploy/install.sh` into an interactive installer with Docker support. All broken URLs in docs (`get.serverbee.io`, stale `raw.githubusercontent.com` paths) are replaced with the canonical raw URL: `https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh`. |
| Docs site URL | `https://server-bee-docs.vercel.app` — used in install script output and docs cross-references |
| Agent Docker deployment | Add docs, but mark as not recommended — Agent is green software (single binary, no residual files) |
| Traefik docs | Docker labels approach |
| `server_url` format | Standardize on `http://` / `https://` (Agent auto-converts to `ws://`/`wss://`) |

## Scope of Changes

### 1. Broken Links Fix (~99 links across 32 MDX files)

**Rule:** `/docs/{lang}/{slug}` → `/{lang}/docs/{slug}`

Implementation approach: comprehensive find-and-replace of the pattern `/docs/en/` → `/en/docs/` and `/docs/cn/` → `/cn/docs/` across all MDX files. Do not rely on manual link counting — replace all occurrences.

Applies to:
- Card component `href` attributes
- Markdown inline links `[text](/path)`

Affected files (all in `apps/docs/content/docs/`):
- `en/`: index, quick-start, admin, api-reference, capabilities, security, status-page, deployment, terminal, agent
- `cn/`: index, quick-start, agent, server, deployment, alerts, monitoring, capabilities, ping, terminal, configuration, security, admin, api-reference, architecture, status-page

### 2. Health Check Endpoint Fix

All references to `/api/health` or `/api/status/health` → `/healthz`

Affected locations:
- `cn/deployment.mdx` — all occurrences (healthcheck command, health check section, external monitoring section)
- `en/deployment.mdx` — all occurrences (healthcheck command, health check section, external monitoring URL example)
- Docker healthcheck commands in both deployment docs and root `docker-compose.yml`

Implementation: search for `/api/health` and `/api/status/health` across all docs and replace with `/healthz`.

### 3. Docker Image Name Fix

All references to `ghcr.io/zingerbee/serverbee:latest` updated to:
- Server: `ghcr.io/zingerlittlebee/serverbee-server:latest`
- Agent: `ghcr.io/zingerlittlebee/serverbee-agent:latest`

Affected files:
- Root `docker-compose.yml`
- `cn/quick-start.mdx`, `en/quick-start.mdx`
- `cn/deployment.mdx`, `en/deployment.mdx`
- `cn/server.mdx`, `en/server.mdx`

### 3.1 GitHub Repository URL Normalization

All GitHub URLs must use the canonical owner `ZingerLittleBee/ServerBee`. Replace:
- `zingerbee/ServerBee` → `ZingerLittleBee/ServerBee`
- `ZingerBee/ServerBee` → `ZingerLittleBee/ServerBee`

This applies to release page links, `wget`/download URLs, `git clone` URLs, and any other GitHub references.

Affected files:
- `en/quick-start.mdx` (releases link, git clone URL)
- `cn/quick-start.mdx` (git clone URL)
- `en/agent.mdx` (releases link)
- `cn/agent.mdx` (releases link, wget URL, git clone URL)
- `en/server.mdx` (releases link)
- `cn/server.mdx` (releases link, wget URL, git clone URL)
- `cn/deployment.mdx` (wget URL)

Implementation: search-and-replace all `github.com/zingerbee/ServerBee` and `github.com/ZingerBee/ServerBee` (case-insensitive) → `github.com/ZingerLittleBee/ServerBee`.

### 4. Quick Start Rewrite (`quick-start.mdx` CN + EN)

- Replace all broken install script URLs with the canonical raw URL: `https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh`
  - Server: `curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh | sudo bash -s server`
  - Agent: `curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh | sudo bash -s agent`
- Keep three methods: Docker Compose (recommended), Install Script, Source Build
- EN-specific fixes: remove `version: "3.8"`, fix volume path `/app/data` → `/data`, fix default password description
- Standardize `server_url` to `http://` format

### 5. Agent Docs Update (`agent.mdx` CN + EN)

- Replace all broken install script URLs (`get.serverbee.io`, `raw.githubusercontent.com/.../install-agent.sh`) with the canonical raw URL: `curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh | sudo bash -s agent`
- Binary download as primary method
- New section: "Docker（不推荐）" / "Docker (Not Recommended)"
  - Callout: Agent is green software — single binary, no folders or residual files
  - Docker run example with `--privileged`, `--net=host`, `--pid=host`, volume mounts (`/proc`, `/sys`, `/etc/serverbee`)
  - `/etc/serverbee` volume is mandatory for token persistence — without it, container recreation causes duplicate server entries
  - Limitations: needs privileged mode, temperature/GPU may not work, terminal accesses container not host
  - Image: `ghcr.io/zingerlittlebee/serverbee-agent:latest`
- Standardize all `server_url` to `http://` format

### 6. Deployment Guide Update (`deployment.mdx` CN + EN)

#### 6.1 Docker Compose Section
- EN: remove `version: "3.8"`, fix volume `/app/data` → `/data`, healthcheck `curl` → `wget`, endpoint → `/healthz`, change `container_name: serverbee` → `container_name: serverbee-server`
- CN: fix healthcheck endpoint → `/healthz` (CN already uses `wget`, no change needed there)
- Both: fix image name, service name `serverbee` → `serverbee-server`, add `container_name: serverbee-server` (required for `docker cp` commands in backup section), unify to consistent configuration
- **Propagate name change to all commands**: update `docker compose logs -f serverbee` → `docker compose logs -f serverbee-server`, `docker compose exec serverbee ...` → `docker compose exec serverbee-server ...`, `docker cp serverbee:...` → `docker cp serverbee-server:...`, etc. across both CN and EN deployment docs. The `container_name: serverbee-server` ensures `docker cp` works (it requires real container names, not Compose service names).

#### 6.1.1 `server_url` Standardization in Deployment Docs
- `cn/deployment.mdx` TLS section: change `server_url = "wss://..."` → `server_url = "https://..."`
- `en/deployment.mdx` Agent HTTPS section: ensure `server_url` uses `https://` format

#### 6.2 New Traefik Section (after Caddy)
Docker labels approach:

```yaml
services:
  serverbee-server:
    image: ghcr.io/zingerlittlebee/serverbee-server:latest
    volumes:
      - serverbee-data:/data
    environment:
      - SERVERBEE_ADMIN__PASSWORD=your_secure_password
    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.serverbee.rule=Host(`monitor.example.com`)"
      - "traefik.http.routers.serverbee.entrypoints=websecure"
      - "traefik.http.routers.serverbee.tls.certresolver=letsencrypt"
      - "traefik.http.services.serverbee.loadbalancer.server.port=9527"
    restart: unless-stopped
    networks:
      - traefik

networks:
  traefik:
    external: true

volumes:
  serverbee-data:
```

Note that Traefik auto-detects WebSocket upgrades — no extra configuration needed.

#### 6.3 Install Script References
Replace any broken install script URLs (`get.serverbee.io`, stale `raw.githubusercontent.com` paths) with the canonical raw URL pointing to `deploy/install.sh` in the repo. For systemd section binary acquisition, use GitHub Releases direct download.

### 7. Server Docs Update (`server.mdx` CN + EN)

- Fix Docker image name
- Standardize `server_url` to `http://` format
- Add cross-reference to deployment guide for full reverse proxy docs (including Traefik)

### 7.1 Configuration & Agent Docs — `server_url` Description Text

Beyond code examples, the descriptive text must also be updated:
- `cn/agent.mdx` line 128: `Server 的 WebSocket 地址` → `Server 的地址`
- `cn/configuration.mdx` line 219: `# Server 的 WebSocket 地址（必填）` → `# Server 地址（必填）`
- Standardize any `ws://` / `wss://` references in `server_url` examples to `http://` / `https://` across `configuration.mdx` (CN + EN) and `agent.mdx` (CN + EN)

### 8. Root `docker-compose.yml` Update

```yaml
services:
  serverbee-server:
    image: ghcr.io/zingerlittlebee/serverbee-server:latest
    container_name: serverbee-server
    ports:
      - "9527:9527"
    volumes:
      - serverbee-data:/data
    environment:
      - SERVERBEE_ADMIN__USERNAME=admin
      - SERVERBEE_ADMIN__PASSWORD=changeme
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "wget", "--spider", "-q", "http://localhost:9527/healthz"]
      interval: 30s
      timeout: 5s
      retries: 3
      start_period: 10s

volumes:
  serverbee-data:
```

Changes: image name fix, service name `serverbee` → `serverbee-server`, add `container_name: serverbee-server` (for `docker cp` compat), add healthcheck with correct `/healthz` endpoint.

### 9. Install Script Refactor (`deploy/install.sh`)

Complete rewrite of the install script into an interactive installer with hybrid CLI/interactive mode.

#### 9.1 Command Line Interface

The script accepts both **positional arguments** (backward compatible with README and existing docs) and **named flags** (new, more expressive).

**Positional argument (backward compat):** The first non-flag argument is treated as `--component`. This preserves compatibility with `bash install.sh server` and `curl ... | sudo bash -s agent`.

```bash
# Backward compatible (positional arg = component)
sudo bash deploy/install.sh server
sudo bash deploy/install.sh agent
curl -fsSL <url> | sudo bash -s server
curl -fsSL <url> | sudo bash -s agent

# Full auto with named flags (CI/scripted)
sudo bash deploy/install.sh \
  --component agent \
  --method binary \
  --server-url http://10.0.0.1:9527 \
  --discovery-key abc123

# Mixed: positional component + named flags
sudo bash deploy/install.sh server --method docker --password mypass

# Fully interactive
sudo bash deploy/install.sh
```

Parameters:

| Parameter | Description | Default |
|-----------|-------------|---------|
| First positional arg | `server` or `agent` (alias for `--component`) | Interactive selection |
| `--component` | `server` or `agent` | Interactive selection |
| `--method` | `binary` or `docker` | Interactive selection |
| `--server-url` | Agent connection URL | Interactive input (required for agent) |
| `--discovery-key` | Auto-discovery key | Interactive input (required for agent) |
| `--password` | Server admin password | Optional, see §9.6 |
| `--yes` / `-y` | Skip confirmation prompts | false |

If both a positional arg and `--component` are provided, `--component` takes precedence.

#### 9.2 Interactive Flow

1. **Select component**: `[1] Server  [2] Agent`
2. **Select method**: `[1] Binary (recommended)  [2] Docker`
   - If agent + docker: display warning (see §9.4), ask confirmation, default No
3. **Collect parameters**:
   - Server: admin password (optional, Enter to skip = auto-generate)
   - Agent: `server_url` (required), `auto_discovery_key` (required)
4. **Execute installation**
5. **Print result** with success message, docs links, and optional config hints

#### 9.3 Installation Behavior

**Binary method:**
- Detect OS (linux/darwin) and arch (amd64/arm64), fail on unsupported
- Fetch latest release tag from GitHub API
- Download binary to `/usr/local/bin/serverbee-{component}`
- Server: create `/var/lib/serverbee`, generate systemd service, `systemctl enable && start`
- Agent: create `/etc/serverbee/agent.toml` with user-provided `server_url` and `auto_discovery_key`, generate systemd service, `systemctl enable` but **do NOT start** — prompt user to verify config first

**Docker method:**
- Check `docker` and `docker compose` are available, fail with install instructions if not
- Server: generate `docker-compose.yml` in `/opt/serverbee/` (or current dir), run `docker compose up -d`
  - Image: `ghcr.io/zingerlittlebee/serverbee-server:latest`
  - Ports: `9527:9527`, volume: `serverbee-data:/data`, healthcheck on `/healthz`
- Agent: generate `/etc/serverbee/agent.toml` with user-provided `server_url` and `auto_discovery_key`, then run `docker run` with `--name serverbee-agent --privileged --net=host --pid=host -v /proc:/host/proc:ro -v /sys:/host/sys:ro -v /etc/serverbee:/etc/serverbee`
  - Image: `ghcr.io/zingerlittlebee/serverbee-agent:latest`
  - **Config is passed via mounted `agent.toml`, NOT env vars** — this ensures `docker restart` picks up file edits, and the token written by Agent after registration persists across container recreation.
  - The installer creates the config file on the host, same as the binary path. This makes "edit agent.toml + docker restart" the single consistent workflow for both methods.

#### 9.4 Agent Docker Warning

When user selects agent + docker (interactive or CLI without `--yes`):

```
⚠️  不推荐使用 Docker 部署 Agent / Docker is NOT recommended for Agent

  ServerBee Agent 是绿色软件 / ServerBee Agent is portable software:
  - 只有一个二进制文件，不会产生文件夹和其他文件残留
    Single binary, no folders or residual files created
  - 卸载只需删除二进制文件和配置文件
    Uninstall by simply deleting the binary and config file
  - Docker 部署需要 --privileged 权限才能采集完整指标
    Docker deployment requires --privileged for full metrics collection
  - Web 终端功能将访问容器内环境，而非宿主机
    Web terminal accesses the container, not the host

  推荐选择 binary 方式安装 / Binary installation is recommended

  是否仍要使用 Docker？/ Continue with Docker? [y/N]
```

Default is No. `--yes` flag skips this prompt.

#### 9.5 Post-Install Output

**Server (binary):**
```
✅ ServerBee Server installed successfully!

  Dashboard:  http://<detected-ip>:9527
  Username:   admin
  Password:   <if --password: show it; else: "(check logs: sudo journalctl -u serverbee-server | grep 'Generated admin password')">

📖 More configuration:
  - Reverse proxy (Nginx/Caddy/Traefik): https://server-bee-docs.vercel.app/en/docs/deployment
  - Alerts & Notifications:              https://server-bee-docs.vercel.app/en/docs/alerts
  - Full configuration reference:        https://server-bee-docs.vercel.app/en/docs/configuration

  Config file: /etc/serverbee/server.toml
  Apply changes: edit the config file, then run: sudo systemctl restart serverbee-server
```

**Agent (binary):**
```
✅ ServerBee Agent installed successfully!

  Server URL: http://10.0.0.1:9527
  Status:     Awaiting start

  Start:  sudo systemctl start serverbee-agent
  Logs:   sudo journalctl -u serverbee-agent -f

📖 More configuration:
  - GPU monitoring:              https://server-bee-docs.vercel.app/en/docs/agent#gpu-monitoring
  - Full configuration reference: https://server-bee-docs.vercel.app/en/docs/configuration

  Config file: /etc/serverbee/agent.toml
  Apply changes: edit the config file, then run: sudo systemctl restart serverbee-agent
```

**Server (docker):**
```
✅ ServerBee Server started via Docker!

  Dashboard:  http://<detected-ip>:9527
  Username:   admin
  Password:   <if --password: show it; else: "(check logs: cd /opt/serverbee && docker compose logs serverbee-server | grep 'Generated admin password')">

  Compose file: /opt/serverbee/docker-compose.yml
  View logs:    cd /opt/serverbee && docker compose logs -f

📖 More configuration:
  - Reverse proxy: https://server-bee-docs.vercel.app/en/docs/deployment
  - Full reference: https://server-bee-docs.vercel.app/en/docs/configuration

  Apply changes: edit docker-compose.yml environment variables, then run:
    cd /opt/serverbee && docker compose up -d
```

**Agent (docker):**
```
✅ ServerBee Agent started via Docker!

  Server URL: http://10.0.0.1:9527
  Container:  serverbee-agent
  Config dir: /etc/serverbee (mounted as volume — token persists across restarts)

  View logs: docker logs -f serverbee-agent

📖 More configuration: https://server-bee-docs.vercel.app/en/docs/configuration

  Apply changes: edit /etc/serverbee/agent.toml, then:
    docker restart serverbee-agent
```

#### 9.6 Password Handling

The installer does NOT generate passwords itself. The server binary handles auto-generation internally on first startup (prints to stdout/log). The installer's behavior:

- **`--password` provided**: write it to config file (binary) or `SERVERBEE_ADMIN__PASSWORD` env var (docker). Print the password in post-install output.
- **`--password` omitted (binary)**: do NOT set `admin.password` in config. Server will auto-generate on first start and print to its own log. Post-install output says: `Password: (auto-generated, check server logs: sudo journalctl -u serverbee-server | grep "Generated admin password")`
- **`--password` omitted (docker)**: do NOT set `SERVERBEE_ADMIN__PASSWORD` env var. Post-install output says: `Password: (auto-generated, check logs: cd /opt/serverbee && docker compose logs serverbee-server | grep "Generated admin password")`

This delegates password generation entirely to the server binary, avoiding duplication of logic.

#### 9.7 Acceptance Criteria

All 4 installation paths must be verified:

| Path | Verification |
|------|-------------|
| Binary + Server | Service running, dashboard accessible at `:9527`, login works with configured/auto password |
| Binary + Agent | Config written to `/etc/serverbee/agent.toml`, after manual start: agent appears online in dashboard |
| Docker + Server | Container running, healthcheck passing, dashboard accessible, `docker compose logs` shows startup |
| Docker + Agent | Container running, `/etc/serverbee/agent.toml` persists token after registration, `docker restart serverbee-agent` reconnects without creating duplicate server entry |

Additional checks:
- Positional arg backward compat: `bash install.sh server` and `bash install.sh agent` work
- Piped mode: `curl ... | sudo bash -s server` works
- `--yes` flag skips all prompts (for CI)
- Agent Docker warning appears and defaults to No
- Missing required params (agent without `--server-url`) trigger interactive prompt or error with `--yes`

## Files Changed (Updated)

| File | Change Type |
|------|-------------|
| `deploy/install.sh` | **Rewrite** — interactive installer with Docker support |
| `docker-compose.yml` | Edit |
| `apps/docs/content/docs/en/index.mdx` | Links fix |
| `apps/docs/content/docs/en/quick-start.mdx` | Rewrite |
| `apps/docs/content/docs/en/deployment.mdx` | Major edit |
| `apps/docs/content/docs/en/agent.mdx` | Major edit (install script URL, Docker section, links fix) |
| `apps/docs/content/docs/en/server.mdx` | Edit |
| `apps/docs/content/docs/en/admin.mdx` | Links fix |
| `apps/docs/content/docs/en/api-reference.mdx` | Links fix |
| `apps/docs/content/docs/en/capabilities.mdx` | Links fix |
| `apps/docs/content/docs/en/security.mdx` | Links fix |
| `apps/docs/content/docs/en/status-page.mdx` | Links fix |
| `apps/docs/content/docs/en/terminal.mdx` | Links fix |
| `apps/docs/content/docs/cn/index.mdx` | Links fix |
| `apps/docs/content/docs/cn/quick-start.mdx` | Rewrite |
| `apps/docs/content/docs/cn/deployment.mdx` | Major edit |
| `apps/docs/content/docs/cn/agent.mdx` | Major edit |
| `apps/docs/content/docs/cn/server.mdx` | Edit |
| `apps/docs/content/docs/cn/admin.mdx` | Links fix |
| `apps/docs/content/docs/cn/api-reference.mdx` | Links fix |
| `apps/docs/content/docs/cn/capabilities.mdx` | Links fix |
| `apps/docs/content/docs/cn/security.mdx` | Links fix |
| `apps/docs/content/docs/cn/status-page.mdx` | Links fix |
| `apps/docs/content/docs/cn/terminal.mdx` | Links fix |
| `apps/docs/content/docs/cn/alerts.mdx` | Links fix |
| `apps/docs/content/docs/cn/monitoring.mdx` | Links fix |
| `apps/docs/content/docs/cn/ping.mdx` | Links fix |
| `apps/docs/content/docs/cn/architecture.mdx` | Links fix |
| `apps/docs/content/docs/cn/configuration.mdx` | Links fix + `server_url` standardization |
| `apps/docs/content/docs/en/configuration.mdx` | `server_url` standardization |

Total: 30 files modified (1 rewrite + 29 edits), 0 new files.

## Out of Scope

- New documentation pages (no `reverse-proxy.mdx` split)
- README.md updates (README uses `bash deploy/install.sh server|agent` which remains backward compatible via positional arg support in the refactored script)
- Fumadocs site structural changes
- English deployment.mdx full rewrite to match CN structure (only targeted fixes)
