#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
SERVERBEE_NO_MAIN=1
export SERVERBEE_NO_MAIN
# shellcheck source=deploy/install.sh
. "${SCRIPT_DIR}/install.sh"
UPGRADE_HEALTH_ATTEMPTS=3
UPGRADE_STABILITY_CHECKS=2
DOCKER_UPGRADE_HEALTH_ATTEMPTS=3
DOCKER_UPGRADE_STABILITY_CHECKS=2

TEST_ROOT=$(mktemp -d)
trap 'rm -rf "$TEST_ROOT"' EXIT HUP INT TERM

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

assert_eq() {
    [ "$1" = "$2" ] || fail "expected '$1' to equal '$2'"
}

write_probe_binary() {
    local path version
    path="$1"; version="$2"
    printf '%s\n' \
        '#!/bin/sh' \
        'if [ "${1:-}" = "--serverbee-upgrade-probe" ]; then' \
        "    printf '%s\\n' '${version}'" \
        '    exit 0' \
        'fi' \
        'exit 0' > "$path"
    chmod +x "$path"
}

setup_case() {
    local name
    name="$1"
    CASE_DIR="${TEST_ROOT}/${name}"
    INSTALL_DIR="${CASE_DIR}/bin"
    ACTION_LOG="${CASE_DIR}/actions.log"
    TEST_CANDIDATE="${CASE_DIR}/candidate"
    TEST_ACTIVE_MODE=active
    TEST_FAIL_CANDIDATE_START=false
    TEST_RESTART_MODE=stable
    TEST_SERVICE_STATE=active
    TEST_COMPONENT=agent
    TEST_SERVER_HEALTH_MODE=healthy
    mkdir -p "$INSTALL_DIR"
    : > "$ACTION_LOG"
    write_probe_binary "${INSTALL_DIR}/serverbee-agent" "1.0.0-alpha.12"
    write_probe_binary "$TEST_CANDIDATE" "1.0.0-beta.1"
}

detect_os() { echo linux; }
detect_arch() { echo amd64; }
download_verified() { cp "$TEST_CANDIDATE" "$2"; }
svc_logs_tail() { echo "test service log"; }
sleep() { :; }

svc_restart_count() {
    local installed_version count
    installed_version=$("${INSTALL_DIR}/serverbee-${TEST_COMPONENT}" --serverbee-upgrade-probe)
    if [ "$TEST_RESTART_MODE" = candidate-increments ] && [ "$installed_version" = "1.0.0-beta.1" ]; then
        count=$(cat "${CASE_DIR}/restart-count" 2>/dev/null || echo 0)
        echo $((count + 1)) > "${CASE_DIR}/restart-count"
        echo "$count"
    else
        echo 0
    fi
}

svc_action() {
    local action component installed_version
    action="$1"; component="$2"
    printf '%s %s\n' "$action" "$component" >> "$ACTION_LOG"
    case "$action" in
        stop) TEST_SERVICE_STATE=inactive ;;
        start)
            if [ "$TEST_FAIL_CANDIDATE_START" = true ]; then
                installed_version=$("${INSTALL_DIR}/serverbee-${component}" --serverbee-upgrade-probe)
                [ "$installed_version" != "1.0.0-beta.1" ] || return 1
            fi
            TEST_SERVICE_STATE=active
            ;;
        restart) TEST_SERVICE_STATE=active ;;
    esac
}

svc_is_active() {
    local installed_version
    if [ "$TEST_SERVICE_STATE" != active ]; then
        echo inactive
    elif [ "$TEST_ACTIVE_MODE" = candidate-inactive ]; then
        installed_version=$("${INSTALL_DIR}/serverbee-${TEST_COMPONENT}" --serverbee-upgrade-probe)
        [ "$installed_version" = "1.0.0-beta.1" ] && echo inactive || echo active
    elif [ "$TEST_ACTIVE_MODE" = active ]; then
        echo active
    else
        echo inactive
    fi
}

svc_health_check() {
    local component installed_version
    component="$1"
    [ "$component" = server ] || return 0
    installed_version=$("${INSTALL_DIR}/serverbee-server" --serverbee-upgrade-probe)
    [ "$TEST_SERVER_HEALTH_MODE" != candidate-unhealthy ] \
        || [ "$installed_version" != "1.0.0-beta.1" ]
}

svc_reset_failed() {
    printf 'reset-failed %s\n' "$1" >> "$ACTION_LOG"
}

test_successful_upgrade() {
    setup_case success
    upgrade_binary agent v1.0.0-beta.1 >/dev/null

    assert_eq "$("${INSTALL_DIR}/serverbee-agent" --serverbee-upgrade-probe)" "1.0.0-beta.1"
    [ ! -e "${INSTALL_DIR}/serverbee-agent.rollback" ] || fail "successful upgrade left a rollback backup"
    assert_eq "$(cat "$ACTION_LOG")" "stop agent
start agent"
}

test_probe_mismatch_preserves_current_binary() {
    setup_case probe-mismatch
    write_probe_binary "$TEST_CANDIDATE" "1.0.0-beta.2"

    if (upgrade_binary agent v1.0.0-beta.1) >/dev/null 2>&1; then
        fail "version mismatch unexpectedly succeeded"
    fi
    assert_eq "$("${INSTALL_DIR}/serverbee-agent" --serverbee-upgrade-probe)" "1.0.0-alpha.12"
    [ ! -s "$ACTION_LOG" ] || fail "probe failure stopped the running service"
}

test_server_candidate_probe_requires_exact_version() {
    setup_case server-probe

    probe_upgrade_candidate server "$TEST_CANDIDATE" v1.0.0-beta.1 \
        || fail "matching Server candidate version was rejected"
    write_probe_binary "$TEST_CANDIDATE" "1.0.0-beta.2"
    if probe_upgrade_candidate server "$TEST_CANDIDATE" v1.0.0-beta.1; then
        fail "mismatched Server candidate version was accepted"
    fi
}

test_start_failure_rolls_back() {
    setup_case start-failure
    TEST_FAIL_CANDIDATE_START=true

    if (upgrade_binary agent v1.0.0-beta.1) >/dev/null 2>&1; then
        fail "candidate start failure unexpectedly succeeded"
    fi
    assert_eq "$("${INSTALL_DIR}/serverbee-agent" --serverbee-upgrade-probe)" "1.0.0-alpha.12"
    [ ! -e "${INSTALL_DIR}/serverbee-agent.rollback" ] || fail "rollback backup was not consumed"
    assert_eq "$(cat "$ACTION_LOG")" "stop agent
start agent
stop agent
reset-failed agent
start agent"
}

