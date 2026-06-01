# install.sh POSIX sh + OpenRC Refactor — Design

- **Date**: 2026-06-01
- **Status**: Approved
- **Owner**: deploy / installer
- **Target file**: `deploy/install.sh` (currently 3234 lines, bash 4+, systemd-only)

## 1. Goals & Constraints

### Goal

Refactor `deploy/install.sh` from "bash 4+ / systemd-only" into "POSIX sh /
systemd + OpenRC dual-init", preserving all existing behaviour and folding in
four adjacent improvements.

Existing behaviour that MUST be preserved:

- Nine subcommands: `install`, `uninstall`, `upgrade`, `status`, `start`,
  `stop`, `restart`, `config`, `env`, `domain`, plus the interactive menu.
- Four install modes: Server/Agent × Binary/Docker.
- Bilingual i18n (en/zh), language detection + cache.
- Agent capability picker (interactive multi-select + `--caps` CLI), mapped to
  `--allow-cap`/`--deny-cap` (binary) and compose `command:` (docker).
- Caddy / DNS / domain HTTPS automation (Cloudsmith apt / COPR, A/AAAA checks).
- Legacy FHS-split → `/opt/serverbee` layout auto-migration.
- `.install-meta` metadata tracking, unmanaged-component detection.
- Management CLI self-install (`/usr/local/bin/serverbee`) + refresh on upgrade.
- snap-confined Docker directory adaptation.

### Hard Constraints

1. **Single file.** `curl | sh` requires one file. The refactor happens
   in-place in `deploy/install.sh`, organised by section comments. No module
   split, no external sourced files.
2. **Portability.** Shebang `#!/bin/sh`. Must run under `dash` and busybox
   `ash` (Alpine). CI gates with `shellcheck --shell=dash` + `dash -n`.
3. **Backward compatibility.** Existing systemd installs (their `.install-meta`,
   units, and the `/opt` legacy migration) must keep working. Upgrading an
   already-installed host must be seamless.
4. **Functional equivalence.** POSIX-ization only changes the "syntax shell".
   Business logic (i18n strings, capability matrix, Caddy/DNS validation,
   snap-docker adaptation) stays identical.

### Init scope

Only **systemd** and **OpenRC**. sysvinit and launchd are out of scope; when
neither systemd nor OpenRC is present, `INIT=none` triggers a "start manually"
fallback (no error), matching today's degraded behaviour.

## 2. Bashism → POSIX Replacement

| Current (bashism) | POSIX replacement |
|---|---|
| `#!/usr/bin/env bash` + `BASH_VERSINFO` 4.0+ guard | `#!/bin/sh`, guard removed |
| `set -euo pipefail` | `set -eu` (no pipefail — see risk below) |
| `declare -A I18N_EN/ZH` (~110 keys ×2) | single `tr_text()` `case`, one branch per key, both languages inline |
| `declare -A AGENT_CAPS_*` (4 assoc arrays) | `case` functions: `cap_default_on()`, `cap_risk()`, `cap_desc()` |
| index arrays `AGENT_CAPS_ALL`, `MISSING_DEPS`, `MANAGED_COMPONENTS` | space-separated strings + `for x in $list` |
| `[[ ... ]]` / `[[ $x =~ re ]]` (domain validation, caps set membership) | `[ ... ]` / `case` glob / `printf %s \| grep -qE` |
| `read -rp "p" v` | `printf '%s' "p"; read -r v` |
| `echo -e "${RED}.."` | `printf '%b\n'` (wrapped in `info/warn/error`) |
| `cmd &>/dev/null` | `cmd >/dev/null 2>&1` |
| `str+="x"` | `str="${str}x"` |
| `local x=$(cmd)` | keep `local` (with `# shellcheck disable=SC3043`) but split into two lines to avoid swallowing the exit code |
| `${BASH_SOURCE[0]}` (file vs `curl\|bash`) | `$0` + readability test |
| associative `checked[$cap]` in caps picker | space-separated `checked` string + `case` membership |
| `${#ARR[@]}` length checks | string-empty checks / counting words |
| `$'\n'` literal newlines (compose command) | real newlines in a quoted heredoc/printf |

### pipefail risk

