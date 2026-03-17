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
