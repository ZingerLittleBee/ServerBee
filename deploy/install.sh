#!/usr/bin/env bash
set -euo pipefail

# ServerBee Installation Script
# Usage: curl -fsSL https://get.serverbee.io | bash
#   or:  bash install.sh [server|agent]

REPO="ZingerLittleBee/ServerBee"
INSTALL_DIR="/usr/local/bin"
CONFIG_DIR="/etc/serverbee"
DATA_DIR="/var/lib/serverbee"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*" >&2; exit 1; }

detect_arch() {
    local arch
    arch=$(uname -m)
    case "$arch" in
        x86_64|amd64)  echo "amd64" ;;
        aarch64|arm64) echo "arm64" ;;
        *) error "Unsupported architecture: $arch" ;;
    esac
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

get_latest_version() {
    curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep '"tag_name"' \
        | head -1 \
        | sed 's/.*"tag_name": *"//;s/".*//'
}

install_component() {
    local component="$1"
    local version="$2"
    local os="$3"
    local arch="$4"

    local binary="serverbee-${component}"
    local filename="${binary}-${os}-${arch}"
    local url="https://github.com/${REPO}/releases/download/${version}/${filename}"

    info "Downloading ${binary} ${version} for ${os}/${arch}..."
    curl -fsSL -o "/tmp/${binary}" "$url" || error "Download failed: $url"
    chmod +x "/tmp/${binary}"

    info "Installing to ${INSTALL_DIR}/${binary}..."
    mv "/tmp/${binary}" "${INSTALL_DIR}/${binary}"
}

setup_server() {
    info "Setting up server..."
    mkdir -p "$DATA_DIR"

    if command -v systemctl &>/dev/null; then
        local service_url="https://raw.githubusercontent.com/${REPO}/main/deploy/serverbee-server.service"
        curl -fsSL -o /etc/systemd/system/serverbee-server.service "$service_url" 2>/dev/null || {
            # Fallback: create service file inline
            cat > /etc/systemd/system/serverbee-server.service << 'UNIT'
[Unit]
Description=ServerBee Dashboard
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/serverbee-server
WorkingDirectory=/var/lib/serverbee
Environment=SERVERBEE_SERVER_DATA_DIR=/var/lib/serverbee
Restart=always
RestartSec=5
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
UNIT
        }

        systemctl daemon-reload
        systemctl enable serverbee-server
        systemctl start serverbee-server
        info "Server service started and enabled"
    else
        warn "systemd not found. Start manually: serverbee-server"
    fi
}

setup_agent() {
    info "Setting up agent..."
    mkdir -p "$CONFIG_DIR"

    if [ ! -f "${CONFIG_DIR}/agent.toml" ]; then
        cat > "${CONFIG_DIR}/agent.toml" << TOML
# ServerBee Agent Configuration
# Set server_url to your ServerBee dashboard address
server_url = "http://YOUR_SERVER_IP:9527"

# Set the auto_discovery_key from your dashboard settings
auto_discovery_key = ""

[collector]
interval = 3
TOML
        info "Created ${CONFIG_DIR}/agent.toml — edit it with your server details"
    fi

    if command -v systemctl &>/dev/null; then
        local service_url="https://raw.githubusercontent.com/${REPO}/main/deploy/serverbee-agent.service"
        curl -fsSL -o /etc/systemd/system/serverbee-agent.service "$service_url" 2>/dev/null || {
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
        }

        systemctl daemon-reload
        systemctl enable serverbee-agent
        warn "Agent service enabled but NOT started — edit ${CONFIG_DIR}/agent.toml first, then run: systemctl start serverbee-agent"
    else
        warn "systemd not found. Start manually: serverbee-agent"
    fi
}

main() {
    local component="${1:-}"
    local os arch version

    if [ "$(id -u)" -ne 0 ]; then
        error "This script must be run as root (use sudo)"
    fi

    os=$(detect_os)
    arch=$(detect_arch)

    info "Detected: ${os}/${arch}"

    version=$(get_latest_version)
    if [ -z "$version" ]; then
        error "Failed to get latest version from GitHub"
    fi
    info "Latest version: ${version}"

    case "$component" in
        server)
            install_component "server" "$version" "$os" "$arch"
            setup_server
            ;;
        agent)
            install_component "agent" "$version" "$os" "$arch"
            setup_agent
            ;;
        "")
            echo ""
            echo "ServerBee Installer"
            echo "==================="
            echo ""
            echo "Usage: $0 <component>"
            echo ""
            echo "Components:"
            echo "  server  - Install the ServerBee dashboard"
            echo "  agent   - Install the ServerBee agent on monitored servers"
            echo ""
            echo "Examples:"
            echo "  sudo bash install.sh server"
            echo "  sudo bash install.sh agent"
            echo ""
            ;;
        *)
            error "Unknown component: $component (use 'server' or 'agent')"
            ;;
    esac

    info "Done!"
}

main "$@"
