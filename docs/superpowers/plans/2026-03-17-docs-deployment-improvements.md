# Docs & Deployment Improvements Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix ~99 broken links, correct Docker image names/health endpoints/GitHub URLs, add Traefik docs, add Agent Docker docs, rewrite install script as interactive installer.

**Architecture:** Four independent task groups that can run in parallel: (A) install script rewrite, (B) batch mechanical fixes on 19 files, (C) English content updates on 5 files, (D) Chinese content updates on 5 files. Tasks B/C/D share no files. Each task group produces a self-contained commit.

**Tech Stack:** Bash (install script), MDX (Fumadocs docs), YAML (docker-compose)

**Spec:** `docs/superpowers/specs/2026-03-17-docs-deployment-improvements-design.md`

---

## File Map

### Task A — Install Script (1 file)
| File | Action |
|------|--------|
| `deploy/install.sh` | Rewrite |

### Task B — Batch Mechanical Fixes (19 files)
| File | Action |
|------|--------|
| `docker-compose.yml` | Edit (image, service name, container_name, healthcheck) |
| `apps/docs/content/docs/en/index.mdx` | Link fix |
| `apps/docs/content/docs/en/admin.mdx` | Link fix |
| `apps/docs/content/docs/en/api-reference.mdx` | Link fix |
| `apps/docs/content/docs/en/capabilities.mdx` | Link fix |
| `apps/docs/content/docs/en/security.mdx` | Link fix |
| `apps/docs/content/docs/en/status-page.mdx` | Link fix |
| `apps/docs/content/docs/en/terminal.mdx` | Link fix |
| `apps/docs/content/docs/cn/index.mdx` | Link fix |
| `apps/docs/content/docs/cn/admin.mdx` | Link fix |
| `apps/docs/content/docs/cn/api-reference.mdx` | Link fix |
| `apps/docs/content/docs/cn/capabilities.mdx` | Link fix |
| `apps/docs/content/docs/cn/security.mdx` | Link fix |
| `apps/docs/content/docs/cn/status-page.mdx` | Link fix |
| `apps/docs/content/docs/cn/terminal.mdx` | Link fix |
| `apps/docs/content/docs/cn/alerts.mdx` | Link fix |
| `apps/docs/content/docs/cn/monitoring.mdx` | Link fix |
| `apps/docs/content/docs/cn/ping.mdx` | Link fix |
| `apps/docs/content/docs/cn/architecture.mdx` | Link fix |

### Task C — English Content Updates (5 files)
| File | Action |
|------|--------|
| `apps/docs/content/docs/en/quick-start.mdx` | Rewrite |
| `apps/docs/content/docs/en/deployment.mdx` | Major edit |
| `apps/docs/content/docs/en/agent.mdx` | Major edit |
| `apps/docs/content/docs/en/server.mdx` | Edit |
| `apps/docs/content/docs/en/configuration.mdx` | Edit |

### Task D — Chinese Content Updates (5 files)
| File | Action |
|------|--------|
| `apps/docs/content/docs/cn/quick-start.mdx` | Rewrite |
| `apps/docs/content/docs/cn/deployment.mdx` | Major edit |
| `apps/docs/content/docs/cn/agent.mdx` | Major edit |
| `apps/docs/content/docs/cn/server.mdx` | Edit |
| `apps/docs/content/docs/cn/configuration.mdx` | Edit |

---

## Task A: Install Script Rewrite

**Files:**
- Rewrite: `deploy/install.sh`

**Reference:** Spec §9.1–§9.7

- [ ] **Step 1: Write the argument parser**

Replace the entire `deploy/install.sh` with the new script skeleton. Include:
- `set -euo pipefail`
- Color helpers (`info`, `warn`, `error`)
- `REPO="ZingerLittleBee/ServerBee"`, `DOCS_URL="https://server-bee-docs.vercel.app"`
- `parse_args()` function:
  - Parse `--component`, `--method`, `--server-url`, `--discovery-key`, `--password`, `--yes`/`-y`
  - First positional non-flag arg → `COMPONENT` (backward compat with `bash install.sh server`)
  - `--component` overrides positional if both present
- `detect_os()`, `detect_arch()`, `get_latest_version()` (reuse from existing script)
- `get_local_ip()` — detect primary IP for post-install output