test_health_failure_rolls_back() {
    setup_case health-failure
    TEST_ACTIVE_MODE='candidate-inactive'

    if (upgrade_binary agent v1.0.0-beta.1) >/dev/null 2>&1; then
        fail "unhealthy candidate unexpectedly succeeded"
    fi
    assert_eq "$("${INSTALL_DIR}/serverbee-agent" --serverbee-upgrade-probe)" "1.0.0-alpha.12"
    [ ! -e "${INSTALL_DIR}/serverbee-agent.rollback" ] || fail "rollback backup was not consumed"
    assert_eq "$(cat "$ACTION_LOG")" "stop agent
start agent
stop agent
reset-failed agent
start agent"
}

test_server_http_health_failure_rolls_back() {
    setup_case server-http-health-failure
    TEST_COMPONENT=server
    TEST_SERVER_HEALTH_MODE='candidate-unhealthy'
    mv "${INSTALL_DIR}/serverbee-agent" "${INSTALL_DIR}/serverbee-server"

    if (upgrade_binary server v1.0.0-beta.1) >/dev/null 2>&1; then
        fail "Server candidate without a healthy HTTP endpoint unexpectedly succeeded"
    fi
    assert_eq "$("${INSTALL_DIR}/serverbee-server" --serverbee-upgrade-probe)" "1.0.0-alpha.12"
    [ ! -e "${INSTALL_DIR}/serverbee-server.rollback" ] || fail "rollback backup was not consumed"
    assert_eq "$(cat "$ACTION_LOG")" "stop server
start server
stop server
reset-failed server
start server"
}

test_server_health_url_uses_configured_listener() {
    setup_case server-health-url
    CONFIG_DIR="${CASE_DIR}/etc"
    mkdir -p "$CONFIG_DIR"
    printf '%s\n' '[server]' 'listen = "0.0.0.0:9743"' > "${CONFIG_DIR}/server.toml"
    assert_eq "$(server_health_url)" "http://127.0.0.1:9743/healthz"

    printf '%s\n' '[server]' 'listen = "[::]:9744"' > "${CONFIG_DIR}/server.toml"
    assert_eq "$(server_health_url)" "http://[::1]:9744/healthz"
}

test_restart_during_stability_window_rolls_back() {
    setup_case restart-detected
    TEST_RESTART_MODE='candidate-increments'

    if (upgrade_binary agent v1.0.0-beta.1) >/dev/null 2>&1; then
        fail "restarting candidate unexpectedly succeeded"
    fi
    assert_eq "$("${INSTALL_DIR}/serverbee-agent" --serverbee-upgrade-probe)" "1.0.0-alpha.12"
    [ ! -e "${INSTALL_DIR}/serverbee-agent.rollback" ] || fail "rollback backup was not consumed"
}

test_stale_backup_blocks_upgrade() {
    setup_case stale-backup
    : > "${INSTALL_DIR}/serverbee-agent.rollback"

    if (upgrade_binary agent v1.0.0-beta.1) >/dev/null 2>&1; then
        fail "upgrade unexpectedly overwrote a stale rollback backup"
    fi
    assert_eq "$("${INSTALL_DIR}/serverbee-agent" --serverbee-upgrade-probe)" "1.0.0-alpha.12"
    [ ! -s "$ACTION_LOG" ] || fail "stale backup check stopped the running service"
}

setup_docker_case() {
    local name
    name="$1"
    CASE_DIR="${TEST_ROOT}/${name}"
    DOCKER_DIR="$CASE_DIR"
    ACTION_LOG="${CASE_DIR}/actions.log"
    TEST_DOCKER_PULL_FAIL=false
    TEST_DOCKER_UP_FAIL=false
    TEST_DOCKER_STATE_MODE=healthy
    TEST_DOCKER_RESTART_MODE=stable
    mkdir -p "$DOCKER_DIR"
    : > "$ACTION_LOG"
    printf '%s\n' \
        'services:' \
        '  serverbee-agent:' \
        '    image: ghcr.io/zingerlittlebee/serverbee-agent:1.0.0-alpha.12' \
        '    container_name: serverbee-agent' > "${DOCKER_DIR}/docker-compose.agent.yml"
}

compose_image_tag() {
    sed -n 's|.*image: ghcr.io/zingerlittlebee/serverbee-agent:||p' "${DOCKER_DIR}/docker-compose.agent.yml"
}

docker() {
    local compose_file action tag
    [ "$1" = compose ] || fail "unexpected docker command: $*"
    shift
    [ "$1" = -f ] || fail "docker compose command omitted -f"
    compose_file="$2"
    shift 2
    action="$1"
    tag=$(sed -n 's|.*image: ghcr.io/zingerlittlebee/serverbee-agent:||p' "$compose_file")
    case "$action" in
        pull)
            printf 'pull %s\n' "$tag" >> "$ACTION_LOG"
            [ "$TEST_DOCKER_PULL_FAIL" != true ] || return 1
            ;;
        up)
            printf 'up %s\n' "$tag" >> "$ACTION_LOG"
            [ "$TEST_DOCKER_UP_FAIL" != true ] || [ "$tag" != "1.0.0-beta.1" ] || return 1
            ;;
        logs) echo "test Docker log" ;;
        *) fail "unexpected docker compose action: $action" ;;
    esac
}

docker_container_state() {
    if [ "$TEST_DOCKER_STATE_MODE" = candidate-unhealthy ] && [ "$(compose_image_tag)" = "1.0.0-beta.1" ]; then
        echo restarting
    else
        echo running
    fi
}

docker_restart_count() {
    local count
    if [ "$TEST_DOCKER_RESTART_MODE" = candidate-increments ] && [ "$(compose_image_tag)" = "1.0.0-beta.1" ]; then
        count=$(cat "${CASE_DIR}/docker-restart-count" 2>/dev/null || echo 0)
        echo $((count + 1)) > "${CASE_DIR}/docker-restart-count"
        echo "$count"
    else
        echo 0
    fi
}

