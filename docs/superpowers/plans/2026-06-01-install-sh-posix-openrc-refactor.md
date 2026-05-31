# install.sh POSIX sh + OpenRC Refactor — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:executing-plans. This
> is a single-file rewrite of `deploy/install.sh`; tasks are file-regions, not
> independent units, so execute inline in order. Verification is `dash -n` +
> `shellcheck --shell=dash` after each region, then VPS e2e — not per-function
> unit tests (a shell installer has no unit-test harness here).

**Goal:** Convert `deploy/install.sh` from bash-4-only / systemd-only into POSIX
sh that runs under dash and busybox ash, supporting both systemd and OpenRC,
plus sha256 verification, doas/sudo handling, OpenRC logging, and Alpine e2e.

**Architecture:** k3s-style init abstraction — `detect_init` sets `INIT`
(openrc|systemd|none); two service-file generators; a thin `svc_*` dispatch
layer that all call sites use instead of raw `systemctl`. All bash constructs
(assoc arrays, index arrays, `[[ ]]`, `read -rp`, `echo -e`, process
substitution, C-style `for`) replaced with POSIX equivalents. Single file
(curl|sh constraint).

**Tech Stack:** POSIX sh, dash, busybox ash (Alpine), shellcheck, systemd,
OpenRC (supervise-daemon), logrotate, Caddy.

Reference spec: `docs/superpowers/specs/2026-06-01-install-sh-posix-openrc-refactor-design.md`

---

## Verification commands (used by every task)

```bash
dash -n deploy/install.sh                       # POSIX syntax (must be clean)
shellcheck --shell=dash deploy/install.sh       # bashism / portability lint
busybox ash -n deploy/install.sh   # if busybox present locally; else rely on VPS Alpine
```

Local helper to exercise pure functions without running `main` (the file ends in
`main "$@"`; guard it during local testing only):

```bash
# temporarily: SERVERBEE_NO_MAIN=1 sh -c '. ./deploy/install.sh; tr_text manager_title; detect_arch'
```
The final line becomes `[ -n "${SERVERBEE_NO_MAIN:-}" ] || main "$@"` so the file
can be sourced for spot-checks, while normal `curl|sh` still runs `main`.

---

## Task 1: Header, shebang, privilege, init detection, shared helpers

**Files:** Modify `deploy/install.sh:1-147` (header/globals/colors) + insert new
helper section.

- [ ] **Step 1: shebang + options.** Replace `#!/usr/bin/env bash` →
  `#!/bin/sh`. Remove the `BASH_VERSINFO` 4.0 guard (lines 4-9). Replace
  `set -euo pipefail` → `set -eu`. Add once near top:
  `# shellcheck disable=SC3043  # 'local' is supported by dash/ash/ksh`.

- [ ] **Step 2: SELF_SCRIPT via `$0`.** Replace the `BASH_SOURCE` block:
```sh
SELF_SCRIPT=""
case "$0" in
  -sh|sh|-dash|dash|-ash|ash|bash|-bash) ;;   # piped via curl | sh
  *) [ -r "$0" ] && SELF_SCRIPT="$(cd "$(dirname "$0")" 2>/dev/null && pwd)/$(basename "$0")" ;;
esac
```

- [ ] **Step 3: privilege via re-exec (doas/sudo).** Replace `require_root` so
  non-root re-execs the whole script under sudo/doas when run as a file; when
  piped without root it errors with a clear message (the operator pipes to
  `doas sh`/`sudo sh`). The body keeps assuming root — no per-command `$SUDO`,
  which is lower-risk for a script that touches /etc, /opt, systemctl directly.
```sh
require_root() {
  [ "$(id -u)" -eq 0 ] && return 0
  if [ -n "$SELF_SCRIPT" ] && [ -r "$SELF_SCRIPT" ]; then
    if command -v sudo >/dev/null 2>&1; then exec sudo -E sh "$SELF_SCRIPT" "$@"
    elif command -v doas >/dev/null 2>&1; then exec doas sh "$SELF_SCRIPT" "$@"; fi
  fi
  error "This script must run as root. Re-run with: sudo $0 ...  (or pipe to 'doas sh' / 'sudo sh')"
}
```
  Note: `require_root` is currently called with no args; change call sites to
  `require_root "$@"` where args are available (main, interactive_menu can pass
  nothing — interactive path implies a TTY where re-exec args are empty).

