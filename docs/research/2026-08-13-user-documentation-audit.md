# ServerBee User Documentation Audit

**Audit date:** 2026-08-13
**Repository revision:** `0334375f05fe12865cdbf1dd28814f31c8c842c0` (`main`)
**Primary scope:** `apps/docs/content/docs/zh`, with English pages, the public documentation site, the user-facing landing page, installer, current source, deployment artifacts, and public GitHub releases used as corroborating sources.

## Executive summary

The Chinese documentation is broad and unusually detailed for a pre-1.0 self-hosted project. It explains the Server/Agent model, first-run credentials, enrollment, reverse proxies, capabilities, upgrades, capacity planning, and migration recovery. The MDX typecheck and production build both pass.

It is not currently safe to call the published user journey correct and complete, however. Six issues can materially change a user's install or product decision:

1. Direct Chinese documentation URLs currently redirect to the English home page and discard the requested page.
2. The documentation landing page says MIT, while the repository is AGPL-3.0-or-later.
3. GitHub marks `v1.0.0-beta.1` as a stable/latest release, contradicting both the documented channel contract and the installer implementation.
4. Both the recommended Docker Compose example and installer-managed Docker mode mishandle the advertised configuration file; the published example also has a fragile health check and incorrect self-healing claim.
5. The documented piped “interactive wizard” cannot read user input, because the script itself occupies standard input.
6. The release installed by the documented direct-IP HTTP command crashes on the Servers page, blocking the documented Agent enrollment journey.

The user journey is **partial**, not failed end-to-end: the happy-path binary install and Agent enrollment instructions align closely with the installer and Web UI, but deployment choice, HTTPS prerequisites, backup/restore, uninstall, and general troubleshooting need consolidation before a new user can operate the product confidently.

## Post-audit remediation status

The repository source has since been repaired and independently re-reviewed through Herdr with Grok. The review converged on a maintainable release policy rather than a temporary documentation-only beta flag:

- the installer now defaults to `auto`, preferring a suffix-free stable release and falling back to the newest prerelease only when no stable release exists;
- the selected `auto`/`stable`/`beta` policy is stored per component and reused by upgrades, including an already-current explicit channel change;
- generated Web/iOS commands and normal copy/paste docs no longer hard-code `--channel beta`;
- release automation moves GHCR `:beta` only for prereleases and `:latest` only for stable releases;
- current raw Docker and binary examples pin the workspace package version, with automated drift checks against `Cargo.toml`;
- bilingual route/content contracts, Docker paths and health checks, installer pipe prompts, backup/purge warnings, troubleshooting, configuration coverage, licensing, protocol version, capability ownership, rate limits, and mobile acquisition claims were corrected or added.

The following are **external release/deployment blockers and are not fixed merely by the source diff**:

1. `https://docs.serverbee.app/zh...` still needs the updated docs deployment and a live-host locale smoke test; the audited deployment redirects Chinese deep links to English and still exposes stale MIT copy.
2. GitHub still marks `v1.0.0-beta.1` as non-prerelease/Latest. The workflow is hardened for future releases, but existing public metadata requires an authorized GitHub update.
3. The already-published `v1.0.0-beta.1` Web bundle still crashes on remote plain-HTTP origins. The UUID fallback and regression test exist in source, but users need a new published artifact; the quick start now warns them to use HTTPS meanwhile.
4. GHCR `:latest` still resolves to the old 0.9.4-era image, and `:beta` will not exist until a new prerelease runs the updated workflow. Current docs therefore pin `1.0.0-beta.1` or recommend the installer instead of claiming those external tags have already changed.

Source checks can establish repository correctness, not these live-state outcomes. Do not mark the overall published user journey complete until all four checks pass against public endpoints/artifacts.

## Status and severity model

- **Confirmed:** reproduced directly, or matched current documentation to current source/artifacts.
- **Partial:** some of the journey is documented and correct, but important user-facing steps or variants are missing or scattered.
- **Unverified:** a claim could not be established without a destructive, privileged, account-dependent, or long-running external acceptance test.
- **Critical:** blocks access or gives legally/security-significant false information.
- **High:** likely to break installation, upgrades, or production operation.
- **Medium:** material ambiguity, contradiction, or missing operational guidance.
- **Low:** polish, navigation, or conservative accuracy issue that does not normally block operation.

## Verification performed

