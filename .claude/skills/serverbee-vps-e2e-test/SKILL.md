---
name: serverbee-vps-e2e-test
description: Run the ServerBee install.sh deploy / agent-recover end-to-end regression on a real Linux VPS — cross-compile the current branch locally, push the binary or docker image to the VPS, then execute install.sh against it and verify HTTPS + agent connection. Use this whenever the user wants to validate deploy changes on a real VPS, smoke-test install.sh after editing it, verify the agent.toml refresh / recover flow, check that Caddy + Let's Encrypt automation still works, or test the binary vs docker agent install paths. Trigger on phrases like "用 VPS 跑下测试", "跑 deploy 回归", "测 recover 闭环", "test the installer on VPS", "VPS regression", "在 VPS 上验一下", "跑下 runbook" — even when the user doesn't name a specific runbook.
---

# ServerBee VPS e2e regression

End-to-end regression for ServerBee's `deploy/install.sh` against a real Linux VPS, using the **current branch's** code (not the released ghcr image). Two canonical runbooks already live in the repo:

- `tests/manual/full-deploy-e2e.md` — full lifecycle (server + agent docker + agent binary + uninstall)
- `tests/manual/agent-recover-e2e.md` — narrow recover-only flow (agent.toml refresh, ~3 min)

This skill orchestrates them autonomously. The runbooks are reference; SKILL.md routes + handles credentials + reports.

## When to use

Run this skill whenever the user wants to exercise the deploy flow on a real VPS — explicitly named or not. Concrete triggers:

- After editing `deploy/install.sh` (any path)
- After editing `crates/server/src/router/api/server.rs` recover endpoint or related routes
- After editing front-end `recover-agent-dialog.tsx` / `regenerate-code-dialog.tsx` / `add-server-dialog.tsx` mutation paths
- After editing `Dockerfile*` or the `docker-compose.*.yml` shapes install.sh generates
- After a sea-orm migration / OnboardingResponse change
- Whenever the user says some variation of "在 VPS 上跑一遍", "测下 deploy", "跑下 recover", "VPS 回归", "run the install.sh test"

Do **not** trigger for: `cargo test` / `bun run test` unit tests, local-only smoke tests on the user's laptop, or anything that doesn't touch deploy/install.

## Step 1 — Pick the runbook

Default to **full-deploy**. Narrow to recover-only if and only if the change is clearly localized to the recover slice.

| Recent change touches… | Use |
| --- | --- |
| `deploy/install.sh` main paths / `cmd_domain` / Caddyfile gen / `install_*_{server,agent}` | full-deploy |
| Migration / `auth.rs` middleware / OnboardingResponse / Dockerfiles | full-deploy |
| `Cargo.toml` / dep bumps where you want a smoke confirmation | full-deploy |
| Only the recover endpoint, recover-agent-dialog, or the `agent.toml` `else` branch (`toml_set` of `server_url` / `enrollment_code` / `token`) | agent-recover |
| User says "全部跑一遍" / "complete deploy" / "from scratch" | full-deploy |
| User says "只测 recover" / "fast" / "just the refresh" | agent-recover |
| Genuinely unsure | full-deploy (superset) |

State your choice in one sentence before going further: e.g. *"Running tests/manual/full-deploy-e2e.md because the change touches install.sh §install_docker_server."*

## Step 2 — Collect credentials (once, then stop asking)

The runbooks deliberately omit IPs, domains, emails, passwords — they go through shell variables. Ask the user for these in one batch:

- `VPS_IP` — IPv4 of the test VPS
- `VPS_USER` — usually `root`
- `VPS_PASS` — sshpass password (or path to ssh key if they prefer)
- `DOMAIN` — host with an A record already resolving to `VPS_IP`
- `ACME_EMAIL` — email for Let's Encrypt registration

If the user has a memory file pointing at a reusable test VPS (e.g. `~/.claude/projects/.../memory/reference_test_vps.md`), mention it so they only need to paste the rotating password + per-session domain. If anything is missing, ask once and stop — do not guess values, and do not invent placeholders for missing IPs.

Don't write these into any committed file. They only live in your local shell.

## Step 3 — Execute

Read the chosen runbook top to bottom. Follow it section by section, but keep these load-bearing details in mind — they are not obvious from the file names and have bitten previous runs:

- **Build**: `cargo zigbuild --release -p serverbee-server -p serverbee-agent --target x86_64-unknown-linux-musl`. Apple Silicon must not fall back to `docker buildx --platform linux/amd64` — QEMU emulation is 5-10× slower (~30-60 min cold vs ~4 min cold for zigbuild). Web first: `cd apps/web && bun install --frozen-lockfile && bun run build`.

- **Image tag**: the local image's tag *must* equal the GitHub-release version string with no suffix (today: `1.0.0-alpha.4`). `deploy/install.sh:745` sets `RESOLVED_VERSION=""`, which wipes any env override — you cannot bypass it with `RESOLVED_VERSION=v1.0.0-alpha.4-dev bash install.sh`, that has been tried and didn't work. The only working trick is matching the release tag. Keep a `-dev` alias too for `docker images` readability.