test_successful_docker_upgrade() {
    setup_docker_case docker-success
    upgrade_docker agent v1.0.0-beta.1 >/dev/null

    assert_eq "$(compose_image_tag)" "1.0.0-beta.1"
    [ ! -e "${DOCKER_DIR}/docker-compose.agent.yml.rollback" ] || fail "successful Docker upgrade left a backup"
    assert_eq "$(cat "$ACTION_LOG")" "pull 1.0.0-beta.1
up 1.0.0-beta.1"
}

test_docker_pull_failure_restores_compose() {
    setup_docker_case docker-pull-failure
    TEST_DOCKER_PULL_FAIL=true

    if (upgrade_docker agent v1.0.0-beta.1) >/dev/null 2>&1; then
        fail "failed Docker pull unexpectedly succeeded"
    fi
    assert_eq "$(compose_image_tag)" "1.0.0-alpha.12"
    [ ! -e "${DOCKER_DIR}/docker-compose.agent.yml.rollback" ] || fail "pull failure left a backup"
    assert_eq "$(cat "$ACTION_LOG")" "pull 1.0.0-beta.1"
}

test_docker_start_failure_rolls_back() {
    setup_docker_case docker-start-failure
    TEST_DOCKER_UP_FAIL=true

    if (upgrade_docker agent v1.0.0-beta.1) >/dev/null 2>&1; then
        fail "failed Docker start unexpectedly succeeded"
    fi
    assert_eq "$(compose_image_tag)" "1.0.0-alpha.12"
    [ ! -e "${DOCKER_DIR}/docker-compose.agent.yml.rollback" ] || fail "Docker rollback left a backup"
    assert_eq "$(cat "$ACTION_LOG")" "pull 1.0.0-beta.1
up 1.0.0-beta.1
up 1.0.0-alpha.12"
}

test_unhealthy_docker_upgrade_rolls_back() {
    setup_docker_case docker-unhealthy
    TEST_DOCKER_STATE_MODE='candidate-unhealthy'

    if (upgrade_docker agent v1.0.0-beta.1) >/dev/null 2>&1; then
        fail "unhealthy Docker candidate unexpectedly succeeded"
    fi
    assert_eq "$(compose_image_tag)" "1.0.0-alpha.12"
    [ ! -e "${DOCKER_DIR}/docker-compose.agent.yml.rollback" ] || fail "Docker rollback left a backup"
    assert_eq "$(cat "$ACTION_LOG")" "pull 1.0.0-beta.1
up 1.0.0-beta.1
up 1.0.0-alpha.12"
}

test_restarting_docker_upgrade_rolls_back() {
    setup_docker_case docker-restarting
    TEST_DOCKER_RESTART_MODE='candidate-increments'

    if (upgrade_docker agent v1.0.0-beta.1) >/dev/null 2>&1; then
        fail "restarting Docker candidate unexpectedly succeeded"
    fi
    assert_eq "$(compose_image_tag)" "1.0.0-alpha.12"
    [ ! -e "${DOCKER_DIR}/docker-compose.agent.yml.rollback" ] || fail "Docker rollback left a backup"
}

test_stale_docker_backup_blocks_upgrade() {
    setup_docker_case docker-stale-backup
    : > "${DOCKER_DIR}/docker-compose.agent.yml.rollback"

    if (upgrade_docker agent v1.0.0-beta.1) >/dev/null 2>&1; then
        fail "Docker upgrade unexpectedly overwrote a stale backup"
    fi
    assert_eq "$(compose_image_tag)" "1.0.0-alpha.12"
    [ ! -s "$ACTION_LOG" ] || fail "stale Docker backup changed the deployment"
}

test_docker_server_compose_mounts_generated_config() (
    setup_case docker-server-compose
    DEFAULT_DOCKER_DIR="$CASE_DIR"
    DOCKER_DIR="$CASE_DIR"
    CONFIG_DIR="${CASE_DIR}/etc"
    REQUESTED_VERSION="v1.0.0-beta.1"
    RESOLVED_VERSION=""
    mkdir -p "$CONFIG_DIR"

    docker_is_snap() { return 1; }
    check_docker() { :; }
    check_unmanaged_container() { :; }
    docker() { printf '%s\n' "$*" >> "$ACTION_LOG"; }
    install_cli() { :; }
    meta_write() { :; }
    verify_server_install_or_exit() { :; }
    print_server_result() { :; }

    install_docker_server >/dev/null

    compose_file="${DOCKER_DIR}/docker-compose.server.yml"
    grep -Fq "${CONFIG_DIR}/server.toml:/etc/serverbee/server.toml:ro" "$compose_file" \
        || fail "generated Docker server config is not mounted"
    grep -Fq 'http://127.0.0.1:9527/healthz' "$compose_file" \
        || fail "generated Docker server health check is not IPv4-explicit"
    grep -Fq 'MALLOC_ARENA_MAX=2' "$compose_file" \
        || fail "generated Docker server omits allocator tuning"
)

test_docker_agent_custom_caps_keep_executable_and_secure_config() (
    setup_case docker-agent-custom-caps
    DEFAULT_DOCKER_DIR="$CASE_DIR"
    DOCKER_DIR="$CASE_DIR"
    CONFIG_DIR="${CASE_DIR}/etc"
    REQUESTED_VERSION="v1.0.0-beta.1"
    RESOLVED_VERSION=""
    SERVER_URL="https://monitor.example.com"
    ENROLLMENT_CODE="test-enrollment-code"
    AGENT_CAPS_USER_SPECIFIED=true
    AGENT_CAPS_SELECTED=""

    docker_is_snap() { return 1; }
    check_docker() { :; }
    check_unmanaged_container() { :; }
    docker() { printf '%s\n' "$*" >> "$ACTION_LOG"; }
    install_cli() { :; }
    meta_write() { :; }
    verify_agent_install_or_exit() { :; }
    print_agent_result() { :; }

    install_docker_agent >/dev/null

    compose_file="${DOCKER_DIR}/docker-compose.agent.yml"
    first_command=$(awk '/^    command:$/ { getline; print; exit }' "$compose_file")
    assert_eq "$first_command" "      - serverbee-agent"

    config_mode=$(LC_ALL=C ls -l "${CONFIG_DIR}/agent.toml" | cut -c 1-10)
    assert_eq "$config_mode" "-rw-------"
    grep -Fq 'up -d --force-recreate' "$ACTION_LOG" \
        || fail "Docker Agent install did not force a fresh container for the new config"
)