```bash
#!/usr/bin/env bash
set -euo pipefail

REPO="ZingerLittleBee/ServerBee"
INSTALL_DIR="/usr/local/bin"
CONFIG_DIR="/etc/serverbee"
DATA_DIR="/var/lib/serverbee"
DOCKER_DIR="/opt/serverbee"
DOCS_URL="https://server-bee-docs.vercel.app"

# Defaults
COMPONENT=""
METHOD=""
SERVER_URL=""
DISCOVERY_KEY=""
PASSWORD=""
YES=false

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*" >&2; exit 1; }

parse_args() {
    local positional_set=false
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --component)  COMPONENT="$2"; shift 2 ;;
            --method)     METHOD="$2"; shift 2 ;;
            --server-url) SERVER_URL="$2"; shift 2 ;;
            --discovery-key) DISCOVERY_KEY="$2"; shift 2 ;;
            --password)   PASSWORD="$2"; shift 2 ;;
            --yes|-y)     YES=true; shift ;;
            -*)           error "Unknown option: $1" ;;
            *)
                if [ "$positional_set" = false ] && [ -z "$COMPONENT" ]; then
                    COMPONENT="$1"
                    positional_set=true
                fi
                shift ;;
        esac
    done
}
```

- [ ] **Step 2: Write interactive prompts**

Add functions for each interactive step:

```bash
prompt_component() {
    if [ -n "$COMPONENT" ]; then return; fi
    echo ""
    echo -e "${BOLD}ServerBee Installer${NC}"
    echo "==================="
    echo ""
    echo "  [1] Server  — Dashboard & API"
    echo "  [2] Agent   — System metrics collector"
    echo ""
    read -rp "Select component [1/2]: " choice
    case "$choice" in
        1|server)  COMPONENT="server" ;;
        2|agent)   COMPONENT="agent" ;;
        *) error "Invalid choice: $choice" ;;
    esac
}

prompt_method() {
    if [ -n "$METHOD" ]; then return; fi
    echo ""
    echo "  [1] Binary  (recommended)"
    echo "  [2] Docker"
    echo ""
    read -rp "Select installation method [1/2]: " choice
    case "$choice" in
        1|binary)  METHOD="binary" ;;
        2|docker)  METHOD="docker" ;;
        *) error "Invalid choice: $choice" ;;
    esac
}

prompt_agent_docker_warning() {
    if [ "$COMPONENT" != "agent" ] || [ "$METHOD" != "docker" ]; then return; fi
    if [ "$YES" = true ]; then return; fi
    echo ""
    warn "不推荐使用 Docker 部署 Agent / Docker is NOT recommended for Agent"
    echo ""
    echo "  ServerBee Agent 是绿色软件 / ServerBee Agent is portable software:"
    echo "  - 只有一个二进制文件，不会产生文件夹和其他文件残留"
    echo "    Single binary, no folders or residual files created"
    echo "  - 卸载只需删除二进制文件和配置文件"
    echo "    Uninstall by simply deleting the binary and config file"
    echo "  - Docker 部署需要 --privileged 权限才能采集完整指标"
    echo "    Docker deployment requires --privileged for full metrics collection"
    echo "  - Web 终端功能将访问容器内环境，而非宿主机"
    echo "    Web terminal accesses the container, not the host"
    echo ""
    echo "  推荐选择 binary 方式安装 / Binary installation is recommended"
    echo ""
    read -rp "  是否仍要使用 Docker？/ Continue with Docker? [y/N]: " confirm
    case "$confirm" in
        [yY]|[yY][eE][sS]) ;;
        *) METHOD="binary"; info "Switched to binary installation." ;;
    esac
}

prompt_server_params() {
    if [ "$COMPONENT" != "server" ]; then return; fi
    if [ -z "$PASSWORD" ] && [ "$YES" != true ]; then
        echo ""
        read -rp "Admin password (Enter to skip, auto-generated on first start): " PASSWORD
    fi
}

prompt_agent_params() {
    if [ "$COMPONENT" != "agent" ]; then return; fi
    while [ -z "$SERVER_URL" ]; do
        if [ "$YES" = true ]; then error "--server-url is required for agent installation"; fi
        read -rp "Server URL (e.g., http://10.0.0.1:9527): " SERVER_URL
    done
    while [ -z "$DISCOVERY_KEY" ]; do
        if [ "$YES" = true ]; then error "--discovery-key is required for agent installation"; fi
        read -rp "Auto-discovery key: " DISCOVERY_KEY
    done
}
```

- [ ] **Step 3: Write OS/arch detection and download helpers**

Reuse and clean up from existing script:

```bash
detect_os() {
    local os
    os=$(uname -s | tr '[:upper:]' '[:lower:]')
    case "$os" in
        linux)  echo "linux" ;;
        darwin) echo "darwin" ;;
        *) error "Unsupported OS: $os" ;;
    esac
}

detect_arch() {
    local arch
    arch=$(uname -m)
    case "$arch" in
        x86_64|amd64)  echo "amd64" ;;
        aarch64|arm64) echo "arm64" ;;
        *) error "Unsupported architecture: $arch" ;;
    esac
}

get_latest_version() {
    curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep '"tag_name"' \
        | head -1 \
        | sed 's/.*"tag_name": *"//;s/".*//'
}

get_local_ip() {
    # Try multiple methods
    ip -4 route get 1.1.1.1 2>/dev/null | awk '{print $7; exit}' \
        || hostname -I 2>/dev/null | awk '{print $1}' \
        || echo "localhost"
}

download_binary() {
    local component="$1" version="$2" os="$3" arch="$4"
    local filename="serverbee-${component}-${os}-${arch}"
    local url="https://github.com/${REPO}/releases/download/${version}/${filename}"

    info "Downloading serverbee-${component} ${version} for ${os}/${arch}..."
    curl -fsSL -o "/tmp/serverbee-${component}" "$url" \
        || error "Download failed: $url"
    chmod +x "/tmp/serverbee-${component}"
    mv "/tmp/serverbee-${component}" "${INSTALL_DIR}/serverbee-${component}"
    info "Installed to ${INSTALL_DIR}/serverbee-${component}"
}

check_docker() {
    command -v docker &>/dev/null || error "Docker is not installed. Install it first: https://docs.docker.com/get-docker/"
    docker compose version &>/dev/null || error "Docker Compose V2 is not available. Install it first: https://docs.docker.com/compose/install/"
}
```

- [ ] **Step 4: Write binary installation functions**

```bash
install_binary_server() {
    local version os arch
    os=$(detect_os)
    arch=$(detect_arch)
    version=$(get_latest_version)
    [ -z "$version" ] && error "Failed to get latest version from GitHub"

    download_binary "server" "$version" "$os" "$arch"
    mkdir -p "$DATA_DIR" "$CONFIG_DIR"

    # Generate server.toml if password provided
    if [ -n "$PASSWORD" ] && [ ! -f "${CONFIG_DIR}/server.toml" ]; then
        cat > "${CONFIG_DIR}/server.toml" << TOML
[server]
data_dir = "${DATA_DIR}"

[admin]
password = "${PASSWORD}"
TOML
    fi

    # Create systemd service
    if command -v systemctl &>/dev/null; then
        cat > /etc/systemd/system/serverbee-server.service << 'UNIT'
[Unit]
Description=ServerBee Dashboard
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/serverbee-server
WorkingDirectory=/var/lib/serverbee
Environment=SERVERBEE_SERVER__DATA_DIR=/var/lib/serverbee
Restart=always
RestartSec=5
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
UNIT
        systemctl daemon-reload
        systemctl enable serverbee-server
        systemctl start serverbee-server
        info "Server service started and enabled"
    else
        warn "systemd not found. Start manually: serverbee-server"
    fi
}

install_binary_agent() {
    local version os arch
    os=$(detect_os)
    arch=$(detect_arch)
    version=$(get_latest_version)
    [ -z "$version" ] && error "Failed to get latest version from GitHub"

    download_binary "agent" "$version" "$os" "$arch"
    mkdir -p "$CONFIG_DIR"

    # Generate agent.toml
    if [ ! -f "${CONFIG_DIR}/agent.toml" ]; then
        cat > "${CONFIG_DIR}/agent.toml" << TOML
server_url = "${SERVER_URL}"
auto_discovery_key = "${DISCOVERY_KEY}"

[collector]
interval = 3
enable_temperature = true
TOML
        info "Created ${CONFIG_DIR}/agent.toml"
    else
        warn "${CONFIG_DIR}/agent.toml already exists, not overwriting"
    fi

    # Create systemd service
    if command -v systemctl &>/dev/null; then
        cat > /etc/systemd/system/serverbee-agent.service << 'UNIT'
[Unit]
Description=ServerBee Agent
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/serverbee-agent
WorkingDirectory=/etc/serverbee
Restart=always
RestartSec=5
AmbientCapabilities=CAP_NET_RAW

[Install]
WantedBy=multi-user.target
UNIT
        systemctl daemon-reload
        systemctl enable serverbee-agent
        warn "Agent service enabled but NOT started — verify config first, then run: sudo systemctl start serverbee-agent"
    else
        warn "systemd not found. Start manually: serverbee-agent"
    fi
}
```

- [ ] **Step 5: Write Docker installation functions**

