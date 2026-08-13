#!/bin/sh
set -eu
# shellcheck disable=SC3043  # 'local' is supported by every shell we target (dash, busybox ash, ksh).
#
# ServerBee installer / management CLI.
# POSIX sh (runs under dash and busybox ash). Supports systemd and OpenRC.

# Absolute path to this script when run as a regular file; empty when the
# script was piped via stdin (curl | sh). Used so the installed management CLI
# matches the installer that created the layout, and so we can re-exec under
# sudo/doas when not root.
SELF_SCRIPT=""
case "$0" in
    -sh|sh|-dash|dash|-ash|ash|bash|-bash) ;;
    *)
        if [ -r "$0" ]; then
            SELF_SCRIPT="$(cd "$(dirname "$0")" 2>/dev/null && pwd)/$(basename "$0")"
        fi
        ;;
esac

# ─── Constants ────────────────────────────────────────────────────────────────
REPO="ZingerLittleBee/ServerBee"
INSTALLER_VERSION="1.0.0-beta.1"
# Everything ServerBee installs lives under a single base directory for
# unified management. The PATH-visible management CLI is the only exception.
BASE_DIR="/opt/serverbee"
INSTALL_DIR="${BASE_DIR}/bin"
CONFIG_DIR="${BASE_DIR}/etc"
DATA_DIR="${BASE_DIR}/data"
DOCKER_DIR="${BASE_DIR}"
DEFAULT_DOCKER_DIR="${BASE_DIR}"
SNAP_DOCKER_DIR="/var/snap/docker/common/serverbee"
META_FILE="${CONFIG_DIR}/.install-meta"
LANG_CACHE_FILE="${CONFIG_DIR}/.install-lang"
DOMAIN_CACHE_FILE="${CONFIG_DIR}/.install-domain"
CLI_PATH="/usr/local/bin/serverbee"
# Legacy FHS-split layout (pre-/opt). Kept only for one-time auto-migration.
LEGACY_BIN_DIR="/usr/local/bin"
LEGACY_CONFIG_DIR="/etc/serverbee"
LEGACY_DATA_DIR="/var/lib/serverbee"
DOCS_URL="https://docs.serverbee.app"
CADDY_CONFIG_DIR="/etc/caddy"
CADDYFILE="${CADDY_CONFIG_DIR}/Caddyfile"

# ─── Globals ──────────────────────────────────────────────────────────────────
COMMAND=""
COMPONENT=""
METHOD=""
SERVER_URL=""
ENROLLMENT_CODE=""
DOMAIN=""
EMAIL=""
LANG_CODE="${SERVERBEE_LANG:-}"
YES=false
PURGE=false
SKIP_DNS_CHECK=false
NO_WAIT=false
CONFIG_KEY=""
CONFIG_VALUE=""
POSITIONAL_COUNT=0
MISSING_DEPS=""
MANAGED_COMPONENTS=""
UNMANAGED_COMPONENTS=""
INIT=""
RESOLVED_VERSION=""
REQUESTED_VERSION=""
RELEASE_CHANNEL="${SERVERBEE_CHANNEL:-auto}"
RELEASE_CHANNEL_USER_SPECIFIED=false
[ -z "${SERVERBEE_CHANNEL:-}" ] || RELEASE_CHANNEL_USER_SPECIFIED=true
CLI_REFRESHED=""
UPGRADE_HEALTH_ATTEMPTS=30
UPGRADE_STABILITY_CHECKS=10
DOCKER_UPGRADE_HEALTH_ATTEMPTS=90
DOCKER_UPGRADE_STABILITY_CHECKS=10
INSTALL_SERVER_HEALTH_ATTEMPTS=30
INSTALL_AGENT_ATTEMPTS=30
AGENT_LOG_START_LINE=0
AGENT_LOG_SINCE=0
AGENT_INSTALL_PROOF=""

# ─── Agent capability toggles ────────────────────────────────────────────────
# Keys MUST match the CapabilityKey strings in crates/common/src/constants.rs.
# Order in AGENT_CAPS_ALL is the display order in the interactive picker.
AGENT_CAPS_ALL="upgrade ping_icmp ping_tcp ping_http security_events firewall_block ip_quality terminal exec file docker"
AGENT_CAPS_COUNT=$(set -- $AGENT_CAPS_ALL; echo $#)
# Final selection as a comma-separated list of cap keys. Empty + not user-specified = use defaults.
AGENT_CAPS_SELECTED=""
AGENT_CAPS_USER_SPECIFIED=false

# Mirror of CAP_DEFAULT (1852): caps that are on by default.
cap_default_on() {
    case "$1" in
        upgrade|ping_icmp|ping_tcp|ping_http|security_events|firewall_block|ip_quality) return 0 ;;
        *) return 1 ;;
    esac
}

cap_risk() {
    case "$1" in
        terminal|exec|file|docker|firewall_block) echo "high" ;;
        ip_quality) echo "medium" ;;
        *) echo "low" ;;
    esac
}

cap_desc() {
    if [ "${LANG_CODE:-en}" = "zh" ]; then
        case "$1" in
            terminal) echo "Web 终端（PTY）" ;;
            exec) echo "远程执行命令" ;;
            upgrade) echo "Agent 自动升级" ;;
            ping_icmp) echo "ICMP ping 探测" ;;
            ping_tcp) echo "TCP 端口探测" ;;
            ping_http) echo "HTTP 探测" ;;
            file) echo "文件浏览/编辑/上传" ;;
            docker) echo "Docker 容器监控与操作" ;;
            security_events) echo "SSH 登录 / 爆破 / 端口扫描事件采集" ;;
            firewall_block) echo "nftables 黑名单（需 root + nft）" ;;
            ip_quality) echo "第三方 IP 质量评分" ;;
        esac
    else
        case "$1" in
            terminal) echo "Web terminal (PTY)" ;;
            exec) echo "Remote command execution" ;;
            upgrade) echo "Agent auto-upgrade" ;;
            ping_icmp) echo "ICMP ping probes" ;;
            ping_tcp) echo "TCP probes" ;;
            ping_http) echo "HTTP probes" ;;
            file) echo "File browse / edit / upload" ;;
            docker) echo "Docker container monitoring & control" ;;
            security_events) echo "SSH login / brute-force / port-scan events" ;;
            firewall_block) echo "nftables blocklist (needs root + nft)" ;;
            ip_quality) echo "Third-party IP quality scoring" ;;
        esac
    fi
}

# ─── Colors ───────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m'

info()  { printf '%b\n' "${GREEN}[INFO]${NC} $*"; }
warn()  { printf '%b\n' "${YELLOW}[WARN]${NC} $*"; }
error() { printf '%b\n' "${RED}[ERROR]${NC} $*" >&2; exit 1; }

# Print with %b (interprets the color escapes), replacing bash `echo -e`.
cecho() { printf '%b\n' "$*"; }

# ─── Shared helpers ──────────────────────────────────────────────────────────
capitalize() {
    _cap_c=$(printf '%s' "$1" | cut -c1 | tr '[:lower:]' '[:upper:]')
    _cap_r=$(printf '%s' "$1" | cut -c2-)
    printf '%s%s' "$_cap_c" "$_cap_r"
}

# Portable in-place sed (busybox sed has no `-i.bak` backup-suffix form).
sed_inplace() {
    _si_expr="$1"; _si_file="$2"
    _si_tmp=$(mktemp)
    sed "$_si_expr" "$_si_file" > "$_si_tmp" && mv "$_si_tmp" "$_si_file"
}

validate_single_line_value() {
    # Configuration formats below cannot represent control characters safely.
    # Keep validation separate from rendering so rejected values never modify
    # the destination file.
    case "$1" in
        *'
'*) return 1 ;;
    esac
    ! LC_ALL=C printf '%s' "$1" | grep '[[:cntrl:]]' >/dev/null 2>&1
}

atomic_replace_preserving_mode() {
    # $1 = destination, $2 = generated replacement. Both temporary files live
    # next to the destination so the final rename is atomic on one filesystem.
    local destination generated staged
    destination="$1"
    generated="$2"
    staged=$(mktemp "${destination}.tmp.XXXXXX") || return 1
    if ! cp -p "$destination" "$staged" \
        || ! cat "$generated" > "$staged" \
        || ! mv -f "$staged" "$destination"; then
        rm -f "$staged"
        return 1
    fi
}

render_double_quoted_value() {
    # TOML basic strings and YAML double-quoted scalars share the escaping we
    # need for printable installer values: backslash and double quote.
    SB_INSTALL_EDIT_VALUE="$1"
    export SB_INSTALL_EDIT_VALUE
    awk 'BEGIN {
        value = ENVIRON["SB_INSTALL_EDIT_VALUE"]
        for (i = 1; i <= length(value); i++) {
            c = substr(value, i, 1)
            if (c == "\\" || c == "\"") out = out "\\"
            out = out c
        }
        printf "%s", out
    }'
    unset SB_INSTALL_EDIT_VALUE
}

render_shell_double_quoted_value() {
    SB_INSTALL_EDIT_VALUE="$1"
    export SB_INSTALL_EDIT_VALUE
    awk 'BEGIN {
        value = ENVIRON["SB_INSTALL_EDIT_VALUE"]
        for (i = 1; i <= length(value); i++) {
            c = substr(value, i, 1)
            if (c == "\\" || c == "\"" || c == "$" || c == "`") out = out "\\"
            out = out c
        }
        printf "%s", out
    }'
    unset SB_INSTALL_EDIT_VALUE
}

atomic_set_assignment_line() {
    # $1 = file, $2 = plain key, $3 = rendered line, $4 = openrc|systemd
    local file key replacement kind generated
    file="$1"; key="$2"; replacement="$3"; kind="$4"
    generated=$(mktemp "${file}.generated.XXXXXX") || return 1
    SB_INSTALL_EDIT_KEY="$key"
    SB_INSTALL_EDIT_LINE="$replacement"
    SB_INSTALL_EDIT_KIND="$kind"
    export SB_INSTALL_EDIT_KEY SB_INSTALL_EDIT_LINE SB_INSTALL_EDIT_KIND
    awk '
        BEGIN {
            key=ENVIRON["SB_INSTALL_EDIT_KEY"]
            replacement=ENVIRON["SB_INSTALL_EDIT_LINE"]
            kind=ENVIRON["SB_INSTALL_EDIT_KIND"]
            written=0
        }
        {
            candidate=$0
            if (kind == "systemd") {
                sub(/^[[:space:]]*Environment=/, "", candidate)
                sub(/^"/, "", candidate)
            } else {
                sub(/^[[:space:]]*/, "", candidate)
            }
            if (index(candidate, key "=") == 1) {
                if (!written) print replacement
                written=1
                next
            }
            print
        }
        END { if (!written) print replacement }
    ' "$file" > "$generated"
    unset SB_INSTALL_EDIT_KEY SB_INSTALL_EDIT_LINE SB_INSTALL_EDIT_KIND
    if ! atomic_replace_preserving_mode "$file" "$generated"; then
        rm -f "$generated"
        return 1
    fi
    rm -f "$generated"
}

openrc_set_env() {
    local file key value escaped
    file="$1"; key="$2"; value="$3"
    validate_single_line_value "$value" || return 1
    escaped=$(render_shell_double_quoted_value "$value")
    atomic_set_assignment_line "$file" "$key" "${key}=\"${escaped}\"" openrc
}

systemd_set_env() {
    local file key value escaped
    file="$1"; key="$2"; value="$3"
    validate_single_line_value "$value" || return 1
    SB_INSTALL_EDIT_VALUE="$value"
    export SB_INSTALL_EDIT_VALUE
    escaped=$(awk 'BEGIN {
        value = ENVIRON["SB_INSTALL_EDIT_VALUE"]
        for (i = 1; i <= length(value); i++) {
            c = substr(value, i, 1)
            if (c == "\\" || c == "\"") out = out "\\"
            if (c == "%") out = out "%"
            out = out c
        }
        printf "%s", out
    }')
    unset SB_INSTALL_EDIT_VALUE
    atomic_set_assignment_line "$file" "$key" "Environment=\"${key}=${escaped}\"" systemd
}

redact_toml_file() {
    awk '
        /^[[:space:]]*\[/ {
            section = $0
            sub(/^[[:space:]]*\[/, "", section)
            sub(/\][[:space:]]*$/, "", section)
        }
        /^[[:space:]]*[A-Za-z0-9_]+[[:space:]]*=/ {
            key = $0
            sub(/^[[:space:]]*/, "", key)
            sub(/[[:space:]]*=.*/, "", key)
            full = (section == "" ? key : section "." key)
            upper = toupper(full)
            if (upper ~ /(PASSWORD|TOKEN|SECRET|ENROLLMENT_CODE)/) {
                prefix = $0
                sub(/=.*/, "=", prefix)
                print prefix " \"********\""
                next
            }
        }
        { print }
    ' "$1"
}

redact_env_lines() {
    awk '
        {
            upper = toupper($0)
            if (match(upper, /SERVERBEE_[A-Z0-9_]*(PASSWORD|TOKEN|SECRET|ENROLLMENT_CODE)[A-Z0-9_]*=/)) {
                suffix = ($0 ~ /"[[:space:]]*$/ ? "\"" : "")
                $0 = substr($0, 1, RSTART + RLENGTH - 1) "********" suffix
            }
            print
        }
    '
}

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d' ' -f1
    elif command -v openssl >/dev/null 2>&1; then
        openssl dgst -sha256 "$1" | awk '{print $NF}'
    else
        return 1
    fi
}

prompt_tty_available() {
    [ -r /dev/tty ] && [ -w /dev/tty ] || return 1
    (: </dev/tty) 2>/dev/null && (: >/dev/tty) 2>/dev/null
}

has_prompt_input() {
    [ -t 0 ] || prompt_tty_available
}

should_prompt() {
    [ "$YES" != true ] && has_prompt_input
}

# A piped installer consumes stdin as script input. Read interactive answers
# from the controlling terminal so `curl ... | sudo sh` can still run the
# no-argument wizard. Fully parameterized installs never call this helper.
prompt_read() {
    local variable_name
    variable_name="$1"
    if [ -t 0 ]; then
        # shellcheck disable=SC2229  # The variable name is intentionally dynamic.
        read -r "$variable_name"
    elif prompt_tty_available; then
        # shellcheck disable=SC2229  # The variable name is intentionally dynamic.
        read -r "$variable_name" < /dev/tty
    else
        error "Interactive input requires a terminal. Download the installer and run it directly, or pass all required options with -y."
    fi
}

normalize_lang() {
    case "${LANG_CODE:-}" in
        zh|zh_*|zh-*|cn|CN) LANG_CODE="zh" ;;
        en|en_*|en-*) LANG_CODE="en" ;;
        "") ;;
        *) error "Unsupported language: ${LANG_CODE} (use 'en' or 'zh')" ;;
    esac
}

lang_cache_read() {
    [ -f "$LANG_CACHE_FILE" ] || return 1
    _lc_cached=$(head -n1 "$LANG_CACHE_FILE" 2>/dev/null | tr -d '[:space:]')
    case "$_lc_cached" in
        en|zh) printf '%s' "$_lc_cached"; return 0 ;;
        *) return 1 ;;
    esac
}

lang_cache_write() {
    [ -n "${LANG_CODE:-}" ] || return 0
    mkdir -p "$CONFIG_DIR" 2>/dev/null || return 0
    printf '%s\n' "$LANG_CODE" > "$LANG_CACHE_FILE" 2>/dev/null || return 0
    chmod 600 "$LANG_CACHE_FILE" 2>/dev/null || true
}

detect_lang() {
    if [ -n "${LANG_CODE:-}" ]; then
        normalize_lang
        return
    fi

    local cached
    if cached=$(lang_cache_read); then
        LANG_CODE="$cached"
        return
    fi

    case "${LC_ALL:-${LANG:-en}}" in
        zh*|ZH*) LANG_CODE="zh" ;;
        *) LANG_CODE="en" ;;
    esac
}

select_language() {
    [ -z "${LANG_CODE:-}" ] || { normalize_lang; return; }

    local cached
    if cached=$(lang_cache_read); then
        LANG_CODE="$cached"
        return
    fi

    if ! should_prompt; then
        detect_lang
        return
    fi

    local choice
    echo ""
    cecho "${BOLD}Select language / 选择语言${NC}"
    echo ""
    echo "  [1] English"
    echo "  [2] 简体中文"
    echo ""
    printf '%s' "Select language [1/2]: "; prompt_read choice
    case "$choice" in
        1|en|EN|English|english) LANG_CODE="en" ;;
        2|zh|ZH|cn|CN|中文) LANG_CODE="zh" ;;
        *) error "Invalid language choice: ${choice}" ;;
    esac
    lang_cache_write
}

