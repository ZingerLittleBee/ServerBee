#!/usr/bin/env bash
set -euo pipefail

# ─── Constants ────────────────────────────────────────────────────────────────
REPO="ZingerLittleBee/ServerBee"
INSTALL_DIR="/usr/local/bin"
CONFIG_DIR="/etc/serverbee"
DATA_DIR="/var/lib/serverbee"
DOCKER_DIR="/opt/serverbee"
META_FILE="${CONFIG_DIR}/.install-meta"
DOCS_URL="https://server-bee-docs.vercel.app"

# ─── Globals ──────────────────────────────────────────────────────────────────
COMMAND=""
COMPONENT=""
METHOD=""
SERVER_URL=""
DISCOVERY_KEY=""
PASSWORD=""
YES=false
PURGE=false
CONFIG_KEY=""
CONFIG_VALUE=""

# ─── Colors ───────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*" >&2; exit 1; }

# ─── Root check ───────────────────────────────────────────────────────────────
require_root() {
    if [ "$(id -u)" -ne 0 ]; then
        error "This script must be run as root (use sudo)"
    fi
}

# ─── Known subcommands ───────────────────────────────────────────────────────
KNOWN_COMMANDS="install uninstall upgrade status start stop restart config env"

# ─── Argument parsing ─────────────────────────────────────────────────────────
parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --method)        METHOD="$2"; shift 2 ;;
            --server-url)    SERVER_URL="$2"; shift 2 ;;
            --discovery-key) DISCOVERY_KEY="$2"; shift 2 ;;
            --password)      PASSWORD="$2"; shift 2 ;;
            --purge)         PURGE=true; shift ;;
            --yes|-y)        YES=true; shift ;;
            -*)              error "Unknown option: $1" ;;
            *)
                # First positional arg after command = component or config subcommand
                if [ -z "$COMPONENT" ]; then
                    COMPONENT="$1"
                elif [ -z "$CONFIG_KEY" ]; then
                    CONFIG_KEY="$1"
                elif [ -z "$CONFIG_VALUE" ]; then
                    CONFIG_VALUE="$1"
                fi
                shift ;;
        esac
    done
}

# ─── Platform detection ──────────────────────────────────────────────────────
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
    local tag
    tag=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep '"tag_name"' \
        | head -1 \
        | sed 's/.*"tag_name": *"//;s/".*//')
    [ -z "$tag" ] && error "Failed to get latest version from GitHub"
    echo "$tag"
}

get_local_ip() {
    ip -4 route get 1.1.1.1 2>/dev/null | awk '{print $7; exit}' \
        || hostname -I 2>/dev/null | awk '{print $1}' \
        || echo "localhost"
}

# ─── Install metadata (.install-meta JSON) ───────────────────────────────────
# Uses basic grep/sed for JSON manipulation to avoid jq dependency.
# The JSON is simple (flat per-component objects) and always written by us.

meta_read() {
    # Usage: meta_read <component> <field>
    # Returns the value or empty string
    local component="$1" field="$2"
    if [ ! -f "$META_FILE" ]; then echo ""; return; fi
    # Extract value: find component block, then field within it
    sed -n "/\"${component}\"/,/}/p" "$META_FILE" \
        | grep "\"${field}\"" \
        | sed 's/.*: *"//;s/".*//' \
        || echo ""
}

meta_write() {
    # Usage: meta_write <component> <method> <version>
    local component="$1" method="$2" version="$3"
    local timestamp
    timestamp=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

    mkdir -p "$CONFIG_DIR"

    if [ ! -f "$META_FILE" ]; then
        echo "{}" > "$META_FILE"
    fi

    # Build the component JSON block
    local block
    block=$(cat <<JSONBLOCK
    "${component}": {
        "method": "${method}",
        "version": "${version}",
        "installed_at": "${timestamp}"
    }
JSONBLOCK
)

    if grep -q "\"${component}\"" "$META_FILE" 2>/dev/null; then
        # Remove existing component block and re-add
        local tmp
        tmp=$(mktemp)
        awk -v comp="\"${component}\"" '
            BEGIN { skip=0 }
            $0 ~ comp { skip=1; next }
            skip && /}/ { skip=0; next }
            !skip { print }
        ' "$META_FILE" > "$tmp"
        mv "$tmp" "$META_FILE"
    fi

    # Insert the block before the closing }
    local tmp
    tmp=$(mktemp)
    if [ "$(wc -l < "$META_FILE")" -le 1 ]; then
        # File is just {} — rewrite entirely
        echo "{" > "$tmp"
        echo "$block" >> "$tmp"
        echo "}" >> "$tmp"
    else
        # Insert before last }
        sed '$ d' "$META_FILE" > "$tmp"
        # Add comma after previous block if needed
        if grep -q "}" "$tmp" 2>/dev/null; then
            # There are other components — ensure trailing comma
            sed -i.bak 's/}$/},/' "$tmp" && rm -f "$tmp.bak"
        fi
        echo "$block" >> "$tmp"
        echo "}" >> "$tmp"
    fi
    mv "$tmp" "$META_FILE"
    chmod 600 "$META_FILE"
}

meta_remove() {
    # Usage: meta_remove <component>
    local component="$1"
    if [ ! -f "$META_FILE" ]; then return; fi

    local tmp
    tmp=$(mktemp)
    awk -v comp="\"${component}\"" '
        BEGIN { skip=0; prev="" }
        $0 ~ comp { skip=1; next }
        skip && /}/ { skip=0; next }
        !skip { print }
    ' "$META_FILE" > "$tmp"

    # Clean up trailing commas before }
    sed -i.bak 's/,\([[:space:]]*\)}/\1}/' "$tmp" && rm -f "$tmp.bak"
    mv "$tmp" "$META_FILE"
}

meta_has() {
    # Usage: meta_has <component>  — returns 0 if managed, 1 if not
    local component="$1"
    [ -f "$META_FILE" ] && grep -q "\"${component}\"" "$META_FILE" 2>/dev/null
}