```bash
install_docker_server() {
    check_docker
    mkdir -p "$DOCKER_DIR"

    local password_env=""
    if [ -n "$PASSWORD" ]; then
        password_env="      - SERVERBEE_ADMIN__PASSWORD=${PASSWORD}"
    fi

    cat > "${DOCKER_DIR}/docker-compose.yml" << YAML
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
${password_env:+${password_env}
}    restart: unless-stopped
    healthcheck:
      test: ["CMD", "wget", "--spider", "-q", "http://localhost:9527/healthz"]
      interval: 30s
      timeout: 5s
      retries: 3
      start_period: 10s

volumes:
  serverbee-data:
YAML

    info "Generated ${DOCKER_DIR}/docker-compose.yml"
    cd "$DOCKER_DIR"
    docker compose up -d
    info "Server container started"
}

install_docker_agent() {
    check_docker
    mkdir -p "$CONFIG_DIR"

    # Generate agent.toml on host (same as binary path)
    if [ ! -f "${CONFIG_DIR}/agent.toml" ]; then
        cat > "${CONFIG_DIR}/agent.toml" << TOML
server_url = "${SERVER_URL}"
auto_discovery_key = "${DISCOVERY_KEY}"

[collector]
interval = 3
enable_temperature = true
TOML
        info "Created ${CONFIG_DIR}/agent.toml"
    else
        warn "${CONFIG_DIR}/agent.toml already exists, not overwriting"
    fi

    # Stop existing container if present
    docker stop serverbee-agent 2>/dev/null && docker rm serverbee-agent 2>/dev/null || true

    docker run -d \
        --name serverbee-agent \
        --privileged \
        --net=host \
        --pid=host \
        -v /proc:/host/proc:ro \
        -v /sys:/host/sys:ro \
        -v /etc/serverbee:/etc/serverbee \
        --restart unless-stopped \
        ghcr.io/zingerlittlebee/serverbee-agent:latest

    info "Agent container started"
}
```

- [ ] **Step 6: Write post-install output functions**

```bash
print_server_result() {
    local ip
    ip=$(get_local_ip)
    echo ""
    echo -e "${GREEN}✅ ServerBee Server installed successfully!${NC}"
    echo ""
    echo "  Dashboard:  http://${ip}:9527"
    echo "  Username:   admin"
    if [ -n "$PASSWORD" ]; then
        echo "  Password:   ${PASSWORD}"
    elif [ "$METHOD" = "binary" ]; then
        echo "  Password:   (auto-generated, check logs: sudo journalctl -u serverbee-server | grep 'Generated admin password')"
    else
        echo "  Password:   (auto-generated, check logs: cd ${DOCKER_DIR} && docker compose logs serverbee-server | grep 'Generated admin password')"
    fi
    echo ""
    echo -e "📖 More configuration:"
    echo "  - Reverse proxy (Nginx/Caddy/Traefik): ${DOCS_URL}/en/docs/deployment"
    echo "  - Alerts & Notifications:              ${DOCS_URL}/en/docs/alerts"
    echo "  - Full configuration reference:        ${DOCS_URL}/en/docs/configuration"
    echo ""
    if [ "$METHOD" = "binary" ]; then
        echo "  Config file: ${CONFIG_DIR}/server.toml"
        echo "  Apply changes: edit the config file, then run: sudo systemctl restart serverbee-server"
    else
        echo "  Compose file: ${DOCKER_DIR}/docker-compose.yml"
        echo "  View logs:    cd ${DOCKER_DIR} && docker compose logs -f"
        echo "  Apply changes: edit docker-compose.yml, then run: cd ${DOCKER_DIR} && docker compose up -d"
    fi
    echo ""
}

print_agent_result() {
    echo ""
    echo -e "${GREEN}✅ ServerBee Agent installed successfully!${NC}"
    echo ""
    echo "  Server URL: ${SERVER_URL}"
    if [ "$METHOD" = "binary" ]; then
        echo "  Status:     Awaiting start"
        echo ""
        echo "  Start:  sudo systemctl start serverbee-agent"
        echo "  Logs:   sudo journalctl -u serverbee-agent -f"
    else
        echo "  Container:  serverbee-agent"
        echo "  Config dir: /etc/serverbee (mounted as volume — token persists across restarts)"
        echo ""
        echo "  View logs: docker logs -f serverbee-agent"
    fi
    echo ""
    echo -e "📖 More configuration:"
    echo "  - GPU monitoring:              ${DOCS_URL}/en/docs/agent#gpu-monitoring"
    echo "  - Full configuration reference: ${DOCS_URL}/en/docs/configuration"
    echo ""
    echo "  Config file: ${CONFIG_DIR}/agent.toml"
    if [ "$METHOD" = "binary" ]; then
        echo "  Apply changes: edit the config file, then run: sudo systemctl restart serverbee-agent"
    else
        echo "  Apply changes: edit /etc/serverbee/agent.toml, then: docker restart serverbee-agent"
    fi
    echo ""
}
```

- [ ] **Step 7: Write main function and assemble**

```bash
main() {
    parse_args "$@"

    if [ "$(id -u)" -ne 0 ]; then
        error "This script must be run as root (use sudo)"
    fi

    # Interactive prompts (skipped if args already provided)
    prompt_component
    [ -z "$COMPONENT" ] && error "Component is required"
    [[ "$COMPONENT" =~ ^(server|agent)$ ]] || error "Invalid component: $COMPONENT (use 'server' or 'agent')"

    prompt_method
    [ -z "$METHOD" ] && error "Method is required"
    [[ "$METHOD" =~ ^(binary|docker)$ ]] || error "Invalid method: $METHOD (use 'binary' or 'docker')"

    prompt_agent_docker_warning
    prompt_server_params
    prompt_agent_params

    info "Installing ${COMPONENT} via ${METHOD}..."

    case "${COMPONENT}-${METHOD}" in
        server-binary) install_binary_server ;;
        server-docker) install_docker_server ;;
        agent-binary)  install_binary_agent ;;
        agent-docker)  install_docker_agent ;;
    esac

    case "$COMPONENT" in
        server) print_server_result ;;
        agent)  print_agent_result ;;
    esac
}

main "$@"
```