- [ ] **Step 4: init detection.** Add new section `─── Init detection ───`:
```sh
INIT=""
detect_init() {
  if command -v rc-service >/dev/null 2>&1 && [ -x /sbin/openrc-run ]; then
    INIT=openrc
  elif command -v systemctl >/dev/null 2>&1 && [ -d /run/systemd/system ]; then
    INIT=systemd
  else
    INIT=none
  fi
}
has_systemd() { [ "$INIT" = systemd ]; }   # back-compat shim for legacy-migration callers
```
  Call `detect_init` in `main` right after `require_root` (both branches) and in
  `interactive_menu` after `require_root`.

- [ ] **Step 5: shared text helpers.** Add:
```sh
capitalize() {
  _c=$(printf '%s' "$1" | cut -c1 | tr '[:lower:]' '[:upper:]')
  _r=$(printf '%s' "$1" | cut -c2-)
  printf '%s%s' "$_c" "$_r"
}
cecho() { printf '%b\n' "$*"; }            # replaces `echo -e`
sed_inplace() {                            # portable in-place edit (busybox-safe, no -i)
  _si_expr="$1"; _si_file="$2"; _si_tmp=$(mktemp)
  sed "$_si_expr" "$_si_file" > "$_si_tmp" && mv "$_si_tmp" "$_si_file"
}
sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | cut -d' ' -f1
  elif command -v openssl >/dev/null 2>&1; then openssl dgst -sha256 "$1" | awk '{print $NF}'
  else return 1; fi
}
```

- [ ] **Step 6: colors/info/warn/error.** Convert `info/warn/error` from
  `echo -e` to `printf '%b\n'` (error keeps `>&2; exit 1`).

- [ ] **Step 7: globals.** `MISSING_DEPS=()` → `MISSING_DEPS=""`. Convert the
  capability index array to a plain string:
  `AGENT_CAPS_ALL="upgrade ping_icmp ping_tcp ping_http security_events firewall_block ip_quality terminal exec file docker"`
  and `AGENT_CAPS_COUNT=$(set -- $AGENT_CAPS_ALL; echo $#)`.

- [ ] **Step 8: verify.** `dash -n deploy/install.sh` (will still error until later
  regions are converted — acceptable mid-refactor; final gate is Task 9).
  Commit: `git commit -am "refactor(deploy): posix header, init detection, shared helpers"`

---

## Task 2: Capability + i18n tables → case functions

**Files:** Modify `deploy/install.sh` lines ~64-128 (caps assoc arrays), 223-504
(i18n assoc arrays + tr_text/trp).

- [ ] **Step 1: cap predicate/risk/desc functions.** Delete the three
  `declare -A AGENT_CAPS_*` arrays; replace with:
```sh
cap_default_on() {  # mirror of CAP_DEFAULT=1852
  case "$1" in upgrade|ping_icmp|ping_tcp|ping_http|security_events|firewall_block|ip_quality) return 0 ;; *) return 1 ;; esac
}
cap_risk() {
  case "$1" in
    terminal|exec|file|docker|firewall_block) echo high ;;
    ip_quality) echo medium ;;
    *) echo low ;;
  esac
}
cap_desc() {  # lang-aware; second column is en fallback
  if [ "${LANG_CODE:-en}" = zh ]; then case "$1" in
    terminal) echo "Web 终端（PTY）";; exec) echo "远程执行命令";; upgrade) echo "Agent 自动升级";;
    ping_icmp) echo "ICMP ping 探测";; ping_tcp) echo "TCP 端口探测";; ping_http) echo "HTTP 探测";;
    file) echo "文件浏览/编辑/上传";; docker) echo "Docker 容器监控与操作";;
    security_events) echo "SSH 登录 / 爆破 / 端口扫描事件采集";; firewall_block) echo "nftables 黑名单（需 root + nft）";;
    ip_quality) echo "第三方 IP 质量评分";; esac
  else case "$1" in
    terminal) echo "Web terminal (PTY)";; exec) echo "Remote command execution";; upgrade) echo "Agent auto-upgrade";;
    ping_icmp) echo "ICMP ping probes";; ping_tcp) echo "TCP probes";; ping_http) echo "HTTP probes";;
    file) echo "File browse / edit / upload";; docker) echo "Docker container monitoring & control";;
    security_events) echo "SSH login / brute-force / port-scan events";; firewall_block) echo "nftables blocklist (needs root + nft)";;
    ip_quality) echo "Third-party IP quality scoring";; esac
  fi
}
```