# i18n: one branch per key, both languages inline. Add new strings here.
# Parametrized strings use printf format specifiers (%s) — render via trp().
tr_text() {
    _z=""
    [ "${LANG_CODE:-en}" = "zh" ] && _z=1
    case "$1" in
        manager_title)  [ "$_z" ] && echo "ServerBee 管理器" || echo "ServerBee Manager" ;;
        install_menu)   [ "$_z" ] && echo "  [1] 安装      Install" || echo "  [1] Install    安装" ;;
        uninstall_menu) [ "$_z" ] && echo "  [2] 卸载      Uninstall" || echo "  [2] Uninstall  卸载" ;;
        upgrade_menu)   [ "$_z" ] && echo "  [3] 升级      Upgrade" || echo "  [3] Upgrade    升级" ;;
        status_menu)    [ "$_z" ] && echo "  [4] 状态      Status" || echo "  [4] Status     查看状态" ;;
        service_menu)   [ "$_z" ] && echo "  [5] 服务控制  Service (start/stop/restart)" || echo "  [5] Service    服务控制 (start/stop/restart)" ;;
        config_menu)    [ "$_z" ] && echo "  [6] 配置管理  Config" || echo "  [6] Config     配置管理" ;;
        env_menu)       [ "$_z" ] && echo "  [7] 环境变量  Env" || echo "  [7] Env        环境变量" ;;
        domain_menu)    [ "$_z" ] && echo "  [8] 域名 HTTPS Domain" || echo "  [8] Domain     域名 HTTPS" ;;
        exit_menu)      [ "$_z" ] && echo "  [0] 退出      Exit" || echo "  [0] Exit       退出" ;;
        select_menu)    [ "$_z" ] && echo "选择 [0-8]: " || echo "Select [0-8]: " ;;
        install_title)  [ "$_z" ] && echo "安装" || echo "Install" ;;
        agent_option)   [ "$_z" ] && echo "  [1] Agent   — 系统指标采集器" || echo "  [1] Agent   — System metrics collector" ;;
        server_option)  [ "$_z" ] && echo "  [2] Server  — 控制台和 API" || echo "  [2] Server  — Dashboard & API" ;;
        select_component) [ "$_z" ] && echo "选择组件 [1/2]: " || echo "Select component [1/2]: " ;;
        server_docker_recommended) [ "$_z" ] && echo "  [1] Docker  (Server 推荐)" || echo "  [1] Docker  (recommended for Server)" ;;
        agent_binary_recommended)  [ "$_z" ] && echo "  [1] Binary  (Agent 推荐)" || echo "  [1] Binary  (recommended for Agent)" ;;
        binary_option)  echo "  [2] Binary" ;;
        docker_option)  echo "  [2] Docker" ;;
        select_method)  [ "$_z" ] && echo "选择安装方式 [1/2]: " || echo "Select installation method [1/2]: " ;;
        configure_domain) [ "$_z" ] && echo "现在配置 HTTPS 域名（Caddy）吗？[y/N]: " || echo "Configure HTTPS domain with Caddy now? [y/N]: " ;;
        domain_prompt)  [ "$_z" ] && echo "域名（例如 monitor.example.com）: " || echo "Domain (e.g., monitor.example.com): " ;;
        email_prompt)   [ "$_z" ] && echo "证书通知邮箱（可选）: " || echo "Email for certificate notices (optional): " ;;
        server_url_prompt) echo "Server URL [%s]: " ;;
        enrollment_prompt) [ "$_z" ] && echo "Enrollment code（注册码）: " || echo "Enrollment code: " ;;
        install_plan_title) [ "$_z" ] && echo "安装计划" || echo "Installation plan" ;;
        domain_plan_title)  [ "$_z" ] && echo "域名配置计划" || echo "Domain setup plan" ;;
        will_add_download)  [ "$_z" ] && echo "将添加或下载:" || echo "Will add or download:" ;;
        start_install)  [ "$_z" ] && echo "现在开始安装？[y/N]: " || echo "Start installation now? [y/N]: " ;;
        start_domain)   [ "$_z" ] && echo "现在开始域名配置？[y/N]: " || echo "Start domain setup now? [y/N]: " ;;
        preflight)      [ "$_z" ] && echo "安装前检查:" || echo "Preflight checks:" ;;
        svc_title)      [ "$_z" ] && echo "服务控制" || echo "Service control" ;;
        svc_start)      [ "$_z" ] && echo "  [1] 启动" || echo "  [1] Start" ;;
        svc_stop)       [ "$_z" ] && echo "  [2] 停止" || echo "  [2] Stop" ;;
        svc_restart)    [ "$_z" ] && echo "  [3] 重启" || echo "  [3] Restart" ;;
        svc_select)     [ "$_z" ] && echo "选择 [1-3]: " || echo "Select [1-3]: " ;;
        uninstall_title) [ "$_z" ] && echo "卸载" || echo "Uninstall" ;;
        opt_agent)      echo "  [1] Agent" ;;
        opt_server)     echo "  [2] Server" ;;
        uninstall_confirm) [ "$_z" ] && echo "卸载 serverbee-%s（%s）%s ? [y/N]: " || echo "Uninstall serverbee-%s (%s)%s? [y/N]: " ;;
        uninstall_purge_note) [ "$_z" ] && echo "（含配置与数据）" || echo " (including config and data)" ;;
        uninstall_preserved) [ "$_z" ] && echo "  配置与数据已保留,如需移除请执行:" || echo "  Config and data preserved. To remove them, run:" ;;
        deps_install_confirm) [ "$_z" ] && echo "  现在安装它们？[y/N]: " || echo "  Install them now? [y/N]: " ;;
        docker_continue_confirm) [ "$_z" ] && echo "  仍然继续使用 Docker？[y/N]: " || echo "  Continue with Docker? [y/N]: " ;;
        docker_agent_note)  [ "$_z" ] && echo "  ServerBee Agent 是便携软件:" || echo "  ServerBee Agent is portable software:" ;;
        docker_agent_note1) [ "$_z" ] && echo "  - 单一二进制，配置与运行状态路径明确" || echo "  - Single binary with explicit config and state paths" ;;
        docker_agent_note2) [ "$_z" ] && echo "  - Docker 需 --privileged 才能采集完整指标" || echo "  - Docker requires --privileged for full metrics" ;;
        docker_agent_note3) [ "$_z" ] && echo "  - Web 终端访问的是容器而非宿主机" || echo "  - Web terminal accesses container, not host" ;;
        upgrade_confirm) [ "$_z" ] && echo "确认升级？[y/N]: " || echo "Proceed with upgrade? [y/N]: " ;;
        restart_apply_q) [ "$_z" ] && echo "  重启服务以应用更改？" || echo "  Restart service to apply changes?" ;;
        restart_apply_confirm) echo "  [y/N]: " ;;
        plan_component) [ "$_z" ] && echo "组件:" || echo "Component:" ;;
        plan_method)    [ "$_z" ] && echo "方式:" || echo "Method:" ;;
        plan_access)    [ "$_z" ] && echo "访问:" || echo "Access:" ;;
        plan_access_ip_val) [ "$_z" ] && echo "IP / 直连端口 (:9527)" || echo "IP / direct port (:9527)" ;;
        plan_access_domain_val) [ "$_z" ] && echo "域名" || echo "domain" ;;
        plan_server_url) echo "Server URL:" ;;
        plan_cfg_file)  [ "$_z" ] && echo "  - 配置文件:" || echo "  - Config file:" ;;
        plan_data_dir)  [ "$_z" ] && echo "  - 数据目录:" || echo "  - Data directory:" ;;
        plan_compose_file) [ "$_z" ] && echo "  - Compose 文件:" || echo "  - Compose file:" ;;
        plan_docker_volume) [ "$_z" ] && echo "  - Docker 卷: serverbee-data" || echo "  - Docker volume: serverbee-data" ;;
        plan_systemd)   [ "$_z" ] && echo "  - systemd 服务:" || echo "  - systemd service:" ;;
        plan_pkgs)      [ "$_z" ] && echo "  - 系统软件包:" || echo "  - System packages:" ;;
        plan_pkgs_suffix) [ "$_z" ] && echo "（脚本所需工具）" || echo "(required script tools)" ;;
        plan_gh_meta)   [ "$_z" ] && echo "  - GitHub API: 最新 ServerBee 发布元数据" || echo "  - GitHub API: latest ServerBee release metadata" ;;
        plan_binary_adopt_pre) [ "$_z" ] && echo "  - 二进制: 已存在" || echo "  - Binary: existing" ;;
        plan_binary_adopt_suf) [ "$_z" ] && echo "将被沿用（不下载二进制）" || echo "will be adopted (no binary download)" ;;
        plan_binary_dl) [ "$_z" ] && echo "  - 二进制:" || echo "  - Binary:" ;;
        plan_cli_script) [ "$_z" ] && echo "  - CLI 脚本:" || echo "  - CLI script:" ;;
        plan_docker_prereq) [ "$_z" ] && echo "  - 前置条件: 需已安装 Docker 与 Docker Compose V2" || echo "  - Prerequisite: Docker and Docker Compose V2 must already be installed" ;;
        plan_docker_image) [ "$_z" ] && echo "  - Docker 镜像:" || echo "  - Docker image:" ;;
        domain_plan_header) [ "$_z" ] && echo "HTTPS 域名配置:" || echo "HTTPS domain setup:" ;;
        dp_dns_pre)     [ "$_z" ] && echo "  - DNS 校验:" || echo "  - DNS validation:" ;;
        dp_dns_suf)     [ "$_z" ] && echo "必须解析到本机" || echo "must resolve to this server" ;;
        dp_repo)        [ "$_z" ] && echo "  - Caddy 仓库: Debian/Ubuntu 用 Cloudsmith apt 源，Fedora/CentOS 用 COPR" || echo "  - Caddy repository: Cloudsmith apt repo on Debian/Ubuntu, or COPR on Fedora/CentOS" ;;
        dp_key)         echo "  - Caddy apt key:" ;;
        dp_src)         echo "  - Caddy apt source:" ;;
        dp_pkgs)        [ "$_z" ] && echo "  - 系统软件包: 缺失时安装 Caddy 及其仓库依赖" || echo "  - System packages: Caddy and its repository dependencies when missing" ;;
        dp_caddyfile)   echo "  - Caddyfile:" ;;
        dp_bind)        [ "$_z" ] && echo "  - 服务监听地址: 127.0.0.1:9527" || echo "  - Server bind address: 127.0.0.1:9527" ;;
        dp_cookie)      echo "  - secure_cookie: true" ;;
        dp_url)         [ "$_z" ] && echo "  - 公网地址:" || echo "  - Public URL:" ;;
        domain_label)   [ "$_z" ] && echo "域名:" || echo "Domain:" ;;
        email_label)    [ "$_z" ] && echo "邮箱: " || echo "Email: " ;;
        result_server_ok) [ "$_z" ] && echo "ServerBee Server 安装成功！" || echo "ServerBee Server installed successfully!" ;;
        result_agent_ok)  [ "$_z" ] && echo "ServerBee Agent 安装成功！" || echo "ServerBee Agent installed successfully!" ;;
        result_server_unverified) [ "$_z" ] && echo "ServerBee Server 已安装，但未验证健康状态。" || echo "ServerBee Server installed, but health was not verified." ;;
        result_agent_unverified) [ "$_z" ] && echo "ServerBee Agent 已安装，但未验证 Server 连接。" || echo "ServerBee Agent installed, but its Server connection was not verified." ;;
        lbl_dashboard)  [ "$_z" ] && echo "  控制台:" || echo "  Dashboard:" ;;
        lbl_username)   [ "$_z" ] && echo "  用户名:" || echo "  Username:" ;;
        lbl_password)   [ "$_z" ] && echo "  密码:" || echo "  Password:" ;;
        pw_docker)      [ "$_z" ] && echo "（自动生成，取最后一段: docker compose -f %s logs serverbee-server | grep -A8 'FIRST-RUN ADMIN CREDENTIALS' | tail -n 9）" || echo "(auto-generated, use the LAST block from: docker compose -f %s logs serverbee-server | grep -A8 'FIRST-RUN ADMIN CREDENTIALS' | tail -n 9)" ;;
        pw_systemd)     [ "$_z" ] && echo "（自动生成，取最后一段: sudo journalctl -u serverbee-server --no-pager | grep -A8 'FIRST-RUN ADMIN CREDENTIALS' | tail -n 9）" || echo "(auto-generated, use the LAST block from: sudo journalctl -u serverbee-server --no-pager | grep -A8 'FIRST-RUN ADMIN CREDENTIALS' | tail -n 9)" ;;
        pw_proc)        [ "$_z" ] && echo "（自动生成，在进程输出中查找 'FIRST-RUN ADMIN CREDENTIALS')" || echo "(auto-generated, check process output for 'FIRST-RUN ADMIN CREDENTIALS')" ;;
        pw_must_change) [ "$_z" ] && echo "  （一次性密码 —— 首次登录后必须修改）" || echo "  (one-time password — you must change it on first login)" ;;
        lbl_docs)       [ "$_z" ] && echo "  文档:" || echo "  Docs:" ;;
        lbl_server_url) echo "  Server URL:" ;;
        lbl_logs)       [ "$_z" ] && echo "  日志:" || echo "  Logs:" ;;
        lbl_start)      [ "$_z" ] && echo "  启动:" || echo "  Start:" ;;
        lbl_config)     [ "$_z" ] && echo "  配置:" || echo "  Config:" ;;
        status_none)    [ "$_z" ] && echo "未找到任何 ServerBee 组件。运行 'serverbee install' 开始安装。" || echo "No ServerBee components found. Run 'serverbee install' to get started." ;;
        status_title)   [ "$_z" ] && echo "ServerBee 状态" || echo "ServerBee Status" ;;
        st_version)     [ "$_z" ] && echo "  版本:" || echo "  Version:" ;;
        st_binary)      [ "$_z" ] && echo "  二进制:" || echo "  Binary:" ;;
        st_config)      [ "$_z" ] && echo "  配置:" || echo "  Config:" ;;
        st_service)     [ "$_z" ] && echo "  服务:" || echo "  Service:" ;;
        st_active)      [ "$_z" ] && echo "运行中" || echo "active (running)" ;;
        st_since)       [ "$_z" ] && echo "自" || echo "since" ;;
        st_recent_logs) [ "$_z" ] && echo "  最近日志:" || echo "  Recent logs:" ;;
        st_no_logs)     [ "$_z" ] && echo "    （无日志）" || echo "    (no logs)" ;;
        st_server)      echo "  Server:" ;;
        st_dashboard)   [ "$_z" ] && echo "  控制台:" || echo "  Dashboard:" ;;
        st_reverse_proxy) [ "$_z" ] && echo "位于反向代理之后（未记录配置的域名）" || echo "behind reverse proxy (configured domain unavailable)" ;;
        st_container)   [ "$_z" ] && echo "  容器:" || echo "  Container:" ;;
        st_stopped)     [ "$_z" ] && echo "已停止" || echo "stopped" ;;
        st_image)       [ "$_z" ] && echo "  镜像:" || echo "  Image:" ;;
        st_port)        [ "$_z" ] && echo "  端口:" || echo "  Port:" ;;
        st_unknown)     [ "$_z" ] && echo "未知" || echo "unknown" ;;
        caps_title)     [ "$_z" ] && echo "Agent 能力开关" || echo "Agent capability toggles" ;;
        caps_intro)     [ "$_z" ] && echo "选择该 Agent 将在本机启用并上报的能力。默认开启项已勾选。" || echo "Pick which capabilities this agent will enable locally and report. Defaults are already checked." ;;
        caps_legend)    [ "$_z" ] && echo "风险：[low] 可放心保留 · [medium] 需访问外网 · [high] 可远程控制本机" || echo "Risk: [low] safe to leave on · [medium] outbound network · [high] gives remote control over this host" ;;
        caps_hint)      [ "$_z" ] && echo "输入序号切换（如 '8 10'），'a'=全开，'n'=全关，'d' 或 Enter=完成" || echo "Toggle by number(s) (e.g. '8 10'), 'a'=all, 'n'=none, 'd' or Enter=done" ;;
        caps_prompt)    echo "> " ;;
        caps_invalid)   [ "$_z" ] && echo "  已忽略：%s" || echo "  ignored: %s" ;;
        caps_unknown_cli) [ "$_z" ] && echo "--caps 中存在未知能力：%s\n  可选键：%s" || echo "Unknown capability in --caps: %s\n  Valid keys: %s" ;;
        caps_plan_label) [ "$_z" ] && echo "  - 能力:" || echo "  - Capabilities:" ;;
        caps_plan_default) [ "$_z" ] && echo "默认" || echo "default" ;;
        caps_plan_none) [ "$_z" ] && echo "（无）" || echo "(none)" ;;
        *) echo "??$1??" ;;
    esac
}

# Docs site language segment (apps/docs is bilingual: zh / en).
docs_lang() {
    [ "${LANG_CODE:-en}" = "zh" ] && echo "zh" || echo "en"
}

# Translate + printf for parametrized strings (no trailing newline added).
trp() {
    local key fmt
    key="$1"; shift
    fmt="$(tr_text "$key")"
    # shellcheck disable=SC2059
    printf "$fmt" "$@"
}

# ─── Dependency check ─────────────────────────────────────────────────────────
install_deps() {
    # Auto-install missing packages using the available package manager
    if command -v apt-get >/dev/null 2>&1; then
        info "Installing missing tools via apt-get: $*"
        apt-get update -qq >/dev/null 2>&1
        apt-get install -y -qq "$@" >/dev/null 2>&1 || error "Failed to install: $*"
    elif command -v yum >/dev/null 2>&1; then
        info "Installing missing tools via yum: $*"
        yum install -y -q "$@" >/dev/null 2>&1 || error "Failed to install: $*"
    elif command -v dnf >/dev/null 2>&1; then
        info "Installing missing tools via dnf: $*"
        dnf install -y -q "$@" >/dev/null 2>&1 || error "Failed to install: $*"
    elif command -v apk >/dev/null 2>&1; then
        info "Installing missing tools via apk: $*"
        apk add --quiet "$@" >/dev/null 2>&1 || error "Failed to install: $*"
    else
        error "Missing required tools: $*\n  No supported package manager found (apt-get/yum/dnf/apk). Install them manually."
    fi
}

check_deps() {
    local missing cmd confirm
    missing=""
    for cmd in curl grep sed awk mktemp; do
        if ! command -v "$cmd" >/dev/null 2>&1; then
            missing="${missing:+$missing }$cmd"
        fi
    done
    [ -z "$missing" ] && return

    if [ "$YES" = true ] || ! has_prompt_input; then
        install_deps $missing
    else
        warn "Missing required tools: $missing"
        printf '%s' "$(tr_text deps_install_confirm)"; prompt_read confirm
        case "$confirm" in
            [yY]|[yY][eE][sS]) install_deps $missing ;;
            *) error "Cannot continue without: $missing" ;;
        esac
    fi
}

collect_missing_deps() {
    local cmd
    MISSING_DEPS=""
    for cmd in curl grep sed awk mktemp; do
        if ! command -v "$cmd" >/dev/null 2>&1; then
            MISSING_DEPS="${MISSING_DEPS:+$MISSING_DEPS }$cmd"
        fi
    done
}

docker_is_snap() {
    command -v docker >/dev/null 2>&1 || return 1
    local docker_path
    docker_path=$(command -v docker)
    case "$(readlink -f "$docker_path" 2>/dev/null || echo "$docker_path")" in
        /snap/*|/usr/bin/snap) return 0 ;;
        *) return 1 ;;
    esac
}

configure_docker_dir() {
    if docker_is_snap; then
        DOCKER_DIR="$SNAP_DOCKER_DIR"
    else
        DOCKER_DIR="$DEFAULT_DOCKER_DIR"
    fi
}

# Config directory for docker-mode components. The snap-confined Docker daemon
# cannot bind-mount paths under /opt or /etc, so config that must be visible
# inside a container has to live under the snap-accessible tree.
docker_conf_dir() {
    if docker_is_snap; then
        echo "${SNAP_DOCKER_DIR}/etc"
    else
        echo "${CONFIG_DIR}"
    fi
}

# Resolve a component's config file based on how it was installed.
conf_file_for() {
    local comp method
    comp="$1"
    method=$(meta_read "$comp" "method" 2>/dev/null || echo "")
    if [ "$method" = "docker" ]; then
        echo "$(docker_conf_dir)/${comp}.toml"
    else
        echo "${CONFIG_DIR}/${comp}.toml"
    fi
}

# ─── Privilege ────────────────────────────────────────────────────────────────
# Re-exec under sudo/doas when not root and run as a file; error when piped
# without root (the operator then pipes to 'doas sh' / 'sudo sh'). The body
# assumes root throughout, so we elevate the whole process rather than prefix
# every privileged command.
require_root() {
    local _v
    [ "$(id -u)" -eq 0 ] && return 0
    if [ -n "$SELF_SCRIPT" ] && [ -r "$SELF_SCRIPT" ]; then
        if command -v sudo >/dev/null 2>&1; then
            exec sudo -E sh "$SELF_SCRIPT" "$@"
        elif command -v doas >/dev/null 2>&1; then
            # doas has no `sudo -E`; it resets the environment. Forward the vars
            # the script reads via `env` as separate argv pairs (no re-quoting),
            # so SERVERBEE_*/LANG_CODE survive elevation. env assignments go
            # right after `env`; the original CLI args stay after the script.
            set -- sh "$SELF_SCRIPT" "$@"
            for _v in LANG_CODE SERVERBEE_LANG SERVERBEE_NO_MAIN \
                      SERVERBEE_ADMIN__USERNAME SERVERBEE_AUTH__SECURE_COOKIE \
                      SERVERBEE_SERVER__DATA_DIR SERVERBEE_TEST_DNS_A \
                      SERVERBEE_TEST_DNS_AAAA SERVERBEE_TEST_PUBLIC_IPV4 \
                      SERVERBEE_TEST_PUBLIC_IPV6; do
                if eval "[ -n \"\${$_v+x}\" ]"; then
                    eval "set -- \"$_v=\${$_v}\" \"\$@\""
                fi
            done
            exec doas env "$@"
        fi
    fi
    error "This script must run as root.\n  Re-run as root, or pipe to a privileged shell, e.g.:\n    curl -fsSL ... | sudo sh\n    curl -fsSL ... | doas sh"
}

# ─── Init detection ──────────────────────────────────────────────────────────
detect_init() {
    if command -v rc-service >/dev/null 2>&1 && [ -x /sbin/openrc-run ]; then
        INIT=openrc
    elif command -v systemctl >/dev/null 2>&1 && [ -d /run/systemd/system ]; then
        INIT=systemd
    else
        INIT=none
    fi
}

# Back-compat predicate used by legacy-layout migration (legacy installs were
# always systemd).
has_systemd() {
    [ "$INIT" = systemd ]
}