test_docker_agent_start_failure_cleans_generated_files() (
    setup_case docker-agent-install-failure
    DEFAULT_DOCKER_DIR="$CASE_DIR"
    DOCKER_DIR="$CASE_DIR"
    CONFIG_DIR="${CASE_DIR}/etc"
    REQUESTED_VERSION="v1.0.0-beta.1"
    RESOLVED_VERSION=""
    SERVER_URL="https://monitor.example.com"
    ENROLLMENT_CODE="test-enrollment-code"
    AGENT_CAPS_USER_SPECIFIED=true
    AGENT_CAPS_SELECTED=""

    docker_is_snap() { return 1; }
    check_docker() { :; }
    check_unmanaged_container() { :; }
    docker() {
        shift
        [ "$1" = -f ] || fail "docker compose command omitted -f"
        shift 2
        action="$1"
        printf '%s\n' "$action" >> "$ACTION_LOG"
        [ "$action" != up ]
    }
    install_cli() { :; }
    meta_write() { :; }
    verify_agent_install_or_exit() { :; }
    print_agent_result() { :; }

    if (install_docker_agent) >/dev/null 2>&1; then
        fail "failed Docker Agent start unexpectedly succeeded"
    fi

    [ ! -e "${DOCKER_DIR}/docker-compose.agent.yml" ] \
        || fail "failed Docker Agent install left its generated Compose file"
    [ ! -e "${CONFIG_DIR}/agent.toml" ] \
        || fail "failed Docker Agent install left its generated enrollment config"
    assert_eq "$(cat "$ACTION_LOG")" "config
up
down"
    grep -qx 'down' "$ACTION_LOG" \
        || fail "failed Docker Agent install did not tear down its partial container"
)

test_docker_agent_config_write_failure_cleans_partial_file() (
    setup_case docker-agent-config-write-failure
    DEFAULT_DOCKER_DIR="$CASE_DIR"
    DOCKER_DIR="$CASE_DIR"
    CONFIG_DIR="${CASE_DIR}/etc"
    REQUESTED_VERSION="v1.0.0-beta.1"
    RESOLVED_VERSION=""
    SERVER_URL="https://monitor.example.com"
    ENROLLMENT_CODE="test-enrollment-code"

    docker_is_snap() { return 1; }
    check_docker() { :; }
    check_unmanaged_container() { :; }
    docker() { :; }
    cat() { return 1; }

    if (install_docker_agent) >/dev/null 2>&1; then
        fail "failed Agent config write unexpectedly succeeded"
    fi

    [ ! -e "${CONFIG_DIR}/agent.toml" ] \
        || fail "failed Agent config write left a partial enrollment config"
)

test_docker_agent_start_failure_restores_existing_files() (
    setup_case docker-agent-existing-files
    DEFAULT_DOCKER_DIR="$CASE_DIR"
    DOCKER_DIR="$CASE_DIR"
    CONFIG_DIR="${CASE_DIR}/etc"
    REQUESTED_VERSION="v1.0.0-beta.1"
    RESOLVED_VERSION=""
    SERVER_URL="https://new.example.com"
    ENROLLMENT_CODE="new-enrollment-code"
    AGENT_CAPS_USER_SPECIFIED=true
    AGENT_CAPS_SELECTED=""
    mkdir -p "$CONFIG_DIR"
    printf '%s\n' \
        'server_url = "https://old.example.com"' \
        'token = "old-run-token"' > "${CONFIG_DIR}/agent.toml"
    chmod 600 "${CONFIG_DIR}/agent.toml"
    printf '%s\n' \
        'services:' \
        '  preserved:' \
        '    image: example.invalid/preserved:1' > "${DOCKER_DIR}/docker-compose.agent.yml"
    cp "${CONFIG_DIR}/agent.toml" "${CASE_DIR}/expected-agent.toml"
    cp "${DOCKER_DIR}/docker-compose.agent.yml" "${CASE_DIR}/expected-compose.yml"
    up_calls=0

    docker_is_snap() { return 1; }
    check_docker() { :; }
    check_unmanaged_container() { :; }
    docker() {
        shift
        [ "$1" = -f ] || fail "docker compose command omitted -f"
        shift 2
        action="$1"
        printf '%s\n' "$action" >> "$ACTION_LOG"
        if [ "$action" = up ]; then
            up_calls=$((up_calls + 1))
            if [ "$up_calls" -eq 1 ]; then
                return 1
            fi
            cmp -s "${CASE_DIR}/expected-agent.toml" "${CONFIG_DIR}/agent.toml" \
                || fail "rollback restarted Agent before restoring its config"
            cmp -s "${CASE_DIR}/expected-compose.yml" "${DOCKER_DIR}/docker-compose.agent.yml" \
                || fail "rollback restarted Agent before restoring its Compose file"
        fi
    }
    install_cli() { :; }
    meta_write() { :; }
    verify_agent_install_or_exit() { :; }
    print_agent_result() { :; }

    if (install_docker_agent) >/dev/null 2>&1; then
        fail "failed Docker Agent reinstall unexpectedly succeeded"
    fi

    cmp -s "${CASE_DIR}/expected-agent.toml" "${CONFIG_DIR}/agent.toml" \
        || fail "failed Docker Agent reinstall did not restore the existing config"
    cmp -s "${CASE_DIR}/expected-compose.yml" "${DOCKER_DIR}/docker-compose.agent.yml" \
        || fail "failed Docker Agent reinstall did not restore the existing Compose file"
    [ ! -e "${CONFIG_DIR}/agent.toml.install-rollback" ] \
        || fail "failed Docker Agent reinstall left a config rollback file"
    [ ! -e "${DOCKER_DIR}/docker-compose.agent.yml.install-rollback" ] \
        || fail "failed Docker Agent reinstall left a Compose rollback file"
    assert_eq "$(cat "$ACTION_LOG")" "config
up
up"
)

