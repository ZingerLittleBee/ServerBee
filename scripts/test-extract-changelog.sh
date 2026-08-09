#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
TEST_ROOT=$(mktemp -d)
trap 'rm -rf "$TEST_ROOT"' EXIT HUP INT TERM

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

write_fixture() {
    printf '%s\n' "$@" > "${TEST_ROOT}/CHANGELOG.md"
}

extract() {
    "${SCRIPT_DIR}/extract-changelog.sh" "$1" "${TEST_ROOT}/CHANGELOG.md"
}

write_fixture \
    '# Changelog' \
    '' \
    '## [Unreleased]' \
    '' \
    '## [1.0.0-beta.1] - 2026-08-10' \
    '' \
    '### Fixed' \
    '- Rollback works' \
    '' \
    '## [1.0.0-alpha.12] - 2026-08-03' \
    '- Older notes'
expected='
### Fixed
- Rollback works'
actual=$(extract v1.0.0-beta.1)
[ "$actual" = "$expected" ] || fail "dated heading extraction returned unexpected notes"

write_fixture \
    '## [Unreleased]' \
    '- Future notes' \
    '## [1.0.0-beta.1]' \
    '- Undated release notes' \
    '## [1.0.0-alpha.12] - 2026-08-03' \
    '- Older notes'
[ "$(extract 1.0.0-beta.1)" = '- Undated release notes' ] \
    || fail "undated heading extraction failed"

write_fixture \
    '## [1.0.0-beta.10] - 2026-08-10' \
    '- Similar version' \
    '## [1.0.0-beta.1-extra] - 2026-08-10' \
    '- Wrong suffix'
if extract 1.0.0-beta.1 >/dev/null 2>&1; then
    fail "similar version heading unexpectedly matched"
fi

write_fixture \
    '## [1.0.0-beta.1] - 2026-08-10' \
    '' \
    '   ' \
    '## [1.0.0-alpha.12] - 2026-08-03' \
    '- Older notes'
if extract 1.0.0-beta.1 >/dev/null 2>&1; then
    fail "blank release notes unexpectedly succeeded"
fi

printf 'PASS: changelog extraction tests\n'
