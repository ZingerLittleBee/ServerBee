# Beta release validation

Use this runbook before publishing a ServerBee prerelease. It separates the destructive, pre-tag VPS trial from the post-publish distribution smoke test so an unpublished candidate never needs to be exposed as a GitHub release.

## Authority and host gate

Do not start the VPS trial until the operator has provided all of the following:

- A local SSH alias. Keep addresses, usernames, and key paths in the local SSH configuration instead of repository files or chat logs.
- Confirmation that the host is disposable and contains no ServerBee data that must be preserved.
- Explicit permission to create and remove `/opt/serverbee`, `serverbee-server.service`, `serverbee-agent.service`, ServerBee containers, images, and test volumes.
- Explicit permission to install build tools or Docker when they are absent.

The host must provide:

- Ubuntu 24.04 or Debian 12 on `x86_64`, with systemd.
- Root or passwordless sudo access.
- At least 2 vCPU, 4 GiB RAM, 15 GiB free disk space, and outbound access to GitHub, GHCR, crates.io, and the Bun package registry.
- Docker Engine with Compose V2 when the Docker transaction is in scope.

A domain is not required for upgrade and rollback validation. Request a domain only when Caddy, TLS, and proxied WebSockets are part of the release acceptance scope.

Before writing anything, capture and review these read-only checks:

```bash
uname -a
cat /etc/os-release
systemctl --version | head -n 1
df -h /
free -h
systemctl list-unit-files 'serverbee-*' --no-legend
sudo test ! -e /opt/serverbee
docker compose version
docker ps -a --filter 'name=serverbee-' --format '{{.Names}} {{.Status}}'
```

Stop if `/opt/serverbee`, a ServerBee unit, or a ServerBee container already exists. Do not repurpose an existing installation as the disposable test target.

## Evidence directory

Create a host-local evidence directory outside the managed installation:

```bash
sudo install -d -m 0700 /var/tmp/serverbee-beta-evidence
```

Record the candidate commit, expected version, command output, service status, restart counts, recent logs, and HTTP version responses there. Do not record generated admin passwords, enrollment codes, API keys, cookies, or Agent tokens.

## Candidate preparation

The pre-tag candidate must be built from the exact local release commit without publishing a tag or release:

1. Transfer a `git archive` of the candidate commit to a new temporary directory on the VPS.
2. Change only the temporary archive's workspace and web package versions to the intended prerelease.
3. Regenerate the temporary Cargo and Bun locks.
4. Build the web bundle, `serverbee-server`, and `serverbee-agent` in release mode without GPU features.
5. Record SHA-256 digests for both candidate binaries.
6. Require both binaries to report the intended version through the side-effect-free probe:

```bash
./serverbee-server --serverbee-upgrade-probe
./serverbee-agent --serverbee-upgrade-probe
```

Both commands must print exactly the prerelease version and must not create configuration, data, logs, network listeners, or background processes.

## Binary transaction matrix

Use the current candidate `deploy/install.sh`, pinned to the latest published Alpha, for the baseline installation. Inject unpublished candidate binaries only by overriding the sourced script's `download_verified` test boundary inside a subshell. Do not edit the production upgrade functions on the VPS.

Capture evidence for both Server and Agent:

| Case | Required evidence |
| --- | --- |
| Alpha baseline | Installed metadata reports the Alpha; binary SHA-256 matches the published Alpha checksum; `/api/about` or the Agent startup log reports the Alpha; systemd is active; restart count is stable |
| Successful Alpha to Beta | Candidate and installed-binary probes report the Beta; metadata changes to the Beta; service remains active for the full stability window; rollback file is absent |
| Wrong candidate version | Upgrade fails before stopping the service; Alpha/Beta baseline binary and metadata remain unchanged |
| Candidate start failure | Upgrade returns failure; previous binary is restored; service becomes active and stable; metadata remains on the previous version |
| Candidate restart loop | A restart-count increase fails the stability trial; previous binary is restored and stable |
| Stale rollback file | Upgrade refuses to overwrite the rollback file and does not stop the running service |

For the Agent baseline, use a test-only configuration with a non-secret dummy token and an unreachable local Server URL so the Agent stays alive while reconnecting. Do not consume production enrollment codes or connect the disposable Agent to a production Server.

After each case, capture:

```bash
sudo systemctl status serverbee-server serverbee-agent --no-pager
sudo systemctl show serverbee-server serverbee-agent -p ActiveState -p SubState -p NRestarts
sudo journalctl -u serverbee-server -u serverbee-agent -n 80 --no-pager
sudo find /opt/serverbee/bin -maxdepth 1 -name '*.rollback' -ls
```