# ─── Legacy layout migration ──────────────────────────────────────────────────
migrate_legacy_layout() {
    [ -f "$META_FILE" ] && return 0

    local legacy_meta has_legacy comp f d svc unit
    legacy_meta="${LEGACY_CONFIG_DIR}/.install-meta"
    has_legacy=false
    [ -f "$legacy_meta" ] && has_legacy=true
    [ -f "${LEGACY_BIN_DIR}/serverbee-server" ] && has_legacy=true
    [ -f "${LEGACY_BIN_DIR}/serverbee-agent" ] && has_legacy=true
    [ "$has_legacy" = true ] || return 0

    info "Detected legacy install layout — migrating to ${BASE_DIR}"
    mkdir -p "$INSTALL_DIR" "$CONFIG_DIR" "$DATA_DIR"

    for comp in server agent; do
        if [ -f "${LEGACY_BIN_DIR}/serverbee-${comp}" ]; then
            mv -f "${LEGACY_BIN_DIR}/serverbee-${comp}" "${INSTALL_DIR}/serverbee-${comp}"
        fi
    done

    if [ -d "$LEGACY_CONFIG_DIR" ]; then
        for f in "$LEGACY_CONFIG_DIR"/* "$LEGACY_CONFIG_DIR"/.install-meta; do
            [ -e "$f" ] || continue
            mv -f "$f" "$CONFIG_DIR"/ 2>/dev/null || true
        done
        rmdir "$LEGACY_CONFIG_DIR" 2>/dev/null || true
    fi

    if [ -d "$LEGACY_DATA_DIR" ] && [ "$LEGACY_DATA_DIR" != "$DATA_DIR" ]; then
        for d in "$LEGACY_DATA_DIR"/* "$LEGACY_DATA_DIR"/.[!.]*; do
            [ -e "$d" ] || continue
            mv -f "$d" "$DATA_DIR"/ 2>/dev/null || true
        done
        rmdir "$LEGACY_DATA_DIR" 2>/dev/null || true
    fi

    if [ -f "${CONFIG_DIR}/server.toml" ]; then
        sed_inplace "s#${LEGACY_DATA_DIR}#${DATA_DIR}#g" "${CONFIG_DIR}/server.toml" 2>/dev/null || true
    fi

    if has_systemd; then
        for comp in server agent; do
            svc="serverbee-${comp}"
            unit="/etc/systemd/system/${svc}.service"
            [ -f "$unit" ] || continue
            _tmp=$(mktemp)
            if sed \
                -e "s#${LEGACY_BIN_DIR}/serverbee-${comp}#${INSTALL_DIR}/serverbee-${comp}#g" \
                -e "s#WorkingDirectory=${LEGACY_DATA_DIR}#WorkingDirectory=${CONFIG_DIR}#g" \
                -e "s#WorkingDirectory=${LEGACY_CONFIG_DIR}#WorkingDirectory=${CONFIG_DIR}#g" \
                -e "s#SERVERBEE_SERVER__DATA_DIR=${LEGACY_DATA_DIR}#SERVERBEE_SERVER__DATA_DIR=${DATA_DIR}#g" \
                "$unit" > "$_tmp" 2>/dev/null; then
                mv "$_tmp" "$unit"
            else
                rm -f "$_tmp"
            fi
        done
        systemctl daemon-reload 2>/dev/null || true
        for comp in server agent; do
            svc="serverbee-${comp}"
            if systemctl is-active --quiet "$svc" 2>/dev/null; then
                systemctl restart "$svc" 2>/dev/null || true
            fi
        done
    fi

    info "Migration complete — ServerBee now lives under ${BASE_DIR}"
}

# ─── Known subcommands ───────────────────────────────────────────────────────
is_known_command() {
    case "$1" in
        install|uninstall|upgrade|status|start|stop|restart|config|env|domain|help|version) return 0 ;;
        *) return 1 ;;
    esac
}

# ─── Argument parsing ─────────────────────────────────────────────────────────
option_requires_value() {
    error "Option $1 requires a value. Run 'serverbee --help' for usage."
}

parse_args() {
    while [ $# -gt 0 ]; do
        case "$1" in
            --method)        [ $# -ge 2 ] || option_requires_value "$1"; METHOD="$2"; shift 2 ;;
            --server-url)    [ $# -ge 2 ] || option_requires_value "$1"; SERVER_URL="$2"; shift 2 ;;
            --enrollment-code) [ $# -ge 2 ] || option_requires_value "$1"; ENROLLMENT_CODE="$2"; shift 2 ;;
            --password)      error "--password is no longer supported. ServerBee always generates a one-time first-run admin password; check the server logs after installation." ;;
            --domain)        [ $# -ge 2 ] || option_requires_value "$1"; DOMAIN="$2"; shift 2 ;;
            --email)         [ $# -ge 2 ] || option_requires_value "$1"; EMAIL="$2"; shift 2 ;;
            --lang)          [ $# -ge 2 ] || option_requires_value "$1"; LANG_CODE="$2"; normalize_lang; shift 2 ;;
            --version)       [ $# -ge 2 ] || option_requires_value "$1"; REQUESTED_VERSION="$2"; shift 2 ;;
            --channel)       [ $# -ge 2 ] || option_requires_value "$1"; RELEASE_CHANNEL="$2"; RELEASE_CHANNEL_USER_SPECIFIED=true; shift 2 ;;
            --skip-dns-check) SKIP_DNS_CHECK=true; shift ;;
            --caps)          [ $# -ge 2 ] || option_requires_value "$1"; set_caps_from_cli "$2"; shift 2 ;;
            --no-wait)       NO_WAIT=true; shift ;;
            --purge)         PURGE=true; shift ;;
            --yes|-y)        YES=true; shift ;;
            -*)              error "Unknown option: $1" ;;
            *)
                POSITIONAL_COUNT=$((POSITIONAL_COUNT + 1))
                case "$POSITIONAL_COUNT" in
                    1) COMPONENT="$1" ;;
                    2) CONFIG_KEY="$1" ;;
                    3) CONFIG_VALUE="$1" ;;
                    *) error "Unexpected argument: $1" ;;
                esac
                shift ;;
        esac
    done
}

validate_parsed_args() {
    case "$COMMAND" in
        install|uninstall|upgrade|status|start|stop|restart)
            [ "$POSITIONAL_COUNT" -le 1 ] || error "Unexpected argument: ${CONFIG_KEY}"
            ;;
        domain)
            [ "$POSITIONAL_COUNT" -le 1 ] || error "Unexpected argument: ${CONFIG_KEY}"
            ;;
        config|env)
            if [ "$COMPONENT" = set ]; then
                [ "$POSITIONAL_COUNT" -le 3 ] || error "Too many arguments for serverbee ${COMMAND} set"
            else
                [ "$POSITIONAL_COUNT" -le 1 ] || error "Unexpected argument: ${CONFIG_KEY}"
            fi
            ;;
    esac
}

print_usage() {
    local topic
    topic="${1:-}"
    case "$topic" in
        install)
            echo "Usage: serverbee install <server|agent> [--method binary|docker] [options]"
            echo "       serverbee <server|agent> [options]"
            echo "Options: --version <release> --channel <auto|stable|beta> --lang <en|zh> -y"
            echo "Agent:  --server-url <url> --enrollment-code <code> [--caps <list>] [--no-wait]"
            echo "Server: --domain <name> [--email <address>] [--skip-dns-check] [--no-wait]"
            echo "Verification exits: 0=verified/skipped, 1=Server unhealthy, 75=Agent temporarily unverified, 78=Agent authentication rejected"
            ;;
        config)
            echo "Usage: serverbee config [server|agent]"
            echo "       serverbee config set <key> <value> [-y]"
            ;;
        env)
            echo "Usage: serverbee env"
            echo "       serverbee env set <KEY> <value>"
            ;;
        domain)
            echo "Usage: serverbee domain setup --domain <name> [--email <address>] [--skip-dns-check]"
            ;;
        uninstall)
            echo "Usage: serverbee uninstall <server|agent> [--purge] [-y]"
            ;;
        upgrade)
            echo "Usage: serverbee upgrade [server|agent] [--version <release>] [--channel <auto|stable|beta>] [-y]"
            ;;
        start|stop|restart|status)
            echo "Usage: serverbee ${topic} [server|agent]"
            ;;
        *)
            echo "Usage: serverbee <command> [options]"
            echo "Commands: install, uninstall, upgrade, status, start, stop, restart, config, env, domain, version"
            echo "Run 'serverbee <command> --help' for command-specific usage."
            ;;
    esac
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
    if [ -n "$RESOLVED_VERSION" ]; then
        echo "$RESOLVED_VERSION"
        return
    fi
    local tag releases_json
    if [ -n "$REQUESTED_VERSION" ]; then
        case "$REQUESTED_VERSION" in
            v*) tag="$REQUESTED_VERSION" ;;
            *) tag="v${REQUESTED_VERSION}" ;;
        esac
    else
        case "$RELEASE_CHANNEL" in
            auto|stable|beta)
                releases_json=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases?per_page=100") \
                    || error "Failed to fetch release metadata from GitHub"
                tag=$(printf '%s\n' "$releases_json" \
                    | awk -v channel="$RELEASE_CHANNEL" '
                        BEGIN { fallback=""; selected="" }
                        /^  \{/ { in_release=1; tag=""; draft=0 }
                        in_release && /"tag_name":/ {
                            line=$0
                            sub(/^.*"tag_name": *"/, "", line)
                            sub(/".*$/, "", line)
                            tag=line
                        }
                        in_release && /"draft": true/ { draft=1 }
                        in_release && /^  }[,]?$/ {
                            version=tag
                            sub(/^v/, "", version)
                            is_prerelease=(version ~ /-/)
                            if (tag != "" && !draft) {
                                if (channel == "stable" && !is_prerelease && selected == "") selected=tag
                                if (channel == "beta" && is_prerelease && selected == "") selected=tag
                                if (channel == "auto") {
                                    if (!is_prerelease && selected == "") selected=tag
                                    if (fallback == "") fallback=tag
                                }
                            }
                            in_release=0
                        }
                        END {
                            if (selected != "") print selected
                            else if (channel == "auto" && fallback != "") print fallback
                        }
                    ')
                ;;
            *) error "Invalid release channel: ${RELEASE_CHANNEL} (expected auto, stable, or beta)" ;;
        esac
    fi
    [ -z "$tag" ] && error "Failed to get latest version from GitHub"
    semver_is_valid "$tag" || error "Invalid release version: ${tag}"
    RESOLVED_VERSION="$tag"
    echo "$tag"
}

semver_is_valid() {
    awk -v version="${1#v}" 'BEGIN {
        exit(version ~ /^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z]+([.-][0-9A-Za-z]+)*)?(\+[0-9A-Za-z]+([.-][0-9A-Za-z]+)*)?$/ ? 0 : 1)
    }'
}

semver_is_greater() {
    awk -v target="${1#v}" -v current="${2#v}" '
        function compare_identifiers(left, right, left_numeric, right_numeric) {
            left_numeric = left ~ /^[0-9]+$/
            right_numeric = right ~ /^[0-9]+$/
            if (left_numeric && right_numeric) return (left + 0 > right + 0) ? 1 : ((left + 0 < right + 0) ? -1 : 0)
            if (left_numeric != right_numeric) return left_numeric ? -1 : 1
            return (left > right) ? 1 : ((left < right) ? -1 : 0)
        }
        function split_version(version, core, prerelease, plus, dash) {
            plus = index(version, "+")
            if (plus) version = substr(version, 1, plus - 1)
            dash = index(version, "-")
            parsed_core = dash ? substr(version, 1, dash - 1) : version
            parsed_pre = dash ? substr(version, dash + 1) : ""
        }
        BEGIN {
            split_version(target); target_core=parsed_core; target_pre=parsed_pre
            split_version(current); current_core=parsed_core; current_pre=parsed_pre
            split(target_core, target_parts, ".")
            split(current_core, current_parts, ".")
            for (i=1; i<=3; i++) {
                left=target_parts[i] + 0; right=current_parts[i] + 0
                if (left != right) exit(left > right ? 0 : 1)
            }
            if (target_pre == "" && current_pre != "") exit 0
            if (target_pre != "" && current_pre == "") exit 1
            if (target_pre == current_pre) exit 1
            target_count=split(target_pre, target_ids, ".")
            current_count=split(current_pre, current_ids, ".")
            count=(target_count > current_count ? target_count : current_count)
            for (i=1; i<=count; i++) {
                if (i > target_count) exit 1
                if (i > current_count) exit 0
                compared=compare_identifiers(target_ids[i], current_ids[i])
                if (compared != 0) exit(compared > 0 ? 0 : 1)
            }
            exit 1
        }
    '
}

docker_image_tag() {
    echo "${1#v}"
}

get_local_ip() {
    ip -4 route get 1.1.1.1 2>/dev/null | awk '{print $7; exit}' \
        || hostname -I 2>/dev/null | awk '{print $1}' \
        || echo "localhost"
}

get_public_ipv4() {
    if [ -n "${SERVERBEE_TEST_PUBLIC_IPV4:-}" ]; then
        echo "$SERVERBEE_TEST_PUBLIC_IPV4"
        return
    fi
    curl -4 -fsS --max-time 5 https://api.ipify.org 2>/dev/null || true
}

get_public_ipv6() {
    if [ -n "${SERVERBEE_TEST_PUBLIC_IPV6:-}" ]; then
        echo "$SERVERBEE_TEST_PUBLIC_IPV6"
        return
    fi
    curl -6 -fsS --max-time 5 https://api6.ipify.org 2>/dev/null || true
}

resolve_domain_a() {
    local domain
    domain="$1"
    if [ -n "${SERVERBEE_TEST_DNS_A:-}" ]; then
        echo "$SERVERBEE_TEST_DNS_A" | tr ',' '\n' | sed '/^$/d'
        return
    fi
    if command -v getent >/dev/null 2>&1; then
        getent ahostsv4 "$domain" 2>/dev/null | awk '{print $1}' | sort -u
    elif command -v dig >/dev/null 2>&1; then
        dig +short A "$domain" 2>/dev/null | sed '/^$/d'
    elif command -v host >/dev/null 2>&1; then
        host -t A "$domain" 2>/dev/null | awk '/has address/ {print $4}'
    fi
}

resolve_domain_aaaa() {
    local domain
    domain="$1"
    if [ -n "${SERVERBEE_TEST_DNS_AAAA:-}" ]; then
        echo "$SERVERBEE_TEST_DNS_AAAA" | tr ',' '\n' | sed '/^$/d'
        return
    fi
    if command -v getent >/dev/null 2>&1; then
        getent ahostsv6 "$domain" 2>/dev/null | awk '{print $1}' | grep -vi '^::ffff:' | sort -u
    elif command -v dig >/dev/null 2>&1; then
        dig +short AAAA "$domain" 2>/dev/null | sed '/^$/d'
    elif command -v host >/dev/null 2>&1; then
        host -t AAAA "$domain" 2>/dev/null | awk '/has IPv6 address/ {print $5}'
    fi
}

validate_domain_name() {
    printf '%s' "$1" | grep -Eq '^[A-Za-z0-9]([A-Za-z0-9-]{0,61}[A-Za-z0-9])?(\.[A-Za-z0-9]([A-Za-z0-9-]{0,61}[A-Za-z0-9])?)+$' \
        || error "Invalid domain: ${1}\n  Use a hostname like monitor.example.com, without http:// or a path."
}

line_contains_value() {
    local haystack needle
    haystack="$1"; needle="$2"
    [ -n "$needle" ] && echo "$haystack" | grep -Fxq "$needle"
}

domain_points_to_server() {
    local dns_a dns_aaaa public_ipv4 public_ipv6
    dns_a="$1"; dns_aaaa="$2"; public_ipv4="$3"; public_ipv6="$4"
    line_contains_value "$dns_a" "$public_ipv4" || line_contains_value "$dns_aaaa" "$public_ipv6"
}

warn_mismatched_aaaa_if_present() {
    local domain dns_aaaa public_ipv6
    domain="$1"; dns_aaaa="$2"; public_ipv6="$3"

    [ -n "$dns_aaaa" ] || return 0
    if [ -n "$public_ipv6" ] && line_contains_value "$dns_aaaa" "$public_ipv6"; then
        return 0
    fi

    if [ "${LANG_CODE:-en}" = "zh" ]; then
        warn "${domain} 的 AAAA 记录没有指向当前服务器。"
        if [ -n "$public_ipv6" ]; then
            echo "  当前服务器 IPv6: ${public_ipv6}"
            echo "  DNS AAAA: ${dns_aaaa}"
            echo "  请修正 AAAA 记录；如果只使用 IPv4，请删除 AAAA 记录。"
        else
            echo "  当前服务器未检测到公网 IPv6，但 DNS 存在 AAAA: ${dns_aaaa}"
            echo "  除非你确认 IPv6 可以访问这台服务器，否则请删除 AAAA 记录。"
        fi
        echo "  Caddy/Let's Encrypt 可能会尝试 IPv6，导致证书申请失败。"
    else
        warn "AAAA record for ${domain} does not point to this server."
        if [ -n "$public_ipv6" ]; then
            echo "  Current server IPv6: ${public_ipv6}"
            echo "  DNS AAAA: ${dns_aaaa}"
            echo "  Fix the AAAA record or remove it if you only want IPv4."
        else
            echo "  This server has no detected public IPv6, but DNS has AAAA: ${dns_aaaa}"
            echo "  Remove the AAAA record unless you have verified IPv6 reaches this server."
        fi
        echo "  Caddy/Let's Encrypt may try IPv6 and certificate issuance may fail."
    fi
}

print_dns_mismatch_help() {
    local domain public_ipv4 public_ipv6 dns_a dns_aaaa
    domain="$1"; public_ipv4="$2"; public_ipv6="$3"; dns_a="$4"; dns_aaaa="$5"

    if [ "${LANG_CODE:-en}" = "zh" ]; then
        echo ""
        echo "域名 ${domain} 还没有解析到当前服务器。"
        echo ""
        echo "当前服务器 IP:"
        echo "  IPv4: ${public_ipv4:-未知}"
        echo "  IPv6: ${public_ipv6:-未知}"
        echo ""
        echo "当前 DNS 记录:"
        echo "  A:    ${dns_a:-无}"
        echo "  AAAA: ${dns_aaaa:-无}"
        echo ""
        echo "请添加或更新 DNS:"
        [ -n "$public_ipv4" ] && echo "  A    ${domain} -> ${public_ipv4}"
        [ -n "$public_ipv6" ] && echo "  AAAA ${domain} -> ${public_ipv6}"
        echo ""
        echo "继续之前 DNS 必须匹配。"
        echo "如果不匹配，Caddy/Let's Encrypt 证书申请会失败。"
        echo "更新 DNS 后按 Enter 重新校验，按 Ctrl+C 停止。"
        echo ""
    else
        echo ""
        echo "Domain ${domain} does not resolve to this server yet."
        echo ""
        echo "Current server IP:"
        echo "  IPv4: ${public_ipv4:-unknown}"
        echo "  IPv6: ${public_ipv6:-unknown}"
        echo ""
        echo "Current DNS records:"
        echo "  A:    ${dns_a:-none}"
        echo "  AAAA: ${dns_aaaa:-none}"
        echo ""
        echo "Please add/update DNS:"
        [ -n "$public_ipv4" ] && echo "  A    ${domain} -> ${public_ipv4}"
        [ -n "$public_ipv6" ] && echo "  AAAA ${domain} -> ${public_ipv6}"
        echo ""
        echo "DNS must match before continuing."
        echo "If this does not match, Caddy/Let's Encrypt certificate issuance will fail."
        echo "Update DNS, then press Enter to check again. Press Ctrl+C to stop."
        echo ""
    fi
}

check_domain_points_here() {
    local domain public_ipv4 public_ipv6 dns_a dns_aaaa _
    domain="$1"
    if [ "$SKIP_DNS_CHECK" = true ]; then
        warn "Skipping DNS check for ${domain}."
        return
    fi

    public_ipv4=$(get_public_ipv4)
    public_ipv6=$(get_public_ipv6)

    while true; do
        dns_a=$(resolve_domain_a "$domain" || true)
        dns_aaaa=$(resolve_domain_aaaa "$domain" || true)

        if domain_points_to_server "$dns_a" "$dns_aaaa" "$public_ipv4" "$public_ipv6"; then
            info "DNS check passed: ${domain} resolves to this server."
            warn_mismatched_aaaa_if_present "$domain" "$dns_aaaa" "$public_ipv6"
            return
        fi

        print_dns_mismatch_help "$domain" "$public_ipv4" "$public_ipv6" "$dns_a" "$dns_aaaa"
        if ! should_prompt; then
            error "DNS validation failed for ${domain}. Fix DNS and re-run, or pass --skip-dns-check if you have configured TLS another way."
        fi
        if [ "${LANG_CODE:-en}" = "zh" ]; then
            printf '%s' "按 Enter 重新校验 DNS..."; prompt_read _
        else
            printf '%s' "Press Enter to re-check DNS..."; prompt_read _
        fi
    done
}

# ─── Install metadata (.install-meta JSON) ───────────────────────────────────
# Uses basic grep/sed/awk for JSON manipulation to avoid a jq dependency.

meta_read() {
    local component field
    component="$1"; field="$2"
    if [ ! -f "$META_FILE" ]; then echo ""; return; fi
    sed -n "/\"${component}\"/,/}/p" "$META_FILE" \
        | grep "\"${field}\"" \
        | sed 's/.*: *"//;s/".*//' \
        || echo ""
}

meta_write() {
    local component method version timestamp block tmp
    component="$1"; method="$2"; version="$3"
    timestamp=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

    mkdir -p "$CONFIG_DIR"

    if [ ! -f "$META_FILE" ]; then
        echo "{}" > "$META_FILE"
    fi

    block=$(cat <<JSONBLOCK
    "${component}": {
        "method": "${method}",
        "version": "${version}",
        "channel": "${RELEASE_CHANNEL}",
        "installed_at": "${timestamp}"
    }
JSONBLOCK
)

    if grep -q "\"${component}\"" "$META_FILE" 2>/dev/null; then
        tmp=$(mktemp)
        awk -v comp="\"${component}\"" '
            BEGIN { skip=0 }
            $0 ~ comp { skip=1; next }
            skip && /}/ { skip=0; next }
            !skip { print }
        ' "$META_FILE" > "$tmp"
        mv "$tmp" "$META_FILE"
    fi

    tmp=$(mktemp)
    if [ "$(wc -l < "$META_FILE")" -le 1 ]; then
        echo "{" > "$tmp"
        echo "$block" >> "$tmp"
        echo "}" >> "$tmp"
    else
        sed '$ d' "$META_FILE" > "$tmp"
        if grep -q "}" "$tmp" 2>/dev/null; then
            sed_inplace '/^[[:space:]].*}$/s/}$/},/' "$tmp"
        fi
        echo "$block" >> "$tmp"
        echo "}" >> "$tmp"
    fi
    mv "$tmp" "$META_FILE"
    chmod 600 "$META_FILE"
}

meta_remove() {
    local component tmp tmp2
    component="$1"
    if [ ! -f "$META_FILE" ]; then return; fi

    tmp=$(mktemp)
    awk -v comp="\"${component}\"" '
        BEGIN { skip=0 }
        $0 ~ comp { skip=1; next }
        skip && /}/ { skip=0; next }
        !skip { print }
    ' "$META_FILE" > "$tmp"

    # Clean up trailing commas before a lone closing brace.
    tmp2=$(mktemp)
    awk '
        { lines[NR] = $0 }
        END {
            for (i = 1; i <= NR; i++) {
                line = lines[i]
                if (line ~ /,$/) {
                    j = i + 1
                    while (j <= NR && lines[j] ~ /^[[:space:]]*$/) j++
                    if (j <= NR && lines[j] ~ /^[[:space:]]*}[[:space:]]*$/) {
                        sub(/,$/, "", line)
                    }
                }
                print line
            }
        }
    ' "$tmp" > "$tmp2"
    mv "$tmp2" "$META_FILE"
    rm -f "$tmp"
}

meta_has() {
    local component
    component="$1"
    [ -f "$META_FILE" ] && grep -q "\"${component}\"" "$META_FILE" 2>/dev/null
}

# ─── Detection (metadata-first, with unmanaged warning) ─────────────────────
detect_installed() {
    local comp method
    MANAGED_COMPONENTS=""
    if [ -f "$META_FILE" ]; then
        for comp in agent server; do
            if meta_has "$comp"; then
                method=$(meta_read "$comp" "method")
                MANAGED_COMPONENTS="${MANAGED_COMPONENTS:+$MANAGED_COMPONENTS }${comp}:${method}"
            fi
        done
    fi
}

detect_unmanaged() {
    UNMANAGED_COMPONENTS=""
    if ! meta_has "agent"; then
        if [ -f "${INSTALL_DIR}/serverbee-agent" ]; then
            UNMANAGED_COMPONENTS="${UNMANAGED_COMPONENTS:+$UNMANAGED_COMPONENTS }agent:binary"
        fi
        if command -v docker >/dev/null 2>&1 && docker ps -a --format '{{.Names}}' 2>/dev/null | grep -q "^serverbee-agent$"; then
            UNMANAGED_COMPONENTS="${UNMANAGED_COMPONENTS:+$UNMANAGED_COMPONENTS }agent:docker"
        fi
    fi
    if ! meta_has "server"; then
        if [ -f "${INSTALL_DIR}/serverbee-server" ]; then
            UNMANAGED_COMPONENTS="${UNMANAGED_COMPONENTS:+$UNMANAGED_COMPONENTS }server:binary"
        fi
        if command -v docker >/dev/null 2>&1 && docker ps -a --format '{{.Names}}' 2>/dev/null | grep -q "^serverbee-server$"; then
            UNMANAGED_COMPONENTS="${UNMANAGED_COMPONENTS:+$UNMANAGED_COMPONENTS }server:docker"
        fi
    fi
}

check_docker() {
    command -v docker >/dev/null 2>&1 || error "Docker is not installed. Install it first: https://docs.docker.com/get-docker/"
    docker compose version >/dev/null 2>&1 || error "Docker Compose V2 is not available. Install it first: https://docs.docker.com/compose/install/"
    configure_docker_dir
}

check_unmanaged_container() {
    local component
    component="$1"
    if ! meta_has "$component" && command -v docker >/dev/null 2>&1; then
        if docker ps -a --format '{{.Names}}' 2>/dev/null | grep -q "^serverbee-${component}$"; then
            error "Found existing container 'serverbee-${component}' not managed by this script.\n  Please remove it first:  docker stop serverbee-${component} && docker rm serverbee-${component}\n  Then re-run:  serverbee install ${component} --method docker ..."
        fi
    fi
}

# ─── CLI self-install ────────────────────────────────────────────────────────
install_cli() {
    local target version
    target="$CLI_PATH"
    version="${1:-main}"

    if (
        local target_dir tmp url
        target_dir=$(dirname "$target")
        tmp=$(mktemp "${target_dir}/.serverbee-cli.XXXXXX")
        trap 'rm -f "$tmp"' EXIT

        if [ -n "$SELF_SCRIPT" ] && [ -r "$SELF_SCRIPT" ]; then
            cp "$SELF_SCRIPT" "$tmp"
        else
            url="https://raw.githubusercontent.com/${REPO}/${version}/deploy/install.sh"
            curl -fsSL -o "$tmp" "$url"
        fi

        chmod +x "$tmp"
        mv "$tmp" "$target"
        trap - EXIT
    ); then
        info "Management CLI installed: serverbee"
    else
        warn "Failed to install CLI to ${target} — component installation continues"
    fi
}

