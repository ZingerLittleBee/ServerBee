# Install Script Self-Deploy Design

## Problem

After installing ServerBee via `curl | bash`, users have no local management tool. The `deploy/serverbee.sh` path referenced in some docs only exists in a cloned repo. Users must either clone the repo or re-curl the script for every management operation (status, upgrade, restart, config, etc.), which is impractical.

Additionally, `deploy/install.sh` and `deploy/serverbee.sh` are identical 56KB files — a maintenance hazard.

## Solution

During installation, the script installs itself to `/usr/local/bin/serverbee` so users can run `sudo serverbee status`, `sudo serverbee upgrade`, etc. directly.

## Design

### 1. Self-Install Function

Add an `install_cli()` function that downloads the latest `install.sh` from GitHub and writes it to `/usr/local/bin/serverbee`:

```bash
install_cli() {
    local url="https://raw.githubusercontent.com/${REPO}/main/deploy/install.sh"
    info "Installing management CLI..."
    curl -fsSL -o "/usr/local/bin/serverbee" "$url" \
        || warn "Failed to install CLI to /usr/local/bin/serverbee"
    chmod +x "/usr/local/bin/serverbee"
    info "Management CLI installed: serverbee"
}
```

Always fetches from GitHub `main` branch rather than trying to copy `$0`, because in pipe execution (`curl | bash`) the script content is consumed from stdin and cannot be re-read.

### 2. Call Sites

`install_cli()` is called in:

- `install_binary_server()` — after binary + systemd setup
- `install_binary_agent()` — after binary + systemd setup
- `install_docker_server()` — after compose up
- `install_docker_agent()` — after compose up

Each call happens right before `meta_write()`.

### 3. Upgrade Self-Update

In `upgrade_component()`, after upgrading the component binary/container and writing metadata, call `install_cli()` to update the management script to the latest version as well.

Add the call after `meta_write()` at the end of `upgrade_component()`.

### 4. File Cleanup

- Delete `deploy/serverbee.sh` (duplicate of `install.sh`)
- Keep `deploy/install.sh` as the sole source file

### 5. Documentation Updates

Replace all `sudo bash deploy/serverbee.sh <cmd>` and `curl ... | sudo bash -s -- <cmd>` management commands with `sudo serverbee <cmd>`.

Installation commands (the initial curl pipe) remain unchanged.

Files to update:
- `README.md` — management commands section
- `apps/docs/content/docs/en/quick-start.mdx`
- `apps/docs/content/docs/en/deployment.mdx`
- `apps/docs/content/docs/cn/quick-start.mdx`
- `apps/docs/content/docs/cn/deployment.mdx`

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

- If `/usr/local/bin/serverbee` already exists, `install_cli()` overwrites it (ensures latest version)
- `install_cli()` failure is a warning, not a fatal error — the component installation should still succeed even if the CLI install fails (e.g., network issues on subsequent curl)

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

- Modify: `deploy/install.sh`
- Delete: `deploy/serverbee.sh`
- Update docs: `README.md`, 4 MDX files (en/cn quick-start + deployment)
- No Rust code changes
- No frontend changes
