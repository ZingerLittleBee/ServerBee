# Install Script Self-Deploy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the install script self-install to `/usr/local/bin/serverbee` so users have a local management CLI after installation, and update all documentation to use `sudo serverbee <cmd>` instead of `sudo bash deploy/serverbee.sh <cmd>` or long curl commands.

**Architecture:** Add an `install_cli()` function to `deploy/install.sh` that copies itself (local execution) or downloads from the same release tag (pipe/installed CLI execution) to `/usr/local/bin/serverbee`. Call it from all install and upgrade paths. Delete the duplicate `deploy/serverbee.sh`.

**Tech Stack:** Bash, GitHub raw content for downloads

**Spec:** `docs/superpowers/specs/2026-03-29-install-script-self-deploy-design.md`

---

### Task 1: Add `install_cli()` function to `deploy/install.sh`

**Files:**
- Modify: `deploy/install.sh:337` (insert after `check_unmanaged_container()`, before install helpers)

- [ ] **Step 1: Add the `install_cli()` function**

Insert after line 336 (end of `check_unmanaged_container()`) in `deploy/install.sh`:

```bash
# ─── CLI self-install ────────────────────────────────────────────────────────

install_cli() {
    local target="/usr/local/bin/serverbee"
    local version="${1:-main}"

    # Entire body runs in a subshell so any failure (cp, curl, chmod, mv)
    # is caught by the guard, keeping set -e from killing the caller.
    if (
        # Create temp file on the SAME filesystem as target for atomic mv
        local target_dir
        target_dir=$(dirname "$target")
        local tmp
        tmp=$(mktemp "${target_dir}/.serverbee-cli.XXXXXX")
        trap 'rm -f "$tmp"' EXIT

        if [ -f "$0" ] && ! [ "$0" -ef "$target" ]; then
            # Repo-local execution — $0 is a real file AND not the installed CLI
            cp "$0" "$tmp"
        else
            # Installed CLI or pipe execution — download from release tag
            local url="https://raw.githubusercontent.com/${REPO}/${version}/deploy/install.sh"
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
```

- [ ] **Step 2: Verify the script still parses correctly**

Run: `bash -n deploy/install.sh`
Expected: no output (clean parse)

- [ ] **Step 3: Commit**

```bash
git add deploy/install.sh
git commit -m "feat(deploy): add install_cli() self-install function"
```

---

### Task 2: Call `install_cli()` from all install paths

**Files:**
- Modify: `deploy/install.sh` — functions `install_binary_server()`, `install_binary_agent()`, `install_docker_server()`, `install_docker_agent()`

- [ ] **Step 1: Add `install_cli` call to `install_binary_server()`**

In `install_binary_server()`, insert `install_cli "$version"` right before `meta_write "server" "binary" "$version"` (line 407):

```bash
    install_cli "$version"
    meta_write "server" "binary" "$version"
```

- [ ] **Step 2: Add `install_cli` call to `install_binary_agent()`**

In `install_binary_agent()`, insert `install_cli "$version"` right before `meta_write "agent" "binary" "$version"` (line 474):

```bash
    install_cli "$version"
    meta_write "agent" "binary" "$version"
```

- [ ] **Step 3: Add `install_cli` call to `install_docker_server()`**

In `install_docker_server()`, insert `install_cli "$version"` right before `meta_write "server" "docker" "$version"` (line 538):

```bash
    install_cli "$version"
    meta_write "server" "docker" "$version"
```

- [ ] **Step 4: Add `install_cli` call to `install_docker_agent()`**

In `install_docker_agent()`, insert `install_cli "$version"` right before `meta_write "agent" "docker" "$version"` (line 588):

```bash
    install_cli "$version"
    meta_write "agent" "docker" "$version"
```

- [ ] **Step 5: Verify the script still parses correctly**

Run: `bash -n deploy/install.sh`
Expected: no output (clean parse)

- [ ] **Step 6: Commit**

```bash
git add deploy/install.sh
git commit -m "feat(deploy): call install_cli from all install paths"
```

---

### Task 3: Call `install_cli()` from upgrade path

**Files:**
- Modify: `deploy/install.sh` — function `upgrade_component()` (around line 843)

- [ ] **Step 1: Add unconditional `install_cli` call at the version-equal early return**

Replace the existing early return block in `upgrade_component()`:

```bash
    if [ -n "$current_version" ] && [ "$current_version" = "$latest_version" ]; then
        info "serverbee-${component} is already up to date (${current_version})"
        return
    fi
```

With:

```bash
    if [ -n "$current_version" ] && [ "$current_version" = "$latest_version" ]; then
        # Always ensure CLI matches the current release (repairs missing or stale)
        install_cli "$latest_version"
        info "serverbee-${component} is already up to date (${current_version})"
        return
    fi
```