# Refresh the installed management CLI from the release script itself.
refresh_cli_from_release() {
    local version target
    version="${1:-main}"
    [ -z "$CLI_REFRESHED" ] || return 0

    target="$CLI_PATH"
    if (
        local target_dir tmp url
        target_dir=$(dirname "$target")
        tmp=$(mktemp "${target_dir}/.serverbee-cli.XXXXXX")
        trap 'rm -f "$tmp"' EXIT

        url="https://raw.githubusercontent.com/${REPO}/${version}/deploy/install.sh"
        curl -fsSL -o "$tmp" "$url" || exit 1

        # Sanity-check the download before trusting it as our own CLI.
        [ -s "$tmp" ] || exit 1
        sh -n "$tmp" 2>/dev/null || exit 1
        grep -q 'REPO="ZingerLittleBee/ServerBee"' "$tmp" || exit 1

        chmod +x "$tmp"
        mv "$tmp" "$target"
        trap - EXIT
    ); then
        CLI_REFRESHED=1
        info "Management CLI refreshed to ${version} (applies on next 'serverbee' run)"
    else
        warn "Could not refresh management CLI from ${version} — keeping existing CLI"
    fi
}

# ─── Agent capability helpers ────────────────────────────────────────────────
cap_is_valid() {
    local cap k
    cap="$1"
    for k in $AGENT_CAPS_ALL; do
        [ "$k" = "$cap" ] && return 0
    done
    return 1
}

# Normalize a comma-separated cap list. Accepts `default`, `all`, `none`.
set_caps_from_cli() {
    local raw cap final list
    raw="$1"
    raw="$(echo "$raw" | tr -d '[:space:]')"
    AGENT_CAPS_USER_SPECIFIED=true

    case "$raw" in
        default|DEFAULT)
            final=""
            for cap in $AGENT_CAPS_ALL; do
                cap_default_on "$cap" || continue
                final="${final:+$final,}$cap"
            done
            AGENT_CAPS_SELECTED="$final"
            return ;;
        all|ALL)
            AGENT_CAPS_SELECTED="$(printf '%s' "$AGENT_CAPS_ALL" | tr ' ' ',')"
            return ;;
        none|NONE|"")
            AGENT_CAPS_SELECTED=""
            return ;;
    esac

    final=""
    list=$(printf '%s' "$raw" | tr ',' ' ')
    for cap in $list; do
        if ! cap_is_valid "$cap"; then
            error "$(trp caps_unknown_cli "$cap" "$AGENT_CAPS_ALL")"
        fi
        case ",${final}," in
            *",${cap},"*) ;;
            *) final="${final:+$final,}$cap" ;;
        esac
    done
    AGENT_CAPS_SELECTED="$final"
}

ensure_caps_initialized() {
    local cap final
    if [ "$AGENT_CAPS_USER_SPECIFIED" = false ] && [ -z "$AGENT_CAPS_SELECTED" ]; then
        final=""
        for cap in $AGENT_CAPS_ALL; do
            cap_default_on "$cap" || continue
            final="${final:+$final,}$cap"
        done
        AGENT_CAPS_SELECTED="$final"
    fi
}

# Whether the selection equals CAP_DEFAULT exactly.
caps_match_default() {
    local cap in_sel in_def selected_set
    selected_set=",${AGENT_CAPS_SELECTED},"
    for cap in $AGENT_CAPS_ALL; do
        in_sel=0; in_def=0
        case "$selected_set" in *,"$cap",*) in_sel=1 ;; esac
        cap_default_on "$cap" && in_def=1
        [ "$in_sel" -ne "$in_def" ] && return 1
    done
    return 0
}

# Emit a space-joined --allow-cap/--deny-cap argument string.
compute_cap_cli_args() {
    local cap in_sel in_def out selected_set
    out=""
    selected_set=",${AGENT_CAPS_SELECTED},"
    for cap in $AGENT_CAPS_ALL; do
        in_sel=0; in_def=0
        case "$selected_set" in *,"$cap",*) in_sel=1 ;; esac
        cap_default_on "$cap" && in_def=1
        if [ "$in_sel" = 1 ] && [ "$in_def" = 0 ]; then
            out="${out:+$out }--allow-cap $cap"
        elif [ "$in_sel" = 0 ] && [ "$in_def" = 1 ]; then
            out="${out:+$out }--deny-cap $cap"
        fi
    done
    printf '%s' "$out"
}

# Emit YAML list items for docker-compose `command:`. Empty when defaults.
compute_cap_compose_command() {
    local args token
    args=$(compute_cap_cli_args)
    [ -z "$args" ] && return 0
    printf '    command:\n'
    printf '      - serverbee-agent\n'
    for token in $args; do
        printf '      - %s\n' "$token"
    done
}

render_caps_for_plan() {
    if [ -z "$AGENT_CAPS_SELECTED" ]; then
        tr_text caps_plan_none
        return
    fi
    if caps_match_default; then
        echo "${AGENT_CAPS_SELECTED} ($(tr_text caps_plan_default))"
    else
        echo "${AGENT_CAPS_SELECTED}"
    fi
}

caps_is_checked() {
    case " $CHECKED " in *" $1 "*) return 0 ;; *) return 1 ;; esac
}

cap_by_index() {
    local i c
    i=1
    for c in $AGENT_CAPS_ALL; do
        [ "$i" = "$1" ] && { printf '%s' "$c"; return 0; }
        i=$((i + 1))
    done
    return 1
}

# Interactive multi-select. Mutates AGENT_CAPS_SELECTED.
prompt_agent_capabilities() {
    local cap i mark input tok bad final
    [ "$YES" = true ] && return 0
    [ "$AGENT_CAPS_USER_SPECIFIED" = true ] && return 0
    has_prompt_input || return 0

    ensure_caps_initialized
    CHECKED=$(printf '%s' "$AGENT_CAPS_SELECTED" | tr ',' ' ')

    while true; do
        echo ""
        cecho "${BOLD}$(tr_text caps_title)${NC}"
        tr_text caps_intro
        tr_text caps_legend
        echo ""
        i=1
        for cap in $AGENT_CAPS_ALL; do
            if caps_is_checked "$cap"; then mark="x"; else mark=" "; fi
            printf "  [%s] %2d. %-17s (%-6s) — %s\n" \
                "$mark" "$i" "$cap" "$(cap_risk "$cap")" "$(cap_desc "$cap")"
            i=$((i + 1))
        done
        echo ""
        tr_text caps_hint
        printf '%s' "$(tr_text caps_prompt)"; prompt_read input
        input="$(echo "$input" | xargs || true)"

        case "$input" in
            ""|d|D|done|DONE) break ;;
            a|A|all|ALL) CHECKED="$AGENT_CAPS_ALL"; continue ;;
            n|N|none|NONE) CHECKED=""; continue ;;
        esac

        bad=""
        for tok in $input; do
            cap=""
            case "$tok" in
                ''|*[!0-9]*) ;;
                *)
                    if [ "$tok" -ge 1 ] && [ "$tok" -le "$AGENT_CAPS_COUNT" ]; then
                        cap=$(cap_by_index "$tok")
                    fi ;;
            esac
            if [ -z "$cap" ] && cap_is_valid "$tok"; then
                cap="$tok"
            fi
            if [ -n "$cap" ]; then
                if caps_is_checked "$cap"; then
                    CHECKED=$(printf '%s' " $CHECKED " | sed "s/ $cap / /g")
                else
                    CHECKED="${CHECKED:+$CHECKED }$cap"
                fi
            else
                bad="${bad:+$bad }$tok"
            fi
        done
        [ -n "$bad" ] && warn "$(trp caps_invalid "$bad")"
    done

    final=""
    for cap in $AGENT_CAPS_ALL; do
        caps_is_checked "$cap" && final="${final:+$final,}$cap"
    done
    AGENT_CAPS_SELECTED="$final"
    AGENT_CAPS_USER_SPECIFIED=true
}

# ─── Download with checksum ──────────────────────────────────────────────────
# Downloads to <dest>, verifies against the release's sha256sums.txt when
# available (older releases without it warn-and-continue), leaving <dest> ready
# for an atomic install. <dest> is removed on checksum failure.
download_verified() {
    local url dest filename version sums want got
    url="$1"; dest="$2"; filename="$3"; version="$4"
    curl -fsSL -o "$dest" "$url" || {
        rm -f "$dest"
        error "Download failed: $url"
    }

    sums=$(curl -fsSL "https://github.com/${REPO}/releases/download/${version}/sha256sums.txt" 2>/dev/null || true)
    if [ -z "$sums" ]; then
        warn "No sha256sums.txt for ${version}; skipping checksum verification (older release)."
        return 0
    fi
    # Match on the exact filename field (coreutils `sha256sum` emits "<hash>  <name>"),
    # not a substring/regex, so a line like "<hash>  evil-${filename}" can't match.
    want=$(printf '%s\n' "$sums" | awk -v f="$filename" '$2 == f { print $1; exit }')
    if [ -z "$want" ]; then
        warn "sha256sums.txt has no entry for ${filename}; skipping checksum verification."
        return 0
    fi
    got=$(sha256_of "$dest") || { warn "No sha256 tool available; skipping checksum verification."; return 0; }
    if [ "$got" != "$want" ]; then
        rm -f "$dest"
        error "Checksum mismatch for ${filename}\n  expected ${want}\n  got      ${got}"
    fi
    info "Checksum OK: ${filename}"
}

# ─── Service (init) abstraction ──────────────────────────────────────────────
svc_unit_path()      { echo "/etc/systemd/system/serverbee-$1.service"; }
svc_openrc_path()    { echo "/etc/init.d/serverbee-$1"; }
svc_log_path()       { echo "/var/log/serverbee-$1.log"; }
svc_env_path()       { echo "${CONFIG_DIR}/serverbee-$1.env"; }
svc_logrotate_path() { echo "/etc/logrotate.d/serverbee-$1"; }

svc_action() {
    # $1 = start|stop|restart   $2 = component
    local svc
    svc="serverbee-$2"
    case "$INIT" in
        systemd) systemctl "$1" "$svc" ;;
        openrc)  rc-service "$svc" "$1" ;;
        none)    [ "$1" = stop ] && return 0; error "No init manager available to $1 ${svc}." ;;
    esac
}

svc_is_active() {
    case "$INIT" in
        systemd) systemctl is-active --quiet "serverbee-$1" 2>/dev/null && echo active || echo inactive ;;
        openrc)  rc-service "serverbee-$1" status >/dev/null 2>&1 && echo active || echo inactive ;;
        *)       echo unknown ;;
    esac
}

server_health_url() {
    local listen host port
    listen="${SERVERBEE_SERVER__LISTEN:-}"
    if [ -z "$listen" ] && [ -f "${CONFIG_DIR}/server.toml" ]; then
        listen=$(awk '
            /^[[:space:]]*\[server\][[:space:]]*$/ { in_server=1; next }
            in_server && /^[[:space:]]*\[/ { exit }
            in_server && /^[[:space:]]*listen[[:space:]]*=/ {
                sub(/^[^=]*=[[:space:]]*/, "")
                gsub(/^[[:space:]]*"|"[[:space:]]*$/, "")
                print
                exit
            }
        ' "${CONFIG_DIR}/server.toml")
    fi
    : "${listen:=0.0.0.0:9527}"

    case "$listen" in
        0.0.0.0:*) host="127.0.0.1"; port="${listen##*:}" ;;
        \[::\]:*) host="[::1]"; port="${listen##*:}" ;;
        \[*\]:*) host="${listen%:*}"; port="${listen##*:}" ;;
        *:*) host="${listen%:*}"; port="${listen##*:}" ;;
        *) return 1 ;;
    esac
    printf 'http://%s:%s/healthz\n' "$host" "$port"
}

configured_server_listen() {
    local config_file
    config_file="${CONFIG_DIR}/server.toml"
    [ -f "$config_file" ] || return 1
    awk '
        /^[[:space:]]*\[server\][[:space:]]*$/ { in_server=1; next }
        in_server && /^[[:space:]]*\[/ { exit }
        in_server && /^[[:space:]]*listen[[:space:]]*=/ {
            sub(/^[^=]*=[[:space:]]*/, "")
            gsub(/^[[:space:]]*"|"[[:space:]]*$/, "")
            print
            exit
        }
    ' "$config_file"
}

server_dashboard_display() {
    local method domain listen ip compose_file
    method="$1"
    domain=""
    if [ -f "$DOMAIN_CACHE_FILE" ]; then
        domain=$(head -n1 "$DOMAIN_CACHE_FILE" 2>/dev/null | tr -d '[:space:]')
    fi
    if [ -n "$domain" ]; then
        printf 'https://%s\n' "$domain"
        return 0
    fi

    if [ "$method" = binary ]; then
        listen=$(configured_server_listen || true)
        case "$listen" in
            127.0.0.1:*|localhost:*|\[::1\]:*)
                tr_text st_reverse_proxy
                return 0
                ;;
        esac
    elif [ "$method" = docker ]; then
        compose_file="${DOCKER_DIR}/docker-compose.server.yml"
        if [ -f "$compose_file" ] \
            && grep -Eq '127\.0\.0\.1:9527:9527|\[::1\]:9527:9527' "$compose_file"; then
            tr_text st_reverse_proxy
            return 0
        fi
    fi

    ip=$(get_local_ip)
    printf 'http://%s:9527\n' "$ip"
}

write_domain_cache() {
    local generated
    mkdir -p "$CONFIG_DIR"
    generated=$(mktemp "${DOMAIN_CACHE_FILE}.tmp.XXXXXX") || return 1
    if ! printf '%s\n' "$DOMAIN" > "$generated" \
        || ! chmod 600 "$generated" \
        || ! mv -f "$generated" "$DOMAIN_CACHE_FILE"; then
        rm -f "$generated"
        return 1
    fi
}

svc_health_check() {
    local component url
    component="$1"
    [ "$component" = server ] || return 0
    url=$(server_health_url) || return 1
    curl -fsS --max-time 2 "$url" >/dev/null 2>&1
}

server_install_health_check() {
    if [ "$METHOD" = docker ]; then
        [ "$(docker_container_state server)" = "running healthy" ]
    else
        [ "$(svc_is_active server)" = active ] && svc_health_check server
    fi
}

wait_for_server_install() {
    local attempt
    if [ "$METHOD" = docker ]; then
        wait_for_docker_stability server
        return
    fi
    attempt=0
    while [ "$attempt" -lt "$INSTALL_SERVER_HEALTH_ATTEMPTS" ]; do
        server_install_health_check && return 0
        attempt=$((attempt + 1))
        [ "$attempt" -ge "$INSTALL_SERVER_HEALTH_ATTEMPTS" ] || sleep 1
    done
    return 1
}

agent_install_logs() {
    local invocation_id
    if [ "$METHOD" = docker ]; then
        docker compose -f "${DOCKER_DIR}/docker-compose.agent.yml" \
            logs --no-color --since "${AGENT_LOG_SINCE}" --tail 200 serverbee-agent 2>/dev/null || true
    elif [ "$INIT" = systemd ]; then
        invocation_id=$(systemctl show serverbee-agent --property=InvocationID --value 2>/dev/null || true)
        if [ -n "$invocation_id" ]; then
            journalctl "_SYSTEMD_INVOCATION_ID=${invocation_id}" --no-pager 2>/dev/null || true
        else
            journalctl -u serverbee-agent --since "@${AGENT_LOG_SINCE}" --no-pager 2>/dev/null || true
        fi
    elif [ "$INIT" = openrc ]; then
        tail -n "+$((AGENT_LOG_START_LINE + 1))" "$(svc_log_path agent)" 2>/dev/null || true
    else
        svc_logs_tail agent 200 2>/dev/null || true
    fi
}

agent_install_exit_status() {
    if [ "$METHOD" = docker ]; then
        docker inspect --format '{{.State.ExitCode}}' serverbee-agent 2>/dev/null || true
    elif [ "$INIT" = systemd ]; then
        systemctl show serverbee-agent --property=ExecMainStatus --value 2>/dev/null || true
    fi
}

agent_install_pid() {
    local process_dir executable
    if [ "$METHOD" = docker ]; then
        docker inspect --format '{{.State.Pid}}' serverbee-agent 2>/dev/null || true
    elif [ "$INIT" = systemd ]; then
        systemctl show serverbee-agent --property=MainPID --value 2>/dev/null || true
    elif [ "$INIT" = openrc ]; then
        # supervise-daemon's pidfile identifies the supervisor, not its Agent
        # child. Resolve the exact executable from procfs instead.
        for process_dir in /proc/[0-9]*; do
            [ -r "${process_dir}/cmdline" ] || continue
            executable=$(tr '\000' '\n' < "${process_dir}/cmdline" 2>/dev/null | head -n1)
            if [ "$executable" = "${INSTALL_DIR}/serverbee-agent" ]; then
                printf '%s\n' "${process_dir##*/}"
                return 0
            fi
        done
    fi
}

agent_server_endpoint() {
    local url scheme authority host port
    url="$SERVER_URL"
    scheme="${url%%://*}"
    [ "$scheme" != "$url" ] || return 1
    authority="${url#*://}"
    authority="${authority%%/*}"
    authority="${authority%%\?*}"
    case "$authority" in
        \[*\]:*)
            host="${authority%%]*}"
            host="${host#[}"
            port="${authority##*:}"
            ;;
        \[*\])
            host="${authority#[}"
            host="${host%]}"
            port=""
            ;;
        *:*)
            host="${authority%:*}"
            port="${authority##*:}"
            ;;
        *)
            host="$authority"
            port=""
            ;;
    esac
    if [ -z "$port" ]; then
        case "$scheme" in
            https|wss) port=443 ;;
            http|ws) port=80 ;;
            *) return 1 ;;
        esac
    fi
    case "$port" in
        ''|*[!0-9]*) return 1 ;;
    esac
    [ -n "$host" ] || return 1
    printf '%s %s\n' "$host" "$port"
}

agent_connection_established() {
    # Compatibility proof for releases or logging configurations that suppress
    # the INFO-level Welcome line. Match the Agent process itself to the
    # resolved Server endpoint, not merely any connection on the host.
    local pid endpoint host port addresses
    command -v ss >/dev/null 2>&1 || return 1
    pid=$(agent_install_pid)
    case "$pid" in
        ''|0|*[!0-9]*) return 1 ;;
    esac
    endpoint=$(agent_server_endpoint) || return 1
    host="${endpoint% *}"
    port="${endpoint##* }"
    if printf '%s' "$host" | grep -Eq '^[0-9]+(\.[0-9]+){3}$|:'; then
        addresses="$host"
    elif command -v getent >/dev/null 2>&1; then
        addresses=$(getent ahosts "$host" 2>/dev/null | awk '{ print $1 }' | sort -u)
    else
        return 1
    fi
    [ -n "$addresses" ] || return 1

    ss -Htnp state established 2>/dev/null | awk -v pid="$pid" -v port="$port" -v addresses="$addresses" '
        BEGIN {
            count = split(addresses, address, "\n")
            wanted = "pid=" pid ","
        }
        index($0, wanted) {
            peer = $4
            if (substr(peer, 1, 1) == "[") {
                sub(/^\[/, "", peer)
                sub(/\]:[0-9]+$/, "", peer)
            } else {
                sub(":[0-9]+$", "", peer)
            }
            if ($4 !~ (":" port "$") ) next
            for (i = 1; i <= count; i++) {
                if (peer == address[i]) {
                    print $3 "|" $4
                    found = 1
                    exit
                }
            }
        }
        END { exit(found ? 0 : 1) }
    '
}

wait_for_agent_install() {
    local attempt logs exit_status connection_stable connection_id current_connection
    attempt=0
    connection_stable=0
    connection_id=""
    AGENT_INSTALL_PROOF=""
    while [ "$attempt" -lt "$INSTALL_AGENT_ATTEMPTS" ]; do
        logs=$(agent_install_logs)
        if printf '%s\n' "$logs" | grep -Fq 'Welcome from server '; then
            AGENT_INSTALL_PROOF=welcome
            return 0
        fi
        if printf '%s\n' "$logs" | grep -Fq 'Permanent registration failure:'; then
            return 78
        fi
        exit_status=$(agent_install_exit_status)
        [ "$exit_status" != 78 ] || return 78
        if current_connection=$(agent_connection_established); then
            if [ "$current_connection" = "$connection_id" ]; then
                connection_stable=$((connection_stable + 1))
            else
                connection_id="$current_connection"
                connection_stable=1
            fi
            # Three observations span roughly two seconds and must identify the
            # same TCP four-tuple, so separate short retries cannot look stable.
            if [ "$connection_stable" -ge 3 ]; then
                AGENT_INSTALL_PROOF=connection
                return 0
            fi
        else
            connection_stable=0
            connection_id=""
        fi
        attempt=$((attempt + 1))
        [ "$attempt" -ge "$INSTALL_AGENT_ATTEMPTS" ] || sleep 1
    done
    return 75
}

verify_server_install_or_exit() {
    local logs
    if [ "$METHOD" != docker ] && [ "$INIT" = none ]; then
        NO_WAIT=true
        warn "Server service was not started because no init manager was found; health verification was skipped."
        return 0
    fi
    if [ "$NO_WAIT" = true ]; then
        warn "Server was installed, but health verification was skipped (--no-wait)."
        return 0
    fi
    if wait_for_server_install; then
        info "Server health check passed."
        return 0
    fi
    if [ "$METHOD" = docker ]; then
        logs=$(docker compose -f "${DOCKER_DIR}/docker-compose.server.yml" logs --tail 20 2>/dev/null || true)
    else
        logs=$(svc_logs_tail server 20 2>/dev/null || true)
    fi
    [ -z "$logs" ] || warn "Recent serverbee-server logs:\n${logs}"
    error "Server was installed, but /healthz did not become ready. The installation was kept for troubleshooting."
}

verify_agent_install_or_exit() {
    local result logs
    if [ "$METHOD" != docker ] && [ "$INIT" = none ]; then
        NO_WAIT=true
        warn "Agent service was not started because no init manager was found; Server connection verification was skipped."
        return 0
    fi
    if [ "$NO_WAIT" = true ]; then
        warn "Agent was installed, but Server enrollment was not verified (--no-wait). It will keep retrying in the background."
        return 0
    fi
    if wait_for_agent_install; then
        if [ "$AGENT_INSTALL_PROOF" = welcome ]; then
            info "Agent connected and received the Server welcome message."
        else
            info "Agent established a persistent connection to the Server endpoint."
        fi
        return 0
    else
        result=$?
    fi
    logs=$(agent_install_logs)
    [ -z "$logs" ] || warn "Recent serverbee-agent logs:\n${logs}"
    if [ "$result" -eq 78 ]; then
        if [ "$METHOD" = docker ]; then
            docker compose -f "${DOCKER_DIR}/docker-compose.agent.yml" stop serverbee-agent >/dev/null 2>&1 || true
        fi
        printf '%b\n' "${RED}[ERROR]${NC} Agent authentication was permanently rejected. The installation was kept. Generate a fresh code in Servers → Add Server, update enrollment_code, then restart the Agent." >&2
        exit 78
    fi
    warn "Agent was installed but did not connect within ${INSTALL_AGENT_ATTEMPTS}s. It will keep retrying in the background. Check 'serverbee status' and the Agent logs."
    exit 75
}