- Read all 28 Chinese MDX pages and their navigation order; consulted English pages where the translations diverged.
- Compared instructions with `deploy/install.sh`, Dockerfiles, the Makefile, Rust configuration defaults, Agent capability/state definitions, API routes, Web onboarding command generation, and the release workflow.
- Ran `make docs-typecheck`: **passed**.
- Ran `make docs-build`: **passed**. The build emitted only a Node deprecation warning and a JavaScript chunk-size warning.
- Queried the public GitHub release API and release asset list with `gh`: **11 expected artifacts exist** (10 platform binaries plus `sha256sums.txt`).
- Requested representative public Chinese documentation URLs with a browser user agent and `Accept-Language: zh-CN`: **all returned HTTP 307 with `Location: /en`**.
- Consulted Docker's official restart-policy, health-check, and Compose bind-mount documentation, and SQLite's official backup/corruption guidance.
- Backed up and removed an existing ServerBee-only installation on the designated test VPS, then executed the documented direct-IP binary Server install from a clean managed layout. The installer selected `v1.0.0-beta.1`, verified its checksum, started systemd with zero restarts, and returned healthy `/healthz` and `/api/about` responses.
- Completed first browser login and mandatory password change. The documented Servers page then crashed reproducibly over direct-IP HTTP with `crypto.randomUUID is not a function`; a session-only polyfill allowed the remaining journey to continue without changing the app or VPS.
- Created a Server enrollment offer in the Web UI and installed the binary Agent on the same test VPS. The first GitHub asset download returned HTTP 503; an immediate retry succeeded with checksum verification. The Agent appeared Online with live metrics, reconnected after `serverbee restart agent`, and `serverbee upgrade agent -y` correctly reported the installed version as current.
- Verified both Server and Agent stayed active with zero restarts, the Agent configuration was `0600 root:root`, and the co-located GitHub Actions Runner stayed active with zero restarts. The second offered VPS was not accessed after SSH reported a changed host key.
- No existing product documentation or implementation was modified; this audit report is the only repository artifact.

## User-journey coverage

| Journey | Status | Assessment |
|---|---|---|
| Understand the product | Partial | Architecture and features are clear, but the landing page publishes the wrong license and overgeneralizes systemd/single-file behavior. |
| Choose a deployment | Partial | Railway, Docker, binary, and source builds exist, but there is no early decision table and the recommended method conflicts with the primary command. |
| Check prerequisites | Partial | Root, architecture, Docker, DNS, and port 9527 are mentioned; domain installs omit required inbound 80/443 and free-port checks. |
| Install Server | Confirmed / Partial | The binary happy path matches the installer. Docker documentation has a non-functional config mount and fragile health check. |
| Obtain first credentials and log in | Confirmed | Random first-run credentials, log locations, forced password change, and secure-cookie behavior are described. |
| Register an Agent | Confirmed / Partial | Quick start matches the Web-generated command; the capabilities page contains a separate unusable install command. |
| Configure HTTPS/reverse proxy | Partial | Caddy and Nginx examples are substantial, but prerequisites and uninstall side effects are not complete. |
| Operate securely | Partial | TOTP, RBAC, trusted proxies, secure cookies, capabilities, and checklists are covered; capability ownership and rate-limit defaults contradict other pages. |
| Upgrade | Partial | CLI, Docker, binary, channels, migration behavior, and rollback constraints are documented; public release metadata currently invalidates channel selection. |
| Backup and restore | Partial | SQLite, stopped-file, and Docker volume backups exist, plus detailed migration recovery; prerequisites, built-in API endpoints, and a single normal restore runbook are incomplete. |
| Uninstall | Partial | Commands exist, but preservation versus purge and Caddy/state leftovers are not explained as a user lifecycle. |
| Troubleshoot | Partial | Excellent migration-failure detail and a few feature-specific sections exist; there is no general install/login/connection/TLS troubleshooting entry point. |

## Findings

### DOC-001: Published Chinese documentation routes discard the language and page

**Severity:** Critical
**Status:** Confirmed

On 2026-08-13, each of these requests returned `307` with `Location: /en`:

```text
https://docs.serverbee.app/zh
https://docs.serverbee.app/zh/docs
https://docs.serverbee.app/zh/docs/quick-start
https://docs.serverbee.app/zh/docs/configuration
```

Following the redirect lands on the English home page, not the requested Chinese page. The same behavior was observed on the legacy Vercel hostname. This breaks Chinese links from the README and installer even though the source explicitly declares both languages in `apps/docs/src/lib/i18n.ts:3-6` and the Chinese content builds successfully.

**User impact:** a Chinese-speaking user cannot reliably open the documentation they were sent, and deep links lose their destination.

**Recommendation:** fix the deployed routing/middleware first, then add production smoke checks for `/zh`, `/zh/docs/quick-start`, and `/zh/docs/configuration` that assert a 200 Chinese page rather than merely following redirects.