- [ ] **Step 8: Verify the script**

Run: `bash -n deploy/install.sh`
Expected: no syntax errors (exit 0)

- [ ] **Step 9: Commit**

```bash
git add deploy/install.sh
git commit -m "feat: rewrite install script as interactive installer with Docker support"
```

---

## Task B: Batch Mechanical Fixes

**Files:** 19 files (see File Map — Task B)

This task applies four mechanical find-and-replace operations across files that need NO content changes beyond these patterns.

- [ ] **Step 1: Fix root docker-compose.yml**

Replace entire `docker-compose.yml` with:

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

- [ ] **Step 2: Fix broken internal links in EN pure-link files (7 files)**

In each of these files, replace all occurrences of `/docs/en/` → `/en/docs/`:

- `apps/docs/content/docs/en/index.mdx`
- `apps/docs/content/docs/en/admin.mdx`
- `apps/docs/content/docs/en/api-reference.mdx`
- `apps/docs/content/docs/en/capabilities.mdx` (also fix markdown links)
- `apps/docs/content/docs/en/security.mdx` (also fix markdown links)
- `apps/docs/content/docs/en/status-page.mdx`
- `apps/docs/content/docs/en/terminal.mdx` (also fix markdown links)

- [ ] **Step 3: Fix broken internal links in CN pure-link files (11 files)**

In each of these files, replace all occurrences of `/docs/cn/` → `/cn/docs/`:

- `apps/docs/content/docs/cn/index.mdx`
- `apps/docs/content/docs/cn/admin.mdx` (also fix markdown links)
- `apps/docs/content/docs/cn/api-reference.mdx`
- `apps/docs/content/docs/cn/capabilities.mdx` (also fix markdown links)
- `apps/docs/content/docs/cn/security.mdx` (also fix markdown links)
- `apps/docs/content/docs/cn/status-page.mdx`
- `apps/docs/content/docs/cn/terminal.mdx`
- `apps/docs/content/docs/cn/alerts.mdx`
- `apps/docs/content/docs/cn/monitoring.mdx`
- `apps/docs/content/docs/cn/ping.mdx`
- `apps/docs/content/docs/cn/architecture.mdx`

- [ ] **Step 4: Verify no broken link patterns remain in Task B files**

Run:
```bash
grep -r '/docs/en/' apps/docs/content/docs/en/{index,admin,api-reference,capabilities,security,status-page,terminal}.mdx || echo "EN clean"
grep -r '/docs/cn/' apps/docs/content/docs/cn/{index,admin,api-reference,capabilities,security,status-page,terminal,alerts,monitoring,ping,architecture}.mdx || echo "CN clean"
```

Expected: both print "clean"

- [ ] **Step 5: Commit**

```bash
git add docker-compose.yml apps/docs/content/docs/en/{index,admin,api-reference,capabilities,security,status-page,terminal}.mdx apps/docs/content/docs/cn/{index,admin,api-reference,capabilities,security,status-page,terminal,alerts,monitoring,ping,architecture}.mdx
git commit -m "fix: fix broken internal links and update root docker-compose.yml"
```

---

## Task C: English Content Updates

**Files:** 5 files (see File Map — Task C)

Each file needs multiple types of changes: link fixes, image names, GitHub URLs, install script URLs, health check endpoints, server_url standardization, and/or new content (Traefik, Agent Docker).

### C1: en/quick-start.mdx

- [ ] **Step 1: Read the file**

Read: `apps/docs/content/docs/en/quick-start.mdx`

- [ ] **Step 2: Apply all fixes**

Changes:
1. Link fix: `/docs/en/` → `/en/docs/` (all occurrences)
2. GitHub URL: `ZingerBee/ServerBee` → `ZingerLittleBee/ServerBee` (all occurrences)
3. Docker image: `ghcr.io/zingerbee/serverbee:latest` → `ghcr.io/zingerlittlebee/serverbee-server:latest`
4. Docker Compose: remove `version: "3.8"`, remove `container_name: serverbee`, fix volume `serverbee-data:/app/data` → `serverbee-data:/data`
5. Install script: replace `curl -fsSL https://raw.githubusercontent.com/ZingerBee/ServerBee/main/install.sh | bash` → `curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh | sudo bash -s server`
6. Agent install reference: update to `curl -fsSL ... | sudo bash -s agent`
7. Default password description: align with CN version (auto-generated if not set, check logs)
8. `server_url`: any `ws://` → `http://`