# ─── Detection (metadata-first, with unmanaged warning) ─────────────────────
detect_installed() {
    # Populates global arrays: MANAGED_COMPONENTS=("agent:binary" "server:docker")
    MANAGED_COMPONENTS=()
    if [ -f "$META_FILE" ]; then
        for comp in agent server; do
            if meta_has "$comp"; then
                local method
                method=$(meta_read "$comp" "method")
                MANAGED_COMPONENTS+=("${comp}:${method}")
            fi
        done
    fi
}

detect_unmanaged() {
    # Check for unmanaged binaries/containers — used by status for warnings
    UNMANAGED_COMPONENTS=()
    if ! meta_has "agent"; then
        if [ -f "${INSTALL_DIR}/serverbee-agent" ]; then
            UNMANAGED_COMPONENTS+=("agent:binary")
        fi
        if command -v docker &>/dev/null && docker ps -a --format '{{.Names}}' 2>/dev/null | grep -q "^serverbee-agent$"; then
            UNMANAGED_COMPONENTS+=("agent:docker")
        fi
    fi
    if ! meta_has "server"; then
        if [ -f "${INSTALL_DIR}/serverbee-server" ]; then
            UNMANAGED_COMPONENTS+=("server:binary")
        fi
        if command -v docker &>/dev/null && docker ps -a --format '{{.Names}}' 2>/dev/null | grep -q "^serverbee-server$"; then
            UNMANAGED_COMPONENTS+=("server:docker")
        fi
    fi
}

has_systemd() {
    # Check if systemd is actually running (not just installed)
    command -v systemctl &>/dev/null && systemctl is-system-running &>/dev/null 2>&1
    # is-system-running returns non-zero for "degraded" too, so also accept that
    local rc=$?
    if [ $rc -eq 0 ]; then return 0; fi
    # "degraded" means systemd is running but some units failed — still usable
    local state
    state=$(systemctl is-system-running 2>/dev/null || echo "")
    [ "$state" = "degraded" ] || [ "$state" = "running" ]
}

check_docker() {
    command -v docker &>/dev/null || error "Docker is not installed. Install it first: https://docs.docker.com/get-docker/"
    docker compose version &>/dev/null || error "Docker Compose V2 is not available. Install it first: https://docs.docker.com/compose/install/"
}

check_unmanaged_container() {
    # Usage: check_unmanaged_container <component>
    # Fails if an unmanaged container exists with the same name
    local component="$1"
    if ! meta_has "$component" && command -v docker &>/dev/null; then
        if docker ps -a --format '{{.Names}}' 2>/dev/null | grep -q "^serverbee-${component}$"; then
            error "Found existing container 'serverbee-${component}' not managed by this script.\n  Please remove it first:  docker stop serverbee-${component} && docker rm serverbee-${component}\n  Then re-run:  serverbee.sh install ${component} --method docker ..."
        fi
    fi
}

# ─── Install helpers ─────────────────────────────────────────────────────────

install_binary_server() {
    local version os arch
    os=$(detect_os)
    arch=$(detect_arch)
    version=$(get_latest_version)

    # Download (skip if binary already exists — adopt mode)
    if [ -f "${INSTALL_DIR}/serverbee-server" ]; then
        warn "Binary already exists at ${INSTALL_DIR}/serverbee-server — skipping download (adopting existing)"
    else
        local filename="serverbee-server-${os}-${arch}"
        local url="https://github.com/${REPO}/releases/download/${version}/${filename}"
        info "Downloading serverbee-server ${version} for ${os}/${arch}..."
        curl -fsSL -o "/tmp/serverbee-server" "$url" \
            || error "Download failed: $url"
        chmod +x "/tmp/serverbee-server"
        mv "/tmp/serverbee-server" "${INSTALL_DIR}/serverbee-server"
        info "Installed to ${INSTALL_DIR}/serverbee-server"
    fi

    mkdir -p "$DATA_DIR" "$CONFIG_DIR"

    # Generate server.toml (skip if exists)
    if [ ! -f "${CONFIG_DIR}/server.toml" ]; then
        cat > "${CONFIG_DIR}/server.toml" << TOML
[server]
data_dir = "${DATA_DIR}"
TOML
        if [ -n "$PASSWORD" ]; then
            cat >> "${CONFIG_DIR}/server.toml" << TOML

[admin]
password = "${PASSWORD}"
TOML
        fi
        info "Created ${CONFIG_DIR}/server.toml"
    else
        warn "${CONFIG_DIR}/server.toml already exists, not overwriting"
    fi

    # systemd service
    if has_systemd; then
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

    meta_write "server" "binary" "$version"
    print_server_result
}

install_binary_agent() {
    local version os arch
    os=$(detect_os)
    arch=$(detect_arch)
    version=$(get_latest_version)

    # Download (skip if binary already exists — adopt mode)
    if [ -f "${INSTALL_DIR}/serverbee-agent" ]; then
        warn "Binary already exists at ${INSTALL_DIR}/serverbee-agent — skipping download (adopting existing)"
    else
        local filename="serverbee-agent-${os}-${arch}"
        local url="https://github.com/${REPO}/releases/download/${version}/${filename}"
        info "Downloading serverbee-agent ${version} for ${os}/${arch}..."
        curl -fsSL -o "/tmp/serverbee-agent" "$url" \
            || error "Download failed: $url"
        chmod +x "/tmp/serverbee-agent"
        mv "/tmp/serverbee-agent" "${INSTALL_DIR}/serverbee-agent"
        info "Installed to ${INSTALL_DIR}/serverbee-agent"
    fi

    mkdir -p "$CONFIG_DIR"

    # Generate agent.toml (skip if exists)
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

    # systemd service
    if has_systemd; then
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

    meta_write "agent" "binary" "$version"
    print_agent_result
}