- [ ] **Step 2: Add `install_cli` call after successful upgrade**

At the end of `upgrade_component()`, after the existing `meta_write` call (line 875) and `info` line (line 876), add:

```bash
    meta_write "$component" "$method" "$latest_version"
    install_cli "$latest_version"
    info "serverbee-${component} upgraded to ${latest_version}"
```

- [ ] **Step 3: Verify the script still parses correctly**

Run: `bash -n deploy/install.sh`
Expected: no output (clean parse)

- [ ] **Step 4: Commit**

```bash
git add deploy/install.sh
git commit -m "feat(deploy): self-update CLI on upgrade and no-op upgrade"
```

---

### Task 4: Add CLI cleanup to uninstall path

**Files:**
- Modify: `deploy/install.sh` — function `cmd_uninstall()` (around line 831)

- [ ] **Step 1: Add CLI cleanup after the last component is uninstalled**

In `cmd_uninstall()`, after the existing `meta_remove "$COMPONENT"` line (line 830), add CLI cleanup logic. The full end of the function should become:

```bash
    meta_remove "$COMPONENT"
    info "serverbee-${COMPONENT} has been uninstalled."

    # Remove CLI when no managed components remain
    if [ -f "$META_FILE" ]; then
        local remaining
        remaining=$(grep -c '"method"' "$META_FILE" 2>/dev/null || echo "0")
        if [ "$remaining" -eq 0 ]; then
            rm -f "/usr/local/bin/serverbee"
            rm -f "$META_FILE"
            info "All components removed. CLI uninstalled."
        fi
    fi

    if [ "$PURGE" != true ]; then
```

- [ ] **Step 2: Verify the script still parses correctly**

Run: `bash -n deploy/install.sh`
Expected: no output (clean parse)

- [ ] **Step 3: Commit**

```bash
git add deploy/install.sh
git commit -m "feat(deploy): remove CLI when last component is uninstalled"
```

---

### Task 5: Update internal help/error strings in `deploy/install.sh`

**Files:**
- Modify: `deploy/install.sh` — 6 occurrences of `serverbee.sh` in user-facing strings

- [ ] **Step 1: Replace all 6 `serverbee.sh` references with `serverbee`**

Line 333 — change:
```
Then re-run:  serverbee.sh install ${component} --method docker ...
```
To:
```
Then re-run:  serverbee install ${component} --method docker ...
```

Line 1023 — change:
```
No ServerBee components found. Run 'serverbee.sh install' to get started.
```
To:
```
No ServerBee components found. Run 'serverbee install' to get started.
```

Line 1045 — change:
```
To bring it under management, run: serverbee.sh install ${comp} [options]
```
To:
```
To bring it under management, run: serverbee install ${comp} [options]
```

Lines 1223-1224 — change:
```
Usage: serverbee.sh config set <key> <value>
```
To:
```
Usage: serverbee config set <key> <value>
```

Lines 1346-1347 — change:
```
Usage: serverbee.sh env set <KEY> <value>
```
To:
```
Usage: serverbee env set <KEY> <value>
```

- [ ] **Step 2: Verify the script still parses correctly**

Run: `bash -n deploy/install.sh`
Expected: no output (clean parse)

- [ ] **Step 3: Commit**

```bash
git add deploy/install.sh
git commit -m "fix(deploy): update internal strings from serverbee.sh to serverbee"
```

---

### Task 6: Delete `deploy/serverbee.sh`

**Files:**
- Delete: `deploy/serverbee.sh`

- [ ] **Step 1: Remove the duplicate file**

```bash
git rm deploy/serverbee.sh
```

- [ ] **Step 2: Commit**

```bash
git commit -m "chore(deploy): remove duplicate serverbee.sh (install.sh is sole source)"
```

---

### Task 7: Update `README.md` — management commands

**Files:**
- Modify: `README.md` — lines 33, 232-256, 258-268

- [ ] **Step 1: Update the feature description on line 33**

Change:
```
- **Guided Deployment Management** -- `deploy/serverbee.sh` installs, upgrades, inspects, reconfigures, and uninstalls server and agent deployments in interactive or unattended mode
```
To:
```
- **Guided Deployment Management** -- `serverbee` CLI installs, upgrades, inspects, reconfigures, and uninstalls server and agent deployments in interactive or unattended mode
```

- [ ] **Step 2: Update the "Install Script" section (lines 232-256)**

Replace lines 232-256 with:

```markdown
### Install Script

Install via curl (one-liner):

```bash
# Server
curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh | sudo bash -s -- server