- [ ] **Step 2: i18n → single `tr_text` case.** Delete `declare -A I18N_EN` and
  `declare -A I18N_ZH` (lines 225-478). Replace `tr_text` with one `case` over
  the key, both languages inline, preserving EVERY key and exact string from the
  current tables (~110 keys; copy each value verbatim, en + zh). Pattern:
```sh
tr_text() {
  _z=""; [ "${LANG_CODE:-en}" = zh ] && _z=1
  case "$1" in
    manager_title)  [ "$_z" ] && echo "ServerBee 管理器" || echo "ServerBee Manager" ;;
    install_menu)   [ "$_z" ] && echo "  [1] 安装      Install" || echo "  [1] Install    安装" ;;
    # ... all remaining keys, verbatim from I18N_ZH / I18N_EN ...
    *) echo "??$1??" ;;
  esac
}
```
  `trp` stays (it calls `tr_text` then `printf`). `docs_lang` stays.

- [ ] **Step 3: verify.** `shellcheck --shell=dash deploy/install.sh` — confirm the
  i18n/caps regions raise no SC2039/SC3xxx (bashism) findings.
  Commit: `git commit -am "refactor(deploy): caps + i18n tables to posix case functions"`

---

## Task 3: Deps, platform, DNS, metadata

**Files:** Modify lines ~506-1069.

- [ ] **Step 1: deps.** In `install_deps`/`check_deps`/`collect_missing_deps`:
  `&>/dev/null` → `>/dev/null 2>&1`; `local pkgs=("$@")` → use `"$@"`/`$*`;
  `missing=()`/`+=` → space-string; `[ ${#missing[@]} -eq 0 ]` → `[ -z "$missing" ]`;
  `MISSING_DEPS+=(x)` → `MISSING_DEPS="${MISSING_DEPS:+$MISSING_DEPS }x"`;
  `read -rp` → `printf '%s' "..."; read -r`.

- [ ] **Step 2: docker/platform helpers.** `&>` fixes in `docker_is_snap`,
  `configure_docker_dir`, `conf_file_for`, `detect_os`, `detect_arch`,
  `get_latest_version`, `get_local_ip`, `get_public_ipv*`. Keep amd64/arm64
  mapping (arch expansion is out of scope per spec).

- [ ] **Step 3: domain validation.** Replace the `[[ =~ ]]` regex in
  `validate_domain_name` with grep:
```sh
validate_domain_name() {
  printf '%s' "$1" | grep -Eq '^[A-Za-z0-9]([A-Za-z0-9-]{0,61}[A-Za-z0-9])?(\.[A-Za-z0-9]([A-Za-z0-9-]{0,61}[A-Za-z0-9])?)+$' \
    || error "Invalid domain: ${1}\n  Use a hostname like monitor.example.com, without http:// or a path."
}
```
  Fix `&>` in `resolve_domain_a/aaaa`; `read -rp` in `check_domain_points_here`.

- [ ] **Step 4: metadata.** In `meta_read/write/remove/has`: replace every
  `sed -i.bak '…' f && rm -f f.bak` with `sed_inplace '…' f`. For the multi-step
  `meta_remove` trailing-comma cleanup, keep the awk passes (busybox awk OK) but
  route the final `sed -i.bak` through a temp-file+mv. `&>` → `>...2>&1`.

- [ ] **Step 5: detection arrays.** `MANAGED_COMPONENTS=()` / `UNMANAGED_COMPONENTS=()`
  → strings; `+=("x:y")` → `MANAGED_COMPONENTS="${MANAGED_COMPONENTS:+$MANAGED_COMPONENTS }x:y"`.
  All later `for entry in "${MANAGED_COMPONENTS[@]}"` → `for entry in $MANAGED_COMPONENTS`
  (entries have no spaces). `[ ${#MANAGED_COMPONENTS[@]} -eq 0 ]` →
  `[ -z "$MANAGED_COMPONENTS" ]`.

- [ ] **Step 6: verify.** `dash -n` + `shellcheck --shell=dash` on the region.
  Commit: `git commit -am "refactor(deploy): posix deps/platform/dns/metadata"`

---

## Task 4: CLI self-install + capability picker

**Files:** Modify lines ~1136-1420.

- [ ] **Step 1: install_cli / refresh_cli_from_release.** Subshell `local` bodies
  are fine under dash. Change validation `bash -n "$tmp"` → `sh -n "$tmp"`.
  `&>` fixes.