install_docker_server() {
    check_docker
    check_unmanaged_container "server"

    local version
    version=$(get_latest_version)

    mkdir -p "$DOCKER_DIR" "$CONFIG_DIR"

    # Generate server.toml (skip if exists)
    if [ ! -f "${CONFIG_DIR}/server.toml" ]; then
        cat > "${CONFIG_DIR}/server.toml" << TOML
[server]
data_dir = "/data"
TOML
        if [ -n "$PASSWORD" ]; then
            cat >> "${CONFIG_DIR}/server.toml" << TOML

[admin]
password = "${PASSWORD}"
TOML
        fi
        info "Created ${CONFIG_DIR}/server.toml"
    else
        warn "${CONFIG_DIR}/server.toml already exists, not overwriting"
    fi

    local password_env=""
    if [ -n "$PASSWORD" ]; then
        password_env="      - SERVERBEE_ADMIN__PASSWORD=${PASSWORD}"
    fi

    cat > "${DOCKER_DIR}/docker-compose.server.yml" << YAML
services:
  serverbee-server:
    image: ghcr.io/zingerlittlebee/serverbee-server:${version}
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

    info "Generated ${DOCKER_DIR}/docker-compose.server.yml"
    docker compose -f "${DOCKER_DIR}/docker-compose.server.yml" up -d
    info "Server container started"

    meta_write "server" "docker" "$version"
    print_server_result
}

install_docker_agent() {
    check_docker
    check_unmanaged_container "agent"

    local version
    version=$(get_latest_version)

    mkdir -p "$CONFIG_DIR"

    # Generate agent.toml (skip if exists)
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

    mkdir -p "$DOCKER_DIR"

    cat > "${DOCKER_DIR}/docker-compose.agent.yml" << YAML
services:
  serverbee-agent:
    image: ghcr.io/zingerlittlebee/serverbee-agent:${version}
    container_name: serverbee-agent
    privileged: true
    network_mode: host
    pid: host
    volumes:
      - /proc:/host/proc:ro
      - /sys:/host/sys:ro
      - /etc/serverbee:/etc/serverbee
    restart: unless-stopped
YAML

    info "Generated ${DOCKER_DIR}/docker-compose.agent.yml"
    docker compose -f "${DOCKER_DIR}/docker-compose.agent.yml" up -d
    info "Agent container started"

    meta_write "agent" "docker" "$version"
    print_agent_result
}

print_server_result() {
    local ip
    ip=$(get_local_ip)
    echo ""
    echo -e "${GREEN}ServerBee Server installed successfully!${NC}"
    echo ""
    echo "  Dashboard:  http://${ip}:9527"
    echo "  Username:   admin"
    if [ -n "$PASSWORD" ]; then
        echo "  Password:   ${PASSWORD}"
    elif [ "$METHOD" = "binary" ]; then
        echo "  Password:   (auto-generated, check: sudo journalctl -u serverbee-server | grep 'Generated admin password')"
    else
        echo "  Password:   (auto-generated, check: docker compose -f ${DOCKER_DIR}/docker-compose.server.yml logs | grep 'Generated admin password')"
    fi
    echo ""
    echo "  Docs: ${DOCS_URL}/en/docs/configuration"
    echo ""
}

print_agent_result() {
    echo ""
    echo -e "${GREEN}ServerBee Agent installed successfully!${NC}"
    echo ""
    echo "  Server URL: ${SERVER_URL}"
    if [ "$METHOD" = "binary" ]; then
        echo "  Start:  sudo systemctl start serverbee-agent"
        echo "  Logs:   sudo journalctl -u serverbee-agent -f"
    else
        echo "  Logs:   docker compose -f ${DOCKER_DIR}/docker-compose.agent.yml logs -f"
    fi
    echo ""
    echo "  Config: ${CONFIG_DIR}/agent.toml"
    echo "  Docs:   ${DOCS_URL}/en/docs/configuration"
    echo ""
}

# ─── Install command ──────────────────────────────────────────────────────────

cmd_install() {
    # Interactive: prompt for component if not provided
    if [ -z "$COMPONENT" ]; then
        echo ""
        echo -e "${BOLD}Install${NC}"
        echo ""
        echo "  [1] Server  — Dashboard & API"
        echo "  [2] Agent   — System metrics collector"
        echo ""
        read -rp "Select component [1/2]: " choice
        case "$choice" in
            1|server) COMPONENT="server" ;;
            2|agent)  COMPONENT="agent" ;;
            *) error "Invalid choice: $choice" ;;
        esac
    fi

    [[ "$COMPONENT" =~ ^(server|agent)$ ]] || error "Invalid component: $COMPONENT (use 'server' or 'agent')"

    # Check if already managed
    if meta_has "$COMPONENT"; then
        local existing_version
        existing_version=$(meta_read "$COMPONENT" "version")
        error "serverbee-${COMPONENT} is already installed (${existing_version}). Use 'upgrade' to update."
    fi

    # Interactive: prompt for method if not provided
    if [ -z "$METHOD" ]; then
        echo ""
        echo "  [1] Binary  (recommended)"
        echo "  [2] Docker"
        echo ""
        read -rp "Select installation method [1/2]: " choice
        case "$choice" in
            1|binary) METHOD="binary" ;;
            2|docker) METHOD="docker" ;;
            *) error "Invalid choice: $choice" ;;
        esac
    fi
    : "${METHOD:=binary}"
    [[ "$METHOD" =~ ^(binary|docker)$ ]] || error "Invalid method: $METHOD (use 'binary' or 'docker')"

    # Docker agent warning
    if [ "$COMPONENT" = "agent" ] && [ "$METHOD" = "docker" ] && [ "$YES" != true ]; then
        echo ""
        warn "Docker is NOT recommended for Agent"
        echo ""
        echo "  ServerBee Agent is portable software:"
        echo "  - Single binary, no residual files"
        echo "  - Docker requires --privileged for full metrics"
        echo "  - Web terminal accesses container, not host"
        echo ""
        read -rp "  Continue with Docker? [y/N]: " confirm
        case "$confirm" in
            [yY]|[yY][eE][sS]) ;;
            *) METHOD="binary"; info "Switched to binary installation." ;;
        esac
    fi

    # Prompt for component-specific params
    if [ "$COMPONENT" = "server" ]; then
        if [ -z "$PASSWORD" ] && [ "$YES" != true ]; then
            echo ""
            read -rp "Admin password (Enter to skip, auto-generated on first start): " PASSWORD
        fi
    elif [ "$COMPONENT" = "agent" ]; then
        while [ -z "$SERVER_URL" ]; do
            if [ "$YES" = true ]; then error "--server-url is required for agent installation"; fi
            read -rp "Server URL (e.g., http://10.0.0.1:9527): " SERVER_URL
        done
        while [ -z "$DISCOVERY_KEY" ]; do
            if [ "$YES" = true ]; then error "--discovery-key is required for agent installation"; fi
            read -rp "Auto-discovery key: " DISCOVERY_KEY
        done
    fi

    info "Installing ${COMPONENT} via ${METHOD}..."

    case "${COMPONENT}-${METHOD}" in
        server-binary) install_binary_server ;;
        server-docker) install_docker_server ;;
        agent-binary)  install_binary_agent ;;
        agent-docker)  install_docker_agent ;;
    esac
}