# Agent (replace with your server URL and discovery key)
curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh | sudo bash -s -- agent \
  --server-url http://YOUR_SERVER:9527 --discovery-key YOUR_KEY
```

The installer automatically places a `serverbee` management CLI at `/usr/local/bin/serverbee`.

> **Note**: Re-running `install agent` adopts an existing `/usr/local/bin/serverbee-agent` instead of replacing it. Use `sudo serverbee upgrade agent -y` (or replace the binary manually) when you need to refresh an existing installation.
```

- [ ] **Step 3: Update the "Management" section (lines 258-268)**

Replace lines 258-268 with:

```markdown
### Management

```bash
sudo serverbee status              # View status of all components
sudo serverbee upgrade -y           # Upgrade all to latest version
sudo serverbee restart              # Restart all services
sudo serverbee config               # View current config
sudo serverbee config set <key> <value>  # Update config
sudo serverbee uninstall agent -y   # Uninstall agent
sudo serverbee uninstall server --purge  # Uninstall server + remove data
```
```

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs: update README.md management commands to use serverbee CLI"
```

---

### Task 8: Update `README.zh-CN.md` — management commands

**Files:**
- Modify: `README.zh-CN.md` — lines 33, 232-256, 258-268

- [ ] **Step 1: Update the feature description on line 33**

Change:
```
- **一体化部署管理** -- `deploy/serverbee.sh` 支持以交互式或无人值守方式安装、升级、查看状态、修改配置和卸载 Server/Agent
```
To:
```
- **一体化部署管理** -- `serverbee` CLI 支持以交互式或无人值守方式安装、升级、查看状态、修改配置和卸载 Server/Agent
```

- [ ] **Step 2: Update the "安装脚本" section (lines 232-256)**

Replace lines 232-256 with:

```markdown
### 安装脚本

通过 curl 一键安装：

```bash
# 服务端
curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh | sudo bash -s -- server

# Agent（替换为你的服务端地址和发现密钥）
curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh | sudo bash -s -- agent \
  --server-url http://YOUR_SERVER:9527 --discovery-key YOUR_KEY
```

安装脚本会自动将 `serverbee` 管理 CLI 安装到 `/usr/local/bin/serverbee`。

> **说明**：重复执行 `install agent` 时，如果 `/usr/local/bin/serverbee-agent` 已存在，脚本会直接沿用现有二进制而不会覆盖。需要刷新已安装版本时，请使用 `sudo serverbee upgrade agent -y`，或手动替换该二进制文件。
```

- [ ] **Step 3: Update the "管理命令" section (lines 258-268)**

Replace lines 258-268 with:

```markdown
### 管理命令

```bash
sudo serverbee status              # 查看所有组件状态
sudo serverbee upgrade -y           # 升级到最新版
sudo serverbee restart              # 重启所有服务
sudo serverbee config               # 查看当前配置
sudo serverbee config set <key> <value>  # 修改配置
sudo serverbee uninstall agent -y   # 卸载 Agent
sudo serverbee uninstall server --purge  # 卸载服务端并清除数据
```
```

- [ ] **Step 4: Commit**

```bash
git add README.zh-CN.md
git commit -m "docs: update README.zh-CN.md management commands to use serverbee CLI"
```

---

### Task 9: Update `apps/docs/content/docs/en/quick-start.mdx`

**Files:**
- Modify: `apps/docs/content/docs/en/quick-start.mdx` — lines 57-65 (management commands)

- [ ] **Step 1: Replace the management commands block**

Replace the curl-based management commands (lines 57-65) with:

```markdown
After installation, manage your ServerBee instance with the `serverbee` CLI:

```bash
sudo serverbee status              # View status
sudo serverbee upgrade -y           # Upgrade to latest
sudo serverbee restart              # Restart services
sudo serverbee config               # View config
sudo serverbee config set <key> <value>  # Update config
sudo serverbee uninstall server --purge   # Uninstall + remove data
```
```

- [ ] **Step 2: Commit**

```bash
git add apps/docs/content/docs/en/quick-start.mdx
git commit -m "docs: update en/quick-start.mdx to use serverbee CLI"
```

---

### Task 10: Update `apps/docs/content/docs/cn/quick-start.mdx`

**Files:**
- Modify: `apps/docs/content/docs/cn/quick-start.mdx` — lines 84-93 (management commands)

- [ ] **Step 1: Replace the management commands block**

Replace the curl-based management commands (lines 84-93) with:

```markdown
安装完成后，使用 `serverbee` CLI 管理你的实例：

