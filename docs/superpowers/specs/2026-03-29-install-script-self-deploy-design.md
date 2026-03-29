# Install Script Self-Deploy Design

> Supersedes `2026-03-29-serverbee-sh-design.md` — that doc defined `serverbee.sh` as the primary file and `install.sh` as the copy. This design inverts that: `install.sh` is the sole source, `serverbee.sh` is deleted, and the script self-installs to `/usr/local/bin/serverbee`.

## Problem

After installing ServerBee via `curl | bash`, users have no local management tool. The `deploy/serverbee.sh` path referenced in some docs only exists in a cloned repo. Users must either clone the repo or re-curl the script for every management operation (status, upgrade, restart, config, etc.), which is impractical.

Additionally, `deploy/install.sh` and `deploy/serverbee.sh` are identical 56KB files — a maintenance hazard.

## Solution

During installation, the script installs itself to `/usr/local/bin/serverbee` so users can run `sudo serverbee status`, `sudo serverbee upgrade`, etc. directly.

## Design

### 1. Self-Install Function

Add an `install_cli()` function that installs the script to `/usr/local/bin/serverbee`. The source depends on three contexts:

- **Repo-local execution** (`bash deploy/install.sh`): `$0` points to a file inside the repo's `deploy/` directory — copy it directly.
- **Installed CLI execution** (`sudo serverbee upgrade`): `$0` is `/usr/local/bin/serverbee` itself — copying it would be a no-op. Must download from GitHub.
- **Pipe execution** (`curl | bash`): `$0` is `bash`, not a file — must download from GitHub.

Both download cases pin to the **same release tag** used for binaries (not `main`).

```bash
install_cli() {
    local target="/usr/local/bin/serverbee"
    local version="${1:-main}"

    # Entire body runs in a subshell so any failure (cp, curl, chmod, mv)
    # is caught by the || guard, keeping set -e from killing the caller.
    if (
        # Create temp file on the SAME filesystem as target for atomic mv
        local target_dir
        target_dir=$(dirname "$target")
        local tmp
        tmp=$(mktemp "${target_dir}/.serverbee-cli.XXXXXX")
        trap 'rm -f "$tmp"' EXIT

        if [ -f "$0" ] && ! [ "$0" -ef "$target" ]; then
            # Repo-local execution — $0 is a real file AND not the installed CLI
            # -ef compares inodes, so symlinks and relative paths are handled
            cp "$0" "$tmp"
        else
            # Installed CLI or pipe execution — download from release tag
            local url="https://raw.githubusercontent.com/${REPO}/${version}/deploy/install.sh"
            curl -fsSL -o "$tmp" "$url"
        fi

        chmod +x "$tmp"
        mv "$tmp" "$target"
        # Disable EXIT trap — mv succeeded, tmp no longer exists
        trap - EXIT
    ); then
        info "Management CLI installed: serverbee"
    else
        warn "Failed to install CLI to ${target} — component installation continues"
    fi
}
```

Key properties:
- **Truly non-fatal**: the entire body runs in a subshell guarded by `if (...); then ... else ... fi`. Under `set -euo pipefail`, any step failure (`cp`, `curl`, `chmod`, `mv`) exits the subshell, hits the `else` branch, and warns — the caller continues normally.
- **Atomic write**: temp file created in the target directory (`/usr/local/bin/.serverbee-cli.XXXXXX`) — same filesystem, so `mv` is a guaranteed atomic rename. EXIT trap cleans up on failure.
- **Version-pinned**: download cases use the same release tag as the binaries, avoiding version skew between CLI and components.
- **Self-update safe**: `[ "$0" -ef "$target" ]` (inode comparison) prevents the installed CLI from copying itself onto itself; it downloads the pinned version instead. Handles symlinks, relative paths, and alternate invocation paths without external dependencies — `-ef` is a bash builtin.

### 2. Call Sites

`install_cli()` is called in:

- `install_binary_server()` — after binary + systemd setup
- `install_binary_agent()` — after binary + systemd setup
- `install_docker_server()` — after compose up
- `install_docker_agent()` — after compose up

Each call happens right before `meta_write()`. The release version is passed as the first argument: `install_cli "$version"`.

### 3. Upgrade Self-Update

In `upgrade_component()`, call `install_cli "$latest_version"` to update the management script. Two call sites:

1. **After successful upgrade** — after `meta_write()` at the end of `upgrade_component()`.
2. **On version-equal early return** — unconditionally call `install_cli "$latest_version"` before the return. This repairs both missing CLIs and stale CLIs in a single path. The function is non-fatal and idempotent, so calling it when the CLI is already current has no harmful effect.

```bash
# In upgrade_component(), at the early return:
if [ -n "$current_version" ] && [ "$current_version" = "$latest_version" ]; then
    # Always ensure CLI matches the current release (repairs missing or stale)
    install_cli "$latest_version"
    info "serverbee-${component} is already up to date (${current_version})"
    return
fi
```

### 4. File Cleanup

- Delete `deploy/serverbee.sh` (duplicate of `install.sh`)
- Keep `deploy/install.sh` as the sole source file
- Mark `docs/superpowers/specs/2026-03-29-serverbee-sh-design.md` as superseded by this document

### 5. Documentation Updates

Replace all `sudo bash deploy/serverbee.sh <cmd>` and `curl ... | sudo bash -s -- <cmd>` management commands with `sudo serverbee <cmd>`.

Installation commands (the initial curl pipe) remain unchanged.

Files to update:
- `README.md` — management commands section (14 references to `deploy/serverbee.sh`)
- `README.zh-CN.md` — same section, Chinese version (14 references)
- `apps/docs/content/docs/en/quick-start.mdx` — management commands
- `apps/docs/content/docs/en/deployment.mdx` — management commands + "cloned repo" note
- `apps/docs/content/docs/en/agent.mdx` — `deploy/serverbee.sh` reference
- `apps/docs/content/docs/cn/quick-start.mdx` — management commands
- `apps/docs/content/docs/cn/deployment.mdx` — management commands + "克隆仓库" note
- `apps/docs/content/docs/cn/agent.mdx` — `deploy/serverbee.sh` reference
- `deploy/install.sh` — internal help/error strings (6 occurrences of `serverbee.sh` → `serverbee`)

**Before:**
```bash
sudo bash deploy/serverbee.sh status
# or
curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh | sudo bash -s -- status
```

**After:**
```bash
sudo serverbee status
```

### 6. Idempotency

- If `/usr/local/bin/serverbee` already exists, `install_cli()` overwrites it via atomic temp+mv (ensures latest version)
- `install_cli()` failure is a warning, not a fatal error — the component installation succeeds even if the CLI install fails

### 7. Uninstall

`uninstall_binary()` and `uninstall_docker()` already clean up component-specific files. When the last managed component is uninstalled (no more entries in `.install-meta`), also remove `/usr/local/bin/serverbee` and the meta file:

```bash
# At end of cmd_uninstall(), after meta_remove:
if [ -f "$META_FILE" ]; then
    local remaining
    remaining=$(grep -c '"method"' "$META_FILE" 2>/dev/null || echo "0")
    if [ "$remaining" -eq 0 ]; then
        rm -f "/usr/local/bin/serverbee"
        rm -f "$META_FILE"
        info "All components removed. CLI uninstalled."
    fi
fi
```

## Scope

- Modify: `deploy/install.sh` (add `install_cli()`, update internal strings)
- Delete: `deploy/serverbee.sh`
- Update docs: `README.md`, `README.zh-CN.md`, 6 MDX files
- Supersede: `docs/superpowers/specs/2026-03-29-serverbee-sh-design.md`
- No Rust code changes
- No frontend changes