- [ ] **Step 3: Verify**

```bash
grep -E 'docs/en/|ZingerBee|zingerbee/serverbee:|get\.serverbee\.io|ws://' apps/docs/content/docs/en/quick-start.mdx || echo "Clean"
```

- [ ] **Step 4: Commit**

```bash
git add apps/docs/content/docs/en/quick-start.mdx
git commit -m "fix: rewrite en/quick-start.mdx — fix links, images, install script, Docker config"
```

### C2: en/deployment.mdx

- [ ] **Step 5: Read the file**

Read: `apps/docs/content/docs/en/deployment.mdx`

- [ ] **Step 6: Apply all fixes**

Changes:
1. Link fix: `/docs/en/` → `/en/docs/` (all markdown links)
2. Docker image: `ghcr.io/zingerbee/serverbee:latest` → `ghcr.io/zingerlittlebee/serverbee-server:latest`
3. Docker Compose: remove `version: "3.8"`, fix volume `/app/data` → `/data`, remove old `container_name: serverbee` → add `container_name: serverbee-server`, service name `serverbee` → `serverbee-server`
4. Healthcheck: `curl -f http://localhost:9527/api/status/health` → `wget --spider -q http://localhost:9527/healthz` (Alpine has wget not curl)
5. All `/api/status/health` and `/api/health` → `/healthz`
6. All `docker compose logs -f serverbee` → `docker compose logs -f serverbee-server`
7. `server_url = "https://..."` format (confirm no `wss://`)
8. **Add Traefik section** after Caddy section. Content per spec §6.2:

```markdown
### Traefik

Traefik integrates with Docker via labels — no separate config file needed. Traefik automatically detects WebSocket connections, so no extra configuration is required.

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
```

- [ ] **Step 7: Verify**

```bash
grep -E '/api/(status/)?health|docs/en/|zingerbee/serverbee:|version.*3\.8|ws(s)?://' apps/docs/content/docs/en/deployment.mdx || echo "Clean"
```

- [ ] **Step 8: Commit**

```bash
git add apps/docs/content/docs/en/deployment.mdx
git commit -m "fix: update en/deployment.mdx — Traefik, healthcheck, image names, service name"
```

### C3: en/agent.mdx

- [ ] **Step 9: Read the file**

Read: `apps/docs/content/docs/en/agent.mdx`

- [ ] **Step 10: Apply all fixes**

Changes:
1. Link fix: `/docs/en/` → `/en/docs/`
2. GitHub URL: `ZingerBee/ServerBee` → `ZingerLittleBee/ServerBee`
3. Install script: replace `curl -fsSL https://raw.githubusercontent.com/ZingerBee/ServerBee/main/install-agent.sh | bash` → `curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh | sudo bash -s agent`
4. `server_url`: any `ws://` → `http://`
5. **Add Docker (Not Recommended) section** after Binary Download section:

```markdown
### Docker (Not Recommended)

<Callout type="info">
ServerBee Agent is portable software — it's a single binary file that creates no folders or residual files on your system. Uninstall by simply deleting the binary and config file. We recommend binary installation for the best experience.
</Callout>

If you still prefer Docker, the agent needs privileged access to collect host system metrics:

\`\`\`bash
docker run -d \
  --name serverbee-agent \
  --privileged \
  --net=host \
  --pid=host \
  -v /proc:/host/proc:ro \
  -v /sys:/host/sys:ro \
  -v /etc/serverbee:/etc/serverbee \
  --restart unless-stopped \
  ghcr.io/zingerlittlebee/serverbee-agent:latest
\`\`\`

<Callout type="warn">
The `/etc/serverbee` volume mount is mandatory. Agent writes its registration token to `agent.toml` after first connection. Without this volume, container recreation causes duplicate server entries.
</Callout>

**Limitations of Docker deployment:**

- Requires `--privileged` for full metrics collection
- Temperature and GPU monitoring may not work inside containers
- Web terminal accesses the container environment, not the host
```

- [ ] **Step 11: Verify**

```bash
grep -E 'docs/en/|ZingerBee|install-agent\.sh|get\.serverbee\.io|ws://' apps/docs/content/docs/en/agent.mdx || echo "Clean"
```

- [ ] **Step 12: Commit**

```bash
git add apps/docs/content/docs/en/agent.mdx
git commit -m "fix: update en/agent.mdx — install script URL, Docker section, link fixes"
```

### C4: en/server.mdx

- [ ] **Step 13: Read and apply fixes**

Read: `apps/docs/content/docs/en/server.mdx`

Changes:
1. GitHub URL: `ZingerBee/ServerBee` → `ZingerLittleBee/ServerBee`
2. Docker image: `ghcr.io/zingerbee/serverbee:latest` → `ghcr.io/zingerlittlebee/serverbee-server:latest`
3. `server_url`: any `ws://` → `http://`
4. Add note in reverse proxy section: "For more reverse proxy configurations including Traefik, see the [Deployment Guide](/en/docs/deployment)."