### DOC-002: The documentation landing page states the wrong software license

**Severity:** Critical
**Status:** Confirmed

The English landing page says “MIT” at `apps/docs/src/components/landing/translations.ts:9` and `:84`; the Chinese landing page repeats it at `:91` and `:160`. The workspace package metadata declares `AGPL-3.0-or-later` at `Cargo.toml:9-13`, the repository `LICENSE` contains GNU AGPL v3, and the Chinese README links AGPL at `README.zh-CN.md:145-147`.

**User impact:** license terms affect whether an individual or company may deploy, modify, or offer the software as a service. This is a product-decision error, not cosmetic copy drift.

**Recommendation:** replace all MIT claims with the repository's exact SPDX license expression, and add a simple automated check that public license copy agrees with `Cargo.toml`.

### DOC-003: Public release metadata violates the documented stable/beta contract

**Severity:** High
**Status:** Confirmed

The deployment guide says prereleases do not move `:latest` at `apps/docs/content/docs/zh/deployment.mdx:39-53`, and says the installer defaults to stable while `--channel beta` selects a published prerelease at `:85`. The installer resolves stable from GitHub's `/releases/latest` endpoint and beta only from releases with `"prerelease": true` at `deploy/install.sh:679-703`. The workflow is also intended to mark hyphenated versions as prereleases and not latest at `.github/workflows/release.yml:300-307`; GHCR `:latest` is only emitted for versions without a prerelease suffix at `:250-263`.

Current public state contradicts that contract:

```json
{"tag_name":"v1.0.0-beta.1","prerelease":false,"draft":false}
```

GitHub's `/releases/latest` endpoint also returns `v1.0.0-beta.1`, and `gh release list` labels it `Latest`. All 11 expected release assets are present, so this is a metadata/channel issue rather than a missing-artifact issue.

**User impact:** the default “stable” binary installer receives a beta; `--channel beta` cannot discover that same release because it is not marked prerelease. GHCR and binary users may therefore follow different effective “stable” versions.

**Recommendation:** correct the GitHub release metadata, verify both installer selectors against the live API, and make release acceptance check the public `prerelease`/`latest` fields, not only workflow inputs.

### DOC-004: Domain HTTPS prerequisites are incomplete and can lead to failed issuance or needless backend exposure

**Severity:** High
**Status:** Confirmed

The quick-start prerequisites tell all users to allow port 9527 at `apps/docs/content/docs/zh/quick-start.mdx:18-23`. The domain path then promises automatic Caddy and certificate setup at `:44-55`, but does not say that inbound TCP 80 and 443 must be available and permitted.

The installer explicitly rejects a non-Caddy listener on 80/443 at `deploy/install.sh:2043-2055`, installs and configures Caddy at `:2003-2120`, binds ServerBee to `127.0.0.1:9527` for domain installs at `:2123-2143`, and validates the public HTTPS endpoint at `:2165-2191`.

**User impact:** certificate issuance can fail even when the documented prerequisite checklist is satisfied. Conversely, users may expose 9527 publicly even though the domain topology is designed to keep it loopback-only.

**Recommendation:** split prerequisites by topology. IP/HTTP should require 9527; managed domain/HTTPS should require A/AAAA correctness, inbound 80/443, free local listeners on 80/443, and no public 9527 exposure.

### DOC-005: The recommended Docker Compose example is not a reliable production recipe

**Severity:** High
**Status:** Confirmed

The recommended Compose example at `apps/docs/content/docs/zh/deployment.mdx:87-129` has three independent problems:

1. It mounts `./server.toml` to `/app/server.toml` at `:99-105`, but never tells the user to create the host file. Compose short bind syntax can create a directory when the source does not exist. More importantly, the image has no `/app` working directory (`Dockerfile.server:1-12`) and the server only reads `/etc/serverbee/server.toml`, `/opt/serverbee/etc/server.toml`, or `server.toml` in its current directory (`crates/server/src/config.rs:606-613`). The documented mount is therefore not a supported configuration location.
2. Its health check uses `localhost` at `deployment.mdx:104-109`. The installer already documents in source that Alpine BusyBox `wget` prefers `::1` while ServerBee listens on IPv4, and uses `127.0.0.1` instead (`deploy/install.sh:1817-1824`).
3. The guide says three failed checks cause Docker to restart the container at `deployment.mdx:503-515`. Docker health checks set container health status; restart policies act when a container exits. Docker does not restart a merely unhealthy container by default.
4. Installer-managed Docker mode creates a configuration file at `deploy/install.sh:1792-1803`, but its generated Compose file mounts only the data volume at `:1805-1828`. The documented `serverbee config` workflow therefore edits a file the container never receives.
5. Docker naming is inconsistent: `apps/docs/content/docs/zh/server.mdx:35-43` creates `serverbee`, while quick-start, deployment, installer output, and subsequent log commands use `serverbee-server`.