- **Binary mode test**: install.sh's `install_binary_agent` has an adopt-mode short-circuit at [`deploy/install.sh:1502`](../../../deploy/install.sh#L1502): `if [ -f "${INSTALL_DIR}/serverbee-agent" ] then ... "skipping download (adopting existing)"`. Before running install.sh in binary mode, `scp` your locally built `target/x86_64-unknown-linux-musl/release/serverbee-agent` to `/opt/serverbee/bin/serverbee-agent` and `chmod +x`. install.sh will then use *your* binary and still write agent.toml + systemd unit. Without this, install.sh downloads the released v1.0.0-alpha.4 binary from GitHub — useful for testing install.sh itself, but not testing your branch's binary.

- **Recover test** (the agent.toml `else` branch): the exact sequence is `uninstall agent --yes` (no `--purge` — that preserves agent.toml, which is the prerequisite for hitting the refresh branch) → recover endpoint with `revoke_immediately: true` → `install agent --enrollment-code <new>` → cat `/opt/serverbee/etc/agent.toml` and check the three fields (see §7.4 of the recover runbook). Directly re-running `install agent` without uninstall is rejected by install.sh's `meta_has` guard.

- **Onboarding**: a freshly installed server has `must_change_password=true` on the admin user. Only `POST /api/auth/onboarding` is whitelisted (see `is_onboarding_whitelisted` in `crates/server/src/middleware/auth.rs`); calling `PUT /api/auth/password` first returns `MUST_CHANGE_PASSWORD` and stops you. Login → `/api/auth/onboarding` with `new_password` → then everything else opens up.

- **Online state**: REST `/api/servers/{id}` never returns `online=true`; that field is `null` even with a healthy agent. Online status is broadcast over the browser WebSocket. To prove the agent is alive, look at `docker logs serverbee-server | grep "Agent.*connected"`, the systemd journal, or `ss -tnp | grep :443`.

## Step 4 — Execution mode

Run autonomously. The user's memory establishes "自主执行 / bypass permissions on / 不暂停询问" as the default. Skip mid-flow confirmation prompts and just keep going. Two exceptions that still require pausing:

1. Credentials missing → ask once, stop.
2. The VPS already has a *production-looking* ServerBee install with a different domain or unfamiliar data → stop, surface what you found, and ask before purging. The runbooks assume a disposable test box.

Don't make any git commits. Test runs aren't checked in. If during execution you discover the runbook itself is wrong, leave the file alone for this run — fix it in a follow-up commit after you finish reporting.

## Step 5 — Report

End with a summary in this exact shape so multiple runs are easy to compare:

```
## VPS e2e regression result

Runbook: <full-deploy | agent-recover>
VPS: <ipv4>, <distro/version>, <cpu/cores>
Domain: <domain>
Total: <m:ss>

### Stages
- [✓ | ✗ | -] cargo zigbuild                 (Xm Ys)
- [✓ | ✗ | -] docker save + scp + load       (Xs)
- [✓ | ✗ | -] install.sh install server --domain  (Xs)
- [✓ | ✗ | -] external HTTPS GET /healthz
- [✓ | ✗ | -] onboarding + create server
- [✓ | ✗ | -] install.sh install agent --method docker
- [✓ | ✗ | -] install.sh install agent --method binary
- [✓ | ✗ | -] recover flow (agent.toml three-field check)
- [✓ | ✗ | -] uninstall --purge → clean

### Evidence
<5-15 server-log lines around the most interesting transition: connected, disconnect, unauthorized, connected — copy verbatim>

### Anomalies
<one bullet per surprise; or "none">

### Verdict
<one sentence: PASS / PARTIAL / FAIL, plus the deciding stage>
```

Use `-` for stages the chosen runbook doesn't cover (e.g. recover-only skips most full-deploy stages). On a hard fail, name the runbook section, paste the exact error, and stop the remaining stages rather than charging through teardown.

## Failure-mode quick map

The canonical table is in [`tests/manual/full-deploy-e2e.md` §9](../../../tests/manual/full-deploy-e2e.md). The five you'll actually see:

- `[ERROR] Failed to get latest version from GitHub` → VPS can't reach api.github.com (firewall / DNS), or you tried to use `RESOLVED_VERSION` env to override (doesn't work, line 745 nukes it).
- `MUST_CHANGE_PASSWORD` on `/api/servers` → forgot `POST /api/auth/onboarding` first.
- compose `Pulling` from ghcr instead of using local image → local tag isn't `${PROD_TAG}` exactly. Re-`docker tag` and re-run.
- `serverbee-agent.service: status=78/CONFIG` → enrollment code expired/used; recover for a fresh one, clear `token` line, restart.
- `serverbee-agent is already installed (...). Use 'upgrade'` → meta-file guard; for recover test you want `uninstall agent --yes` (no `--purge`) first, then re-install with new code.

## Related skills / docs

- The two runbooks themselves contain step-by-step shell commands with expected outputs — read them, don't reinvent.
- The fix commit history that motivated these runbooks (latest first): `5747ecde` (front-end cache invalidation), `01b6fcd9` (install.sh agent.toml refresh), `2b40aeef` (server semver pre-release acceptance).