test_docker_agent_stale_rollback_symlinks_block_install() (
    for backup_kind in config compose; do
        setup_case "docker-agent-stale-${backup_kind}-symlink"
        DEFAULT_DOCKER_DIR="$CASE_DIR"
        DOCKER_DIR="$CASE_DIR"
        CONFIG_DIR="${CASE_DIR}/etc"
        REQUESTED_VERSION="v1.0.0-beta.1"
        RESOLVED_VERSION=""
        SERVER_URL="https://monitor.example.com"
        ENROLLMENT_CODE="test-enrollment-code"
        AGENT_CAPS_USER_SPECIFIED=true
        AGENT_CAPS_SELECTED=""
        mkdir -p "$CONFIG_DIR"
        if [ "$backup_kind" = config ]; then
            backup_path="${CONFIG_DIR}/agent.toml.install-rollback"
        else
            backup_path="${DOCKER_DIR}/docker-compose.agent.yml.install-rollback"
        fi
        ln -s "${CASE_DIR}/missing-${backup_kind}-backup" "$backup_path"

        docker_is_snap() { return 1; }
        check_docker() { :; }
        check_unmanaged_container() { :; }
        docker() { :; }
        install_cli() { :; }
        meta_write() { :; }
        verify_agent_install_or_exit() { :; }
        print_agent_result() { :; }

        if (install_docker_agent) >/dev/null 2>&1; then
            fail "Docker Agent install accepted a stale ${backup_kind} rollback symlink"
        fi
        [ -L "$backup_path" ] \
            || fail "Docker Agent install modified a stale ${backup_kind} rollback symlink"
    done
)

test_non_tty_input_does_not_prompt() (
    if command -v setsid >/dev/null 2>&1; then
        setsid sh -c '
            SERVERBEE_NO_MAIN=1
            export SERVERBEE_NO_MAIN
            . "$1"
            if has_prompt_input </dev/null; then
                exit 10
            fi
            if (prompt_read answer </dev/null) >/dev/null 2>&1; then
                exit 11
            fi
        ' sh "${SCRIPT_DIR}/install.sh" \
            || fail "detached process was mistaken for an available controlling terminal"
    elif ! prompt_tty_available; then
        if has_prompt_input </dev/null; then
            fail "non-interactive input was mistaken for an available controlling terminal"
        fi
        if (prompt_read answer </dev/null) >/dev/null 2>&1; then
            fail "prompt_read accepted input without an available controlling terminal"
        fi
    fi
)

curl() {
    case "$*" in
        *'/releases?per_page=100'*)
            if [ "$TEST_RELEASE_MODE" = large ]; then
                printf '%s\n' '[' \
                    '  {' \
                    '    "tag_name": "v1.0.0-beta.1",' \
                    '    "draft": false' \
                    '  },'
                i=0
                while [ "$i" -lt 3000 ]; do
                    printf '%s\n' \
                        '  {' \
                        "    \"tag_name\": \"v0.0.0-draft.${i}\"," \
                        '    "draft": true' \
                        '  },'
                    i=$((i + 1))
                done
                printf '%s\n' \
                    '  {' \
                    '    "tag_name": "v0.0.0-draft.final",' \
                    '    "draft": true' \
                    '  }' \
                    ']'
                : > "${TEST_ROOT}/release-fetch-complete"
            elif [ "$TEST_RELEASE_MODE" = prerelease-only ]; then
                cat <<'JSON'
[
  {
    "tag_name": "v1.0.0-beta.1",
    "draft": false,
    "prerelease": false
  }
]
JSON
            else
                cat <<'JSON'
[
  {
    "tag_name": "v1.0.0-beta.1",
    "draft": false,
    "prerelease": false
  },
  {
    "tag_name": "v0.9.0",
    "draft": false,
    "prerelease": false
  }
]
JSON
            fi
            ;;
        *) fail "unexpected curl request: $*" ;;
    esac
}

test_stable_channel_ignores_misclassified_prerelease() {
    TEST_RELEASE_MODE=stable
    RELEASE_CHANNEL=stable
    RESOLVED_VERSION=""
    REQUESTED_VERSION=""
    assert_eq "$(get_latest_version)" "v0.9.0"
}

test_stable_channel_fails_without_stable_release() {
    TEST_RELEASE_MODE=prerelease-only
    RELEASE_CHANNEL=stable
    RESOLVED_VERSION=""
    REQUESTED_VERSION=""
    if (get_latest_version) >/dev/null 2>&1; then
        fail "stable channel accepted a prerelease"
    fi
}

test_beta_channel_finds_semver_prerelease_when_metadata_is_wrong() {
    TEST_RELEASE_MODE=stable
    RELEASE_CHANNEL=beta
    RESOLVED_VERSION=""
    REQUESTED_VERSION=""
    assert_eq "$(get_latest_version)" "v1.0.0-beta.1"
}

test_auto_channel_prefers_stable_release() {
    TEST_RELEASE_MODE=stable
    RELEASE_CHANNEL=auto
    RESOLVED_VERSION=""
    REQUESTED_VERSION=""
    assert_eq "$(get_latest_version)" "v0.9.0"
}

test_auto_channel_falls_back_to_prerelease() {
    TEST_RELEASE_MODE=prerelease-only
    RELEASE_CHANNEL=auto
    RESOLVED_VERSION=""
    REQUESTED_VERSION=""
    assert_eq "$(get_latest_version)" "v1.0.0-beta.1"
}

test_release_selection_consumes_the_full_response() {
    TEST_RELEASE_MODE=large
    RELEASE_CHANNEL=beta
    RESOLVED_VERSION=""
    REQUESTED_VERSION=""
    assert_eq "$(get_latest_version)" "v1.0.0-beta.1"
    [ -f "${TEST_ROOT}/release-fetch-complete" ] \
        || fail "release selection stopped reading before curl completed"
}

test_install_metadata_persists_upgrade_channel() {
    setup_case metadata-channel
    CONFIG_DIR="${CASE_DIR}/etc"
    META_FILE="${CONFIG_DIR}/install.json"
    RELEASE_CHANNEL=beta
    RELEASE_CHANNEL_USER_SPECIFIED=false
    meta_write agent binary v1.0.0-beta.1
    assert_eq "$(meta_read agent channel)" "beta"
    RELEASE_CHANNEL=auto
    prepare_upgrade_release agent
    assert_eq "$RELEASE_CHANNEL" "beta"
}

test_current_upgrade_persists_explicit_channel() (
    setup_case current-channel
    CONFIG_DIR="${CASE_DIR}/etc"
    META_FILE="${CONFIG_DIR}/install.json"
    RELEASE_CHANNEL=auto
    meta_write agent binary v1.0.0-beta.1
    RELEASE_CHANNEL=beta
    refresh_cli_from_release() { :; }
    upgrade_component agent v1.0.0-beta.1 >/dev/null
    assert_eq "$(meta_read agent channel)" "beta"
)