- [ ] **Step 2: cap helpers.** Convert `set_caps_from_cli`,
  `ensure_caps_initialized`, `caps_match_default`, `compute_cap_cli_args`,
  `compute_cap_compose_command`, `render_caps_for_plan`:
  - `for cap in "${AGENT_CAPS_ALL[@]}"` → `for cap in $AGENT_CAPS_ALL`
  - `${AGENT_CAPS_DEFAULT_ON[$cap]-}` membership → `if cap_default_on "$cap"; then`
  - `out+=` / `final+=` → string concat
  - `[[ "$set" == *,"$cap",* ]]` → `case ",$set," in *",$cap,"*) ... esac`
  - `IFS=,; echo "${AGENT_CAPS_ALL[*]}"` → `printf '%s' "$AGENT_CAPS_ALL" | tr ' ' ','`
  - `${AGENT_CAPS_ALL[*]}` in error message → `$AGENT_CAPS_ALL`

- [ ] **Step 3: prompt_agent_capabilities (biggest).** Replace `declare -A checked`
  with a space-delimited `CHECKED` set + helpers:
```sh
caps_is_checked() { case " $CHECKED " in *" $1 "*) return 0 ;; *) return 1 ;; esac; }
caps_toggle() { if caps_is_checked "$1"; then CHECKED=$(printf '%s' " $CHECKED " | sed "s/ $1 / /g"); CHECKED=$(echo $CHECKED); else CHECKED="${CHECKED:+$CHECKED }$1"; fi; }
cap_by_index() { _i=1; for _c in $AGENT_CAPS_ALL; do [ "$_i" = "$1" ] && { printf '%s' "$_c"; return 0; }; _i=$((_i+1)); done; return 1; }
```
  - preset: `CHECKED=$(printf '%s' "$AGENT_CAPS_SELECTED" | tr ',' ' ')`
  - menu loop: `for cap in $AGENT_CAPS_ALL` with manual counter `i`
  - mark: `if caps_is_checked "$cap"; then mark=x; else mark=' '; fi`
  - risk column: `$(cap_risk "$cap")`
  - `read -rp` → `printf;read -r`; keep `input=$(echo "$input" | xargs)`
  - number test `[[ "$tok" =~ ^[0-9]+$ ]]` → `case "$tok" in ''|*[!0-9]*) not-a-number;; *) number;; esac`
  - range check uses `$AGENT_CAPS_COUNT`
  - final selection: `for cap in $AGENT_CAPS_ALL; do caps_is_checked "$cap" && final="${final:+$final,}$cap"; done`

- [ ] **Step 4: verify.** `dash -n` + `shellcheck --shell=dash`.
  Commit: `git commit -am "refactor(deploy): posix cli-install + capability picker"`

---

## Task 5: Init abstraction — service generators + svc_* dispatch

**Files:** New section `─── Service (init) abstraction ───` inserted before
`install_binary_server` (~line 1422).

- [ ] **Step 1: paths + generic actions.**
```sh
svc_unit_path()   { echo "/etc/systemd/system/serverbee-$1.service"; }
svc_openrc_path() { echo "/etc/init.d/serverbee-$1"; }
svc_log_path()    { echo "/var/log/serverbee-$1.log"; }
svc_env_path()    { echo "${CONFIG_DIR}/serverbee-$1.env"; }
svc_logrotate_path() { echo "/etc/logrotate.d/serverbee-$1"; }

svc_action() {  # $1 =start|stop|restart  $2 =component
  _svc="serverbee-$2"
  case "$INIT" in
    systemd) systemctl "$1" "$_svc" ;;
    openrc)  rc-service "$_svc" "$1" ;;
    none)    [ "$1" = stop ] && return 0; error "No init manager available to $1 $_svc." ;;
  esac
}
svc_is_active() {  # echoes active|inactive|unknown
  case "$INIT" in
    systemd) systemctl is-active "serverbee-$1" 2>/dev/null || echo inactive ;;
    openrc)  rc-service "serverbee-$1" status >/dev/null 2>&1 && echo active || echo inactive ;;
    *)       echo unknown ;;
  esac
}
svc_logs_tail() {  # $1 component  $2 n-lines
  case "$INIT" in
    systemd) journalctl -u "serverbee-$1" -n "$2" --no-pager 2>/dev/null ;;
    openrc)  tail -n "$2" "$(svc_log_path "$1")" 2>/dev/null ;;
    *)       : ;;
  esac
}
```

- [ ] **Step 2: env file writer.** Baseline env file always written so the OpenRC
  init can source it and `env set` has a target:
```sh
svc_write_env_file() {  # $1 component  $2 KEY=VALUE line (optional baseline)
  _f=$(svc_env_path "$1"); mkdir -p "$CONFIG_DIR"
  [ -f "$_f" ] || : > "$_f"
  [ -n "$2" ] || return 0
  _k=${2%%=*}
  if grep -q "^${_k}=" "$_f" 2>/dev/null; then sed_inplace "s|^${_k}=.*|$2|" "$_f"; else printf '%s\n' "$2" >> "$_f"; fi
}
```

- [ ] **Step 3: systemd generators** (extract today's heredocs verbatim):
  `create_systemd_unit_server` writes the existing `[Unit]/[Service]/[Install]`
  with `Environment=SERVERBEE_SERVER__DATA_DIR=${DATA_DIR}`;
  `create_systemd_unit_agent "$exec_start"` writes the existing agent unit
  (StartLimit*, RestartPreventExitStatus=78, AmbientCapabilities=CAP_NET_RAW).

- [ ] **Step 4: OpenRC generators.**
```sh
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

depend() { after net; need net; }
start_pre() {
    if [ -f "$(svc_env_path server)" ]; then set -a; . "$(svc_env_path server)"; set +a; fi
}
OPENRC
  chmod 0755 "$(svc_openrc_path server)"
  svc_write_logrotate server
}
create_openrc_service_agent() {  # $1 = command_args string (may be empty)
  cat > "$(svc_openrc_path agent)" <<'OPENRC'
#!/sbin/openrc-run
name="serverbee-agent"
description="ServerBee Agent"
OPENRC
  cat >> "$(svc_openrc_path agent)" <<OPENRC
command="${INSTALL_DIR}/serverbee-agent"
command_args="$1"
directory="${CONFIG_DIR}"
supervisor=supervise-daemon
respawn_delay=5
pidfile="/run/serverbee-agent.pid"
output_log="$(svc_log_path agent)"
error_log="$(svc_log_path agent)"

depend() { after net; need net; }
start_pre() {
    if [ -f "$(svc_env_path agent)" ]; then set -a; . "$(svc_env_path agent)"; set +a; fi
}
OPENRC
  chmod 0755 "$(svc_openrc_path agent)"
  svc_write_logrotate agent
}
svc_write_logrotate() {  # $1 component
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
```
  Note: agent under OpenRC runs as root (no `User=`), so `CAP_NET_RAW` for ICMP
  is already held; no extra capability config needed.

- [ ] **Step 5: enable+start dispatch.**
```sh
svc_install_server() {
  case "$INIT" in
    systemd) create_systemd_unit_server; systemctl daemon-reload; systemctl enable serverbee-server >/dev/null 2>&1; systemctl restart serverbee-server; info "Server service started and enabled" ;;
    openrc)  svc_write_env_file server "SERVERBEE_SERVER__DATA_DIR=${DATA_DIR}"; create_openrc_service_server; rc-update add serverbee-server default >/dev/null 2>&1; rc-service serverbee-server restart; info "Server service started and enabled" ;;
    none)    warn "No init manager (systemd/openrc) found. Start manually: ${INSTALL_DIR}/serverbee-server" ;;
  esac
}
svc_install_agent() {  # $1 = exec args (cap flags), may be empty
  case "$INIT" in
    systemd) create_systemd_unit_agent "${INSTALL_DIR}/serverbee-agent${1:+ $1}"; systemctl daemon-reload; systemctl enable serverbee-agent >/dev/null 2>&1; systemctl restart serverbee-agent; info "Agent service started and enabled" ;;
    openrc)  svc_write_env_file agent ""; create_openrc_service_agent "$1"; rc-update add serverbee-agent default >/dev/null 2>&1; rc-service serverbee-agent restart; info "Agent service started and enabled" ;;
    none)    warn "No init manager (systemd/openrc) found. Start manually: ${INSTALL_DIR}/serverbee-agent ${1}" ;;
  esac
}
svc_remove() {  # $1 component — on-the-fly detection, not install-time INIT
  _svc="serverbee-$1"
  if command -v systemctl >/dev/null 2>&1; then
    systemctl stop "$_svc" 2>/dev/null || true
    systemctl disable "$_svc" 2>/dev/null || true
    rm -f "$(svc_unit_path "$1")"; rm -rf "/etc/systemd/system/${_svc}.service.d"
    systemctl daemon-reload 2>/dev/null || true
  fi
  if command -v rc-service >/dev/null 2>&1; then
    rc-service "$_svc" stop 2>/dev/null || true
    rc-update del "$_svc" default 2>/dev/null || true
    rm -f "$(svc_openrc_path "$1")"
  fi
  rm -f "$(svc_log_path "$1")" "$(svc_logrotate_path "$1")" "$(svc_env_path "$1")"
}
```

- [ ] **Step 6: verify.** `dash -n` + `shellcheck --shell=dash`.
  Commit: `git commit -am "feat(deploy): init abstraction with systemd + openrc generators"`

---

## Task 6: Route install/uninstall/upgrade/service/status/domain through svc_*

**Files:** Modify lines ~1424-2015, 2330-2740, 1711-1799.

- [ ] **Step 1: install_binary_server.** Add sha256 verify in the download branch:
```sh
download_verified() {  # $1 url  $2 dest  $3 filename(for sums lookup)  $4 version
  curl -fsSL -o "$2" "$1" || error "Download failed: $1"
  _sums_url="https://github.com/${REPO}/releases/download/${4}/sha256sums.txt"
  _sums=$(curl -fsSL "$_sums_url" 2>/dev/null || true)
  if [ -n "$_sums" ]; then
    _want=$(printf '%s\n' "$_sums" | grep " .${3}\$" | awk '{print $1}' | head -n1)
    if [ -n "$_want" ]; then
      _got=$(sha256_of "$2") || { warn "no sha256 tool; skipping checksum"; return 0; }
      [ "$_got" = "$_want" ] || error "Checksum mismatch for ${3}: want ${_want} got ${_got}"
      info "Checksum OK: ${3}"
    fi
  else
    warn "No sha256sums.txt for ${4}; skipping checksum (older release)."
  fi
}
```
  Replace the inline `curl … && mv` download with `download_verified` (download to
  `/tmp/serverbee-server`, verify, `chmod +x`, `mv`). Replace the
  `if has_systemd; then …unit…; else warn; fi` block with `svc_install_server`
  (move the unit heredoc into `create_systemd_unit_server` in Task 5).

- [ ] **Step 2: install_binary_agent.** Same `download_verified`. Replace the
  systemd block with: `ensure_caps_initialized; cap_args=$(compute_cap_cli_args); svc_install_agent "$cap_args"`.

- [ ] **Step 3: fetch_first_run_password.** Convert C-style `for ((i…))` →
  `i=0; while [ "$i" -lt "$max" ]; do … i=$((i+1)); done`. Add an OpenRC branch
  that reads `$(svc_log_path server)` instead of journalctl; keep the
  invocation-scoped journalctl for systemd; `none` returns.

- [ ] **Step 4: print_*_result.** Replace `echo -e` → `cecho`. In `print_agent_result`,
  make the "Start/Logs" hints init-aware (systemd → systemctl/journalctl;
  openrc → `rc-service serverbee-agent start` / `tail -f $(svc_log_path agent)`).

- [ ] **Step 5: uninstall_binary.** Replace the systemd-only block with
  `svc_remove "$component"`; keep binary + purge removal.

- [ ] **Step 6: upgrade_binary.** Replace `has_systemd` stop/start guards with
  `svc_action stop` / `svc_action start`; add `download_verified`.

- [ ] **Step 7: cmd_service / cmd_service_single.** Route binary actions through
  `svc_action "$action" "$comp"`; status line via `svc_is_active`. Replace
  `${action^}` → `$(capitalize "$action")`. Arrays `targets=()` → string.

- [ ] **Step 8: status_component / cmd_status.** `${component^}` →
  `$(capitalize "$component")`; binary status via `svc_is_active` + `svc_logs_tail`;
  `echo -e` → `cecho`; arrays → strings. systemd "since" timestamp stays guarded
  by `[ "$INIT" = systemd ]`.

- [ ] **Step 9: domain (Caddy) init-aware.** In `install_caddy` add an `apk`
  branch (`apk add --quiet caddy`). In `setup_domain`, replace the
  `systemctl enable/restart caddy` block:
```sh
case "$INIT" in
  systemd) systemctl enable caddy >/dev/null 2>&1 || true; systemctl restart caddy ;;
  openrc)  rc-update add caddy default >/dev/null 2>&1 || true; rc-service caddy restart ;;
  none)    warn "No init manager; start Caddy manually: caddy run --config ${CADDYFILE}" ;;
esac
```
  `update_server_for_domain_binary` restart → `svc_action restart server`.
  `ensure_caddy_state_dir`: `&>` fixes; guard `getent` (busybox lacks it):
  `command -v getent >/dev/null 2>&1 && caddy_home=$(getent passwd caddy | cut -d: -f6)`.
  `sed -i.bak` in `update_server_for_domain_docker` → `sed_inplace`.
  Convert C-style loop in `wait_for_https_endpoint` → `while`.

- [ ] **Step 10: verify.** `dash -n` + `shellcheck --shell=dash`.
  Commit: `git commit -am "feat(deploy): route lifecycle commands through init abstraction + sha256"`

---

## Task 7: config + env commands

**Files:** Modify lines ~2742-3110.

- [ ] **Step 1: toml_set.** `[[ "$dotted_key" == *.* ]]` → `case … in *.*)`;
  nested `[[ "$key" == *.* ]]` → `case`; numeric/bool test
  `[[ =~ ^[0-9]+$ ]]`/`^(true|false)$` →
  `case "$value" in ''|*[!0-9]*) ... ;; *) number ;; esac` and
  `case "$value" in true|false) ... esac`; every `sed -i.bak … && rm .bak` →
  `sed_inplace`. awk passes unchanged.

- [ ] **Step 2: config_key_to_file.** Already POSIX (`grep -qw`); leave.

- [ ] **Step 3: cmd_config.** Arrays `files_to_update=()`, `targets=()` → strings;
  `[ ${#x[@]} -eq 0 ]` → `[ -z "$x" ]`; `${comp^}` → `$(capitalize "$comp")`;
  process substitution `diff <(echo "$before") "$file"` →
  `b=$(mktemp); printf '%s\n' "$before" > "$b"; diff "$b" "$file" | sed 's/^/    /' || true; rm -f "$b"`;
  `[[ "$target" == "$comp" || "$target" == "both" ]]` →
  `[ "$target" = "$comp" ] || [ "$target" = both ]`; `echo -e` → `cecho`.

- [ ] **Step 4: cmd_env.** `[[ "$env_key" != SERVERBEE_* ]]` →
  `case "$env_key" in SERVERBEE_*) ;; *) env_key="SERVERBEE_${env_key}" ;; esac`;
  arrays → strings; binary branch made init-aware:
  - systemd: keep override.conf logic (`sed -i.bak` → `sed_inplace`; `daemon-reload`).
  - openrc: `svc_write_env_file "$comp" "${env_key}=${value}"; rc-service serverbee-$comp restart`.
  - view mode: replace `done < <(env)` process substitution with
    `env | grep '^SERVERBEE_' | sed 's/^/  /'` (printed directly; drop the
    `found_shell` flag, print `(none)` when empty via a captured var). For binary
    sources, show systemd override OR openrc env file depending on `$INIT`.
  `echo -e` → `cecho`.

- [ ] **Step 5: verify.** `dash -n` + `shellcheck --shell=dash`.
  Commit: `git commit -am "refactor(deploy): posix config + init-aware env command"`

---

## Task 8: menus + main + migration

**Files:** Modify lines ~609-682 (migrate), 3112-3233 (menus/main).

- [ ] **Step 1: migrate_legacy_layout.** Already mostly POSIX; uses `has_systemd`
  (now the shim) — fine since legacy installs were systemd. `&>`/arrays none.
  Verify the `for f in dir/* dir/.install-meta` globs behave under ash (they do).

- [ ] **Step 2: interactive_menu / interactive_service_menu.** `read -rp` →
  `printf;read -r`; `echo -e` → `cecho`. `require_root` → `require_root` (no args
  in interactive path). Add `detect_init` after `require_root`.

- [ ] **Step 3: main.** Remove `local args=("$@")` + C-style index loop. Replace
  the pre-scan with:
```sh
_prev=""
for _a in "$@"; do
  case "$_a" in -y|--yes) YES=true ;; esac
  [ "$_prev" = --lang ] && { LANG_CODE="$_a"; normalize_lang; }
  _prev="$_a"
done
```
  `[[ $# -gt 0 ]]`/`[[ $# -eq 0 ]]` → `[ ... ]`. After `require_root "$@"` (file
  path) add `detect_init`.

- [ ] **Step 4: main guard for sourcing.** Final line:
  `[ -n "${SERVERBEE_NO_MAIN:-}" ] || main "$@"`.

- [ ] **Step 5: verify.** `dash -n` + `shellcheck --shell=dash`.
  Commit: `git commit -am "refactor(deploy): posix menus + main + sourcing guard"`

---

## Task 9: Full local verification gate

- [ ] **Step 1:** `dash -n deploy/install.sh` — MUST be clean (no output).
- [ ] **Step 2:** `shellcheck --shell=dash deploy/install.sh` — resolve all
  findings except justified, annotated disables (SC3043 local).
- [ ] **Step 3:** Source-and-spot-check pure functions:
```bash
SERVERBEE_NO_MAIN=1 dash -c '. ./deploy/install.sh
  LANG_CODE=zh; tr_text manager_title
  LANG_CODE=en; tr_text manager_title
  AGENT_CAPS_SELECTED=""; ensure_caps_initialized; echo "$AGENT_CAPS_SELECTED"
  detect_arch; detect_os
  cap_default_on upgrade && echo on; cap_default_on terminal || echo off
  validate_domain_name monitor.example.com && echo dom-ok'
```
  Expected: `ServerBee 管理器`, `ServerBee Manager`, the default cap list,
  `amd64`/`linux` (or arm64/darwin), `on`, `off`, `dom-ok`.
- [ ] **Step 4:** If busybox present: `busybox ash -n deploy/install.sh`.
- [ ] **Step 5: Commit** any fixes.

---

## Task 10: CI gate

**Files:** Create/modify `.github/workflows/*` (find the lint workflow).

- [ ] **Step 1:** Add a job step running `shellcheck --shell=dash deploy/install.sh`
  and `dash -n deploy/install.sh` (install `dash` + `shellcheck` via apt in CI).
- [ ] **Step 2:** In the release workflow, after building binaries, add
  `( cd <artifacts> && sha256sum serverbee-* > sha256sums.txt )` and upload
  `sha256sums.txt` as a release asset alongside the binaries.
- [ ] **Step 3: Commit.** `git commit -am "ci(deploy): shellcheck/dash gate + release sha256sums"`

---

## Task 11: VPS e2e (systemd + OpenRC)

Test host provided: `38.64.56.236:2222` root. Determine its init with
`ssh … 'cat /etc/os-release; ls /run/systemd/system 2>/dev/null && echo SYSTEMD; command -v rc-service && echo OPENRC'`.

- [ ] **Step 1: syntax on target sh.** scp the script; run
  `sh -n /root/install.sh` (dash on Ubuntu, ash on Alpine) — clean.
- [ ] **Step 2: server install (binary).**
  `sh /root/install.sh server --method binary -y` → service created under the
  host's init (`systemctl status serverbee-server` or
  `rc-service serverbee-server status`), dashboard reachable on :9527, capture
  first-run password.