svc_restart_count() {
    case "$INIT" in
        systemd) systemctl show "serverbee-$1" --property=NRestarts --value 2>/dev/null || true ;;
        *)       : ;;
    esac
}

svc_reset_failed() {
    case "$INIT" in
        systemd) systemctl reset-failed "serverbee-$1" >/dev/null 2>&1 || true ;;
        *) : ;;
    esac
}

svc_logs_tail() {
    # $1 = component   $2 = number of lines
    case "$INIT" in
        systemd) journalctl -u "serverbee-$1" -n "$2" --no-pager 2>/dev/null ;;
        openrc)  tail -n "$2" "$(svc_log_path "$1")" 2>/dev/null ;;
        *)       : ;;
    esac
}

svc_write_logrotate() {
    cat > "$(svc_logrotate_path "$1")" <<ROT
$(svc_log_path "$1") {
    weekly
    rotate 4
    missingok
    notifempty
    copytruncate
}
ROT
}

svc_write_env_file() {
    # $1 = component, $2 = key (optional), $3 = value (optional)
    local f
    f=$(svc_env_path "$1")
    mkdir -p "$CONFIG_DIR"
    if [ ! -f "$f" ]; then
        (umask 077; : > "$f") || return 1
    fi
    [ -n "${2:-}" ] || return 0
    openrc_set_env "$f" "$2" "${3:-}"
}

create_systemd_unit_server() {
    cat > "$(svc_unit_path server)" << UNIT
[Unit]
Description=ServerBee Dashboard
After=network.target

[Service]
Type=simple
ExecStart=${INSTALL_DIR}/serverbee-server
WorkingDirectory=${CONFIG_DIR}
Environment=SERVERBEE_SERVER__DATA_DIR=${DATA_DIR}
Environment=MALLOC_ARENA_MAX=2
Restart=always
RestartSec=5
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
UNIT
}

create_systemd_unit_agent() {
    # $1 = ExecStart (binary path + optional cap flags)
    cat > "$(svc_unit_path agent)" << UNIT
[Unit]
Description=ServerBee Agent
After=network.target
StartLimitIntervalSec=300
StartLimitBurst=5

[Service]
Type=simple
ExecStart=$1
WorkingDirectory=${CONFIG_DIR}
Environment=SERVERBEE_SUPERVISED=1
Restart=always
RestartSec=5
# Exit code 78 = permanent enrollment-code failure; don't restart, the operator
# must rotate the code (otherwise we burn the registration rate limit).
RestartPreventExitStatus=78
AmbientCapabilities=CAP_NET_RAW

[Install]
WantedBy=multi-user.target
UNIT
}

create_openrc_service_server() {
    cat > "$(svc_openrc_path server)" <<'OPENRC'
#!/sbin/openrc-run
name="serverbee-server"
description="ServerBee Dashboard"
OPENRC
    cat >> "$(svc_openrc_path server)" <<OPENRC
command="${INSTALL_DIR}/serverbee-server"
command_args=""
directory="${CONFIG_DIR}"
supervisor=supervise-daemon
respawn_delay=5
pidfile="/run/serverbee-server.pid"
output_log="$(svc_log_path server)"
error_log="$(svc_log_path server)"

depend() {
    after net
    need net
}

start_pre() {
    if [ -f "$(svc_env_path server)" ]; then
        set -a
        . "$(svc_env_path server)"
        set +a
    fi
}
OPENRC
    chmod 0755 "$(svc_openrc_path server)"
    svc_write_logrotate server
}

create_openrc_service_agent() {
    # $1 = command_args string (may be empty)
    cat > "$(svc_openrc_path agent)" <<'OPENRC'
#!/sbin/openrc-run
name="serverbee-agent"
description="ServerBee Agent"
export SERVERBEE_SUPERVISED=1
OPENRC
    cat >> "$(svc_openrc_path agent)" <<OPENRC
command="${INSTALL_DIR}/serverbee-agent"
command_args="$1"
directory="${CONFIG_DIR}"
supervisor=supervise-daemon
respawn_delay=5
# OpenRC has no RestartPreventExitStatus=78 equivalent, so a permanent
# enrollment failure would otherwise respawn-loop forever. We bound the
# respawn *burst*: more than 5 respawns within 300s and supervise-daemon
# gives up. NOTE this is weaker than systemd's RestartPreventExitStatus=78
# (which never auto-restarts on 78 again): the counter resets after a
# successful run, and a reboot or manual rc-service restart starts fresh,
# so a stale enrollment code can still burn up to 5 registration attempts
# per restart. Acceptable — it kills the infinite loop, which is the goal.
respawn_max=5
respawn_period=300
pidfile="/run/serverbee-agent.pid"
output_log="$(svc_log_path agent)"
error_log="$(svc_log_path agent)"

depend() {
    after net
    need net
}

start_pre() {
    if [ -f "$(svc_env_path agent)" ]; then
        set -a
        . "$(svc_env_path agent)"
        set +a
    fi
}
OPENRC
    chmod 0755 "$(svc_openrc_path agent)"
    svc_write_logrotate agent
}

svc_install_server() {
    case "$INIT" in
        systemd)
            create_systemd_unit_server
            systemctl daemon-reload
            systemctl enable serverbee-server >/dev/null 2>&1 || true
            systemctl restart serverbee-server
            info "Server service started and enabled"
            ;;
        openrc)
            svc_write_env_file server SERVERBEE_SERVER__DATA_DIR "$DATA_DIR"
            create_openrc_service_server
            rc-update add serverbee-server default >/dev/null 2>&1 || true
            rc-service serverbee-server restart
            info "Server service started and enabled"
            ;;
        none)
            warn "No init manager (systemd/openrc) found. Start manually: ${INSTALL_DIR}/serverbee-server"
            ;;
    esac
}

svc_install_agent() {
    # $1 = cap flag args (may be empty)
    case "$INIT" in
        systemd)
            create_systemd_unit_agent "${INSTALL_DIR}/serverbee-agent${1:+ $1}"
            systemctl daemon-reload
            systemctl enable serverbee-agent >/dev/null 2>&1 || true
            systemctl restart serverbee-agent
            info "Agent service started and enabled"
            ;;
        openrc)
            svc_write_env_file agent
            create_openrc_service_agent "$1"
            rc-update add serverbee-agent default >/dev/null 2>&1 || true
            rc-service serverbee-agent restart
            info "Agent service started and enabled"
            ;;
        none)
            warn "No init manager (systemd/openrc) found. Start manually: ${INSTALL_DIR}/serverbee-agent ${1}"
            ;;
    esac
}

svc_remove() {
    # $1 = component. Detect the supervisor on the fly so uninstall works even
    # if INIT was not set (e.g. partial environment).
    local svc
    svc="serverbee-$1"
    if command -v systemctl >/dev/null 2>&1; then
        systemctl stop "$svc" 2>/dev/null || true
        systemctl disable "$svc" 2>/dev/null || true
        rm -f "$(svc_unit_path "$1")"
        rm -rf "/etc/systemd/system/${svc}.service.d"
        systemctl daemon-reload 2>/dev/null || true
    fi
    if command -v rc-service >/dev/null 2>&1; then
        rc-service "$svc" stop 2>/dev/null || true
        rc-update del "$svc" default 2>/dev/null || true
        rm -f "$(svc_openrc_path "$1")"
    fi
    rm -f "$(svc_log_path "$1")" "$(svc_logrotate_path "$1")" "$(svc_env_path "$1")"
}

# ─── Install helpers ─────────────────────────────────────────────────────────
install_binary_server() {
    local version os arch filename url
    os=$(detect_os)
    arch=$(detect_arch)
    version=$(get_latest_version)

    mkdir -p "$INSTALL_DIR"

    if [ -f "${INSTALL_DIR}/serverbee-server" ]; then
        warn "Binary already exists at ${INSTALL_DIR}/serverbee-server — skipping download (adopting existing)"
    else
        filename="serverbee-server-${os}-${arch}"
        url="https://github.com/${REPO}/releases/download/${version}/${filename}"
        info "Downloading serverbee-server ${version} for ${os}/${arch}..."
        download_verified "$url" "/tmp/serverbee-server" "$filename" "$version"
        chmod +x "/tmp/serverbee-server"
        mv "/tmp/serverbee-server" "${INSTALL_DIR}/serverbee-server"
        info "Installed to ${INSTALL_DIR}/serverbee-server"
    fi

    mkdir -p "$DATA_DIR" "$CONFIG_DIR"

    if [ ! -f "${CONFIG_DIR}/server.toml" ]; then
        cat > "${CONFIG_DIR}/server.toml" << TOML
[server]
data_dir = "${DATA_DIR}"

[auth]
secure_cookie = false
TOML
        info "Created ${CONFIG_DIR}/server.toml"
    else
        warn "${CONFIG_DIR}/server.toml already exists, not overwriting"
    fi

    svc_install_server

    install_cli "$version"
    meta_write "server" "binary" "$version"
    verify_server_install_or_exit
    print_server_result
}

install_binary_agent() {
    local version os arch filename url cap_args
    os=$(detect_os)
    arch=$(detect_arch)
    version=$(get_latest_version)

    mkdir -p "$INSTALL_DIR"

    if [ -f "${INSTALL_DIR}/serverbee-agent" ]; then
        warn "Binary already exists at ${INSTALL_DIR}/serverbee-agent — skipping download (adopting existing)"
    else
        filename="serverbee-agent-${os}-${arch}"
        url="https://github.com/${REPO}/releases/download/${version}/${filename}"
        info "Downloading serverbee-agent ${version} for ${os}/${arch}..."
        download_verified "$url" "/tmp/serverbee-agent" "$filename" "$version"
        chmod +x "/tmp/serverbee-agent"
        mv "/tmp/serverbee-agent" "${INSTALL_DIR}/serverbee-agent"
        info "Installed to ${INSTALL_DIR}/serverbee-agent"
    fi

    mkdir -p "$CONFIG_DIR"

    # Generate agent.toml, or refresh enrollment fields if it already exists so
    # the recover flow (paste a fresh --enrollment-code) re-registers cleanly.
    if [ ! -f "${CONFIG_DIR}/agent.toml" ]; then
        (umask 077; cat > "${CONFIG_DIR}/agent.toml") << TOML
server_url = "${SERVER_URL}"
enrollment_code = "${ENROLLMENT_CODE}"

[collector]
interval = 3
enable_temperature = true
TOML
        info "Created ${CONFIG_DIR}/agent.toml"
    else
        info "${CONFIG_DIR}/agent.toml exists — refreshing server_url, enrollment_code, clearing token"
        toml_set "${CONFIG_DIR}/agent.toml" "server_url" "${SERVER_URL}"
        toml_set "${CONFIG_DIR}/agent.toml" "enrollment_code" "${ENROLLMENT_CODE}"
        toml_set "${CONFIG_DIR}/agent.toml" "token" ""
    fi
    chmod 600 "${CONFIG_DIR}/agent.toml" \
        || error "Could not secure ${CONFIG_DIR}/agent.toml"

    ensure_caps_initialized
    cap_args=$(compute_cap_cli_args)
    AGENT_LOG_SINCE=$(date +%s)
    if [ "$INIT" = openrc ] && [ -f "$(svc_log_path agent)" ]; then
        AGENT_LOG_START_LINE=$(wc -l < "$(svc_log_path agent)")
    fi
    svc_install_agent "$cap_args"

    install_cli "$version"
    meta_write "agent" "binary" "$version"
    verify_agent_install_or_exit
    print_agent_result
}

install_docker_server() {
    local version image_tag conf_dir
    check_docker
    check_unmanaged_container "server"

    version=$(get_latest_version)
    image_tag=$(docker_image_tag "$version")

    conf_dir="$(docker_conf_dir)"
    mkdir -p "$DOCKER_DIR" "$conf_dir"

    if [ ! -f "${conf_dir}/server.toml" ]; then
        cat > "${conf_dir}/server.toml" << TOML
[server]
data_dir = "/data"
TOML
        info "Created ${conf_dir}/server.toml"
    else
        warn "${conf_dir}/server.toml already exists, not overwriting"
    fi

    cat > "${DOCKER_DIR}/docker-compose.server.yml" << YAML
services:
  serverbee-server:
    image: ghcr.io/zingerlittlebee/serverbee-server:${image_tag}
    container_name: serverbee-server
    ports:
      - "9527:9527"
    volumes:
      - serverbee-data:/data
      - ${conf_dir}/server.toml:/etc/serverbee/server.toml:ro
    environment:
      - SERVERBEE_ADMIN__USERNAME=admin
      - SERVERBEE_AUTH__SECURE_COOKIE=false
      - MALLOC_ARENA_MAX=2
    restart: unless-stopped
    # Alpine BusyBox wget prefers ::1 for localhost, while the server listens on IPv4.
    healthcheck:
      test: ["CMD", "wget", "--spider", "-q", "http://127.0.0.1:9527/healthz"]
      interval: 30s
      timeout: 5s
      retries: 3
      start_period: 10s

volumes:
  serverbee-data:
YAML

    info "Generated ${DOCKER_DIR}/docker-compose.server.yml"
    docker compose -f "${DOCKER_DIR}/docker-compose.server.yml" config -q \
        || error "Generated Server Compose file is invalid: ${DOCKER_DIR}/docker-compose.server.yml"
    docker compose -f "${DOCKER_DIR}/docker-compose.server.yml" up -d
    info "Server container started"

    install_cli "$version"
    meta_write "server" "docker" "$version"
    verify_server_install_or_exit
    print_server_result
}

rollback_docker_agent_install() {
    local compose_file compose_backup config_file config_backup config_created compose_restored
    compose_file="$1"
    compose_backup="$2"
    config_file="$3"
    config_backup="$4"
    config_created="$5"
    compose_restored=false

    if [ -f "$compose_backup" ]; then
        if mv -f "$compose_backup" "$compose_file"; then
            compose_restored=true
        else
            warn "Could not restore the previous Agent Compose file: ${compose_backup}"
        fi
    else
        docker compose -f "$compose_file" down --remove-orphans >/dev/null 2>&1 || true
        rm -f "$compose_file"
    fi
    if [ -f "$config_backup" ]; then
        if ! mv -f "$config_backup" "$config_file"; then
            warn "Could not restore the previous Agent config: ${config_backup}"
        fi
    elif [ "$config_created" = true ]; then
        rm -f "$config_file"
    fi
    if [ "$compose_restored" = true ]; then
        if docker compose -f "$compose_file" up -d; then
            info "Previous Docker Agent installation restored"
        else
            warn "The previous Agent files were restored, but its container could not be started"
        fi
    fi
}

install_docker_agent() {
    local version image_tag conf_dir cap_command_block config_created
    local compose_file compose_backup config_file config_backup
    check_docker
    check_unmanaged_container "agent"

    version=$(get_latest_version)
    image_tag=$(docker_image_tag "$version")

    conf_dir="$(docker_conf_dir)"
    mkdir -p "$conf_dir" "$DOCKER_DIR"
    config_created=false
    config_file="${conf_dir}/agent.toml"
    config_backup="${config_file}.install-rollback"
    compose_file="${DOCKER_DIR}/docker-compose.agent.yml"
    compose_backup="${compose_file}.install-rollback"
    if [ -e "$config_backup" ] || [ -L "$config_backup" ]; then
        error "Stale install rollback found: ${config_backup}. Restore or remove it before installing."
    fi
    if [ -e "$compose_backup" ] || [ -L "$compose_backup" ]; then
        error "Stale install rollback found: ${compose_backup}. Restore or remove it before installing."
    fi
    if [ -f "$config_file" ]; then
        cp -p "$config_file" "$config_backup" \
            || error "Could not back up existing Agent config: ${config_file}"
        if ! chmod 600 "$config_backup"; then
            rm -f "$config_backup"
            error "Could not secure Agent config backup: ${config_backup}"
        fi
    fi
    if [ -f "$compose_file" ]; then
        cp -p "$compose_file" "$compose_backup" || {
            rm -f "$config_backup"
            error "Could not back up existing Compose file: ${compose_file}"
        }
    fi

    if [ ! -f "$config_file" ]; then
        config_created=true
        if ! (umask 077; cat > "$config_file") << TOML
server_url = "${SERVER_URL}"
enrollment_code = "${ENROLLMENT_CODE}"

[collector]
interval = 3
enable_temperature = true
TOML
        then
            rollback_docker_agent_install \
                "$compose_file" "$compose_backup" "$config_file" "$config_backup" "$config_created"
            error "Could not create ${config_file}"
        fi
        info "Created ${config_file}"
    else
        info "${config_file} exists — refreshing server_url, enrollment_code, clearing token"
        if ! toml_set "$config_file" "server_url" "$SERVER_URL" \
            || ! toml_set "$config_file" "enrollment_code" "$ENROLLMENT_CODE" \
            || ! toml_set "$config_file" "token" ""; then
            rollback_docker_agent_install \
                "$compose_file" "$compose_backup" "$config_file" "$config_backup" "$config_created"
            error "Could not refresh ${config_file}"
        fi
    fi
    if ! chmod 600 "$config_file"; then
        rollback_docker_agent_install \
            "$compose_file" "$compose_backup" "$config_file" "$config_backup" "$config_created"
        error "Could not secure ${config_file}"
    fi

    ensure_caps_initialized

    if ! cat > "$compose_file" << YAML
services:
  serverbee-agent:
    image: ghcr.io/zingerlittlebee/serverbee-agent:${image_tag}
    container_name: serverbee-agent
    privileged: true
    network_mode: host
    pid: host
    volumes:
      - /proc:/host/proc:ro
      - /sys:/host/sys:ro
      - /etc/os-release:/host/etc/os-release:ro
      - ${conf_dir}:/etc/serverbee
    restart: unless-stopped
YAML
    then
        rollback_docker_agent_install \
            "$compose_file" "$compose_backup" "$config_file" "$config_backup" "$config_created"
        error "Could not generate ${compose_file}"
    fi

    cap_command_block=$(compute_cap_compose_command)
    if [ -n "$cap_command_block" ]; then
        if ! printf '%s\n' "$cap_command_block" >> "$compose_file"; then
            rollback_docker_agent_install \
                "$compose_file" "$compose_backup" "$config_file" "$config_backup" "$config_created"
            error "Could not add Agent capabilities to ${compose_file}"
        fi
    fi

    info "Generated ${compose_file}"
    if ! docker compose -f "$compose_file" config -q; then
        rollback_docker_agent_install \
            "$compose_file" "$compose_backup" "$config_file" "$config_backup" "$config_created"
        error "Generated Agent Compose file is invalid; installation rollback was attempted"
    fi
    AGENT_LOG_SINCE=$(date +%s)
    if ! docker compose -f "$compose_file" up -d --force-recreate; then
        rollback_docker_agent_install \
            "$compose_file" "$compose_backup" "$config_file" "$config_backup" "$config_created"
        error "Failed to start the ServerBee Agent container; installation rollback was attempted"
    fi
    info "Agent container started"

    install_cli "$version"
    if ! meta_write "agent" "docker" "$version"; then
        rollback_docker_agent_install \
            "$compose_file" "$compose_backup" "$config_file" "$config_backup" "$config_created"
        error "Failed to record the ServerBee Agent installation; installation rollback was attempted"
    fi
    rm -f "$compose_backup" "$config_backup" \
        || warn "Could not remove one or more Agent install rollback files"
    verify_agent_install_or_exit
    print_agent_result
}

# Poll the server's startup logs for the one-time first-run admin password.
fetch_first_run_password() {
    local i out pw max inv esc
    if [ "$METHOD" = "docker" ]; then max=45; else max=15; fi
    esc=$(printf '\033')
    i=0
    while [ "$i" -lt "$max" ]; do
        if [ "$METHOD" = "docker" ]; then
            out=$(docker compose -f "${DOCKER_DIR}/docker-compose.server.yml" logs --no-color serverbee-server 2>/dev/null)
        elif [ "$INIT" = systemd ]; then
            inv=$(systemctl show -p InvocationID --value serverbee-server 2>/dev/null)
            if [ -n "$inv" ]; then
                out=$(journalctl _SYSTEMD_INVOCATION_ID="$inv" --no-pager 2>/dev/null)
            else
                out=$(journalctl -u serverbee-server --no-pager 2>/dev/null)
            fi
        elif [ "$INIT" = openrc ]; then
            out=$(cat "$(svc_log_path server)" 2>/dev/null)
        else
            return 0
        fi
        pw=$(printf '%s\n' "$out" \
            | sed "s/${esc}\[[0-9;]*m//g" \
            | grep -A8 'FIRST-RUN ADMIN CREDENTIALS' \
            | grep 'Password:' \
            | tail -n1 \
            | sed 's/.*Password:[[:space:]]*//' \
            | awk '{print $1}')
        if [ -n "$pw" ]; then
            printf '%s' "$pw"
            return 0
        fi
        sleep 1
        i=$((i + 1))
    done
    return 0
}

print_server_result() {
    local ip pw
    ip=$(get_local_ip)
    pw="$(fetch_first_run_password)"
    echo ""
    if [ "$NO_WAIT" = true ]; then
        cecho "${YELLOW}$(tr_text result_server_unverified)${NC}"
    else
        cecho "${GREEN}$(tr_text result_server_ok)${NC}"
    fi
    echo ""
    echo "$(tr_text lbl_dashboard) http://${ip}:9527"
    echo "$(tr_text lbl_username) admin"
    if [ -n "$pw" ]; then
        cecho "$(tr_text lbl_password) ${BOLD}${pw}${NC}"
        tr_text pw_must_change
    elif [ "$METHOD" = "docker" ]; then
        echo "$(tr_text lbl_password) $(trp pw_docker "${DOCKER_DIR}/docker-compose.server.yml")"
    elif [ "$INIT" = systemd ]; then
        echo "$(tr_text lbl_password) $(tr_text pw_systemd)"
    else
        echo "$(tr_text lbl_password) $(tr_text pw_proc)"
    fi
    echo ""
    echo "$(tr_text lbl_docs) ${DOCS_URL}/$(docs_lang)/docs/configuration"
    echo ""
}