Primary references: [Docker restart policies](https://docs.docker.com/engine/containers/start-containers-automatically/), [Compose healthcheck](https://docs.docker.com/reference/compose-file/services/#healthcheck), and [Compose bind mounts](https://docs.docker.com/reference/compose-file/services/#volumes).

**User impact:** a copy-paste deployment can ignore configuration or fail its mount, report a healthy server as unhealthy, and fail to self-heal despite the documentation promising it will.

**Recommendation:** define one supported Docker config path and mount it in both hand-written and installer-generated Compose; use `127.0.0.1`; describe health status accurately; and validate the exact published and generated snippets in CI with `docker compose config`, a configuration-effect assertion, and a running health check.

### DOC-006: The capability-page installation command cannot register an Agent

**Severity:** High
**Status:** Confirmed

`apps/docs/content/docs/zh/capabilities.mdx:94-102` suggests:

```bash
curl -fsSL https://your-server/install.sh | sh -s -- --caps terminal,file
```

The Server does not expose an `install.sh` route. The installer requires a `server` or `agent` component (`deploy/install.sh:2430-2433`) and requires `--server-url` plus an enrollment code for non-interactive Agent installation (`:2484-2499`). The actual Web UI emits the GitHub raw installer URL and all required arguments at `apps/web/src/components/server/add-server-dialog.tsx:577-585`.

The same page tells installer users to edit `/etc/serverbee/agent.toml` at `capabilities.mdx:59-75`, but the current installer layout is `/opt/serverbee/etc/agent.toml` (`apps/docs/content/docs/zh/agent.mdx:168-178`). A syntactically correct capability edit can therefore have no effect.

**User impact:** a security-conscious user following the capability-specific onboarding path cannot install or enroll the Agent.

**Recommendation:** use the same generated command shape as the Add Server dialog and show `--caps` only as an addition to a complete Agent command.

### DOC-007: Deployment recommendation and actual quick-start behavior conflict

**Severity:** Medium
**Status:** Confirmed

The quick start says Docker is recommended for Server at `apps/docs/content/docs/zh/quick-start.mdx:7-14`, but both primary Server commands at `:34-55` omit `--method docker`. Piped non-interactive installation defaults to binary at `deploy/install.sh:2440-2448`. The page then unconditionally says the script downloads a binary and creates a systemd unit at `quick-start.mdx:61-68`, while a later callout says to add `--method docker` at `:70-72`.

The same prerequisite section says binary mode requires systemd (`quick-start.mdx:20-22`), but the installer detects and fully manages OpenRC (`deploy/install.sh:520-529`, `:1548-1667`). The Server page repeats systemd-only language at `apps/docs/content/docs/zh/server.mdx:9-24`.

**User impact:** the user cannot tell which supported method is actually recommended, and Alpine/OpenRC users may conclude that a supported path is unavailable.

**Recommendation:** put a deployment decision table before commands, with persistence, HTTPS, host requirements, upgrade, backup, and uninstall implications. Make the default copy-paste command match the stated recommendation, or state clearly that binary is the non-interactive default.

### DOC-008: Capability ownership and defaults contradict each other across user pages

**Severity:** Medium
**Status:** Confirmed

The dedicated capability page correctly says the Agent exclusively owns capabilities and lists seven default-on capabilities at `apps/docs/content/docs/zh/capabilities.mdx:7-10` and `:34-46`. This matches `CAP_DEFAULT = 1852` in `crates/common/src/constants.rs:47-66` and the configuration reference at `apps/docs/content/docs/zh/configuration.mdx:426-435`.

The Agent page instead lists only upgrade and three ping capabilities as default-on and says the Server can further tighten capability access at `apps/docs/content/docs/zh/agent.mdx:224-237`. Current source explicitly states the Server cannot modify Agent capabilities at `crates/agent/src/config.rs:30-37`.

**User impact:** users can misunderstand both the default network/security behavior and the trust boundary between a compromised Server and an Agent host.

**Recommendation:** make the dedicated capability page canonical, link to it rather than restating a subset, and never describe Server-side tightening as an available control.

### DOC-009: “No residue” Agent claims and uninstall guidance do not match persisted state

**Severity:** Medium
**Status:** Confirmed

The Agent guide says the Agent is only one binary, creates no folders or residual files, and can be removed by deleting the binary and config at `apps/docs/content/docs/zh/agent.mdx:80-84`. In current source, temporary capability grants live under `/var/lib/serverbee/capability_grants.json` (`crates/agent/src/config.rs:38-75`) and security state defaults to `/var/lib/serverbee/security` (`:82-114`). Enrollment also persists an Agent run token in configuration.

Installer-managed uninstall intentionally preserves configuration and data unless `--purge` is used (`deploy/install.sh:2520-2535`, `:2642-2660`). Domain setup can install Caddy and edit `/etc/caddy/Caddyfile` (`:2003-2120`), but uninstall does not reverse those changes. User docs only scatter uninstall commands across quick-start, Agent, and deployment pages; they do not explain preserved data, purge, or proxy cleanup.

**User impact:** users cannot confidently remove credentials/state or predict what data survives a reinstall. A domain uninstall can leave an active proxy configuration and a package the script installed.

**Recommendation:** add one lifecycle section that inventories binary, config, data, state, system service, Docker volume/image, CLI, and Caddy effects; show non-purge and purge outcomes separately; provide explicit Caddy cleanup guidance without automatically deleting unrelated user configuration.

### DOC-010: Backup and restore documentation is capable but fragmented and omits the built-in API path

**Severity:** Medium
**Status:** Partial

The deployment guide correctly recommends SQLite's online backup command and provides stopped-service and Docker-volume backups at `apps/docs/content/docs/zh/deployment.mdx:418-474`. It also contains a strong migration-failure workflow, new-volume Docker restore, post-restore checks, and off-host backup advice later at `:580-768`.

The normal restore section at `:476-501`, however, only gives a systemd main-file replacement. The online backup and cron examples assume `sqlite3` and `/backups` already exist, while the installer dependency check does not install either (`deploy/install.sh:415-444`). A supported built-in path is also undocumented: `POST /api/settings/backup` uses `VACUUM INTO` for a consistent downloadable database (`crates/server/src/router/api/setting.rs:74-139`), and `POST /api/settings/restore` stages an upload, validates its SQLite header, replaces the database, and requires restart (`:142-245`). The API reference only says “backup/restore endpoints” without methods or examples at `apps/docs/content/docs/zh/api-reference.mdx:125-143`.

Primary references: [SQLite backup API](https://www.sqlite.org/backup.html) and [SQLite corruption guidance](https://sqlite.org/howtocorrupt.html).

**User impact:** a normal operator has to assemble a safe runbook from distant sections, may schedule a backup command that cannot run, and may not discover the built-in consistent backup endpoint.

**Recommendation:** create one normal backup/restore runbook per deployment method, document prerequisites and destination creation, add API-key examples for the built-in endpoints, require an off-host copy and integrity test, and link the migration disaster-recovery section as the exceptional path.

### DOC-011: Rate-limit defaults are deployment-specific but documented as universal

**Severity:** Medium
**Status:** Confirmed

The security page says Agent registration defaults to three attempts per 15 minutes at `apps/docs/content/docs/zh/security.mdx:98-113`, and the Server example repeats `register_max = 3` at `apps/docs/content/docs/zh/server.mdx:98-100`. The canonical Rust default is 10 at `crates/server/src/config.rs:177-192`, which the configuration reference correctly reports at `apps/docs/content/docs/zh/configuration.mdx:290-295`. Railway specifically overrides it to three at `deploy/railway/Dockerfile:58-62`.

**User impact:** operators may rely on a stricter registration defense than their binary or standard Docker deployment actually has, or troubleshoot lockouts using the wrong threshold.

**Recommendation:** state “default 10; Railway template override 3,” and label examples as recommended hardening rather than built-in defaults.

### DOC-012: General troubleshooting is not organized around first-time user failures

**Severity:** Medium
**Status:** Partial

The navigation has no troubleshooting or FAQ page (`apps/docs/content/docs/zh/meta.json:1-38`). Migration failures are covered deeply at the end of the deployment reference, and mobile and terminal have feature-specific troubleshooting, but there is no single entry point for:

- first-run password not found;
- login failure caused by `secure_cookie` and HTTP/HTTPS mismatch;
- port 9527, 80, or 443 conflicts;
- DNS/ACME failure;
- expired or consumed enrollment offers;
- Agent service logs and restart behavior across systemd, OpenRC, and Docker;
- stable/beta channel resolution; or
- post-upgrade version and schema verification.

Deployment is placed at the end of the Reference section (`meta.json:34-37`), after users are asked to understand numerous product features.

**User impact:** common recoverable failures require source knowledge or searching several long pages, reducing confidence in the installer even when it behaves correctly.

**Recommendation:** add a task-oriented troubleshooting page reachable from quick start and installer output. Use symptom, check, expected result, fix, and escalation fields; include commands for every supported supervisor/deployment mode.

### DOC-013: Resource requirements understate measured Agent memory

**Severity:** Low
**Status:** Confirmed

The deployment requirements table says 10 MB minimum and 20 MB recommended for the Agent at `apps/docs/content/docs/zh/deployment.mdx:527-539`. The dedicated measured resource page reports approximately 27 MB systemd cgroup memory and 34 MB RSS at steady state at `apps/docs/content/docs/zh/resource-usage.mdx:9-26`. Its Server production figure (140–170 MB) explicitly depends on `MALLOC_ARENA_MAX=2`, and it warns of observed 800 MB–1 GB RSS without that setting at `resource-usage.mdx:38-48`; the published deployment snippets do not apply the recommended environment variable.

**User impact:** very small VPS/container allocations can be planned below the project's own measured steady state.

**Recommendation:** use the measured steady-state range in deployment sizing, keep cold-start/binary size separate from runtime memory, and either include the documented allocator setting in production templates or qualify the recommendation with platform-specific evidence.

### DOC-014: Installer output points to a legacy documentation hostname

**Severity:** Low
**Status:** Confirmed

The current README uses `https://docs.serverbee.app` for user documentation at `README.zh-CN.md:94-119`, while the installer hard-codes `https://server-bee-docs.vercel.app` at `deploy/install.sh:30-40` and prints that host after Server and Agent installs at `:1948-1985`.

**User impact:** users receive inconsistent branding and an unnecessary dependency on an implementation hostname. At audit time, both hosts also exhibit DOC-001's broken Chinese redirect.

**Recommendation:** emit the canonical custom domain and smoke-test every URL the installer prints.

### DOC-015: The documented piped interactive wizard cannot accept input

**Severity:** High
**Status:** Confirmed

The quick start describes a no-argument pipe as an interactive wizard at `apps/docs/content/docs/zh/quick-start.mdx:7-14` and `:57-59`:

```bash
curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh | sudo sh
```

The installer correctly recognizes that piped standard input is not a TTY for language selection (`deploy/install.sh:168-170`, `:215-227`), but then calls the menu unconditionally when there are no arguments and reads its choice from the same standard input (`:3491-3519`, `:3582-3590`). By then standard input is the script stream/EOF. A read-only pipe reproduction printed the menu and exited with status 1 rather than accepting a choice.

**User impact:** the advertised easiest path fails at the first interaction.

**Recommendation:** document download-then-execute for interactive use, or make the script consistently read prompts from `/dev/tty`; keep the piped form for fully parameterized non-interactive commands.

### DOC-016: Agent enrollment correction recommends a command the installer rejects

**Severity:** High
**Status:** Confirmed

The Agent page correctly says a second `install agent` fails and users should upgrade at `apps/docs/content/docs/zh/agent.mdx:35-47`. Its “correct a wrong enrollment code” section then says users can rerun `serverbee install agent` to rewrite the config at `:146-164`. The installer rejects any install when metadata already contains that component at `deploy/install.sh:2435-2438`.

The preceding `serverbee config set enrollment_code ... -y` path is the actionable correction; the reinstall fallback is not.

**User impact:** a user already diagnosing a failed enrollment is sent into a guaranteed second failure.

**Recommendation:** remove the reinstall advice, document `config set`, log verification, offer replacement, and the separate post-claim re-enrollment flow.

### DOC-017: The Agent Docker example lacks the configuration required to enroll

**Severity:** High
**Status:** Confirmed

The Agent Docker example at `apps/docs/content/docs/zh/agent.mdx:80-103` mounts `/etc/serverbee` but neither creates `agent.toml` nor supplies Server URL and enrollment code environment variables. `SERVERBEE_SERVER_URL` is mandatory according to `apps/docs/content/docs/zh/configuration.mdx:169-179`.

**User impact:** the container can start but cannot register with a Server when copied as documented.

**Recommendation:** either remove this discouraged path or provide a complete, persistent, enrollment-safe Compose example with the same required inputs and capability semantics as binary installation.

### DOC-018: Firewall cleanup asks users to operate a Server-side capability control that does not exist

**Severity:** High
**Status:** Confirmed

The firewall guide says to turn off `CAP_FIREWALL_BLOCK` in “Capabilities settings” so the Server pushes a reset at `apps/docs/content/docs/zh/firewall.mdx:70-81`. The canonical capability documentation says Web/iOS is read-only and only the Agent host can change capabilities (`apps/docs/content/docs/zh/capabilities.mdx:104-110`), matching `crates/agent/src/config.rs:30-37`.

**User impact:** a user decommissioning host firewall automation cannot perform the documented first step and may leave ServerBee's nftables table in place.

**Recommendation:** document an Agent-host sequence (`deny = ["firewall_block"]`, controlled restart/sync behavior, verification, then manual `nft` cleanup when needed) that matches the implemented ownership model.

### DOC-019: User-facing transport language overstates encryption in the default path

**Severity:** Medium
**Status:** Confirmed

The Chinese landing page says terminal, file, Docker, and command operations all use the same “encrypted channel” at `apps/docs/src/components/landing/translations.ts:113-116`. The default IP quick start is explicitly plain HTTP at `apps/docs/content/docs/zh/quick-start.mdx:34-42`, which also means plain WebSocket transport on that topology.

**User impact:** a user can expose control-plane traffic believing transport encryption is already provided by the product.

**Recommendation:** say the channel is encrypted when deployed through HTTPS/WSS, and make HTTPS the production recommendation wherever remote-control capabilities are discussed.

### DOC-020: Version-specific facts and configuration coverage have already drifted

**Severity:** Medium
**Status:** Confirmed

Examples of current drift include:

- the architecture page says protocol version 4 at `apps/docs/content/docs/zh/architecture.mdx:119-123`, while current source is version 6 at `crates/common/src/constants.rs:1-5`;
- the Agent page calls its example “all available options” at `apps/docs/content/docs/zh/agent.mdx:168-180`, but it omits current `file`, `capabilities`, `ip_change`, `upgrade`, and `security` groups;
- the configuration reference documents Agent security environment variables at `apps/docs/content/docs/zh/configuration.mdx:220-228` but has no corresponding Agent `[security]` TOML section, and does not present the Server `[network_probe]` TOML group alongside its environment variables;
- the documentation site does not state which release its `main`-branch content describes, while install links combine the `main` installer with latest-channel assets.

**User impact:** a version-pinned user cannot determine whether a discrepancy is an operational mistake or documentation for a newer build.

**Recommendation:** display the documentation version/channel, publish versioned docs or an explicit compatibility statement, and generate protocol/default/config tables from source where practical.

### DOC-021: First-run Docker password retrieval is not reliable through `serverbee status`

**Severity:** Medium
**Status:** Confirmed

Quick start suggests `sudo serverbee status` or `docker logs serverbee-server` for the one-time Docker credential banner at `apps/docs/content/docs/zh/quick-start.mdx:74-85`. Docker status only prints the last five log lines (`deploy/install.sh:3013-3025`), which is not enough to guarantee that the multi-line first-run banner remains visible.

**User impact:** a user may believe the password is unavailable even while it is still present in container logs.

**Recommendation:** use an explicit `docker logs ... | grep` command as the primary retrieval path and reserve `status` for service state.

### DOC-022: The mobile guide assumes access to an iOS app without providing an acquisition path

**Severity:** Medium
**Status:** Partial

The mobile guide lists system requirements and immediately tells the user to open the iOS app at `apps/docs/content/docs/zh/mobile.mdx:17-49`, but it provides no App Store, TestFlight, release artifact, or source-build route.

**User impact:** even a correctly configured Server user cannot begin the documented pairing journey from the documentation alone.

**Recommendation:** state the actual current distribution status and link the supported acquisition/build path. If no public distribution exists, label the feature accordingly rather than presenting it as generally available.

### DOC-023: Two internal section links point to a nonexistent Chinese anchor

**Severity:** Low
**Status:** Confirmed

All 28 Chinese pages are included in navigation, the English and Chinese basename sets match, and all 154 checked `/zh/docs/...` page targets exist. The exception is section anchors: `apps/docs/content/docs/zh/cost-insights.mdx:9` and `:73-75` link to `admin#账单信息`, while the actual heading is “计费信息” at `apps/docs/content/docs/zh/admin.mdx:205`.

**User impact:** the page opens but does not jump to the promised setup instructions.

**Recommendation:** correct both anchors and include heading-fragment validation in link checks.

### DOC-024: The released direct-IP HTTP journey cannot reach Agent enrollment

**Severity:** Critical
**Status:** Confirmed

The quick start presents plain HTTP on a public IP as the fastest supported path and says ordinary browser login works after the installer disables secure cookies (`apps/docs/content/docs/zh/quick-start.mdx:30-42`). A clean install on the designated Debian 13 test VPS selected the current GitHub latest release, `v1.0.0-beta.1`. First login and mandatory password change succeeded, but opening **Servers** reproducibly rendered only the application error boundary. Expanding it and inspecting browser errors showed:

```text
crypto.randomUUID is not a function
```

This blocks **Add server**, so a user following the documented IP/HTTP path cannot obtain the Agent enrollment command. A session-only `crypto.randomUUID` polyfill allowed the rest of the same browser journey to complete, proving that Server health, authentication, enrollment API behavior, Agent installation, and live metrics were otherwise functional.

Current `main` already contains an explicit insecure-context fallback in `apps/web/src/lib/uuid.ts:1-20`, and the Add Server dialog uses that helper at `apps/web/src/components/server/add-server-dialog.tsx:166-176`. The failure is therefore release/documentation skew: current `main` documentation sends users to a latest release whose embedded frontend predates the fix.

**User impact:** the documented easiest deployment succeeds at the shell and login layers but fails at the first action required to connect any monitored server. The workaround is HTTPS or a newer build, neither of which the IP quick start tells the user is required for this release.

**Recommendation:** publish a release containing the fallback, verify the exact released binary over a non-secure IP origin before declaring the path supported, and version documentation/install links so `main` documentation does not silently describe unreleased fixes.

## What is already strong

The following areas are confirmed useful and should be preserved while restructuring:

- The introduction accurately explains the Server/Agent/SQLite/WebSocket model (`apps/docs/content/docs/zh/index.mdx:7-16`, `:74-82`).
- Quick start explains one-time enrollment, the 10-minute default, generated Web commands, first-run credentials, forced password change, and secure-cookie migration (`apps/docs/content/docs/zh/quick-start.mdx:74-96`, `:117-159`).
- After applying only a browser-session polyfill for DOC-024, the generated Agent command enrolled successfully, the Agent appeared Online with live metrics, restart/reconnect worked, and same-version upgrade detection behaved correctly.
- The reverse-proxy section explains WebSocket forwarding and long timeouts; the security checklist covers TLS, secure cookies, loopback binding, TOTP, backups, and external health monitoring (`apps/docs/content/docs/zh/deployment.mdx:541-554`).
- The dedicated capability page reflects the current Agent-owned trust boundary and effective default bitmask (`apps/docs/content/docs/zh/capabilities.mdx:7-57`).
- All 28 Chinese pages are in navigation, the English/Chinese page sets match, and all checked internal page targets exist; only DOC-023's two section anchors failed.
- Docker volume backup avoids hard-coding a Compose-prefixed volume name (`apps/docs/content/docs/zh/deployment.mdx:453-474`).
- Migration failure handling explicitly warns that migrations are forward-only, preserves evidence, avoids `down -v`, restores into a new volume, and defines post-restore acceptance (`apps/docs/content/docs/zh/deployment.mdx:580-768`).
- Resource usage and storage sizing distinguish measured data from estimates and identify version/environment context (`apps/docs/content/docs/zh/resource-usage.mdx:7-29`).

## Recommended remediation order

1. **Restore trustworthy entry points:** fix Chinese production routing and the MIT/AGPL landing-page error.
2. **Repair release safety:** correct beta release metadata and add live channel acceptance checks.
3. **Make one install path executable:** publish a decision table, correct HTTPS prerequisites, and validate exact binary/Docker snippets.
4. **Unify the security model:** remove Agent capability and rate-limit contradictions.
5. **Complete lifecycle operations:** consolidate backup/restore, uninstall/purge, Caddy cleanup, and general troubleshooting.
6. **Add documentation contract tests:** production locale routes, installer-emitted links, release-channel selectors, Compose configuration/health, and license consistency.

## Remaining unverified acceptance

These items remain unverified after the source/public-state audit and targeted Debian 13 runtime acceptance:

- a fresh binary install on other supported distributions/init systems, and a fresh Docker install on any platform;
- public DNS and ACME issuance on a clean domain;
- capability enforcement and broader UI feature visibility beyond first login, Add Server, enrollment, and live Server listing;
- upgrade and rollback across different released versions;
- backup restore into both systemd and Docker deployments followed by data/UI verification;
- complete purge behavior on a host where Caddy or Docker was already used for unrelated services;
- Railway deployment/template behavior and GHCR manifest/tag state.

Those require isolated runtime acceptance. Passing MDX typecheck/build proves documentation structure, imports, and rendering compilation, not the correctness of commands or the live product journey.