For the Server success and rollback cases, also require `/api/about` and `/healthz` to return successfully and report the expected running version.

## Docker transaction matrix

Build a local, single-architecture candidate image from the verified Linux candidate binary and tag it with the managed GHCR repository name plus the prerelease version. Because the tag is intentionally unpublished, override only the sourced script's `docker compose ... pull` call so it uses the already-present local image. All `compose up`, inspect, health, restart-count, rollback, and log operations must use the real Docker CLI.

Capture these cases:

| Case | Required evidence |
| --- | --- |
| Successful local candidate | Compose references the Beta tag; the real container becomes healthy and stays stable; the rollback Compose file is removed |
| Pull failure | The real pull fails; the original Compose file is restored byte-for-byte; the previous container is untouched |
| Candidate exits | Compose starts the deliberately failing local tag, stability fails, the previous Compose file and image are restored, and the previous container becomes healthy |
| Restart loop | Restart count changes during the trial; rollback restores a stable previous container |
| Stale rollback file | Upgrade refuses to change Compose or containers |

Do not treat the locally tagged image as distribution evidence. Public GHCR pull behavior is verified only after the GitHub prerelease exists.

## Cleanup

After evidence has been copied off the disposable host, remove only the explicitly authorized ServerBee test resources. Verify that the exact managed paths, units, containers, and rollback files are gone. Do not run recursive deletion against a variable, home directory, repository root, or filesystem root.

Retain the VPS until the release dry-run has been reviewed. It may be reused for the post-publish smoke test only if the operator confirms that reuse.

## Final local gates

After the VPS matrix passes, cut the version and dated CHANGELOG section, regenerate locks, and run:

```bash
cargo fmt
cargo clippy --workspace --benches --tests --examples --all-features --locked -- -D warnings
cargo test --workspace --locked
(
  cd apps/web
  bun install --frozen-lockfile
  bun run typecheck
  bun run test
  bun run build
)
bun x ultracite check
dash -n deploy/install.sh deploy/test-install-upgrade.sh scripts/extract-changelog.sh scripts/test-extract-changelog.sh
dash deploy/test-install-upgrade.sh
dash scripts/test-extract-changelog.sh
shellcheck --shell=dash --severity=warning deploy/install.sh deploy/test-install-upgrade.sh scripts/extract-changelog.sh scripts/test-extract-changelog.sh
```

Run the iOS test scheme on a named Simulator and require a nonzero test count with zero failures. Keep Xcode and Simulator work serialized.

## Release dry-run and authorization

Use the repository release entrypoint to preview the exact cut:

```bash
make publish VERSION=1.0.0-beta.1 DRY_RUN=1
```

When the preview has been reviewed but external writes are not yet authorized,
prepare and commit the release locally without pushing or tagging:

```bash
make publish VERSION=1.0.0-beta.1 PREPARE_ONLY=1 YES=1
```

Run `make publish VERSION=1.0.0-beta.1 YES=1` only after explicit authorization.
That command pushes the release commit and annotated tag; the tag-triggered GitHub
Actions workflow builds the assets and creates the prerelease.

Before any external write, present:

- The exact release commit and clean worktree status.
- `vX.Y.Z-beta.N` as the proposed tag.
- Version values from Cargo, Cargo.lock, the web package, and Bun lock.
- The dated CHANGELOG body that will become the GitHub release notes.
- Local Rust, web, shell, iOS, and VPS evidence.
- The expected binary asset list and GHCR tags.
- Known residual risks and rollback limitations.

Push the branch and tag only after explicit final authorization.

## Post-publish verification

The release is not complete until all of the following are verified from public distribution endpoints:

- The GitHub release is a prerelease and did not move the stable latest release.
- All expected Server and Agent binaries plus `sha256sums.txt` are present.
- Every downloaded binary matches `sha256sums.txt`.
- Native and emulated Linux Agent and Server probes report the published version.
- `serverbee upgrade --channel beta` resolves the new prerelease.
- The versioned Server and Agent GHCR manifests contain both `linux/amd64` and `linux/arm64`.
- The GHCR `latest` tags still point to the prior stable release.
- A disposable-host Alpha to published-Beta binary upgrade succeeds.
- A disposable-host Alpha to published-Beta Docker upgrade succeeds.

If the workflow or smoke test fails, do not move or recreate the tag silently. Report the failed invariant and obtain approval for the recovery action.