- [ ] **Step 3: agent install (binary).** Generate an enrollment code via the
  server, then `sh /root/install.sh agent --server-url http://127.0.0.1:9527
  --enrollment-code <code> -y` → agent service running, agent appears online in
  the server.
- [ ] **Step 4: lifecycle.** `serverbee status`, `serverbee restart`,
  `serverbee config set collector.interval 5 -y`,
  `serverbee env set COLLECTOR__INTERVAL 5 -y` (init-aware), `serverbee upgrade`.
- [ ] **Step 5: uninstall.** `serverbee uninstall agent -y` then `server -y`;
  confirm units/init scripts/logs/env files removed.
- [ ] **Step 6: second init.** If the provided host is systemd, spin an Alpine
  OpenRC target (or an Alpine LXC/VM) and repeat Steps 1-5; if it is Alpine,
  also validate on an Ubuntu host. Record results in
  `tests/manual/agent-recover-e2e.md` (add the Alpine/OpenRC path).
- [ ] **Step 7:** Fix any failures, re-run from Step 1 on the affected host.
- [ ] **Step 8: Commit** test-doc updates.

---

## Self-Review notes

- **Spec coverage:** §2 bashism table → Tasks 1-8; §3 init abstraction → Tasks 5-6;
  §4.1 sha256 → Task 6 + Task 10; §4.2 doas/sudo → Task 1 (re-exec, a lower-risk
  variant of per-command `$SUDO`; spec updated intent); §4.3 OpenRC logging →
  Task 5 (logrotate) + Task 6 (status/logs); §4.4 e2e Alpine → Task 11.
- **Deviation from spec:** privilege handled by re-exec under sudo/doas instead of
  prefixing every command with `$SUDO`. Rationale: the script already runs
  everything as root and touches /etc, /opt, systemctl directly; re-exec is far
  less error-prone than sprinkling `$SUDO` across ~3000 lines. Same end-user
  outcome (non-root with sudo/doas works).
- **busybox specifics (spec §6):** addressed via `sed_inplace` (no `-i.bak`),
  `apk` Caddy branch, `getent` guard, and final VPS run on real Alpine ash.