# ─── Uninstall command ────────────────────────────────────────────────────────

uninstall_binary() {
    local component="$1"
    local service="serverbee-${component}"

    if has_systemd; then
        systemctl stop "$service" 2>/dev/null || true
        systemctl disable "$service" 2>/dev/null || true
        rm -f "/etc/systemd/system/${service}.service"
        rm -rf "/etc/systemd/system/${service}.service.d"
        systemctl daemon-reload
    fi

    rm -f "${INSTALL_DIR}/${service}"

    if [ "$PURGE" = true ]; then
        rm -f "${CONFIG_DIR}/${component}.toml"
        if [ "$component" = "server" ]; then
            rm -rf "$DATA_DIR"
        fi
        info "Config and data purged"
    fi
}

uninstall_docker() {
    local component="$1"
    local compose_file="${DOCKER_DIR}/docker-compose.${component}.yml"

    if [ -f "$compose_file" ]; then
        docker compose -f "$compose_file" down || true
    else
        # Fallback: stop container directly
        docker stop "serverbee-${component}" 2>/dev/null || true
        docker rm "serverbee-${component}" 2>/dev/null || true
    fi

    if [ "$PURGE" = true ]; then
        # Remove image
        local image_name="ghcr.io/zingerlittlebee/serverbee-${component}"
        docker images --format '{{.Repository}}:{{.Tag}}' | grep "^${image_name}:" | while read -r img; do
            docker rmi "$img" 2>/dev/null || true
        done
        # Remove named volumes (server uses serverbee-data)
        if [ "$component" = "server" ]; then
            docker volume ls --format '{{.Name}}' | grep "serverbee-data" | while read -r vol; do
                docker volume rm "$vol" 2>/dev/null || true
            done
        fi
        rm -f "$compose_file"
        rm -f "${CONFIG_DIR}/${component}.toml"
        info "Config, data, images, and volumes purged"
    fi
}

cmd_uninstall() {
    # Component is required for uninstall
    if [ -z "$COMPONENT" ]; then
        echo ""
        echo -e "${BOLD}Uninstall${NC}"
        echo ""
        echo "  [1] Server"
        echo "  [2] Agent"
        echo ""
        read -rp "Select component [1/2]: " choice
        case "$choice" in
            1|server) COMPONENT="server" ;;
            2|agent)  COMPONENT="agent" ;;
            *) error "Invalid choice: $choice" ;;
        esac
    fi

    [[ "$COMPONENT" =~ ^(server|agent)$ ]] || error "Invalid component: $COMPONENT"

    if ! meta_has "$COMPONENT"; then
        error "serverbee-${COMPONENT} is not installed (not managed by this script)"
    fi

    local method
    method=$(meta_read "$COMPONENT" "method")

    # Confirmation
    if [ "$YES" != true ]; then
        local purge_note=""
        if [ "$PURGE" = true ]; then
            purge_note=" (including config and data)"
        fi
        read -rp "Uninstall serverbee-${COMPONENT} (${method})${purge_note}? [y/N]: " confirm
        case "$confirm" in
            [yY]|[yY][eE][sS]) ;;
            *) info "Cancelled."; exit 0 ;;
        esac
    fi

    info "Uninstalling serverbee-${COMPONENT} (${method})..."

    case "${method}" in
        binary) uninstall_binary "$COMPONENT" ;;
        docker) uninstall_docker "$COMPONENT" ;;
        *) error "Unknown install method: $method" ;;
    esac

    meta_remove "$COMPONENT"
    info "serverbee-${COMPONENT} has been uninstalled."

    if [ "$PURGE" != true ]; then
        echo ""
        echo "  Config preserved at: ${CONFIG_DIR}/${COMPONENT}.toml"
        echo "  To remove all data:  re-run with --purge"
        echo ""
    fi
}

# ─── Upgrade command ──────────────────────────────────────────────────────────

upgrade_component() {
    local component="$1"
    local method current_version latest_version
    method=$(meta_read "$component" "method")
    current_version=$(meta_read "$component" "version")
    latest_version=$(get_latest_version)

    if [ -n "$current_version" ] && [ "$current_version" = "$latest_version" ]; then
        info "serverbee-${component} is already up to date (${current_version})"
        return
    fi

    if [ -z "$current_version" ]; then
        warn "Cannot determine current version for serverbee-${component}, downloading latest..."
    else
        info "Upgrading serverbee-${component}: ${current_version} -> ${latest_version}"
    fi

    # Confirmation
    if [ "$YES" != true ]; then
        read -rp "Proceed with upgrade? [y/N]: " confirm
        case "$confirm" in
            [yY]|[yY][eE][sS]) ;;
            *) info "Skipped."; return ;;
        esac
    fi

    case "$method" in
        binary) upgrade_binary "$component" "$latest_version" ;;
        docker) upgrade_docker "$component" "$latest_version" ;;
        *) error "Unknown install method: $method" ;;
    esac

    meta_write "$component" "$method" "$latest_version"
    info "serverbee-${component} upgraded to ${latest_version}"
}

upgrade_binary() {
    local component="$1" version="$2"
    local os arch service
    os=$(detect_os)
    arch=$(detect_arch)
    service="serverbee-${component}"

    local filename="serverbee-${component}-${os}-${arch}"
    local url="https://github.com/${REPO}/releases/download/${version}/${filename}"
    info "Downloading ${filename} ${version}..."
    curl -fsSL -o "/tmp/serverbee-${component}" "$url" \
        || error "Download failed: $url"
    chmod +x "/tmp/serverbee-${component}"

    # Stop, replace, start
    if has_systemd; then
        systemctl stop "$service" 2>/dev/null || true
    fi
    mv "/tmp/serverbee-${component}" "${INSTALL_DIR}/${service}"
    if has_systemd; then
        systemctl start "$service"
    fi
}