- [ ] **Step 14: Commit**

```bash
git add apps/docs/content/docs/en/server.mdx
git commit -m "fix: update en/server.mdx — image name, GitHub URL, server_url format"
```

### C5: en/configuration.mdx

- [ ] **Step 15: Read and apply fixes**

Read: `apps/docs/content/docs/en/configuration.mdx`

Changes:
1. `server_url` examples: any `ws://` or `wss://` → `http://` or `https://`
2. Descriptive text about `server_url` — ensure it says "URL of the ServerBee server" not "WebSocket address"

- [ ] **Step 16: Verify and commit**

```bash
grep -E 'ws(s)?://' apps/docs/content/docs/en/configuration.mdx || echo "Clean"
git add apps/docs/content/docs/en/configuration.mdx
git commit -m "fix: standardize server_url to http format in en/configuration.mdx"
```

---

## Task D: Chinese Content Updates

**Files:** 5 files (see File Map — Task D)

Mirrors Task C for Chinese docs. Same change types, adapted for Chinese text.

### D1: cn/quick-start.mdx

- [ ] **Step 1: Read the file**

Read: `apps/docs/content/docs/cn/quick-start.mdx`

- [ ] **Step 2: Apply all fixes**

Changes:
1. Link fix: `/docs/cn/` → `/cn/docs/` (all occurrences)
2. GitHub URL: `zingerbee/ServerBee` → `ZingerLittleBee/ServerBee`
3. Docker image: `ghcr.io/zingerbee/serverbee:latest` → `ghcr.io/zingerlittlebee/serverbee-server:latest`
4. Install script: replace `curl -fsSL https://get.serverbee.io | bash` → `curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh | sudo bash -s server`
5. Agent install: replace `curl -fsSL https://get.serverbee.io/agent | bash -s -- ...` → `curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh | sudo bash -s agent`
6. `server_url`: any `ws://` → `http://`

- [ ] **Step 3: Verify and commit**

```bash
grep -E 'docs/cn/|zingerbee/ServerBee|get\.serverbee\.io|ws://' apps/docs/content/docs/cn/quick-start.mdx || echo "Clean"
git add apps/docs/content/docs/cn/quick-start.mdx
git commit -m "fix: rewrite cn/quick-start.mdx — fix links, images, install script"
```

### D2: cn/deployment.mdx

- [ ] **Step 4: Read the file**

Read: `apps/docs/content/docs/cn/deployment.mdx`

- [ ] **Step 5: Apply all fixes**

Changes:
1. Link fix: `/docs/cn/` → `/cn/docs/`
2. Docker image: `ghcr.io/zingerbee/serverbee:latest` → `ghcr.io/zingerlittlebee/serverbee-server:latest`
3. Service name `serverbee` → `serverbee-server` in all Docker Compose YAML blocks
4. Add `container_name: serverbee-server` to Compose blocks
5. Healthcheck endpoint: `/api/health` → `/healthz`
6. All docker commands: `docker compose logs -f serverbee` → `docker compose logs -f serverbee-server`, `docker compose exec serverbee` → `docker compose exec serverbee-server`, `docker cp serverbee:` → `docker cp serverbee-server:`
7. GitHub URL: `zingerbee/ServerBee` → `ZingerLittleBee/ServerBee` (wget URL in upgrade section)
8. TLS section: `server_url = "wss://..."` → `server_url = "https://..."`
9. **Add Traefik section** after Caddy section (Chinese version):

```markdown
## Traefik 反向代理

Traefik 通过 Docker labels 自动发现服务，无需单独配置文件。Traefik 天然支持 WebSocket 自动检测，无需额外配置。

\`\`\`yaml title="docker-compose.yml"
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
\`\`\`
```

- [ ] **Step 6: Verify and commit**

```bash
grep -E '/api/(status/)?health|docs/cn/|zingerbee/serverbee:|ws(s)?://' apps/docs/content/docs/cn/deployment.mdx || echo "Clean"
git add apps/docs/content/docs/cn/deployment.mdx
git commit -m "fix: update cn/deployment.mdx — Traefik, healthcheck, image names, service name"
```

### D3: cn/agent.mdx

- [ ] **Step 7: Read the file**

Read: `apps/docs/content/docs/cn/agent.mdx`

- [ ] **Step 8: Apply all fixes**

Changes:
1. Link fix: `/docs/cn/` → `/cn/docs/`
2. GitHub URL: `zingerbee/ServerBee` → `ZingerLittleBee/ServerBee`
3. Install script: replace `curl -fsSL https://get.serverbee.io/agent | bash -s -- ...` → `curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh | sudo bash -s agent`
4. `server_url`: `ws://` → `http://`
5. Table row: `Server 的 WebSocket 地址` → `Server 的地址`
6. **Add Docker（不推荐）section** after 二进制下载 section:

```markdown
### Docker（不推荐）

<Callout type="info">
ServerBee Agent 是绿色软件——只有一个二进制文件，不会产生文件夹和其他文件残留。卸载只需删除二进制文件和配置文件。推荐直接下载二进制文件运行。
</Callout>

如果仍要使用 Docker，Agent 需要特权权限才能采集宿主机指标：

\`\`\`bash
docker run -d \
  --name serverbee-agent \
  --privileged \
  --net=host \
  --pid=host \
  -v /proc:/host/proc:ro \
  -v /sys:/host/sys:ro \
  -v /etc/serverbee:/etc/serverbee \
  --restart unless-stopped \
  ghcr.io/zingerlittlebee/serverbee-agent:latest
\`\`\`

<Callout type="warn">
`/etc/serverbee` 挂载是必须的。Agent 注册成功后会将 Token 写入 `agent.toml`。如果不挂载此目录，容器重建后会重新注册并产生重复的服务器记录。
</Callout>

**Docker 部署的限制：**

- 需要 `--privileged` 权限才能采集完整指标
- 温度和 GPU 监控在容器内可能无法工作
- Web 终端功能访问的是容器内环境，而非宿主机
```

- [ ] **Step 9: Verify and commit**

```bash
grep -E 'docs/cn/|zingerbee/ServerBee|get\.serverbee\.io|ws://' apps/docs/content/docs/cn/agent.mdx || echo "Clean"
git add apps/docs/content/docs/cn/agent.mdx
git commit -m "fix: update cn/agent.mdx — install script URL, Docker section, link fixes, server_url"
```

### D4: cn/server.mdx

- [ ] **Step 10: Read and apply fixes**

Read: `apps/docs/content/docs/cn/server.mdx`

Changes:
1. GitHub URL: `zingerbee/ServerBee` → `ZingerLittleBee/ServerBee`
2. Docker image: `ghcr.io/zingerbee/serverbee:latest` → `ghcr.io/zingerlittlebee/serverbee-server:latest`
3. `server_url`: any `ws://` → `http://`
4. Add note in reverse proxy section: "更多反向代理配置（含 Traefik）请参阅[部署指南](/cn/docs/deployment)。"

- [ ] **Step 11: Commit**

```bash
git add apps/docs/content/docs/cn/server.mdx
git commit -m "fix: update cn/server.mdx — image name, GitHub URL, server_url format"
```

### D5: cn/configuration.mdx

- [ ] **Step 12: Read and apply fixes**

Read: `apps/docs/content/docs/cn/configuration.mdx`

Changes:
1. Link fix: `/docs/cn/` → `/cn/docs/`
2. `server_url` examples: any `ws://` or `wss://` → `http://` or `https://`
3. Comment: `# Server 的 WebSocket 地址（必填）` → `# Server 地址（必填）`

- [ ] **Step 13: Verify and commit**

```bash
grep -E 'docs/cn/|ws(s)?://|WebSocket 地址' apps/docs/content/docs/cn/configuration.mdx || echo "Clean"
git add apps/docs/content/docs/cn/configuration.mdx
git commit -m "fix: standardize server_url and fix links in cn/configuration.mdx"
```

---

## Final Verification

After all 4 tasks are merged:

- [ ] **Step 1: Verify no broken link patterns remain**

```bash
grep -r '/docs/en/' apps/docs/content/docs/ || echo "EN links clean"
grep -r '/docs/cn/' apps/docs/content/docs/ || echo "CN links clean"
```

- [ ] **Step 2: Verify no wrong image names remain**

```bash
grep -r 'ghcr.io/zingerbee/' . --include='*.mdx' --include='*.yml' || echo "Image names clean"
```

- [ ] **Step 3: Verify no wrong GitHub URLs remain**

```bash
grep -ri 'github.com/zingerbee/ServerBee\|github.com/ZingerBee/ServerBee' apps/docs/content/docs/ || echo "GitHub URLs clean"
```

- [ ] **Step 4: Verify no broken health check endpoints remain**

```bash
grep -r '/api/health\|/api/status/health' apps/docs/content/docs/ docker-compose.yml || echo "Health endpoints clean"
```

- [ ] **Step 5: Verify no old install script URLs remain**

```bash
grep -r 'get\.serverbee\.io\|install-agent\.sh' apps/docs/content/docs/ || echo "Install URLs clean"
```

- [ ] **Step 6: Verify no ws:// in server_url contexts**

```bash
grep -r 'server_url.*ws://' apps/docs/content/docs/ || echo "server_url clean"
```

- [ ] **Step 7: Build docs to verify no MDX errors**

```bash
cd apps/docs && bun install && bun run build
```