```bash
sudo serverbee status              # 查看所有组件状态
sudo serverbee upgrade -y           # 升级到最新版
sudo serverbee restart              # 重启所有服务
sudo serverbee config               # 查看当前配置
sudo serverbee config set <key> <value>  # 修改配置
sudo serverbee uninstall agent      # 卸载 Agent
sudo serverbee uninstall server --purge   # 卸载服务端并清除数据
```
```

- [ ] **Step 2: Commit**

```bash
git add apps/docs/content/docs/cn/quick-start.mdx
git commit -m "docs: update cn/quick-start.mdx to use serverbee CLI"
```

---

### Task 11: Update `apps/docs/content/docs/en/deployment.mdx`

**Files:**
- Modify: `apps/docs/content/docs/en/deployment.mdx` — lines 56-67 (management section)

- [ ] **Step 1: Replace the management section**

Replace lines 56-67 with:

```markdown
After installation, manage your deployment with the `serverbee` CLI (automatically installed to `/usr/local/bin/serverbee`):

```bash
sudo serverbee status
sudo serverbee upgrade -y
sudo serverbee restart
sudo serverbee config
sudo serverbee env
sudo serverbee uninstall agent -y
```
```

This removes both the curl-based commands and the "cloned repo" note (no longer relevant since the CLI is always installed locally).

- [ ] **Step 2: Commit**

```bash
git add apps/docs/content/docs/en/deployment.mdx
git commit -m "docs: update en/deployment.mdx to use serverbee CLI"
```

---

### Task 12: Update `apps/docs/content/docs/cn/deployment.mdx`

**Files:**
- Modify: `apps/docs/content/docs/cn/deployment.mdx` — lines 56-67 (management section)

- [ ] **Step 1: Replace the management section**

Replace lines 56-67 with:

```markdown
安装完成后，使用 `serverbee` CLI 管理你的部署（安装时自动部署到 `/usr/local/bin/serverbee`）：

```bash
sudo serverbee status
sudo serverbee upgrade -y
sudo serverbee restart
sudo serverbee config
sudo serverbee env
sudo serverbee uninstall agent -y
```
```

This removes both the curl-based commands and the "克隆仓库" note.

- [ ] **Step 2: Commit**

```bash
git add apps/docs/content/docs/cn/deployment.mdx
git commit -m "docs: update cn/deployment.mdx to use serverbee CLI"
```

---

### Task 13: Update `apps/docs/content/docs/en/agent.mdx`

**Files:**
- Modify: `apps/docs/content/docs/en/agent.mdx` — lines 41-51 (management section)

- [ ] **Step 1: Replace the management section**

Replace lines 41-51 with:

```markdown
After installation, manage the agent with the `serverbee` CLI (automatically installed):

```bash
sudo serverbee status
sudo serverbee upgrade agent -y
sudo serverbee restart agent
sudo serverbee config agent
sudo serverbee uninstall agent -y
```
```

This removes the curl-based commands and the "cloned repo" note.

- [ ] **Step 2: Commit**

```bash
git add apps/docs/content/docs/en/agent.mdx
git commit -m "docs: update en/agent.mdx to use serverbee CLI"
```

---

### Task 14: Update `apps/docs/content/docs/cn/agent.mdx`

**Files:**
- Modify: `apps/docs/content/docs/cn/agent.mdx` — lines 32-42 (management section)

- [ ] **Step 1: Replace the management section**

Replace lines 32-42 with:

```markdown
安装完成后，使用 `serverbee` CLI 管理 Agent（安装时自动部署）：

```bash
sudo serverbee status
sudo serverbee upgrade agent -y
sudo serverbee restart agent
sudo serverbee config agent
sudo serverbee uninstall agent -y
```
```

This removes the curl-based commands and the "克隆仓库" note.

- [ ] **Step 2: Commit**

```bash
git add apps/docs/content/docs/cn/agent.mdx
git commit -m "docs: update cn/agent.mdx to use serverbee CLI"
```

---

### Task 15: Final verification

**Files:**
- All modified files

- [ ] **Step 1: Verify no stale `serverbee.sh` references remain**

Run:
```bash
grep -r "serverbee\.sh" --include="*.md" --include="*.mdx" --include="*.sh" . | grep -v node_modules | grep -v "2026-03-29-serverbee-sh-design"
```
Expected: no output (all references removed except the superseded design doc)

- [ ] **Step 2: Verify install.sh parses cleanly**

Run: `bash -n deploy/install.sh`
Expected: no output

- [ ] **Step 3: Verify `deploy/serverbee.sh` is gone**

Run: `ls deploy/serverbee.sh 2>&1`
Expected: `No such file or directory`

- [ ] **Step 4: Verify documentation site builds**

Run:
```bash
cd apps/docs && bun run build
```
Expected: build succeeds with no errors

- [ ] **Step 5: Commit any remaining fixes if needed**
