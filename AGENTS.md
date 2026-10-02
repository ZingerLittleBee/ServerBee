# ServerBee agent guide

ServerBee has a Rust Server and Agent, a React dashboard, a bilingual documentation site, and a native iOS client.

## Find the right entry point

- **Code exploration, authentication, WebSocket state, CI, deployment, or backups:** read the matching task row in [source navigation](docs/agents/navigation.md). It links current owners, related tests, and hidden dependencies.
- **Enrollment or re-enrollment:** read [CONTEXT.md](CONTEXT.md) and [ADR-0004](docs/adr/0004-agent-authority-owns-enrollment-lifecycle.md) before changing authority, run tokens, or connection admission.
- **Documentation or site branding:** start with [apps/docs/README.md](apps/docs/README.md). Keep the `en` and `zh` content in sync.
- **Release notes or version preparation:** use [release-docs](.claude/commands/release-docs.md). Prerelease validation follows [the release runbook](docs/agents/beta-release-validation.md); version cuts use `make publish`.
- **iOS:** read [apps/ios/CLAUDE.md](apps/ios/CLAUDE.md) and the [wire-model guide](apps/ios/ServerBee/Models/README.md).
- **Manual verification:** use [tests/README.md](tests/README.md) for local setup and the feature checklist index.
- **Issues or triage:** use `gh` with [issue-tracker.md](docs/agents/issue-tracker.md) and [triage-labels.md](docs/agents/triage-labels.md).
- **Domain decisions:** use the glossary and relevant accepted ADRs through [domain.md](docs/agents/domain.md). Historical plans and specs are implementation records, not the current behavior contract.

## Working conventions

- Rust handlers use `AppError` and `Json<ApiResponse<T>>`; expose endpoint and DTO schemas through Utoipa.
- Configuration changes update [ENV.md](ENV.md) and both configuration pages together. Find the authoritative config types in the navigation map.
- Agent capabilities are agent-owned. Read the current capability implementation before changing gates or generated client metadata.
- Migrations implement `up()` and leave `down()` as `Ok(())` to avoid accidental data loss.
- Web work uses the existing shadcn components. Scrollable containers use `ScrollArea`.
- Build the web bundle before verifying the embedded SPA or built-in widgets; a successful cold Rust build does not establish that those assets are present.
- Use the task-specific checks in the navigation map and the repository CI commands. For tiny presentational changes, use targeted visual, type, or lint verification; behavior changes need relevant tests.
- Format only changed Rust crates. JavaScript formatting and imports come from [biome.json](biome.json).
- Commit messages use Conventional Commits. Attribution trailers, generation signatures, and email addresses are rejected by [Lefthook](lefthook.yml) and CI.

Run `bun run check:agent-navigation` after changing agent entry points or their linked paths.
