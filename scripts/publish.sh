#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT_DIR"

die() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

VERSION=${VERSION#v}
DRY_RUN=${DRY_RUN:-}
YES=${YES:-}
PREPARE_ONLY=${PREPARE_ONLY:-}
TODAY=$(date +%Y-%m-%d)
TAG=""

[[ -z "$(git status --porcelain)" ]] || die "worktree must be clean"

CURRENT_BRANCH=$(git branch --show-current)
DEFAULT_BRANCH=$(git symbolic-ref --quiet --short refs/remotes/origin/HEAD 2>/dev/null || true)
DEFAULT_BRANCH=${DEFAULT_BRANCH#origin/}
: "${DEFAULT_BRANCH:=main}"
[[ "$CURRENT_BRANCH" == "$DEFAULT_BRANCH" ]] \
    || die "publish must run on ${DEFAULT_BRANCH}, current branch is ${CURRENT_BRANCH:-detached}"

mapfile -t CHANGELOG_VERSIONS < <(awk '
    /^## \[[0-9]+\.[0-9]+\.[0-9]+([^]]*)?\] - [0-9]{4}-[0-9]{2}-[0-9]{2}$/ {
        line=$0
        sub(/^## \[/, "", line)
        sub(/\].*$/, "", line)
        print line
    }
' CHANGELOG.md)
LATEST_CHANGELOG_VERSION=${CHANGELOG_VERSIONS[0]:-}
[[ -n "$LATEST_CHANGELOG_VERSION" ]] || die "no released version found in CHANGELOG.md"

LATEST_TAG=$(git tag --list 'v*' --sort=-version:refname | head -n 1)
[[ -n "$LATEST_TAG" ]] || die "no local release tag found"
if [[ -z "$VERSION" ]]; then
    [[ "$LATEST_CHANGELOG_VERSION" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]] \
        || die "VERSION is required when the latest release is a prerelease"
    VERSION="${BASH_REMATCH[1]}.${BASH_REMATCH[2]}.$((BASH_REMATCH[3] + 1))"
fi
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z]+([.-][0-9A-Za-z]+)*)?$ ]] \
    || die "invalid semantic version: ${VERSION}"
TAG="v${VERSION}"

CARGO_VERSION=$(awk -F '"' '/^version = "/ { print $2; exit }' Cargo.toml)
WEB_VERSION=$(bun -e "console.log(require('./apps/web/package.json').version)")
PREPARED_RELEASE=
if [[ "$LATEST_CHANGELOG_VERSION" == "$VERSION" ]]; then
    PREPARED_RELEASE=1
    PREVIOUS_CHANGELOG_VERSION=${CHANGELOG_VERSIONS[1]:-}
    [[ -n "$PREVIOUS_CHANGELOG_VERSION" ]] \
        || die "prepared release ${TAG} has no previous CHANGELOG release"
    [[ "${LATEST_TAG#v}" == "$PREVIOUS_CHANGELOG_VERSION" ]] \
        || die "latest tag ${LATEST_TAG} disagrees with previous CHANGELOG release ${PREVIOUS_CHANGELOG_VERSION}"
    [[ "$CARGO_VERSION" == "$VERSION" ]] \
        || die "Cargo.toml version ${CARGO_VERSION} disagrees with prepared release ${VERSION}"
    [[ "$WEB_VERSION" == "$VERSION" ]] \
        || die "web version ${WEB_VERSION} disagrees with prepared release ${VERSION}"
else
    [[ "${LATEST_TAG#v}" == "$LATEST_CHANGELOG_VERSION" ]] \
        || die "latest tag ${LATEST_TAG} disagrees with CHANGELOG ${LATEST_CHANGELOG_VERSION}"
fi

git rev-parse --verify --quiet "refs/tags/${TAG}" >/dev/null \
    && die "local tag already exists: ${TAG}"
if git ls-remote --exit-code --tags origin "refs/tags/${TAG}" >/dev/null 2>&1; then
    die "remote tag already exists: ${TAG}"
fi