`dash` has no `pipefail`. In pipelines like `curl … | grep | sed`, a curl
failure no longer propagates. Most existing call sites already null-check the
result (e.g. `get_latest_version` errors on empty). During the rewrite, every
"download-then-parse" pipeline gets an explicit empty/exit-code check to restore
the protection pipefail used to give.

### `local` note

`local` is technically non-POSIX but supported by dash, busybox ash, ksh — it is
the universally-accepted exception (rustup does the same). Keep it, annotate with
`# shellcheck disable=SC3043` once near the top.

### `$0` / pipe detection

Today the script uses `${BASH_SOURCE[0]}` to tell "run as a file" (copy self to
CLI) from "piped via `curl | bash`" (download released copy). Under sh, use:

```sh
SELF_SCRIPT=""
case "$0" in
  -sh|sh|-dash|dash|bash|-bash) ;;     # piped
  *) [ -r "$0" ] && SELF_SCRIPT="$(cd "$(dirname "$0")" 2>/dev/null && pwd)/$(basename "$0")" ;;
esac
```

## 3. Init Abstraction (k3s-style)

### Detection

```sh
detect_init() {
  if command -v rc-service >/dev/null 2>&1 && [ -x /sbin/openrc-run ]; then
    INIT=openrc
  elif command -v systemctl >/dev/null 2>&1 && [ -d /run/systemd/system ]; then
    INIT=systemd
  else
    INIT=none
  fi
}
```

- OpenRC is checked first (an Alpine box may carry both `systemctl` shims and
  real OpenRC; it actually runs OpenRC).
- `[ -d /run/systemd/system ]` is stricter than today's
  `systemctl is-system-running` heuristic — it confirms systemd is PID 1, so a
  container that merely has `systemctl` installed is not misdetected.
- `INIT=none` → "start manually" fallback (current degraded behaviour).

### Unified dispatch layer

All call sites use this layer; nothing calls `systemctl` directly anymore.

```
svc_install <comp>          # generate unit / init script + enable + (re)start
svc_enable  <comp>
svc_start | svc_stop | svc_restart <comp>
svc_status  <comp>          # running state (systemctl is-active / rc-service status)
svc_logs    <comp> [n]      # journalctl / tail openrc log file
svc_remove  <comp>          # disable + delete unit/init script + reload
```

Each is `case "$INIT" in systemd) … ;; openrc) … ;; none) … ;; esac`.

### Two generators

- `create_systemd_unit_server()` / `_agent()`: keep today's heredocs verbatim
  (Agent keeps `AmbientCapabilities=CAP_NET_RAW`, `RestartPreventExitStatus=78`,
  `StartLimitIntervalSec/Burst`, `LimitNOFILE`).
- `create_openrc_service_server()` / `_agent()`: k3s-style:

```sh
#!/sbin/openrc-run

name="serverbee-agent"
description="ServerBee Agent"
command="/opt/serverbee/bin/serverbee-agent"
command_args="--deny-cap terminal ..."     # from compute_cap_cli_args
supervisor=supervise-daemon
respawn_delay=5
output_log="/var/log/serverbee-agent.log"
error_log="/var/log/serverbee-agent.log"
pidfile="/run/serverbee-agent.pid"

depend() {
  after net
  need net
}

start_pre() {
  set -o allexport
  [ -f /opt/serverbee/etc/serverbee-agent.env ] && . /opt/serverbee/etc/serverbee-agent.env
  set +o allexport
}
```

Notes:
- Agent runs as root under OpenRC (today's systemd unit has no `User=`, also
  root). Root already holds `CAP_NET_RAW`, so ICMP ping works without extra
  capability config.
- The server unit sets `SERVERBEE_SERVER__DATA_DIR` via the env file
  (`start_pre` sources it), mirroring the systemd `Environment=` line.

### Paths by init

| | systemd | OpenRC |
|---|---|---|
| service file | `/etc/systemd/system/serverbee-<c>.service` | `/etc/init.d/serverbee-<c>` (`chmod 0755`) |
| env file | unit `Environment=` / drop-in | `/opt/serverbee/etc/serverbee-<c>.env` |
| enable | `systemctl enable` | `rc-update add serverbee-<c> default` |
| logs | `journalctl -u` | `tail /var/log/serverbee-<c>.log` |