test_toml_set_roundtrips_special_characters_and_preserves_mode() (
    setup_case toml-special-characters
    config_file="${CASE_DIR}/server.toml"
    printf '%s\n' \
        '[oauth.github]' \
        'client_secret = "old"' > "$config_file"
    chmod 600 "$config_file"

    toml_set "$config_file" 'oauth.github.client_secret' 'a|b&c\q"d e'

    grep -Fqx 'client_secret = "a|b&c\\q\"d e"' "$config_file" \
        || fail "TOML special characters did not round-trip safely"
    mode=$(LC_ALL=C ls -l "$config_file" | cut -c 1-10)
    assert_eq "$mode" '-rw-------'
)

test_compose_env_set_adds_environment_and_is_idempotent() (
    setup_case compose-env-add
    compose_file="${CASE_DIR}/docker-compose.agent.yml"
    printf '%s\n' \
        'services:' \
        '  serverbee-agent:' \
        '    image: example.invalid/agent:1' \
        '    restart: unless-stopped' > "$compose_file"

    compose_set_env "$compose_file" agent SERVERBEE_LOG__LEVEL 'debug|a&b\c" d'
    compose_set_env "$compose_file" agent SERVERBEE_LOG__LEVEL 'warn|x&y\z" q'

    count=$(grep -c 'SERVERBEE_LOG__LEVEL=' "$compose_file")
    assert_eq "$count" 1
    grep -Fq 'SERVERBEE_LOG__LEVEL=warn|x&y\\z\" q' "$compose_file" \
        || fail "Compose env value did not round-trip safely"
    grep -Fq '    environment:' "$compose_file" \
        || fail "Compose environment block was not created"
)

test_compose_env_set_collapses_existing_duplicates() (
    setup_case compose-env-duplicates
    compose_file="${CASE_DIR}/docker-compose.server.yml"
    printf '%s\n' \
        'services:' \
        '  serverbee-server:' \
        '    environment:' \
        '      - SERVERBEE_AUTH__SECURE_COOKIE=false' \
        '      - "SERVERBEE_AUTH__SECURE_COOKIE=false"' \
        '      - MALLOC_ARENA_MAX=2' \
        '    image: example.invalid/server:1' > "$compose_file"

    compose_set_env "$compose_file" server SERVERBEE_AUTH__SECURE_COOKIE true

    count=$(grep -c 'SERVERBEE_AUTH__SECURE_COOKIE=' "$compose_file")
    assert_eq "$count" 1
    grep -Fq 'SERVERBEE_AUTH__SECURE_COOKIE=true' "$compose_file" \
        || fail "Compose env duplicate collapse kept the old value"
)

test_docker_env_transaction_validates_and_restores_on_failure() (
    setup_case docker-env-transaction
    compose_file="${CASE_DIR}/docker-compose.agent.yml"
    printf '%s\n' \
        'services:' \
        '  serverbee-agent:' \
        '    image: example.invalid/agent:1' > "$compose_file"
    cp "$compose_file" "${CASE_DIR}/original.yml"
    TEST_COMPOSE_CONFIG_FAIL=false

    docker() {
        shift
        [ "$1" = -f ] || fail "docker compose command omitted -f"
        shift 2
        action="$1"
        printf '%s\n' "$action" >> "$ACTION_LOG"
        [ "$action" != config ] || [ "$TEST_COMPOSE_CONFIG_FAIL" != true ]
    }

    docker_set_env_transaction agent "$compose_file" SERVERBEE_LOG__LEVEL debug \
        || fail "valid Docker env transaction failed"
    assert_eq "$(cat "$ACTION_LOG")" "config
up"
    [ ! -e "${compose_file}.env-rollback" ] || fail "successful env update left a rollback file"

    cp "${CASE_DIR}/original.yml" "$compose_file"
    : > "$ACTION_LOG"
    TEST_COMPOSE_CONFIG_FAIL=true
    if docker_set_env_transaction agent "$compose_file" SERVERBEE_LOG__LEVEL broken; then
        fail "invalid Compose transaction unexpectedly succeeded"
    fi
    cmp -s "${CASE_DIR}/original.yml" "$compose_file" \
        || fail "failed Compose validation did not restore the original file"
    assert_eq "$(cat "$ACTION_LOG")" config
)

test_openrc_env_roundtrips_shell_metacharacters() (
    setup_case openrc-env-special
    env_file="${CASE_DIR}/agent.env"
    : > "$env_file"
    chmod 600 "$env_file"
    value='a|b&c\q"d e$HOME`literal`'

    openrc_set_env "$env_file" SERVERBEE_TOKEN "$value"
    actual=$(sh -c '. "$1"; printf "%s" "$SERVERBEE_TOKEN"' sh "$env_file")
    assert_eq "$actual" "$value"
    mode=$(LC_ALL=C ls -l "$env_file" | cut -c 1-10)
    assert_eq "$mode" '-rw-------'
)

test_systemd_env_escapes_unit_syntax_and_is_idempotent() (
    setup_case systemd-env-special
    override_file="${CASE_DIR}/override.conf"
    printf '%s\n' '[Service]' > "$override_file"
    chmod 600 "$override_file"

    systemd_set_env "$override_file" SERVERBEE_TOKEN 'a\b"c %n'
    systemd_set_env "$override_file" SERVERBEE_TOKEN 'final\value" %%'

    count=$(grep -c 'SERVERBEE_TOKEN=' "$override_file")
    assert_eq "$count" 1
    grep -Fqx 'Environment="SERVERBEE_TOKEN=final\\value\" %%%%"' "$override_file" \
        || fail "systemd env value was not escaped for unit syntax"
)

test_installer_version_matches_workspace_version() (
    workspace_version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' "${SCRIPT_DIR}/../Cargo.toml" | head -n1)
    assert_eq "$INSTALLER_VERSION" "$workspace_version"
)

test_value_validation_rejects_controls_without_file_changes() (
    setup_case invalid-config-value
    config_file="${CASE_DIR}/agent.toml"
    printf '%s\n' 'token = "old"' > "$config_file"
    cp "$config_file" "${CASE_DIR}/expected.toml"
    invalid_value=$(printf 'line1\nline2')

    if toml_set "$config_file" token "$invalid_value" >/dev/null 2>&1; then
        fail "TOML setter accepted a newline"
    fi
    cmp -s "${CASE_DIR}/expected.toml" "$config_file" \
        || fail "rejected TOML value changed the file"
)