if [[ -n "$PREPARED_RELEASE" ]]; then
    RELEASE_NOTES=$(awk -v version="$VERSION" '
        $0 ~ "^## \\[" version "\\] - [0-9]{4}-[0-9]{2}-[0-9]{2}$" { in_release=1; next }
        in_release && /^## / { exit }
        in_release { print }
    ' CHANGELOG.md)
else
    RELEASE_NOTES=$(awk '
        /^## \[Unreleased\]$/ { in_unreleased=1; next }
        in_unreleased && /^## / { exit }
        in_unreleased { print }
    ' CHANGELOG.md)
fi
[[ -n "${RELEASE_NOTES//[[:space:]]/}" ]] || die "CHANGELOG release notes are empty"

printf 'Publish %s  (from %s)\n\n' "$TAG" "$LATEST_TAG"
if [[ -n "$PREPARED_RELEASE" ]]; then
    printf '  prepared commit       %s\n\n' "$(git rev-parse HEAD)"
else
    printf '  CHANGELOG.md          [Unreleased] -> [%s] - %s, new empty [Unreleased]\n' "$VERSION" "$TODAY"
    printf '  Cargo.toml            %s -> %s  ([workspace.package])\n' "$CARGO_VERSION" "$VERSION"
    printf '  apps/web/package.json %s -> %s  (release metadata)\n' "$WEB_VERSION" "$VERSION"
    printf '  Cargo.lock            regenerate via cargo update --workspace\n'
    printf '  bun.lock              regenerate via bun install\n\n'
    printf '  commit                chore: release %s\n' "$TAG"
fi
if [[ -n "$PREPARE_ONLY" ]]; then
    printf '  external writes       none (PREPARE_ONLY=1)\n'
else
    printf '  push                  origin %s\n' "$DEFAULT_BRANCH"
    printf '  tag                   %s (annotated) -> origin\n' "$TAG"
    printf '  release               GitHub Actions (.github/workflows/release.yml)\n'
fi
printf '\nRelease notes (from CHANGELOG):\n%s\n' "$RELEASE_NOTES"

[[ -z "$DRY_RUN" ]] || exit 0

if [[ -z "$YES" ]]; then
    printf '\nProceed? [y/N] '
    read -r answer
    [[ "$answer" == y || "$answer" == Y || "$answer" == yes || "$answer" == YES ]] || exit 1
fi

if [[ -n "$PREPARED_RELEASE" ]]; then
    if [[ -n "$PREPARE_ONLY" ]]; then
        printf '%s is already prepared locally at %s. No branch, tag, or release was pushed.\n' \
            "$TAG" "$(git rev-parse HEAD)"
        exit 0
    fi

    git push origin "$DEFAULT_BRANCH"
    git tag -a "$TAG" -m "Release ${TAG}"
    git push origin "$TAG"
    printf 'Pushed %s. GitHub Actions will build and publish the release.\n' "$TAG"
    exit 0
fi

VERSION="$VERSION" TODAY="$TODAY" perl -0pi -e '
    my $version = $ENV{VERSION};
    my $today = $ENV{TODAY};
    s/^## \[Unreleased\]$/## [Unreleased]\n\n## [$version] - $today/m
        or die "missing Unreleased heading\n";
' CHANGELOG.md

VERSION="$VERSION" perl -0pi -e '
    my $version = $ENV{VERSION};
    s/(\[workspace\.package\][\s\S]*?^version = ")[^"]+(".*$)/$1$version$2/m
        or die "missing workspace version\n";
' Cargo.toml

VERSION="$VERSION" bun -e '
    const path = "apps/web/package.json";
    const manifest = await Bun.file(path).json();
    manifest.version = process.env.VERSION;
    await Bun.write(path, `${JSON.stringify(manifest, null, 2)}\n`);
'

cargo update --workspace
bun install

[[ "$(awk -F '"' '/^version = "/ { print $2; exit }' Cargo.toml)" == "$VERSION" ]] \
    || die "Cargo.toml version update failed"
[[ "$(bun -e "console.log(require('./apps/web/package.json').version)")" == "$VERSION" ]] \
    || die "web version update failed"
for package in serverbee-common serverbee-agent serverbee-server; do
    lock_version=$(awk -v package="$package" '
        $1 == "name" && $3 == "\"" package "\"" { found=1; next }
        found && $1 == "version" { gsub(/"/, "", $3); print $3; exit }
    ' Cargo.lock)
    [[ "$lock_version" == "$VERSION" ]] || die "Cargo.lock ${package} version is ${lock_version}"
done
grep -A3 '"apps/web":' bun.lock | grep -Fq "\"version\": \"${VERSION}\"" \
    || die "bun.lock web version update failed"
git diff --check

git add CHANGELOG.md Cargo.toml Cargo.lock apps/web/package.json bun.lock
git commit -m "chore: release ${TAG}"

if [[ -n "$PREPARE_ONLY" ]]; then
    printf 'Prepared %s locally at %s. No branch, tag, or release was pushed.\n' "$TAG" "$(git rev-parse HEAD)"
    exit 0
fi

git push origin "$DEFAULT_BRANCH"
git tag -a "$TAG" -m "Release ${TAG}"
git push origin "$TAG"
printf 'Pushed %s. GitHub Actions will build and publish the release.\n' "$TAG"