print_agent_result() {
    echo ""
    if [ "$NO_WAIT" = true ]; then
        cecho "${YELLOW}$(tr_text result_agent_unverified)${NC}"
    else
        cecho "${GREEN}$(tr_text result_agent_ok)${NC}"
    fi
    echo ""
    echo "$(tr_text lbl_server_url) ${SERVER_URL}"
    if [ "$METHOD" = "docker" ]; then
        echo "$(tr_text lbl_logs) docker compose -f ${DOCKER_DIR}/docker-compose.agent.yml logs -f"
    elif [ "$INIT" = systemd ]; then
        echo "$(tr_text lbl_start) sudo systemctl start serverbee-agent"
        echo "$(tr_text lbl_logs) sudo journalctl -u serverbee-agent -f"
    elif [ "$INIT" = openrc ]; then
        echo "$(tr_text lbl_start) rc-service serverbee-agent start"
        echo "$(tr_text lbl_logs) tail -f $(svc_log_path agent)"
    else
        echo "$(tr_text lbl_start) ${INSTALL_DIR}/serverbee-agent &"
    fi
    echo ""
    if [ "$METHOD" = "docker" ]; then
        echo "$(tr_text lbl_config) $(docker_conf_dir)/agent.toml"
    else
        echo "$(tr_text lbl_config) ${CONFIG_DIR}/agent.toml"
    fi
    echo "$(tr_text lbl_docs) ${DOCS_URL}/$(docs_lang)/docs/configuration"
    echo ""
}

# ─── Domain / HTTPS setup ─────────────────────────────────────────────────────
ensure_caddy_state_dir() {
    local caddy_home
    id caddy >/dev/null 2>&1 || return 0
    caddy_home=""
    if command -v getent >/dev/null 2>&1; then
        caddy_home=$(getent passwd caddy | cut -d: -f6)
    fi
    caddy_home="${caddy_home:-/var/lib/caddy}"
    mkdir -p "$caddy_home"
    chown -R caddy:caddy "$caddy_home"
    chmod 0700 "$caddy_home"
}

install_caddy() {
    if command -v caddy >/dev/null 2>&1; then
        info "Caddy is already installed"
        ensure_caddy_state_dir
        return
    fi

    if command -v apt-get >/dev/null 2>&1; then
        info "Installing Caddy via official apt repository..."
        apt-get update -qq >/dev/null 2>&1
        apt-get install -y -qq debian-keyring debian-archive-keyring apt-transport-https curl gpg >/dev/null 2>&1 \
            || error "Failed to install Caddy apt repository dependencies"
        curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' \
            | gpg --dearmor --batch --yes -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
        curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' \
            > /etc/apt/sources.list.d/caddy-stable.list
        chmod o+r /usr/share/keyrings/caddy-stable-archive-keyring.gpg /etc/apt/sources.list.d/caddy-stable.list
        apt-get update -qq >/dev/null 2>&1
        apt-get install -y -qq caddy >/dev/null 2>&1 || error "Failed to install Caddy"
    elif command -v dnf >/dev/null 2>&1; then
        info "Installing Caddy via COPR repository..."
        dnf install -y -q dnf-plugins-core >/dev/null 2>&1 || dnf install -y -q dnf5-plugins >/dev/null 2>&1 \
            || error "Failed to install dnf COPR plugin"
        dnf copr enable -y @caddy/caddy >/dev/null 2>&1 || error "Failed to enable Caddy COPR repository"
        dnf install -y -q caddy >/dev/null 2>&1 || error "Failed to install Caddy"
    elif command -v yum >/dev/null 2>&1; then
        info "Installing Caddy via COPR repository..."
        yum install -y -q yum-plugin-copr >/dev/null 2>&1 || error "Failed to install yum COPR plugin"
        yum copr enable -y @caddy/caddy >/dev/null 2>&1 || error "Failed to enable Caddy COPR repository"
        yum install -y -q caddy >/dev/null 2>&1 || error "Failed to install Caddy"
    elif command -v apk >/dev/null 2>&1; then
        info "Installing Caddy via apk..."
        apk add --quiet caddy >/dev/null 2>&1 || error "Failed to install Caddy via apk"
    else
        error "Cannot install Caddy automatically on this distribution.\n  Install Caddy manually, then configure:\n\n  ${DOMAIN} {\n      reverse_proxy 127.0.0.1:9527\n  }"
    fi

    ensure_caddy_state_dir
}

check_http_ports_available() {
    local listeners
    listeners=""
    if command -v ss >/dev/null 2>&1; then
        listeners=$(ss -ltnp 2>/dev/null | awk '$4 ~ /:80$/ || $4 ~ /:443$/ {print}' || true)
    elif command -v lsof >/dev/null 2>&1; then
        listeners=$(lsof -nP -iTCP:80 -iTCP:443 -sTCP:LISTEN 2>/dev/null || true)
    fi

    if [ -n "$listeners" ] && ! echo "$listeners" | grep -qi caddy; then
        echo "$listeners" | sed 's/^/  /'
        error "Port 80 or 443 is already used by a non-Caddy service.\n  Stop that service or configure your existing reverse proxy manually."
    fi
}

write_caddyfile() {
    local first_nonblank
    mkdir -p "$CADDY_CONFIG_DIR"
    if [ -f "$CADDYFILE" ]; then
        cp "$CADDYFILE" "${CADDYFILE}.serverbee.$(date +%Y%m%d%H%M%S).bak"
        if [ -n "$EMAIL" ] && ! grep -q "^[[:space:]]*email[[:space:]]" "$CADDYFILE"; then
            first_nonblank=$(awk 'NF {print; exit}' "$CADDYFILE")
            if [ "$first_nonblank" = "{" ]; then
                awk -v email="$EMAIL" '
                    !inserted && $0 == "{" { print; print "    email "email; inserted=1; next }
                    { print }
                ' "$CADDYFILE" > /tmp/serverbee-caddyfile
            else
                {
                    echo "{"
                    echo "    email ${EMAIL}"
                    echo "}"
                    echo ""
                    cat "$CADDYFILE"
                } > /tmp/serverbee-caddyfile
            fi
            mv /tmp/serverbee-caddyfile "$CADDYFILE"
        fi
        if grep -q "^${DOMAIN}[[:space:]]*{" "$CADDYFILE"; then
            awk -v domain="$DOMAIN" '
                $0 ~ "^"domain"[[:space:]]*{" { print domain" {\n    reverse_proxy 127.0.0.1:9527\n}"; in_block=1; depth=1; next }
                in_block {
                    depth += gsub(/\{/, "{")
                    depth -= gsub(/\}/, "}")
                    if (depth <= 0) in_block=0
                    next
                }
                { print }
            ' "$CADDYFILE" > /tmp/serverbee-caddyfile
            mv /tmp/serverbee-caddyfile "$CADDYFILE"
        else
            cat >> "$CADDYFILE" << EOF

${DOMAIN} {
    reverse_proxy 127.0.0.1:9527
}
EOF
        fi
    else
        if [ -n "$EMAIL" ]; then
            cat > "$CADDYFILE" << EOF
{
    email ${EMAIL}
}

${DOMAIN} {
    reverse_proxy 127.0.0.1:9527
}
EOF
        else
            cat > "$CADDYFILE" << EOF
${DOMAIN} {
    reverse_proxy 127.0.0.1:9527
}
EOF
        fi
    fi
    info "Configured ${CADDYFILE} for ${DOMAIN}"
}

update_server_for_domain_binary() {
    [ -f "${CONFIG_DIR}/server.toml" ] || error "Server config not found: ${CONFIG_DIR}/server.toml"
    toml_set "${CONFIG_DIR}/server.toml" "server.listen" "127.0.0.1:9527"
    toml_set "${CONFIG_DIR}/server.toml" "auth.secure_cookie" "true"
    case "$INIT" in
        systemd|openrc) svc_action restart server ;;
    esac
}

update_server_for_domain_docker() {
    local compose_file
    compose_file="${DOCKER_DIR}/docker-compose.server.yml"
    [ -f "$compose_file" ] || error "Compose file not found: $compose_file"

    sed_inplace 's|- "9527:9527"|- "127.0.0.1:9527:9527"|' "$compose_file"
    compose_set_env "$compose_file" server SERVERBEE_AUTH__SECURE_COOKIE true \
        || error "Could not update secure-cookie environment in ${compose_file}"
    docker compose -f "$compose_file" config -q \
        || error "Domain setup produced an invalid Compose file: ${compose_file}"
    docker compose -f "$compose_file" up -d
}

wait_for_https_endpoint() {
    local url attempts delay attempt
    url="https://${DOMAIN}/healthz"
    attempts=30
    delay=2
    attempt=1
    while [ "$attempt" -le "$attempts" ]; do
        if curl -fsS --max-time 20 "$url" >/dev/null; then
            return 0
        fi
        if [ "$attempt" -lt "$attempts" ]; then
            info "HTTPS endpoint is not ready yet (attempt ${attempt}/${attempts}); retrying in ${delay}s..."
            sleep "$delay"
        fi
        attempt=$((attempt + 1))
    done
    return 1
}

setup_domain() {
    local method
    validate_domain_name "$DOMAIN"
    check_domain_points_here "$DOMAIN"
    check_http_ports_available
    install_caddy
    write_caddyfile

    detect_installed
    meta_has "server" || error "serverbee-server is not installed. Install the server first."

    method=$(meta_read "server" "method")
    case "$method" in
        binary) update_server_for_domain_binary ;;
        docker) update_server_for_domain_docker ;;
        *) error "Unsupported server install method for domain setup: ${method}" ;;
    esac

    case "$INIT" in
        systemd) systemctl enable caddy >/dev/null 2>&1 || true; systemctl restart caddy ;;
        openrc)  rc-update add caddy default >/dev/null 2>&1 || true; rc-service caddy restart ;;
        none)    warn "No init manager found. Start Caddy manually with: caddy run --config ${CADDYFILE}" ;;
    esac

    info "Verifying HTTPS endpoint..."
    wait_for_https_endpoint \
        || error "HTTPS verification failed for https://${DOMAIN}/healthz. Check Caddy logs and DNS propagation."
    write_domain_cache || error "HTTPS is healthy, but the configured domain could not be saved to ${DOMAIN_CACHE_FILE}"

    echo ""
    cecho "${GREEN}ServerBee HTTPS domain configured successfully!${NC}"
    echo ""
    echo "  Dashboard: https://${DOMAIN}"
    echo "  Agent URL: https://${DOMAIN}"
    echo ""
    echo "Install an agent with:"
    echo "  curl -fsSL https://raw.githubusercontent.com/${REPO}/main/deploy/install.sh | sudo sh -s -- agent \\"
    echo "    --server-url https://${DOMAIN} \\"
    echo "    --enrollment-code YOUR_ONE_TIME_CODE"
    echo ""
}

cmd_domain() {
    if [ -z "$COMPONENT" ]; then
        COMPONENT="setup"
    fi
    [ "$COMPONENT" = "setup" ] || error "Usage: serverbee domain setup --domain monitor.example.com --email admin@example.com"

    if [ -z "$DOMAIN" ]; then
        if [ "$YES" = true ] || ! has_prompt_input; then
            error "--domain is required"
        fi
        printf '%s' "$(tr_text domain_prompt)"; prompt_read DOMAIN
    fi

    if [ -z "$EMAIL" ] && [ "$YES" != true ] && has_prompt_input; then
        printf '%s' "$(tr_text email_prompt)"; prompt_read EMAIL
    fi

    run_domain_setup_with_plan
}

# ─── Install command ──────────────────────────────────────────────────────────
print_missing_deps_plan() {
    collect_missing_deps
    if [ -n "$MISSING_DEPS" ]; then
        echo "$(tr_text plan_pkgs) ${MISSING_DEPS} $(tr_text plan_pkgs_suffix)"
    fi
}

print_common_binary_plan() {
    local component os arch filename version
    component="$1"
    os=$(detect_os)
    arch=$(detect_arch)
    filename="serverbee-${component}-${os}-${arch}"
    version=$(get_latest_version)

    tr_text plan_gh_meta
    if [ -f "${INSTALL_DIR}/serverbee-${component}" ]; then
        echo "$(tr_text plan_binary_adopt_pre) ${INSTALL_DIR}/serverbee-${component} $(tr_text plan_binary_adopt_suf)"
    else
        echo "$(tr_text plan_binary_dl) https://github.com/${REPO}/releases/download/${version}/${filename}"
    fi
    echo "$(tr_text plan_cli_script) https://raw.githubusercontent.com/${REPO}/${version}/deploy/install.sh"
}

print_common_docker_plan() {
    local component version
    component="$1"
    configure_docker_dir
    version=$(get_latest_version)
    tr_text plan_docker_prereq
    tr_text plan_gh_meta
    echo "$(tr_text plan_docker_image) ghcr.io/zingerlittlebee/serverbee-${component}:$(docker_image_tag "$version")"
    echo "$(tr_text plan_cli_script) https://raw.githubusercontent.com/${REPO}/${version}/deploy/install.sh"
}

print_domain_plan() {
    [ -z "$DOMAIN" ] && return

    echo ""
    tr_text domain_plan_header
    echo "$(tr_text dp_dns_pre) ${DOMAIN} $(tr_text dp_dns_suf)"
    tr_text dp_repo
    echo "$(tr_text dp_key) https://dl.cloudsmith.io/public/caddy/stable/gpg.key"
    echo "$(tr_text dp_src) https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt"
    tr_text dp_pkgs
    echo "$(tr_text dp_caddyfile) ${CADDYFILE}"
    tr_text dp_bind
    tr_text dp_cookie
    echo "$(tr_text dp_url) https://${DOMAIN}"
}

run_domain_preflight_checks() {
    [ -z "$DOMAIN" ] && return

    echo ""
    tr_text preflight
    validate_domain_name "$DOMAIN"
    check_domain_points_here "$DOMAIN"
}

confirm_domain_setup_plan() {
    local confirm
    run_domain_preflight_checks

    echo ""
    cecho "${BOLD}$(tr_text domain_plan_title)${NC}"
    echo ""
    echo "$(tr_text domain_label) ${DOMAIN}"
    [ -n "$EMAIL" ] && echo "$(tr_text email_label) ${EMAIL}"
    echo ""
    tr_text will_add_download
    print_missing_deps_plan
    print_domain_plan
    echo ""

    if ! should_prompt; then
        info "Proceeding without prompt."
        return
    fi

    printf '%s' "$(tr_text start_domain)"; prompt_read confirm
    case "$confirm" in
        [yY]|[yY][eE][sS]) ;;
        *) error "Domain setup cancelled." ;;
    esac
}

run_domain_setup_with_plan() {
    confirm_domain_setup_plan
    check_deps
    setup_domain
}

print_install_plan() {
    echo ""
    cecho "${BOLD}$(tr_text install_plan_title)${NC}"
    echo ""
    echo "$(tr_text plan_component) serverbee-${COMPONENT}"
    echo "$(tr_text plan_method) ${METHOD}"

    if [ "$COMPONENT" = "server" ]; then
        if [ -n "$DOMAIN" ]; then
            echo "$(tr_text plan_access) $(tr_text plan_access_domain_val) (${DOMAIN})"
        else
            echo "$(tr_text plan_access) $(tr_text plan_access_ip_val)"
        fi
    else
        echo "$(tr_text plan_server_url) ${SERVER_URL}"
    fi

    echo ""
    tr_text will_add_download
    print_missing_deps_plan
    case "${COMPONENT}-${METHOD}" in
        server-binary)
            print_common_binary_plan "server"
            echo "$(tr_text plan_cfg_file) ${CONFIG_DIR}/server.toml"
            echo "$(tr_text plan_data_dir) ${DATA_DIR}"
            [ "$INIT" = systemd ] && echo "$(tr_text plan_systemd) serverbee-server"
            ;;
        agent-binary)
            print_common_binary_plan "agent"
            echo "$(tr_text plan_cfg_file) ${CONFIG_DIR}/agent.toml"
            echo "$(tr_text caps_plan_label) $(render_caps_for_plan)"
            [ "$INIT" = systemd ] && echo "$(tr_text plan_systemd) serverbee-agent"
            ;;
        server-docker)
            print_common_docker_plan "server"
            echo "$(tr_text plan_cfg_file) $(docker_conf_dir)/server.toml"
            echo "$(tr_text plan_compose_file) ${DOCKER_DIR}/docker-compose.server.yml"
            tr_text plan_docker_volume
            ;;
        agent-docker)
            print_common_docker_plan "agent"
            echo "$(tr_text plan_cfg_file) $(docker_conf_dir)/agent.toml"
            echo "$(tr_text plan_compose_file) ${DOCKER_DIR}/docker-compose.agent.yml"
            echo "$(tr_text caps_plan_label) $(render_caps_for_plan)"
            ;;
    esac
    print_domain_plan
    echo ""
}

confirm_install_plan() {
    local confirm
    run_domain_preflight_checks
    print_install_plan

    if ! should_prompt; then
        info "Proceeding without prompt."
        return
    fi

    printf '%s' "$(tr_text start_install)"; prompt_read confirm
    case "$confirm" in
        [yY]|[yY][eE][sS]) ;;
        *) error "Installation cancelled." ;;
    esac
}

prompt_install_method() {
    local choice
    echo ""
    if [ "$COMPONENT" = "server" ]; then
        tr_text server_docker_recommended
        tr_text binary_option
        echo ""
        printf '%s' "$(tr_text select_method)"; prompt_read choice
        case "$choice" in
            1|docker) METHOD="docker" ;;
            2|binary) METHOD="binary" ;;
            *) error "Invalid choice: $choice" ;;
        esac
    else
        tr_text agent_binary_recommended
        tr_text docker_option
        echo ""
        printf '%s' "$(tr_text select_method)"; prompt_read choice
        case "$choice" in
            1|binary) METHOD="binary" ;;
            2|docker) METHOD="docker" ;;
            *) error "Invalid choice: $choice" ;;
        esac
    fi
}

cmd_install() {
    local choice confirm confirm_domain default_server_url existing_version
    if [ -z "$COMPONENT" ]; then
        echo ""
        cecho "${BOLD}$(tr_text install_title)${NC}"
        echo ""
        tr_text agent_option
        tr_text server_option
        echo ""
        printf '%s' "$(tr_text select_component)"; prompt_read choice
        case "$choice" in
            1|agent)  COMPONENT="agent" ;;
            2|server) COMPONENT="server" ;;
            *) error "Invalid choice: $choice" ;;
        esac
    fi

    case "$COMPONENT" in
        server|agent) ;;
        *) error "Invalid component: $COMPONENT (use 'server' or 'agent')" ;;
    esac

    if meta_has "$COMPONENT"; then
        existing_version=$(meta_read "$COMPONENT" "version")
        error "serverbee-${COMPONENT} is already installed (${existing_version}). Use 'upgrade' to update."
    fi

    if [ -z "$METHOD" ]; then
        if has_prompt_input; then
            prompt_install_method
        else
            METHOD="binary"
            info "Non-interactive mode detected, defaulting to binary installation."
            if [ "$COMPONENT" = "server" ]; then
                info "Docker is recommended for Server when Docker is available; pass --method docker to use it."
            fi
        fi
    fi
    : "${METHOD:=binary}"
    case "$METHOD" in
        binary|docker) ;;
        *) error "Invalid method: $METHOD (use 'binary' or 'docker')" ;;
    esac

    if [ "$COMPONENT" = "agent" ] && [ "$METHOD" = "docker" ] && [ "$YES" != true ]; then
        echo ""
        warn "Docker is NOT recommended for Agent"
        echo ""
        tr_text docker_agent_note
        tr_text docker_agent_note1
        tr_text docker_agent_note2
        tr_text docker_agent_note3
        echo ""
        printf '%s' "$(tr_text docker_continue_confirm)"; prompt_read confirm
        case "$confirm" in
            [yY]|[yY][eE][sS]) ;;
            *) METHOD="binary"; info "Switched to binary installation." ;;
        esac
    fi

    if [ "$COMPONENT" = "server" ]; then
        if [ -z "$DOMAIN" ] && [ "$YES" != true ] && has_prompt_input; then
            echo ""
            printf '%s' "$(tr_text configure_domain)"; prompt_read confirm_domain
            case "$confirm_domain" in
                [yY]*)
                    printf '%s' "$(tr_text domain_prompt)"; prompt_read DOMAIN
                    printf '%s' "$(tr_text email_prompt)"; prompt_read EMAIL
                    ;;
            esac
        fi
    elif [ "$COMPONENT" = "agent" ]; then
        if [ -z "$SERVER_URL" ] && [ "$YES" != true ]; then
            default_server_url="http://$(get_local_ip):9527"
            printf '%s' "$(trp server_url_prompt "$default_server_url")"; prompt_read SERVER_URL
            SERVER_URL="${SERVER_URL:-$default_server_url}"
        fi
        while [ -z "$SERVER_URL" ]; do
            if [ "$YES" = true ]; then error "--server-url is required for agent installation"; fi
            printf '%s' "$(trp server_url_prompt "http://$(get_local_ip):9527")"; prompt_read SERVER_URL
        done
        while [ -z "$ENROLLMENT_CODE" ]; do
            if [ "$YES" = true ]; then error "--enrollment-code is required for agent installation (generate a one-time code in Servers → Add Server)"; fi
            printf '%s' "$(tr_text enrollment_prompt)"; prompt_read ENROLLMENT_CODE
        done
        prompt_agent_capabilities
    fi

    ensure_caps_initialized
    confirm_install_plan
    check_deps

    info "Installing ${COMPONENT} via ${METHOD}..."

    case "${COMPONENT}-${METHOD}" in
        server-binary) install_binary_server ;;
        server-docker) install_docker_server ;;
        agent-binary)  install_binary_agent ;;
        agent-docker)  install_docker_agent ;;
    esac

    if [ "$COMPONENT" = "server" ] && [ -n "$DOMAIN" ]; then
        setup_domain
    fi
}

# ─── Uninstall command ────────────────────────────────────────────────────────
uninstall_binary() {
    local component
    component="$1"

    svc_remove "$component"

    rm -f "${INSTALL_DIR}/serverbee-${component}"

    if [ "$PURGE" = true ]; then
        rm -f "${CONFIG_DIR}/${component}.toml"
        if [ "$component" = "server" ]; then
            rm -rf "$DATA_DIR"
        fi
        info "Config and data purged"
    fi
}

uninstall_docker() {
    local component compose_file image_name img vol
    component="$1"
    compose_file="${DOCKER_DIR}/docker-compose.${component}.yml"

    if [ -f "$compose_file" ]; then
        docker compose -f "$compose_file" down || true
    else
        docker stop "serverbee-${component}" 2>/dev/null || true
        docker rm "serverbee-${component}" 2>/dev/null || true
    fi

    if [ "$PURGE" = true ]; then
        image_name="ghcr.io/zingerlittlebee/serverbee-${component}"
        docker images --format '{{.Repository}}:{{.Tag}}' | grep "^${image_name}:" | while read -r img; do
            docker rmi "$img" 2>/dev/null || true
        done
        if [ "$component" = "server" ]; then
            docker volume ls --format '{{.Name}}' | grep "serverbee-data" | while read -r vol; do
                docker volume rm "$vol" 2>/dev/null || true
            done
        fi
        rm -f "$compose_file"
        rm -f "$(docker_conf_dir)/${component}.toml"
        info "Config, data, images, and volumes purged"
    fi
}