test_agent_install_acceptance_states() (
    INSTALL_AGENT_ATTEMPTS=3
    : > "${TEST_ROOT}/agent-log-reads"
    agent_install_logs() {
        reads=$(wc -l < "${TEST_ROOT}/agent-log-reads")
        printf '.\n' >> "${TEST_ROOT}/agent-log-reads"
        [ "$reads" -lt 1 ] || echo 'Welcome from server test-server, interval=3s'
    }
    wait_for_agent_install || fail "Agent Welcome log was not accepted"

    agent_install_logs() { echo 'Permanent registration failure: HTTP 401 Unauthorized'; }
    set +e
    wait_for_agent_install
    rc=$?
    set -e
    assert_eq "$rc" 78

    agent_install_logs() { echo 'connection timed out'; }
    set +e
    wait_for_agent_install
    rc=$?
    set -e
    assert_eq "$rc" 75
)

test_agent_install_accepts_stable_endpoint_connection_without_info_logs() (
    INSTALL_AGENT_ATTEMPTS=3
    agent_install_logs() { :; }
    agent_install_exit_status() { :; }
    agent_connection_established() { echo '192.0.2.4:41000|203.0.113.10:443'; }

    wait_for_agent_install || fail "stable Agent endpoint connection was not accepted"
    assert_eq "$AGENT_INSTALL_PROOF" connection
)

test_agent_connection_proof_matches_pid_address_and_port() (
    METHOD=binary
    INIT=systemd
    SERVER_URL='https://monitor.example.com/path'
    agent_install_pid() { echo 4242; }
    getent() { printf '%s\n' '203.0.113.10 STREAM monitor.example.com'; }
    ss() {
        printf '%s\n' \
            '0 0 192.0.2.4:41000 203.0.113.10:443 users:(("serverbee-agent",pid=4242,fd=9))' \
            '0 0 192.0.2.4:41001 203.0.113.10:443 users:(("other",pid=9999,fd=9))'
    }

    connection=$(agent_connection_established) \
        || fail "matching Agent PID/address/port connection was rejected"
    assert_eq "$connection" '192.0.2.4:41000|203.0.113.10:443'

    ss() {
        printf '%s\n' '0 0 192.0.2.4:41000 203.0.113.11:443 users:(("serverbee-agent",pid=4242,fd=9))'
    }
    if agent_connection_established; then
        fail "Agent connection proof accepted the wrong Server address"
    fi
)

test_agent_install_rejects_changing_short_connections() (
    INSTALL_AGENT_ATTEMPTS=3
    : > "${TEST_ROOT}/short-connections"
    agent_install_logs() { :; }
    agent_install_exit_status() { :; }
    agent_connection_established() {
        count=$(wc -l < "${TEST_ROOT}/short-connections")
        printf '.\n' >> "${TEST_ROOT}/short-connections"
        printf '192.0.2.4:%s|203.0.113.10:443\n' "$((41000 + count))"
    }

    set +e
    wait_for_agent_install
    rc=$?
    set -e
    assert_eq "$rc" 75
)

test_docker_agent_logs_are_scoped_to_current_install() (
    METHOD=docker
    DOCKER_DIR="${TEST_ROOT}/docker-log-scope"
    AGENT_LOG_SINCE=1234567890
    mkdir -p "$DOCKER_DIR"
    : > "${DOCKER_DIR}/docker-compose.agent.yml"
    docker() {
        case "$*" in
            *'logs --no-color --since 1234567890 --tail 200 serverbee-agent'*)
                echo 'current install log'
                ;;
            *)
                echo 'Welcome from server stale-container'
                ;;
        esac
    }

    assert_eq "$(agent_install_logs)" 'current install log'
)

test_server_install_acceptance_polls_health() (
    INSTALL_SERVER_HEALTH_ATTEMPTS=3
    : > "${TEST_ROOT}/server-health-reads"
    server_install_health_check() {
        reads=$(wc -l < "${TEST_ROOT}/server-health-reads")
        printf '.\n' >> "${TEST_ROOT}/server-health-reads"
        [ "$reads" -ge 1 ]
    }
    wait_for_server_install || fail "healthy Server install was rejected"

    server_install_health_check() { return 1; }
    if wait_for_server_install; then
        fail "unhealthy Server install was accepted"
    fi
)

test_binary_install_without_init_is_unverified_not_failed() (
    METHOD=binary
    INIT=none
    NO_WAIT=false
    output_file="${TEST_ROOT}/no-init-output"
    server_install_health_check() { fail 'no-init Server attempted a health check'; }
    verify_server_install_or_exit > "$output_file" || fail "no-init Server install was rejected"
    output=$(cat "$output_file")
    assert_eq "$NO_WAIT" true
    case "$output" in
        *'health check passed'*) fail "no-init Server falsely claimed a successful health check" ;;
    esac

    NO_WAIT=false
    agent_install_logs() { fail 'no-init Agent attempted to read logs'; }
    verify_agent_install_or_exit > "$output_file" || fail "no-init Agent install was rejected"
    output=$(cat "$output_file")
    assert_eq "$NO_WAIT" true
    case "$output" in
        *'connected and received'*) fail "no-init Agent falsely claimed a successful connection" ;;
    esac
)

test_sensitive_output_is_redacted() (
    setup_case sensitive-redaction
    config_file="${CASE_DIR}/agent.toml"
    printf '%s\n' \
        'server_url = "https://example.com"' \
        'enrollment_code = "enroll-secret"' \
        'token = "run-secret"' \
        '[oauth.github]' \
        'client_secret = "oauth-secret"' > "$config_file"

    output=$(redact_toml_file "$config_file")
    printf '%s\n' "$output" | grep -Fq 'server_url = "https://example.com"' \
        || fail "redaction hid a non-sensitive TOML value"
    for secret in enroll-secret run-secret oauth-secret; do
        case "$output" in
            *"$secret"*) fail "redaction leaked ${secret}" ;;
        esac
    done

    output=$(printf '%s\n' \
        'SERVERBEE_LOG__LEVEL=debug' \
        'SERVERBEE_TOKEN=run-secret' \
        'Environment="SERVERBEE_OAUTH__GITHUB__CLIENT_SECRET=oauth-secret"' \
        | redact_env_lines)
    printf '%s\n' "$output" | grep -Fq 'SERVERBEE_LOG__LEVEL=debug' \
        || fail "redaction hid a non-sensitive env value"
    case "$output" in
        *run-secret*|*oauth-secret*) fail "redaction leaked an env secret" ;;
    esac
)

