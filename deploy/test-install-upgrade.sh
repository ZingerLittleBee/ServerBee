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
    installed_version=$("${INSTALL_DIR}/serverbee-agent" --serverbee-upgrade-probe)
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
        installed_version=$("${INSTALL_DIR}/serverbee-agent" --serverbee-upgrade-probe)
        [ "$installed_version" = "1.0.0-beta.1" ] && echo inactive || echo active
    elif [ "$TEST_ACTIVE_MODE" = active ]; then
        echo active
    else
        echo inactive
    fi
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
start agent"
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

test_successful_upgrade
test_probe_mismatch_preserves_current_binary
test_server_candidate_probe_requires_exact_version
test_start_failure_rolls_back
test_health_failure_rolls_back
test_restart_during_stability_window_rolls_back
test_stale_backup_blocks_upgrade
test_successful_docker_upgrade
test_docker_pull_failure_restores_compose
test_docker_start_failure_rolls_back
test_unhealthy_docker_upgrade_rolls_back
test_restarting_docker_upgrade_rolls_back
test_stale_docker_backup_blocks_upgrade
printf 'PASS: install upgrade transaction tests\n'