cmd_uninstall() {
    local choice method purge_note confirm remaining conf_dir
    if [ -z "$COMPONENT" ]; then
        echo ""
        cecho "${BOLD}$(tr_text uninstall_title)${NC}"
        echo ""
        tr_text opt_agent
        tr_text opt_server
        echo ""
        printf '%s' "$(tr_text select_component)"; prompt_read choice
        case "$choice" in
            1|agent)  COMPONENT="agent" ;;
            2|server) COMPONENT="server" ;;
            *) error "Invalid choice: $choice" ;;
        esac
    fi

    case "$COMPONENT" in
        server|agent) ;;
        *) error "Invalid component: $COMPONENT" ;;
    esac

    if ! meta_has "$COMPONENT"; then
        error "serverbee-${COMPONENT} is not installed (not managed by this script)"
    fi

    method=$(meta_read "$COMPONENT" "method")

    if [ "$YES" != true ]; then
        purge_note=""
        if [ "$PURGE" = true ]; then
            purge_note="$(tr_text uninstall_purge_note)"
        fi
        printf '%s' "$(trp uninstall_confirm "$COMPONENT" "$method" "$purge_note")"; prompt_read confirm
        case "$confirm" in
            [yY]|[yY][eE][sS]) ;;
            *) info "Cancelled."; exit 0 ;;
        esac
    fi

    info "Uninstalling serverbee-${COMPONENT} (${method})..."

    case "$method" in
        binary) uninstall_binary "$COMPONENT" ;;
        docker) uninstall_docker "$COMPONENT" ;;
        *) error "Unknown install method: $method" ;;
    esac

    meta_remove "$COMPONENT"
    if [ "$COMPONENT" = server ]; then
        rm -f "$DOMAIN_CACHE_FILE"
    fi
    info "serverbee-${COMPONENT} has been uninstalled."

    if [ -f "$META_FILE" ]; then
        remaining=$(grep -c '"method"' "$META_FILE" 2>/dev/null || true)
        : "${remaining:=0}"
        if [ "$remaining" -eq 0 ]; then
            rm -f "$CLI_PATH"
            rm -f "$META_FILE"
            rm -f "$LANG_CACHE_FILE"
            rm -f "$DOMAIN_CACHE_FILE"
            if [ "$PURGE" = true ]; then
                # Purge requested and nothing left to manage: remove the whole
                # base directory, including any orphaned files left behind by a
                # prior non-purge uninstall of the other component. A plain
                # rmdir would fail here because those leftovers keep the tree
                # non-empty.
                rm -rf "$BASE_DIR"
                # snap Docker keeps config under its own tree (not $BASE_DIR);
                # drop the whole ServerBee-specific dir so no empty husk lingers.
                rm -rf "$SNAP_DOCKER_DIR"
            else
                # Non-purge: only drop directories that are already empty so any
                # config or data the user chose to keep stays in place.
                rmdir "$INSTALL_DIR" "$CONFIG_DIR" "$DATA_DIR" "$DOCKER_DIR" "$BASE_DIR" 2>/dev/null || true
            fi
            info "All components removed. CLI uninstalled."
        fi
    fi

    if [ "$PURGE" != true ]; then
        echo ""
        tr_text uninstall_preserved
        echo ""
        if [ "$method" = "docker" ]; then
            conf_dir="$(docker_conf_dir)"
            echo "    rm -f ${DOCKER_DIR}/docker-compose.${COMPONENT}.yml"
            echo "    rm -f ${conf_dir}/${COMPONENT}.toml"
            if [ "$COMPONENT" = "server" ]; then
                echo "    docker volume rm serverbee_serverbee-data"
            fi
        else
            echo "    rm -f ${CONFIG_DIR}/${COMPONENT}.toml"
            if [ "$COMPONENT" = "server" ]; then
                echo "    rm -rf ${DATA_DIR}"
            fi
        fi
        echo ""
    fi
}

# ─── Upgrade command ──────────────────────────────────────────────────────────
upgrade_component() {
    local component latest_version method current_version confirm
    component="$1"; latest_version="$2"
    method=$(meta_read "$component" "method")
    current_version=$(meta_read "$component" "version")

    if [ -n "$current_version" ] && [ "$current_version" = "$latest_version" ]; then
        refresh_cli_from_release "$latest_version"
        meta_write "$component" "$method" "$current_version"
        info "serverbee-${component} is already up to date (${current_version})"
        return
    fi

    if [ -n "$current_version" ] && ! semver_is_greater "$latest_version" "$current_version"; then
        error "Refusing downgrade: target ${latest_version} is not newer than ${current_version}"
    fi

    if [ -z "$current_version" ]; then
        warn "Cannot determine current version for serverbee-${component}, downloading latest..."
    else
        info "Upgrading serverbee-${component}: ${current_version} -> ${latest_version}"
    fi

    if [ "$YES" != true ]; then
        printf '%s' "$(tr_text upgrade_confirm)"; prompt_read confirm
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
    refresh_cli_from_release "$latest_version"
    info "serverbee-${component} upgraded to ${latest_version}"
}

probe_upgrade_candidate() {
    local component candidate expected reported
    component="$1"; candidate="$2"; expected="${3#v}"

    # Both binaries expose a side-effect-free version probe before loading
    # configuration or opening network connections.
    reported=$("$candidate" --serverbee-upgrade-probe 2>/dev/null) || return 1
    [ "${reported#v}" = "$expected" ]
}

wait_for_upgrade_stability() {
    local component attempt stable baseline_restarts current_restarts state
    component="$1"
    attempt=0
    stable=0
    baseline_restarts=$(svc_restart_count "$component")

    while [ "$attempt" -lt "$UPGRADE_HEALTH_ATTEMPTS" ]; do
        state=$(svc_is_active "$component")
        current_restarts=$(svc_restart_count "$component")
        if [ -n "$baseline_restarts" ] && [ "$current_restarts" != "$baseline_restarts" ]; then
            return 1
        fi
        if [ "$state" = active ] && svc_health_check "$component"; then
            stable=$((stable + 1))
            [ "$stable" -ge "$UPGRADE_STABILITY_CHECKS" ] && return 0
        else
            stable=0
        fi
        attempt=$((attempt + 1))
        sleep 1
    done
    return 1
}

rollback_binary_upgrade() {
    local component target backup action
    component="$1"; target="$2"; backup="$3"

    warn "Upgrade failed; restoring the previous serverbee-${component} binary..."
    svc_action stop "$component" >/dev/null 2>&1 || true
    if ! mv -f "$backup" "$target"; then
        warn "Automatic rollback could not restore ${backup}; manual recovery is required."
        return 1
    fi
    svc_reset_failed "$component"
    action=start
    [ "$(svc_is_active "$component")" = active ] && action=restart
    if ! svc_action "$action" "$component"; then
        warn "The previous binary was restored, but serverbee-${component} could not be restarted."
        return 1
    fi
    if ! wait_for_upgrade_stability "$component"; then
        warn "The previous binary was restored, but serverbee-${component} did not remain healthy."
        return 1
    fi
    info "Rollback completed; serverbee-${component} is running the previous binary."
}

upgrade_binary() {
    local component version os arch filename url target candidate backup logs
    component="$1"; version="$2"
    os=$(detect_os)
    arch=$(detect_arch)

    filename="serverbee-${component}-${os}-${arch}"
    url="https://github.com/${REPO}/releases/download/${version}/${filename}"
    target="${INSTALL_DIR}/serverbee-${component}"
    backup="${target}.rollback"
    [ -f "$target" ] || error "Installed binary not found: ${target}"
    if [ -e "$backup" ] || [ -L "$backup" ]; then
        error "Stale rollback backup found: ${backup}. Restore or remove it before upgrading."
    fi
    candidate=$(mktemp "${INSTALL_DIR}/.serverbee-${component}.upgrade.XXXXXX") \
        || error "Could not create an upgrade candidate in ${INSTALL_DIR}"

    info "Downloading ${filename} ${version}..."
    download_verified "$url" "$candidate" "$filename" "$version"
    chmod 755 "$candidate" || {
        rm -f "$candidate"
        error "Could not make the upgrade candidate executable"
    }
    if ! probe_upgrade_candidate "$component" "$candidate" "$version"; then
        rm -f "$candidate"
        error "Candidate version probe failed for serverbee-${component} ${version}"
    fi

    if ! svc_action stop "$component" 2>/dev/null && [ "$(svc_is_active "$component")" = active ]; then
        rm -f "$candidate"
        error "Could not stop serverbee-${component}; the existing binary was not changed"
    fi
    if ! mv "$target" "$backup"; then
        rm -f "$candidate"
        error "Could not create rollback backup: ${backup}"
    fi
    if ! mv "$candidate" "$target"; then
        mv -f "$backup" "$target" 2>/dev/null || true
        svc_action start "$component" >/dev/null 2>&1 || true
        error "Could not install the serverbee-${component} upgrade candidate"
    fi

    if ! svc_action start "$component"; then
        if rollback_binary_upgrade "$component" "$target" "$backup"; then
            error "serverbee-${component} ${version} failed to start; the previous version was restored"
        fi
        error "serverbee-${component} ${version} failed to start and automatic rollback was incomplete"
    fi
    if ! wait_for_upgrade_stability "$component"; then
        logs=$(svc_logs_tail "$component" 20 || true)
        [ -z "$logs" ] || warn "Recent serverbee-${component} logs:\n${logs}"
        if rollback_binary_upgrade "$component" "$target" "$backup"; then
            error "serverbee-${component} ${version} did not remain healthy; the previous version was restored"
        fi
        error "serverbee-${component} ${version} did not remain healthy and automatic rollback was incomplete"
    fi

    rm -f "$backup"
    info "serverbee-${component} remained healthy for ${UPGRADE_STABILITY_CHECKS} seconds."
}

docker_container_state() {
    docker inspect \
        --format '{{.State.Status}}{{if .State.Health}} {{.State.Health.Status}}{{end}}' \
        "serverbee-$1" 2>/dev/null || echo unavailable
}

docker_restart_count() {
    docker inspect --format '{{.RestartCount}}' "serverbee-$1" 2>/dev/null || true
}

wait_for_docker_stability() {
    local component attempt stable baseline_restarts current_restarts state
    component="$1"
    attempt=0
    stable=0
    baseline_restarts=$(docker_restart_count "$component")

    while [ "$attempt" -lt "$DOCKER_UPGRADE_HEALTH_ATTEMPTS" ]; do
        state=$(docker_container_state "$component")
        current_restarts=$(docker_restart_count "$component")
        if [ -n "$baseline_restarts" ] && [ "$current_restarts" != "$baseline_restarts" ]; then
            return 1
        fi
        case "$state" in
            running|"running healthy")
                stable=$((stable + 1))
                [ "$stable" -ge "$DOCKER_UPGRADE_STABILITY_CHECKS" ] && return 0
                ;;
            *) stable=0 ;;
        esac
        attempt=$((attempt + 1))
        sleep 1
    done
    return 1
}

rollback_docker_upgrade() {
    local component compose_file backup
    component="$1"; compose_file="$2"; backup="$3"

    warn "Docker upgrade failed; restoring the previous serverbee-${component} image..."
    if ! mv -f "$backup" "$compose_file"; then
        warn "Automatic rollback could not restore ${backup}; manual recovery is required."
        return 1
    fi
    if ! docker compose -f "$compose_file" up -d; then
        warn "The previous Compose file was restored, but its container could not be started."
        return 1
    fi
    if ! wait_for_docker_stability "$component"; then
        warn "The previous Compose file was restored, but its container did not remain healthy."
        return 1
    fi
    info "Rollback completed; serverbee-${component} is running the previous Docker image."
}

upgrade_docker() {
    local component version compose_file image_tag image_base backup logs
    component="$1"; version="$2"
    compose_file="${DOCKER_DIR}/docker-compose.${component}.yml"
    image_tag=$(docker_image_tag "$version")
    backup="${compose_file}.rollback"

    if [ ! -f "$compose_file" ]; then
        error "Compose file not found: $compose_file"
    fi
    if [ -e "$backup" ] || [ -L "$backup" ]; then
        error "Stale rollback backup found: ${backup}. Restore or remove it before upgrading."
    fi

    image_base="ghcr.io/zingerlittlebee/serverbee-${component}"
    grep -Fq "${image_base}:" "$compose_file" \
        || error "Compose file does not reference the managed image: ${image_base}"
    cp -p "$compose_file" "$backup" || error "Could not create rollback backup: ${backup}"
    if ! sed_inplace "s|${image_base}:[^ ]*|${image_base}:${image_tag}|" "$compose_file"; then
        rm -f "$backup"
        error "Could not update the Docker image tag"
    fi

    if ! docker compose -f "$compose_file" pull; then
        if mv -f "$backup" "$compose_file"; then
            error "Could not pull serverbee-${component} ${version}; the previous Compose file was restored"
        fi
        error "Could not pull serverbee-${component} ${version} and the previous Compose file could not be restored"
    fi
    if ! docker compose -f "$compose_file" up -d; then
        if rollback_docker_upgrade "$component" "$compose_file" "$backup"; then
            error "serverbee-${component} ${version} failed to start; the previous Docker image was restored"
        fi
        error "serverbee-${component} ${version} failed to start and automatic Docker rollback was incomplete"
    fi
    if ! wait_for_docker_stability "$component"; then
        logs=$(docker compose -f "$compose_file" logs --tail 20 2>/dev/null || true)
        [ -z "$logs" ] || warn "Recent serverbee-${component} Docker logs:\n${logs}"
        if rollback_docker_upgrade "$component" "$compose_file" "$backup"; then
            error "serverbee-${component} ${version} did not remain healthy; the previous Docker image was restored"
        fi
        error "serverbee-${component} ${version} did not remain healthy and automatic Docker rollback was incomplete"
    fi

    rm -f "$backup"
    info "serverbee-${component} Docker container remained healthy for ${DOCKER_UPGRADE_STABILITY_CHECKS} seconds."
}

prepare_upgrade_release() {
    local component stored_channel
    component="$1"
    if [ "$RELEASE_CHANNEL_USER_SPECIFIED" != true ]; then
        stored_channel=$(meta_read "$component" "channel")
        RELEASE_CHANNEL="${stored_channel:-auto}"
    fi
    RESOLVED_VERSION=""
}

cmd_upgrade() {
    local latest_version entry comp
    detect_installed

    if [ -n "$COMPONENT" ]; then
        case "$COMPONENT" in
            server|agent) ;;
            *) error "Invalid component: $COMPONENT" ;;
        esac
        if ! meta_has "$COMPONENT"; then
            error "serverbee-${COMPONENT} is not installed"
        fi
        prepare_upgrade_release "$COMPONENT"
        latest_version=$(get_latest_version)
        upgrade_component "$COMPONENT" "$latest_version"
    else
        if [ -z "$MANAGED_COMPONENTS" ]; then
            error "No managed components found. Nothing to upgrade."
        fi
        for entry in $MANAGED_COMPONENTS; do
            comp="${entry%%:*}"
            prepare_upgrade_release "$comp"
            latest_version=$(get_latest_version)
            upgrade_component "$comp" "$latest_version"
        done
    fi
}

# ─── Status command ───────────────────────────────────────────────────────────
status_component() {
    local component method version service status_line since srv container_status image_tag ports logs_out dashboard
    component="$1"; method="$2"
    version=$(meta_read "$component" "version")
    service="serverbee-${component}"

    cecho "${BOLD}$(capitalize "$component") (${method})${NC}"

    if [ "$method" = "binary" ]; then
        echo "$(tr_text st_version) ${version:-$(tr_text st_unknown)}"
        echo "$(tr_text st_binary) ${INSTALL_DIR}/${service}"
        echo "$(tr_text st_config) ${CONFIG_DIR}/${component}.toml"

        if [ "$INIT" != none ]; then
            status_line=$(svc_is_active "$component")
            if [ "$status_line" = "active" ]; then
                since=""
                if [ "$INIT" = systemd ]; then
                    since=$(systemctl show "$service" --property=ActiveEnterTimestamp --value 2>/dev/null || echo "")
                fi
                cecho "$(tr_text st_service) ${GREEN}$(tr_text st_active)${NC} $(tr_text st_since) ${since}"
            else
                cecho "$(tr_text st_service) ${RED}${status_line}${NC}"
            fi
            tr_text st_recent_logs
            # sed exits 0 on empty input, so capture first to detect "no logs".
            logs_out=$(svc_logs_tail "$component" 5 2>/dev/null || true)
            if [ -n "$logs_out" ]; then
                printf '%s\n' "$logs_out" | sed 's/^/    /'
            else
                tr_text st_no_logs
            fi
        fi

        if [ "$component" = "agent" ] && [ -f "${CONFIG_DIR}/agent.toml" ]; then
            srv=$(grep "^server_url" "${CONFIG_DIR}/agent.toml" 2>/dev/null | sed 's/.*= *"//;s/".*//' || echo "")
            [ -n "$srv" ] && echo "$(tr_text st_server) ${srv}"
        fi

        if [ "$component" = "server" ]; then
            dashboard=$(server_dashboard_display "$method")
            echo "$(tr_text st_dashboard) ${dashboard}"
        fi

    elif [ "$method" = "docker" ]; then
        echo "$(tr_text st_version) ${version:-$(tr_text st_unknown)}"

        if docker ps --format '{{.Names}} {{.Status}}' 2>/dev/null | grep -q "^${service} "; then
            container_status=$(docker ps --format '{{.Status}}' --filter "name=^${service}$" 2>/dev/null)
            cecho "$(tr_text st_container) ${service} (${GREEN}${container_status}${NC})"
        else
            cecho "$(tr_text st_container) ${service} (${RED}$(tr_text st_stopped)${NC})"
        fi

        image_tag=$(docker inspect "${service}" --format '{{.Config.Image}}' 2>/dev/null || echo "unknown")
        echo "$(tr_text st_image) ${image_tag}"

        if [ "$component" = "server" ]; then
            ports=$(docker port "${service}" 2>/dev/null | head -1 || echo "")
            [ -n "$ports" ] && echo "$(tr_text st_port) ${ports}"
            dashboard=$(server_dashboard_display "$method")
            echo "$(tr_text st_dashboard) ${dashboard}"
        fi

        tr_text st_recent_logs
        logs_out=$(docker logs "${service}" --tail 5 2>/dev/null || true)
        if [ -n "$logs_out" ]; then
            printf '%s\n' "$logs_out" | sed 's/^/    /'
        else
            tr_text st_no_logs
        fi
    fi
}

cmd_status() {
    local entry comp method
    detect_installed
    detect_unmanaged

    if [ -z "$MANAGED_COMPONENTS" ] && [ -z "$UNMANAGED_COMPONENTS" ]; then
        echo ""
        tr_text status_none
        echo ""
        return
    fi

    echo ""
    cecho "${BOLD}$(tr_text status_title)${NC}"
    echo "================"

    for entry in $MANAGED_COMPONENTS; do
        comp="${entry%%:*}"
        method="${entry##*:}"
        echo ""
        status_component "$comp" "$method"
    done

    for entry in $UNMANAGED_COMPONENTS; do
        comp="${entry%%:*}"
        method="${entry##*:}"
        echo ""
        warn "Found serverbee-${comp} (${method}) but it is not managed by this script."
        if [ "$method" = docker ]; then
            echo "    Docker adoption is not supported. Remove the unmanaged container or migrate its config and data into a managed Compose installation first."
        else
            echo "    To bring it under management, run: serverbee install ${comp} [options]"
        fi
    done

    echo ""
}

# ─── Service control (start/stop/restart) ────────────────────────────────────
cmd_service() {
    local action targets method entry comp st
    action="$1"
    detect_installed

    targets=""
    if [ -n "$COMPONENT" ]; then
        case "$COMPONENT" in
            server|agent) ;;
            *) error "Invalid component: $COMPONENT" ;;
        esac
        if ! meta_has "$COMPONENT"; then
            error "serverbee-${COMPONENT} is not installed"
        fi
        method=$(meta_read "$COMPONENT" "method")
        targets="${COMPONENT}:${method}"
    else
        if [ -z "$MANAGED_COMPONENTS" ]; then
            error "No managed components found."
        fi
        targets="$MANAGED_COMPONENTS"
    fi

    for entry in $targets; do
        comp="${entry%%:*}"
        method="${entry##*:}"

        info "$(capitalize "$action")ing serverbee-${comp} (${method})..."

        if [ "$method" = "binary" ]; then
            svc_action "$action" "$comp"
        elif [ "$method" = "docker" ]; then
            case "$action" in
                start)   docker compose -f "${DOCKER_DIR}/docker-compose.${comp}.yml" up -d ;;
                stop)    docker compose -f "${DOCKER_DIR}/docker-compose.${comp}.yml" stop ;;
                restart) docker compose -f "${DOCKER_DIR}/docker-compose.${comp}.yml" restart ;;
            esac
        fi

        if [ "$method" = "binary" ]; then
            st=$(svc_is_active "$comp")
            info "serverbee-${comp}: ${st}"
        elif [ "$method" = "docker" ]; then
            st=$(docker ps --format '{{.Status}}' --filter "name=^serverbee-${comp}$" 2>/dev/null || echo "unknown")
            info "serverbee-${comp}: ${st:-stopped}"
        fi
    done
}

# ─── Config command ───────────────────────────────────────────────────────────
REJECTED_KEYS="admin.password admin.username"
ARRAY_KEYS="file.root_paths file.deny_patterns server.trusted_proxies oauth.oidc.scopes ip_change.external_ip_urls"
AGENT_KEYS="server_url enrollment_code token collector.interval collector.enable_gpu collector.enable_temperature file.enabled file.max_file_size ip_change.enabled ip_change.external_ip_urls ip_change.interval_secs"
SERVER_KEYS="file.max_upload_size server.listen server.data_dir auth.session_ttl auth.secure_cookie geoip.mmdb_path retention.records_days retention.records_hourly_days retention.gpu_records_days retention.ping_records_days retention.network_probe_days retention.network_probe_hourly_days retention.audit_logs_days retention.traffic_hourly_days retention.traffic_daily_days retention.task_results_days retention.docker_events_days retention.service_monitor_days database.path database.max_connections rate_limit.login_max rate_limit.register_max scheduler.timezone upgrade.release_base_url oauth.base_url oauth.allow_registration oauth.github.client_id oauth.github.client_secret oauth.google.client_id oauth.google.client_secret oauth.oidc.issuer_url oauth.oidc.client_id oauth.oidc.client_secret"
LOG_KEYS="log.level log.file"