### Uninstall robustness

Uninstall detects the supervisor on the fly with `command -v systemctl` /
`command -v rc-service` rather than relying on install-time `$INIT`. `--purge`
still required to remove config/data; `/var/log/serverbee-<c>.log` and the
logrotate file are removed on uninstall.

## 4. Adjacent Improvements

### 4.1 sha256 verification

- New helper:
```sh
sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | cut -d' ' -f1
  elif command -v openssl >/dev/null 2>&1; then openssl dgst -sha256 "$1" | awk '{print $NF}'
  else return 1; fi
}
```
- Release/CI publishes `sha256sums.txt` (one line per asset) as a release asset.
  **Requires a `.github/workflows` change**: `sha256sum dist/* > sha256sums.txt`
  uploaded alongside the binaries.
- `download_binary` flow becomes: download to temp → fetch+parse expected hash
  from `sha256sums.txt` → compare → atomic `mv`. If `sha256sums.txt` is missing
  for a release (older releases), warn and continue (do not hard-fail, to keep
  installing pre-checksum releases working).

### 4.2 doas / sudo privilege abstraction

Replace the hard "must be root" with a docker-style `SUDO` variable:

```sh
SUDO=""
if [ "$(id -u)" -ne 0 ]; then
  if command -v sudo >/dev/null 2>&1; then SUDO="sudo"
  elif command -v doas >/dev/null 2>&1; then SUDO="doas"
  else error "must run as root, or install sudo/doas"; fi
fi
```

All privileged commands are prefixed with `$SUDO`. (Alpine ships `doas`, not
`sudo`, so this matters for the OpenRC path.)

### 4.3 OpenRC logging + logrotate

- OpenRC init script redirects `output_log`/`error_log` to
  `/var/log/serverbee-<c>.log`.
- Generate `/etc/logrotate.d/serverbee-<c>` (`missingok notifempty copytruncate`).
- `svc_logs` / `show_status` tail this file under OpenRC; `journalctl` under
  systemd (unchanged).

### 4.4 e2e Alpine coverage

Extend `tests/manual/agent-recover-e2e.md` (and the VPS e2e runbook) with an
Alpine/OpenRC path: install agent via binary on Alpine, verify
`rc-service serverbee-agent status`, recover flow, and (if applicable) Caddy.

## 5. Structure, Testing, Out-of-scope

### Structure

Single file, reorganised under section banners. New sections:
`─── Init abstraction ───`, `─── Checksum ───`. The static templates
`deploy/serverbee-{server,agent}.service` remain the reference source for the
systemd heredocs.

### Testing

- **CI**: add `shellcheck --shell=dash deploy/install.sh` and
  `dash -n deploy/install.sh` to GitHub Actions.
- `refresh_cli_from_release` validation switches `bash -n` → `sh -n`.
- **Local**: `dash -n` + busybox `ash` syntax check.
- **VPS e2e**: Ubuntu (systemd) and Alpine (OpenRC), both binary install paths;
  verify service running, agent connects, status/logs/restart/uninstall.

### Out of scope (YAGNI)

- sysvinit, launchd, macOS install (covered by `INIT=none` degrade).
- Docker path (compose generation) is essentially unchanged — only swept for
  POSIX syntax and `$SUDO`.
- No jq; `.install-meta` keeps grep/sed manipulation, converged to POSIX.
- No module split (single-file constraint).

## 6. Risks

- **pipefail loss** — mitigated by explicit checks on download/parse pipelines.
- **busybox vs GNU tool differences** (`sed -i`, `grep`, `awk`) — Alpine's
  busybox `sed`/`awk` differ from GNU. The `.install-meta`, `toml_set`, and
  `meta_*` awk/sed logic must be validated on busybox specifically (e.g. avoid
  `sed -i.bak` GNU-isms where busybox differs; use temp-file + `mv`).
- **OpenRC supervise-daemon availability** — requires OpenRC ≥ 0.21 (Alpine has
  it). Older OpenRC would need `command_background`; out of scope.
- **Checksum rollout** — old releases without `sha256sums.txt` must still
  install (warn-and-continue).