upgrade_docker() {
    local component="$1" version="$2"
    local compose_file="${DOCKER_DIR}/docker-compose.${component}.yml"

    if [ ! -f "$compose_file" ]; then
        error "Compose file not found: $compose_file"
    fi

    # Update image tag in compose file
    local image_base="ghcr.io/zingerlittlebee/serverbee-${component}"
    sed -i.bak "s|${image_base}:[^ ]*|${image_base}:${version}|" "$compose_file" && rm -f "${compose_file}.bak"

    docker compose -f "$compose_file" pull
    docker compose -f "$compose_file" up -d
}

cmd_upgrade() {
    detect_installed

    if [ -n "$COMPONENT" ]; then
        # Upgrade specific component
        [[ "$COMPONENT" =~ ^(server|agent)$ ]] || error "Invalid component: $COMPONENT"
        if ! meta_has "$COMPONENT"; then
            error "serverbee-${COMPONENT} is not installed"
        fi
        upgrade_component "$COMPONENT"
    else
        # Upgrade all managed components
        if [ ${#MANAGED_COMPONENTS[@]} -eq 0 ]; then
            error "No managed components found. Nothing to upgrade."
        fi
        for entry in "${MANAGED_COMPONENTS[@]}"; do
            local comp="${entry%%:*}"
            upgrade_component "$comp"
        done
    fi
}

# ─── Status command ───────────────────────────────────────────────────────────

status_component() {
    local component="$1" method="$2"
    local version
    version=$(meta_read "$component" "version")
    local service="serverbee-${component}"

    echo -e "${BOLD}${component^} (${method})${NC}"

    if [ "$method" = "binary" ]; then
        echo "  Version:  ${version:-unknown}"
        echo "  Binary:   ${INSTALL_DIR}/${service}"
        echo "  Config:   ${CONFIG_DIR}/${component}.toml"

        if has_systemd; then
            local status_line
            status_line=$(systemctl is-active "$service" 2>/dev/null || echo "inactive")
            if [ "$status_line" = "active" ]; then
                local since
                since=$(systemctl show "$service" --property=ActiveEnterTimestamp --value 2>/dev/null || echo "")
                echo -e "  Service:  ${GREEN}active (running)${NC} since ${since}"
            else
                echo -e "  Service:  ${RED}${status_line}${NC}"
            fi
            echo "  Recent logs:"
            journalctl -u "$service" -n 5 --no-pager 2>/dev/null | sed 's/^/    /' || echo "    (no logs)"
        fi

        # Show server_url for agent
        if [ "$component" = "agent" ] && [ -f "${CONFIG_DIR}/agent.toml" ]; then
            local srv
            srv=$(grep "^server_url" "${CONFIG_DIR}/agent.toml" 2>/dev/null | sed 's/.*= *"//;s/".*//' || echo "")
            [ -n "$srv" ] && echo "  Server:   ${srv}"
        fi

        # Show dashboard URL for server
        if [ "$component" = "server" ]; then
            local ip
            ip=$(get_local_ip)
            echo "  Dashboard: http://${ip}:9527"
        fi

    elif [ "$method" = "docker" ]; then
        local compose_file="${DOCKER_DIR}/docker-compose.${component}.yml"
        echo "  Version:  ${version:-unknown}"

        if docker ps --format '{{.Names}} {{.Status}}' 2>/dev/null | grep -q "^${service} "; then
            local container_status
            container_status=$(docker ps --format '{{.Status}}' --filter "name=^${service}$" 2>/dev/null)
            echo -e "  Container: ${service} (${GREEN}${container_status}${NC})"
        else
            echo -e "  Container: ${service} (${RED}stopped${NC})"
        fi

        local image_tag
        image_tag=$(docker inspect "${service}" --format '{{.Config.Image}}' 2>/dev/null || echo "unknown")
        echo "  Image:    ${image_tag}"

        if [ "$component" = "server" ]; then
            local ports
            ports=$(docker port "${service}" 2>/dev/null | head -1 || echo "")
            [ -n "$ports" ] && echo "  Port:     ${ports}"
            local ip
            ip=$(get_local_ip)
            echo "  Dashboard: http://${ip}:9527"
        fi

        echo "  Recent logs:"
        docker logs "${service}" --tail 5 2>/dev/null | sed 's/^/    /' || echo "    (no logs)"
    fi
}

cmd_status() {
    detect_installed
    detect_unmanaged

    if [ ${#MANAGED_COMPONENTS[@]} -eq 0 ] && [ ${#UNMANAGED_COMPONENTS[@]} -eq 0 ]; then
        echo ""
        echo "No ServerBee components found. Run 'serverbee.sh install' to get started."
        echo ""
        return
    fi

    echo ""
    echo -e "${BOLD}ServerBee Status${NC}"
    echo "================"

    for entry in "${MANAGED_COMPONENTS[@]}"; do
        local comp="${entry%%:*}"
        local method="${entry##*:}"
        echo ""
        status_component "$comp" "$method"
    done

    # Warn about unmanaged instances
    for entry in "${UNMANAGED_COMPONENTS[@]}"; do
        local comp="${entry%%:*}"
        local method="${entry##*:}"
        echo ""
        warn "Found serverbee-${comp} (${method}) but it is not managed by this script."
        echo "    To bring it under management, run: serverbee.sh install ${comp} [options]"
    done

    echo ""
}

# ─── Service control (start/stop/restart) ────────────────────────────────────

cmd_service() {
    local action="$1"
    detect_installed

    local targets=()
    if [ -n "$COMPONENT" ]; then
        [[ "$COMPONENT" =~ ^(server|agent)$ ]] || error "Invalid component: $COMPONENT"
        if ! meta_has "$COMPONENT"; then
            error "serverbee-${COMPONENT} is not installed"
        fi
        local method
        method=$(meta_read "$COMPONENT" "method")
        targets+=("${COMPONENT}:${method}")
    else
        if [ ${#MANAGED_COMPONENTS[@]} -eq 0 ]; then
            error "No managed components found."
        fi
        targets=("${MANAGED_COMPONENTS[@]}")
    fi

    for entry in "${targets[@]}"; do
        local comp="${entry%%:*}"
        local method="${entry##*:}"
        local service="serverbee-${comp}"

        info "${action^}ing serverbee-${comp} (${method})..."

        if [ "$method" = "binary" ]; then
            if has_systemd; then
                systemctl "$action" "$service"
            else
                error "systemd not available. Cannot ${action} ${service}."
            fi
        elif [ "$method" = "docker" ]; then
            local compose_file="${DOCKER_DIR}/docker-compose.${comp}.yml"
            case "$action" in
                start)   docker compose -f "$compose_file" up -d ;;
                stop)    docker compose -f "$compose_file" stop ;;
                restart) docker compose -f "$compose_file" restart ;;
            esac
        fi

        # Print brief status
        if [ "$method" = "binary" ] && has_systemd; then
            local st
            st=$(systemctl is-active "$service" 2>/dev/null || echo "unknown")
            info "serverbee-${comp}: ${st}"
        elif [ "$method" = "docker" ]; then
            local st
            st=$(docker ps --format '{{.Status}}' --filter "name=^${service}$" 2>/dev/null || echo "unknown")
            info "serverbee-${comp}: ${st:-stopped}"
        fi
    done
}

# ─── Config command ───────────────────────────────────────────────────────────

# Config key mapping
REJECTED_KEYS="admin.password admin.username"
ARRAY_KEYS="file.root_paths file.deny_patterns server.trusted_proxies oauth.oidc.scopes"
AGENT_KEYS="server_url auto_discovery_key token collector.interval collector.enable_gpu collector.enable_temperature file.enabled file.max_file_size ip_change.enabled ip_change.check_external_ip ip_change.external_ip_url ip_change.interval_secs"
SERVER_KEYS="file.max_upload_size server.listen server.data_dir server.trusted_proxies auth.auto_discovery_key auth.session_ttl auth.secure_cookie geoip.mmdb_path retention.records_days retention.records_hourly_days retention.gpu_records_days retention.ping_records_days retention.network_probe_days retention.network_probe_hourly_days retention.audit_logs_days retention.traffic_hourly_days retention.traffic_daily_days retention.task_results_days retention.docker_events_days retention.service_monitor_days database.path database.max_connections rate_limit.login_max rate_limit.register_max scheduler.timezone upgrade.release_base_url oauth.base_url oauth.allow_registration oauth.github.client_id oauth.github.client_secret oauth.google.client_id oauth.google.client_secret oauth.oidc.issuer_url oauth.oidc.client_id oauth.oidc.client_secret"
LOG_KEYS="log.level log.file"

config_key_to_file() {
    # Returns "agent" or "server" or "both" (for log.*) or "" if unknown
    local key="$1"
    if echo "$AGENT_KEYS" | grep -qw "$key"; then echo "agent"; return; fi
    if echo "$SERVER_KEYS" | grep -qw "$key"; then echo "server"; return; fi
    if echo "$LOG_KEYS" | grep -qw "$key"; then echo "both"; return; fi
    echo ""
}

toml_set() {
    # Usage: toml_set <file> <dotted_key> <value>
    # Handles top-level keys and one-level sections (e.g., collector.interval)
    local file="$1" dotted_key="$2" value="$3"
    local section="" key=""

    if [[ "$dotted_key" == *.* ]]; then
        section="${dotted_key%%.*}"
        key="${dotted_key#*.}"
        # Handle nested sections like oauth.github.client_id -> [oauth.github] client_id
        if [[ "$key" == *.* ]]; then
            section="${dotted_key%.*}"
            key="${dotted_key##*.}"
        fi
    else
        key="$dotted_key"
    fi

    # Determine if value needs quoting (numbers and bools are unquoted)
    local quoted_value
    if [[ "$value" =~ ^[0-9]+$ ]] || [[ "$value" =~ ^(true|false)$ ]]; then
        quoted_value="$value"
    else
        quoted_value="\"$value\""
    fi

    if [ -z "$section" ]; then
        # Top-level key
        if grep -q "^${key} *=" "$file" 2>/dev/null; then
            sed -i.bak "s|^${key} *=.*|${key} = ${quoted_value}|" "$file" && rm -f "${file}.bak"
        else
            # Insert at top (before first section)
            local tmp
            tmp=$(mktemp)
            echo "${key} = ${quoted_value}" > "$tmp"
            cat "$file" >> "$tmp"
            mv "$tmp" "$file"
        fi
    else
        # Key in section
        if grep -q "^\[${section}\]" "$file" 2>/dev/null; then
            # Section exists — check if key exists in section
            if sed -n "/^\[${section}\]/,/^\[/p" "$file" | grep -q "^${key} *="; then
                # Replace existing key in section
                local tmp
                tmp=$(mktemp)
                awk -v sect="[${section}]" -v k="${key}" -v v="${key} = ${quoted_value}" '
                    BEGIN { in_section=0 }
                    /^\[/ { in_section=($0 == sect) }
                    in_section && $0 ~ "^"k" *=" { print v; next }
                    { print }
                ' "$file" > "$tmp"
                mv "$tmp" "$file"
            else
                # Append key to end of section (before next section or EOF)
                local tmp
                tmp=$(mktemp)
                awk -v sect="[${section}]" -v line="${key} = ${quoted_value}" '
                    BEGIN { in_section=0; added=0 }
                    /^\[/ {
                        if (in_section && !added) { print line; added=1 }
                        in_section=($0 == sect)
                    }
                    { print }
                    END { if (in_section && !added) print line }
                ' "$file" > "$tmp"
                mv "$tmp" "$file"
            fi
        else
            # Section doesn't exist — append
            echo "" >> "$file"
            echo "[${section}]" >> "$file"
            echo "${key} = ${quoted_value}" >> "$file"
        fi
    fi
}

cmd_service_single() {
    # Helper: restart a single component
    local comp="$1" method="$2" action="$3"
    local service="serverbee-${comp}"
    info "${action^}ing serverbee-${comp}..."
    if [ "$method" = "binary" ]; then
        systemctl "$action" "$service" 2>/dev/null || true
    elif [ "$method" = "docker" ]; then
        local compose_file="${DOCKER_DIR}/docker-compose.${comp}.yml"
        docker compose -f "$compose_file" "$action" 2>/dev/null || true
    fi
}

cmd_config() {
    detect_installed

    # config set <key> <value>
    if [ "$COMPONENT" = "set" ]; then
        local key="$CONFIG_KEY"
        local value="$CONFIG_VALUE"
        [ -z "$key" ] && error "Usage: serverbee.sh config set <key> <value>"
        [ -z "$value" ] && error "Usage: serverbee.sh config set <key> <value>"

        # 1. Check rejected keys
        if echo "$REJECTED_KEYS" | grep -qw "$key"; then
            case "$key" in
                admin.password) error "Admin password can only be set during initial installation. To change password, use the Dashboard UI." ;;
                admin.username) error "Admin username can only be set during initial installation." ;;
            esac
        fi

        # 2. Check array keys
        if echo "$ARRAY_KEYS" | grep -qw "$key"; then
            error "Key '${key}' is an array type. Edit the TOML file directly:\n  ${CONFIG_DIR}/agent.toml or ${CONFIG_DIR}/server.toml"
        fi

        # 3. Map key to file
        local target
        target=$(config_key_to_file "$key")
        [ -z "$target" ] && error "Unknown config key: $key"

        local files_to_update=()
        if [ "$target" = "both" ]; then
            meta_has "agent" && files_to_update+=("${CONFIG_DIR}/agent.toml")
            meta_has "server" && files_to_update+=("${CONFIG_DIR}/server.toml")
            [ ${#files_to_update[@]} -eq 0 ] && error "No managed components found to update log config"
        elif [ "$target" = "agent" ]; then
            files_to_update=("${CONFIG_DIR}/agent.toml")
        elif [ "$target" = "server" ]; then
            files_to_update=("${CONFIG_DIR}/server.toml")
        fi

        for file in "${files_to_update[@]}"; do
            if [ ! -f "$file" ]; then
                error "Config file not found: $file"
            fi
            local before
            before=$(cat "$file")
            toml_set "$file" "$key" "$value"
            info "Updated ${key} = ${value} in ${file}"

            # Show diff
            echo "  Changes:"
            diff <(echo "$before") "$file" | sed 's/^/    /' || true
        done

        # Prompt restart
        if [ "$YES" = true ]; then
            for entry in "${MANAGED_COMPONENTS[@]}"; do
                local comp="${entry%%:*}"
                local method="${entry##*:}"
                if [[ "$target" == "$comp" || "$target" == "both" ]]; then
                    cmd_service_single "$comp" "$method" "restart"
                fi
            done
        else
            echo ""
            echo "  Restart service to apply changes?"
            read -rp "  [y/N]: " confirm
            if [[ "$confirm" =~ ^[yY] ]]; then
                for entry in "${MANAGED_COMPONENTS[@]}"; do
                    local comp="${entry%%:*}"
                    local method="${entry##*:}"
                    if [[ "$target" == "$comp" || "$target" == "both" ]]; then
                        cmd_service_single "$comp" "$method" "restart"
                    fi
                done
            fi
        fi
        return
    fi

    # config [agent|server] — view mode
    local targets=()
    if [ -n "$COMPONENT" ]; then
        [[ "$COMPONENT" =~ ^(server|agent)$ ]] || error "Invalid component: $COMPONENT"
        targets+=("$COMPONENT")
    else
        for entry in "${MANAGED_COMPONENTS[@]}"; do
            targets+=("${entry%%:*}")
        done
    fi

    [ ${#targets[@]} -eq 0 ] && error "No managed components found."

    for comp in "${targets[@]}"; do
        local file="${CONFIG_DIR}/${comp}.toml"
        echo ""
        echo -e "${BOLD}${comp^} config (${file})${NC}"
        echo "─────────────────────────────────"
        if [ -f "$file" ]; then
            cat "$file"
        else
            echo "(file not found)"
        fi
        echo ""
    done
}

# ─── Env command ──────────────────────────────────────────────────────────────

env_key_to_component() {
    # Maps an env var name (without SERVERBEE_ prefix) to agent or server
    local key="$1"
    case "$key" in
        SERVER_URL|AUTO_DISCOVERY_KEY|TOKEN|COLLECTOR__*|IP_CHANGE__*|FILE__ENABLED|FILE__MAX_FILE_SIZE|FILE__ROOT_PATHS|FILE__DENY_PATTERNS)
            echo "agent" ;;
        SERVER__*|ADMIN__*|AUTH__*|GEOIP__*|RETENTION__*|DATABASE__*|RATE_LIMIT__*|SCHEDULER__*|UPGRADE__*|OAUTH__*|FILE__MAX_UPLOAD_SIZE)
            echo "server" ;;
        LOG__*)
            echo "both" ;;
        *)
            echo "" ;;
    esac
}

cmd_env() {
    detect_installed

    # env set <key> <value>
    if [ "$COMPONENT" = "set" ]; then
        local raw_key="$CONFIG_KEY"
        local value="$CONFIG_VALUE"
        [ -z "$raw_key" ] && error "Usage: serverbee.sh env set <KEY> <value>"
        [ -z "$value" ] && error "Usage: serverbee.sh env set <KEY> <value>"

        # Normalize: ensure SERVERBEE_ prefix
        local env_key="$raw_key"
        if [[ "$env_key" != SERVERBEE_* ]]; then
            env_key="SERVERBEE_${env_key}"
        fi

        # Determine component
        local stripped="${env_key#SERVERBEE_}"
        local target
        target=$(env_key_to_component "$stripped")
        [ -z "$target" ] && error "Unknown env key: $env_key"

        local components_to_update=()
        if [ "$target" = "both" ]; then
            meta_has "agent" && components_to_update+=("agent")
            meta_has "server" && components_to_update+=("server")
        else
            meta_has "$target" || error "serverbee-${target} is not installed"
            components_to_update+=("$target")
        fi

        for comp in "${components_to_update[@]}"; do
            local method
            method=$(meta_read "$comp" "method")
            local service="serverbee-${comp}"

            if [ "$method" = "binary" ]; then
                local override_dir="/etc/systemd/system/${service}.service.d"
                local override_file="${override_dir}/override.conf"
                mkdir -p "$override_dir"

                if [ -f "$override_file" ] && grep -q "^Environment=${env_key}=" "$override_file" 2>/dev/null; then
                    # Update existing line
                    sed -i.bak "s|^Environment=${env_key}=.*|Environment=${env_key}=${value}|" "$override_file" && rm -f "${override_file}.bak"
                elif [ -f "$override_file" ]; then
                    # Append to existing [Service] block
                    echo "Environment=${env_key}=${value}" >> "$override_file"
                else
                    # Create new override file
                    cat > "$override_file" << EOF
[Service]
Environment=${env_key}=${value}
EOF
                fi
                systemctl daemon-reload
                info "Set ${env_key}=${value} in systemd override for ${service}"

            elif [ "$method" = "docker" ]; then
                local compose_file="${DOCKER_DIR}/docker-compose.${comp}.yml"
                if [ ! -f "$compose_file" ]; then
                    error "Compose file not found: $compose_file"
                fi
                # Check if env var already exists in compose file
                if grep -q "- ${env_key}=" "$compose_file" 2>/dev/null; then
                    sed -i.bak "s|- ${env_key}=.*|- ${env_key}=${value}|" "$compose_file" && rm -f "${compose_file}.bak"
                else
                    # Insert into environment block
                    sed -i.bak "/environment:/a\\      - ${env_key}=${value}" "$compose_file" && rm -f "${compose_file}.bak"
                fi
                info "Set ${env_key}=${value} in ${compose_file}"
                docker compose -f "$compose_file" up -d
            fi
        done
        return
    fi

    # env — view mode
    [ ${#MANAGED_COMPONENTS[@]} -eq 0 ] && error "No managed components found."

    echo ""
    echo -e "${BOLD}Environment Variables${NC}"
    echo "====================="

    # Shell env
    echo ""
    echo "Source: shell"
    local found_shell=false
    while IFS='=' read -r key value; do
        if [[ "$key" == SERVERBEE_* ]]; then
            echo "  ${key}=${value}"
            found_shell=true
        fi
    done < <(env)
    if [ "$found_shell" = false ]; then
        echo "  (none)"
    fi

    for entry in "${MANAGED_COMPONENTS[@]}"; do
        local comp="${entry%%:*}"
        local method="${entry##*:}"
        local service="serverbee-${comp}"

        echo ""
        if [ "$method" = "binary" ]; then
            echo "Source: systemd override (${service})"
            local override_file="/etc/systemd/system/${service}.service.d/override.conf"
            if [ -f "$override_file" ]; then
                grep "^Environment=" "$override_file" 2>/dev/null | sed 's/^Environment=/  /' || echo "  (none)"
            else
                echo "  (none)"
            fi
            # Also show env from unit file itself
            local unit_envs
            unit_envs=$(systemctl show "$service" --property=Environment --value 2>/dev/null || echo "")
            if [ -n "$unit_envs" ]; then
                echo "Source: systemd unit (${service})"
                echo "$unit_envs" | tr ' ' '\n' | sed 's/^/  /'
            fi
        elif [ "$method" = "docker" ]; then
            echo "Source: docker-compose (${service})"
            local compose_file="${DOCKER_DIR}/docker-compose.${comp}.yml"
            if [ -f "$compose_file" ]; then
                grep "^ *- SERVERBEE_" "$compose_file" 2>/dev/null | sed 's/^ *- /  /' || echo "  (none)"
            else
                echo "  (compose file not found)"
            fi
        fi
    done

    echo ""
    echo "Note: env vars override TOML config values"
    echo ""
}

# ─── Interactive menu ─────────────────────────────────────────────────────────

interactive_menu() {
    echo ""
    echo -e "${BOLD}ServerBee Manager${NC}"
    echo "================="
    echo ""
    echo "  [1] Install    安装"
    echo "  [2] Uninstall  卸载"
    echo "  [3] Upgrade    升级"
    echo "  [4] Status     查看状态"
    echo "  [5] Service    服务控制 (start/stop/restart)"
    echo "  [6] Config     配置管理"
    echo "  [7] Env        环境变量"
    echo "  [0] Exit       退出"
    echo ""
    read -rp "Select [0-7]: " choice
    case "$choice" in
        1) COMMAND="install" ;;
        2) COMMAND="uninstall" ;;
        3) COMMAND="upgrade" ;;
        4) COMMAND="status" ;;
        5) interactive_service_menu ;;
        6) COMMAND="config" ;;
        7) COMMAND="env" ;;
        0) exit 0 ;;
        *) error "Invalid choice: $choice" ;;
    esac
    require_root
    run_command
}

