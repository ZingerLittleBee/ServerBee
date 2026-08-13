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
    docker() { :; }
    install_cli() { :; }
    meta_write() { :; }
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
    docker() { :; }
    install_cli() { :; }
    meta_write() { :; }
    print_agent_result() { :; }

    install_docker_agent >/dev/null

    compose_file="${DOCKER_DIR}/docker-compose.agent.yml"
    first_command=$(awk '/^    command:$/ { getline; print; exit }' "$compose_file")
    assert_eq "$first_command" "      - serverbee-agent"

    config_mode=$(LC_ALL=C ls -l "${CONFIG_DIR}/agent.toml" | cut -c 1-10)
    assert_eq "$config_mode" "-rw-------"
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
    print_agent_result() { :; }

    if (install_docker_agent) >/dev/null 2>&1; then
        fail "failed Docker Agent start unexpectedly succeeded"
    fi

    [ ! -e "${DOCKER_DIR}/docker-compose.agent.yml" ] \
        || fail "failed Docker Agent install left its generated Compose file"
    [ ! -e "${CONFIG_DIR}/agent.toml" ] \
        || fail "failed Docker Agent install left its generated enrollment config"
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
    assert_eq "$(cat "$ACTION_LOG")" "up
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
printf 'PASS: install upgrade transaction tests\n'