config_key_to_file() {
    local key
    key="$1"
    if echo "$AGENT_KEYS" | grep -qw "$key"; then echo "agent"; return; fi
    if echo "$SERVER_KEYS" | grep -qw "$key"; then echo "server"; return; fi
    if echo "$LOG_KEYS" | grep -qw "$key"; then echo "both"; return; fi
    echo ""
}

toml_set() {
    local file dotted_key value section key rendered_value replacement generated has_key
    file="$1"; dotted_key="$2"; value="$3"
    section=""; key=""

    validate_single_line_value "$value" || return 1

    case "$dotted_key" in
        *.*)
            section="${dotted_key%%.*}"
            key="${dotted_key#*.}"
            case "$key" in
                *.*)
                    section="${dotted_key%.*}"
                    key="${dotted_key##*.}"
                    ;;
            esac
            ;;
        *)
            key="$dotted_key"
            ;;
    esac

    case "$value" in
        ''|*[!0-9]*)
            case "$value" in
                true|false) rendered_value="$value" ;;
                *) rendered_value="\"$(render_double_quoted_value "$value")\"" ;;
            esac
            ;;
        *) rendered_value="$value" ;;
    esac

    replacement="${key} = ${rendered_value}"
    generated=$(mktemp "${file}.generated.XXXXXX") || return 1
    SB_INSTALL_EDIT_SECTION="$section"
    SB_INSTALL_EDIT_KEY="$key"
    SB_INSTALL_EDIT_LINE="$replacement"
    export SB_INSTALL_EDIT_SECTION SB_INSTALL_EDIT_KEY SB_INSTALL_EDIT_LINE

    if [ -z "$section" ]; then
        has_key=$(awk '
            /^[[:space:]]*\[/ { in_top=0 }
            BEGIN { in_top=1; key=ENVIRON["SB_INSTALL_EDIT_KEY"] }
            in_top {
                line=$0
                sub(/^[[:space:]]*/, "", line)
                if (line ~ "^" key "[[:space:]]*=") { print "yes"; exit }
            }
        ' "$file")
        if [ "$has_key" = yes ]; then
            awk '
                BEGIN { in_top=1; written=0; key=ENVIRON["SB_INSTALL_EDIT_KEY"]; replacement=ENVIRON["SB_INSTALL_EDIT_LINE"] }
                /^[[:space:]]*\[/ { in_top=0 }
                in_top {
                    line=$0
                    sub(/^[[:space:]]*/, "", line)
                    if (line ~ "^" key "[[:space:]]*=") {
                        if (!written) print replacement
                        written=1
                        next
                    }
                }
                { print }
            ' "$file" > "$generated"
        else
            printf '%s\n' "$replacement" > "$generated"
            cat "$file" >> "$generated"
        fi
    else
        awk '
            BEGIN {
                target="[" ENVIRON["SB_INSTALL_EDIT_SECTION"] "]"
                key=ENVIRON["SB_INSTALL_EDIT_KEY"]
                replacement=ENVIRON["SB_INSTALL_EDIT_LINE"]
                in_target=0; section_found=0; written=0
            }
            /^[[:space:]]*\[/ {
                if (in_target && !written) { print replacement; written=1 }
                in_target=($0 == target)
                if (in_target) section_found=1
                print
                next
            }
            in_target {
                line=$0
                sub(/^[[:space:]]*/, "", line)
                if (line ~ "^" key "[[:space:]]*=") {
                    if (!written) print replacement
                    written=1
                    next
                }
            }
            { print }
            END {
                if (in_target && !written) print replacement
                if (!section_found) {
                    print ""
                    print target
                    print replacement
                }
            }
        ' "$file" > "$generated"
    fi

    unset SB_INSTALL_EDIT_SECTION SB_INSTALL_EDIT_KEY SB_INSTALL_EDIT_LINE
    if ! atomic_replace_preserving_mode "$file" "$generated"; then
        rm -f "$generated"
        return 1
    fi
    rm -f "$generated"
}

compose_set_env() {
    # Update one managed service using YAML double-quoted list syntax. Existing
    # duplicates are collapsed and Agent files gain an environment block when
    # they did not have one.
    local file component env_key value service escaped replacement generated has_service has_environment
    file="$1"; component="$2"; env_key="$3"; value="$4"
    validate_single_line_value "$value" || return 1
    service="serverbee-${component}"
    escaped=$(render_double_quoted_value "$value")
    # The quotes are YAML data passed intact through ENVIRON to awk.
    # shellcheck disable=SC2089
    replacement="      - \"${env_key}=${escaped}\""

    SB_INSTALL_EDIT_SERVICE="$service"
    SB_INSTALL_EDIT_KEY="$env_key"
    SB_INSTALL_EDIT_LINE="$replacement"
    # shellcheck disable=SC2090
    export SB_INSTALL_EDIT_SERVICE SB_INSTALL_EDIT_KEY SB_INSTALL_EDIT_LINE
    has_service=$(awk '
        $0 == "  " ENVIRON["SB_INSTALL_EDIT_SERVICE"] ":" { print "yes"; exit }
    ' "$file")
    [ "$has_service" = yes ] || {
        unset SB_INSTALL_EDIT_SERVICE SB_INSTALL_EDIT_KEY SB_INSTALL_EDIT_LINE
        return 1
    }
    has_environment=$(awk '
        BEGIN { service="  " ENVIRON["SB_INSTALL_EDIT_SERVICE"] ":"; in_service=0 }
        $0 == service { in_service=1; next }
        in_service && $0 ~ /^  [^ ]/ { exit }
        in_service && $0 ~ /^    environment:[[:space:]]*$/ { print "yes"; exit }
    ' "$file")
    generated=$(mktemp "${file}.generated.XXXXXX") || {
        unset SB_INSTALL_EDIT_SERVICE SB_INSTALL_EDIT_KEY SB_INSTALL_EDIT_LINE
        return 1
    }

    if [ "$has_environment" = yes ]; then
        awk '
            BEGIN {
                service="  " ENVIRON["SB_INSTALL_EDIT_SERVICE"] ":"
                key=ENVIRON["SB_INSTALL_EDIT_KEY"]
                replacement=ENVIRON["SB_INSTALL_EDIT_LINE"]
                in_service=0; in_environment=0; written=0
            }
            $0 == service { in_service=1; print; next }
            in_service && $0 ~ /^  [^ ]/ {
                if (in_environment && !written) print replacement
                in_service=0; in_environment=0
            }
            in_service && $0 ~ /^    environment:[[:space:]]*$/ {
                in_environment=1
                print
                next
            }
            in_environment && $0 ~ /^    [^ ]/ {
                if (!written) print replacement
                written=1; in_environment=0
            }
            in_environment {
                candidate=$0
                sub(/^[[:space:]]*-[[:space:]]*/, "", candidate)
                sub(/^"/, "", candidate)
                if (index(candidate, key "=") == 1) {
                    if (!written) print replacement
                    written=1
                    next
                }
            }
            { print }
            END { if (in_environment && !written) print replacement }
        ' "$file" > "$generated"
    else
        awk '
            BEGIN { service="  " ENVIRON["SB_INSTALL_EDIT_SERVICE"] ":"; replacement=ENVIRON["SB_INSTALL_EDIT_LINE"] }
            { print }
            $0 == service {
                print "    environment:"
                print replacement
            }
        ' "$file" > "$generated"
    fi
    unset SB_INSTALL_EDIT_SERVICE SB_INSTALL_EDIT_KEY SB_INSTALL_EDIT_LINE

    if ! atomic_replace_preserving_mode "$file" "$generated"; then
        rm -f "$generated"
        return 1
    fi
    rm -f "$generated"
}

docker_set_env_transaction() {
    local component compose_file env_key value backup compose_started
    component="$1"; compose_file="$2"; env_key="$3"; value="$4"
    backup="${compose_file}.env-rollback"
    compose_started=false
    if [ -e "$backup" ] || [ -L "$backup" ]; then
        warn "Stale env rollback found: ${backup}"
        return 1
    fi
    cp -p "$compose_file" "$backup" || return 1
    if ! compose_set_env "$compose_file" "$component" "$env_key" "$value"; then
        mv -f "$backup" "$compose_file" 2>/dev/null || true
        return 1
    fi
    if ! docker compose -f "$compose_file" config -q; then
        mv -f "$backup" "$compose_file" 2>/dev/null || true
        return 1
    fi
    if docker compose -f "$compose_file" up -d; then
        compose_started=true
    fi
    if [ "$compose_started" != true ]; then
        if mv -f "$backup" "$compose_file"; then
            docker compose -f "$compose_file" up -d >/dev/null 2>&1 || true
        fi
        return 1
    fi
    rm -f "$backup"
}

cmd_service_single() {
    local comp method action
    comp="$1"; method="$2"; action="$3"
    info "$(capitalize "$action")ing serverbee-${comp}..."
    if [ "$method" = "binary" ]; then
        svc_action "$action" "$comp" 2>/dev/null || true
    elif [ "$method" = "docker" ]; then
        docker compose -f "${DOCKER_DIR}/docker-compose.${comp}.yml" "$action" 2>/dev/null || true
    fi
}

cmd_config() {
    local key value target files_to_update file before_file after_file redacted_before redacted_after targets comp entry method confirm
    detect_installed

    if [ "$COMPONENT" = "set" ]; then
        key="$CONFIG_KEY"
        value="$CONFIG_VALUE"
        [ -z "$key" ] && error "Usage: serverbee config set <key> <value>"
        [ "$POSITIONAL_COUNT" -ge 3 ] || error "Usage: serverbee config set <key> <value>"

        if echo "$REJECTED_KEYS" | grep -qw "$key"; then
            case "$key" in
                admin.password) error "Admin password is not a runtime config. ServerBee generates a one-time first-run password; change it in the Dashboard UI after login." ;;
                admin.username) error "Admin username is not a runtime config. Change it during first-login onboarding or in the Dashboard UI." ;;
            esac
        fi

        if echo "$ARRAY_KEYS" | grep -qw "$key"; then
            error "Key '${key}' is an array type. Edit the TOML file directly:\n  ${CONFIG_DIR}/agent.toml or ${CONFIG_DIR}/server.toml"
        fi

        target=$(config_key_to_file "$key")
        [ -z "$target" ] && error "Unknown config key: $key"

        files_to_update=""
        if [ "$target" = "both" ]; then
            meta_has "agent" && files_to_update="${files_to_update:+$files_to_update }$(conf_file_for agent)"
            meta_has "server" && files_to_update="${files_to_update:+$files_to_update }$(conf_file_for server)"
            [ -z "$files_to_update" ] && error "No managed components found to update log config"
        elif [ "$target" = "agent" ]; then
            files_to_update="$(conf_file_for agent)"
        elif [ "$target" = "server" ]; then
            files_to_update="$(conf_file_for server)"
        fi

        for file in $files_to_update; do
            if [ ! -f "$file" ]; then
                error "Config file not found: $file"
            fi
            before_file=$(mktemp)
            cp -p "$file" "$before_file"
            if ! toml_set "$file" "$key" "$value"; then
                rm -f "$before_file"
                error "Could not safely update ${key} in ${file}. Values must be one printable line."
            fi
            info "Updated ${key} in ${file}"

            echo "  Changes:"
            after_file=$(mktemp)
            redacted_before=$(mktemp)
            redacted_after=$(mktemp)
            cp -p "$file" "$after_file"
            redact_toml_file "$before_file" > "$redacted_before"
            redact_toml_file "$after_file" > "$redacted_after"
            diff "$redacted_before" "$redacted_after" | sed 's/^/    /' || true
            rm -f "$before_file" "$after_file" "$redacted_before" "$redacted_after"
        done

        if [ "$YES" = true ]; then
            for entry in $MANAGED_COMPONENTS; do
                comp="${entry%%:*}"
                method="${entry##*:}"
                if [ "$target" = "$comp" ] || [ "$target" = "both" ]; then
                    cmd_service_single "$comp" "$method" "restart"
                fi
            done
        else
            if has_prompt_input; then
                echo ""
                tr_text restart_apply_q
                printf '%s' "$(tr_text restart_apply_confirm)"; prompt_read confirm
                case "$confirm" in
                    [yY]*)
                        for entry in $MANAGED_COMPONENTS; do
                            comp="${entry%%:*}"
                            method="${entry##*:}"
                            if [ "$target" = "$comp" ] || [ "$target" = "both" ]; then
                                cmd_service_single "$comp" "$method" "restart"
                            fi
                        done
                        ;;
                esac
            else
                warn "Non-interactive mode detected; services were not restarted. Re-run with -y to restart automatically, or restart manually."
            fi
        fi
        return
    fi

    targets=""
    if [ -n "$COMPONENT" ]; then
        case "$COMPONENT" in
            server|agent) ;;
            *) error "Invalid component: $COMPONENT" ;;
        esac
        targets="$COMPONENT"
    else
        for entry in $MANAGED_COMPONENTS; do
            targets="${targets:+$targets }${entry%%:*}"
        done
    fi

    [ -z "$targets" ] && error "No managed components found."

    for comp in $targets; do
        file="$(conf_file_for "$comp")"
        echo ""
        cecho "${BOLD}$(capitalize "$comp") config (${file})${NC}"
        echo "─────────────────────────────────"
        if [ -f "$file" ]; then
            redact_toml_file "$file"
        else
            echo "(file not found)"
        fi
        echo ""
    done
}

# ─── Env command ──────────────────────────────────────────────────────────────
env_key_to_component() {
    local key
    key="$1"
    case "$key" in
        SERVER_URL|ENROLLMENT_CODE|TOKEN|COLLECTOR__*|IP_CHANGE__*|FILE__ENABLED|FILE__MAX_FILE_SIZE|FILE__ROOT_PATHS|FILE__DENY_PATTERNS)
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
    local raw_key value env_key stripped target components_to_update comp method service override_dir override_file compose_file entry shell_vars unit_envs
    detect_installed

    if [ "$COMPONENT" = "set" ]; then
        raw_key="$CONFIG_KEY"
        value="$CONFIG_VALUE"
        [ -z "$raw_key" ] && error "Usage: serverbee env set <KEY> <value>"
        [ "$POSITIONAL_COUNT" -ge 3 ] || error "Usage: serverbee env set <KEY> <value>"
        validate_single_line_value "$value" \
            || error "Environment values must be one printable line."

        env_key="$raw_key"
        case "$env_key" in
            SERVERBEE_*) ;;
            *) env_key="SERVERBEE_${env_key}" ;;
        esac

        stripped="${env_key#SERVERBEE_}"
        target=$(env_key_to_component "$stripped")
        [ -z "$target" ] && error "Unknown env key: $env_key"

        components_to_update=""
        if [ "$target" = "both" ]; then
            meta_has "agent" && components_to_update="${components_to_update:+$components_to_update }agent"
            meta_has "server" && components_to_update="${components_to_update:+$components_to_update }server"
        else
            meta_has "$target" || error "serverbee-${target} is not installed"
            components_to_update="$target"
        fi

        for comp in $components_to_update; do
            method=$(meta_read "$comp" "method")
            service="serverbee-${comp}"

            if [ "$method" = "binary" ]; then
                if [ "$INIT" = systemd ]; then
                    override_dir="/etc/systemd/system/${service}.service.d"
                    override_file="${override_dir}/override.conf"
                    mkdir -p "$override_dir"
                    if [ ! -f "$override_file" ]; then
                        (umask 077; cat > "$override_file") << EOF
[Service]
EOF
                    fi
                    systemd_set_env "$override_file" "$env_key" "$value" \
                        || error "Could not safely update ${override_file}"
                    systemctl daemon-reload
                    info "Set ${env_key} in systemd override for ${service}"
                    svc_action restart "$comp" 2>/dev/null || true
                elif [ "$INIT" = openrc ]; then
                    svc_write_env_file "$comp" "$env_key" "$value" \
                        || error "Could not safely update $(svc_env_path "$comp")"
                    info "Set ${env_key} in $(svc_env_path "$comp")"
                    svc_action restart "$comp" 2>/dev/null || true
                else
                    warn "No init manager; cannot persist ${env_key} for ${service}. Set it manually."
                fi

            elif [ "$method" = "docker" ]; then
                compose_file="${DOCKER_DIR}/docker-compose.${comp}.yml"
                if [ ! -f "$compose_file" ]; then
                    error "Compose file not found: $compose_file"
                fi
                docker_set_env_transaction "$comp" "$compose_file" "$env_key" "$value" \
                    || error "Could not safely update ${env_key}; the previous Compose file was restored when possible"
                info "Set ${env_key} in ${compose_file}"
            fi
        done
        return
    fi

    # env — view mode
    [ -z "$MANAGED_COMPONENTS" ] && error "No managed components found."

    echo ""
    cecho "${BOLD}Environment Variables${NC}"
    echo "====================="

    echo ""
    echo "Source: shell"
    shell_vars=$(env | grep '^SERVERBEE_' || true)
    if [ -n "$shell_vars" ]; then
        printf '%s\n' "$shell_vars" | redact_env_lines | sed 's/^/  /'
    else
        echo "  (none)"
    fi

    for entry in $MANAGED_COMPONENTS; do
        comp="${entry%%:*}"
        method="${entry##*:}"
        service="serverbee-${comp}"

        echo ""
        if [ "$method" = "binary" ]; then
            if [ "$INIT" = openrc ]; then
                echo "Source: openrc env file (${service})"
                if [ -f "$(svc_env_path "$comp")" ] && [ -s "$(svc_env_path "$comp")" ]; then
                    redact_env_lines < "$(svc_env_path "$comp")" | sed 's/^/  /'
                else
                    echo "  (none)"
                fi
            else
                echo "Source: systemd override (${service})"
                override_file="/etc/systemd/system/${service}.service.d/override.conf"
                if [ -f "$override_file" ]; then
                    grep "^Environment=" "$override_file" 2>/dev/null | redact_env_lines | sed 's/^Environment=/  /' || echo "  (none)"
                else
                    echo "  (none)"
                fi
                unit_envs=$(systemctl show "$service" --property=Environment --value 2>/dev/null || echo "")
                if [ -n "$unit_envs" ]; then
                    echo "Source: systemd unit (${service})"
                    echo "$unit_envs" | tr ' ' '\n' | redact_env_lines | sed 's/^/  /'
                fi
            fi
        elif [ "$method" = "docker" ]; then
            echo "Source: docker-compose (${service})"
            compose_file="${DOCKER_DIR}/docker-compose.${comp}.yml"
            if [ -f "$compose_file" ]; then
                grep -E '^ *- "?SERVERBEE_' "$compose_file" 2>/dev/null | redact_env_lines | sed 's/^ *- /  /' || echo "  (none)"
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
    local choice
    echo ""
    cecho "${BOLD}$(tr_text manager_title)${NC}"
    echo "================="
    echo ""
    tr_text install_menu
    tr_text uninstall_menu
    tr_text upgrade_menu
    tr_text status_menu
    tr_text service_menu
    tr_text config_menu
    tr_text env_menu
    tr_text domain_menu
    tr_text exit_menu
    echo ""
    printf '%s' "$(tr_text select_menu)"; prompt_read choice
    case "$choice" in
        1) COMMAND="install" ;;
        2) COMMAND="uninstall" ;;
        3) COMMAND="upgrade" ;;
        4) COMMAND="status" ;;
        5) interactive_service_menu ;;
        6) COMMAND="config" ;;
        7) COMMAND="env" ;;
        8) COMMAND="domain"; COMPONENT="setup" ;;
        0) exit 0 ;;
        *) error "Invalid choice: $choice" ;;
    esac
    migrate_legacy_layout
    case "$COMMAND" in
        install|domain) ;;
        *) check_deps ;;
    esac
    run_command
}

interactive_service_menu() {
    local choice
    echo ""
    cecho "${BOLD}$(tr_text svc_title)${NC}"
    echo ""
    tr_text svc_start
    tr_text svc_stop
    tr_text svc_restart
    echo ""
    printf '%s' "$(tr_text svc_select)"; prompt_read choice
    case "$choice" in
        1) COMMAND="start" ;;
        2) COMMAND="stop" ;;
        3) COMMAND="restart" ;;
        *) error "Invalid choice: $choice" ;;
    esac
}

# ─── Command dispatch ─────────────────────────────────────────────────────────
run_command() {
    configure_docker_dir

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
        domain)    cmd_domain ;;
        *) error "Unknown command: $COMMAND" ;;
    esac
}

# ─── Main ─────────────────────────────────────────────────────────────────────
main() {
    local a prev
    # Help and installer-version queries are side-effect free and must work for
    # non-root users. `--version <release>` remains the install/upgrade pin.
    case "${1:-}" in
        --help|-h|help)
            detect_lang
            print_usage
            return 0
            ;;
        version)
            printf 'ServerBee installer %s\n' "$INSTALLER_VERSION"
            return 0
            ;;
    esac
    for a in "$@"; do
        case "$a" in
            --help|-h)
                detect_lang
                case "${1:-}" in
                    server|agent) print_usage install ;;
                    *) print_usage "${1:-}" ;;
                esac
                return 0
                ;;
        esac
    done

    # Elevate first so the rest runs as root (re-execs under sudo/doas).
    require_root "$@"
    detect_init

    # Pre-scan for -y and --lang before any prompt or dependency handling.
    prev=""
    for a in "$@"; do
        case "$a" in
            --yes|-y) YES=true ;;
        esac
        [ "$prev" = "--lang" ] && { LANG_CODE="$a"; normalize_lang; }
        prev="$a"
    done

    # Shorthand: first arg not a known command → prepend "install"
    if [ $# -gt 0 ] && ! is_known_command "$1"; then
        set -- install "$@"
    fi

    if [ $# -eq 0 ]; then
        select_language
        interactive_menu
    else
        COMMAND="$1"; shift
        parse_args "$@"
        validate_parsed_args
        case "$COMMAND" in
            install|domain) select_language ;;
            *) detect_lang ;;
        esac
        migrate_legacy_layout
        case "$COMMAND" in
            install|domain) ;;
            *) check_deps ;;
        esac
        run_command
    fi
}

# Allow sourcing for spot-checks (SERVERBEE_NO_MAIN=1) without running install.
[ -n "${SERVERBEE_NO_MAIN:-}" ] || main "$@"