interactive_service_menu() {
    echo ""
    echo "  [1] Start    启动"
    echo "  [2] Stop     停止"
    echo "  [3] Restart  重启"
    echo ""
    read -rp "Select [1-3]: " choice
    case "$choice" in
        1) COMMAND="start" ;;
        2) COMMAND="stop" ;;
        3) COMMAND="restart" ;;
        *) error "Invalid choice: $choice" ;;
    esac
}

# ─── Command dispatch ─────────────────────────────────────────────────────────

run_command() {
    case "$COMMAND" in
        install)   cmd_install ;;
        uninstall) cmd_uninstall ;;
        upgrade)   cmd_upgrade ;;
        status)    cmd_status ;;
        start)     cmd_service start ;;
        stop)      cmd_service stop ;;
        restart)   cmd_service restart ;;
        config)    cmd_config ;;
        env)       cmd_env ;;
        *) error "Unknown command: $COMMAND" ;;
    esac
}

# ─── Main ─────────────────────────────────────────────────────────────────────
main() {
    # Shorthand: first arg not a known command → prepend "install"
    if [[ $# -gt 0 ]] && ! echo "$KNOWN_COMMANDS" | grep -qw "$1"; then
        set -- install "$@"
    fi

    if [[ $# -eq 0 ]]; then
        interactive_menu
    else
        COMMAND="$1"; shift
        parse_args "$@"
        require_root
        run_command
    fi
}

main "$@"
