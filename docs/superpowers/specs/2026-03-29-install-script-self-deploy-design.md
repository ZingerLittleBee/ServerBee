# Install Script Self-Deploy Design

> Supersedes `2026-03-29-serverbee-sh-design.md` — that doc defined `serverbee.sh` as the primary file and `install.sh` as the copy. This design inverts that: `install.sh` is the sole source, `serverbee.sh` is deleted, and the script self-installs to `/usr/local/bin/serverbee`.

## Problem

After installing ServerBee via `curl | bash`, users have no local management tool. The `deploy/serverbee.sh` path referenced in some docs only exists in a cloned repo. Users must either clone the repo or re-curl the script for every management operation (status, upgrade, restart, config, etc.), which is impractical.

Additionally, `deploy/install.sh` and `deploy/serverbee.sh` are identical 56KB files — a maintenance hazard.

## Solution

During installation, the script installs itself to `/usr/local/bin/serverbee` so users can run `sudo serverbee status`, `sudo serverbee upgrade`, etc. directly.

## Design

### 1. Self-Install Function

Add an `install_cli()` function that installs the script to `/usr/local/bin/serverbee`. The source depends on context:

- **Local execution** (`bash deploy/install.sh`): `$0` is a readable file — copy it directly.
- **Pipe execution** (`curl | bash`): `$0` is `bash`, not a file — download from GitHub, pinned to the **same release tag** used for binaries (not `main`).

```bash
install_cli() {
    local target="/usr/local/bin/serverbee"
    local tmp
    tmp=$(mktemp)

    if [ -f "$0" ]; then
        # Local execution — copy the running script
        cp "$0" "$tmp"
    else
        # Pipe execution — download from same release tag
        local version="${1:-main}"
        local url="https://raw.githubusercontent.com/${REPO}/${version}/deploy/install.sh"
        if ! curl -fsSL -o "$tmp" "$url"; then
            rm -f "$tmp"
            warn "Failed to install CLI to ${target}"
            return 0
        fi
    fi

    chmod +x "$tmp"
    mv "$tmp" "$target"
    info "Management CLI installed: serverbee"
}
```

Key properties:
- **Atomic write**: downloads to temp file first, `mv` only on success — never clobbers a working CLI on failure.
- **Version-pinned**: pipe execution uses the same release tag as the binaries, avoiding version skew between CLI and components.
- **Non-fatal**: on failure, warns and returns 0 — component installation still succeeds. No `chmod` runs after a failed download.

### 2. Call Sites

`install_cli()` is called in:

- `install_binary_server()` — after binary + systemd setup
- `install_binary_agent()` — after binary + systemd setup
- `install_docker_server()` — after compose up
- `install_docker_agent()` — after compose up

Each call happens right before `meta_write()`. The release version is passed as the first argument: `install_cli "$version"`.

### 3. Upgrade Self-Update

In `upgrade_component()`, after upgrading the component binary/container and writing metadata, call `install_cli "$latest_version"` to update the management script to the matching release version.

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