test_status_dashboard_uses_domain_cache_and_avoids_dead_loopback_url() (
    setup_case status-domain
    CONFIG_DIR="${CASE_DIR}/etc"
    DOMAIN_CACHE_FILE="${CONFIG_DIR}/.install-domain"
    mkdir -p "$CONFIG_DIR"
    printf '%s\n' '[server]' 'listen = "127.0.0.1:9527"' > "${CONFIG_DIR}/server.toml"

    output=$(server_dashboard_display binary)
    case "$output" in
        *'http://'*) fail "loopback Server status exposed a dead public HTTP URL" ;;
        *'reverse proxy'*) : ;;
        *) fail "loopback Server status omitted reverse-proxy guidance" ;;
    esac

    printf '%s\n' 'monitor.example.com' > "$DOMAIN_CACHE_FILE"
    assert_eq "$(server_dashboard_display binary)" 'https://monitor.example.com'
)

test_server_uninstall_clears_domain_cache_without_purge() (
    setup_case uninstall-domain-cache
    BASE_DIR="$CASE_DIR"
    INSTALL_DIR="${CASE_DIR}/bin"
    CONFIG_DIR="${CASE_DIR}/etc"
    DATA_DIR="${CASE_DIR}/data"
    DOCKER_DIR="$CASE_DIR"
    META_FILE="${CONFIG_DIR}/.install-meta"
    LANG_CACHE_FILE="${CONFIG_DIR}/.install-lang"
    DOMAIN_CACHE_FILE="${CONFIG_DIR}/.install-domain"
    CLI_PATH="${CASE_DIR}/serverbee"
    COMPONENT=server
    YES=true
    PURGE=false
    INIT=none
    RELEASE_CHANNEL=auto
    mkdir -p "$INSTALL_DIR" "$CONFIG_DIR" "$DATA_DIR"
    : > "${INSTALL_DIR}/serverbee-server"
    printf '%s\n' monitor.example.com > "$DOMAIN_CACHE_FILE"
    meta_write server binary v1.0.0-beta.1
    svc_remove() { :; }

    cmd_uninstall >/dev/null

    [ ! -e "$DOMAIN_CACHE_FILE" ] \
        || fail "non-purge Server uninstall retained stale domain metadata"
)

test_help_and_version_do_not_require_root() (
    require_root() { fail 'help/version attempted privilege elevation'; }

    output=$(main --help)
    printf '%s\n' "$output" | grep -Fq 'Usage: serverbee' \
        || fail "root-free help omitted usage"
    output=$(main version)
    printf '%s\n' "$output" | grep -Fq "$INSTALLER_VERSION" \
        || fail "version command omitted installer version"
)

test_parser_reports_missing_values_and_extra_arguments() (
    set +e
    output=$(parse_args --method 2>&1)
    rc=$?
    set -e
    [ "$rc" -ne 0 ] || fail "missing option value was accepted"
    printf '%s\n' "$output" | grep -Fq 'requires a value' \
        || fail "missing option value did not produce a friendly error"

    COMMAND=status
    COMPONENT=""
    CONFIG_KEY=""
    CONFIG_VALUE=""
    set +e
    output=$(parse_args server extra 2>&1 && validate_parsed_args 2>&1)
    rc=$?
    set -e
    [ "$rc" -ne 0 ] || fail "extra positional argument was accepted"
)

test_successful_upgrade
test_probe_mismatch_preserves_current_binary
test_server_candidate_probe_requires_exact_version
test_start_failure_rolls_back
test_health_failure_rolls_back
test_server_http_health_failure_rolls_back
test_server_health_url_uses_configured_listener
test_restart_during_stability_window_rolls_back
test_stale_backup_blocks_upgrade
test_successful_docker_upgrade
test_docker_pull_failure_restores_compose
test_docker_start_failure_rolls_back
test_unhealthy_docker_upgrade_rolls_back
test_restarting_docker_upgrade_rolls_back
test_stale_docker_backup_blocks_upgrade
test_docker_server_compose_mounts_generated_config
test_docker_agent_custom_caps_keep_executable_and_secure_config
test_docker_agent_start_failure_cleans_generated_files
test_docker_agent_config_write_failure_cleans_partial_file
test_docker_agent_start_failure_restores_existing_files
test_docker_agent_stale_rollback_symlinks_block_install
test_non_tty_input_does_not_prompt
test_stable_channel_ignores_misclassified_prerelease
test_stable_channel_fails_without_stable_release
test_beta_channel_finds_semver_prerelease_when_metadata_is_wrong
test_auto_channel_prefers_stable_release
test_auto_channel_falls_back_to_prerelease
test_release_selection_consumes_the_full_response
test_install_metadata_persists_upgrade_channel
test_current_upgrade_persists_explicit_channel
test_toml_set_roundtrips_special_characters_and_preserves_mode
test_compose_env_set_adds_environment_and_is_idempotent
test_compose_env_set_collapses_existing_duplicates
test_docker_env_transaction_validates_and_restores_on_failure
test_openrc_env_roundtrips_shell_metacharacters
test_systemd_env_escapes_unit_syntax_and_is_idempotent
test_installer_version_matches_workspace_version
test_value_validation_rejects_controls_without_file_changes
test_agent_install_acceptance_states
test_agent_install_accepts_stable_endpoint_connection_without_info_logs
test_agent_connection_proof_matches_pid_address_and_port
test_agent_install_rejects_changing_short_connections
test_docker_agent_logs_are_scoped_to_current_install
test_server_install_acceptance_polls_health
test_binary_install_without_init_is_unverified_not_failed
test_sensitive_output_is_redacted
test_status_dashboard_uses_domain_cache_and_avoids_dead_loopback_url
test_server_uninstall_clears_domain_cache_without_purge
test_help_and_version_do_not_require_root
test_parser_reports_missing_values_and_extra_arguments
printf 'PASS: install upgrade transaction tests\n'
