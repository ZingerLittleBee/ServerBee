# Release documentation updater

Update documentation affected by the requested branch or release. Use [source navigation](../../docs/agents/navigation.md) for current owners and [the docs contributor guide](../../apps/docs/README.md) for site checks. Version preparation and publication follow [the release runbook](../../docs/agents/beta-release-validation.md) through [the publish entry point](../../scripts/publish.sh).

## Scope the evidence

1. Record the requested comparison base and current commit. For branch docs, use the merge-base with `main` unless a base was supplied. For release notes, compare with the previous release tag so already merged changes are included.
2. Inspect the commit list and changed-file summary, then read the diff and source owners for affected feature areas.
3. Read the relevant accepted ADR or issue when it explains a changed contract. Historical specs/plans provide background only when that specific change refers to them.
4. List affected user workflows and document supported behavior from current source or verified evidence. Report test counts and runtime results only when measured for the candidate.

## Update affected documentation

- Write release notes under [CHANGELOG.md](../../CHANGELOG.md)'s `[Unreleased]` section, matching existing categories and avoiding duplicates.
- Update [README.md](../../README.md) and [README.zh-CN.md](../../README.zh-CN.md) when feature highlights, installation, or config examples change.
- For config changes, update [ENV.md](../../ENV.md), [English configuration](../../apps/docs/content/docs/en/configuration.mdx), and [Chinese configuration](../../apps/docs/content/docs/zh/configuration.mdx) together.
- For user-facing changes, review the affected pages in [English content](../../apps/docs/content/docs/en/) and [Chinese content](../../apps/docs/content/docs/zh/). Add a page in both languages when needed and register it in the respective `meta.json`.
- Keep capability ownership and other current contracts aligned with their source owners. Review feature pages affected by the diff rather than every historical design file.

Complete this step when every affected workflow has matching bilingual docs and every factual claim has a current source or an identified verification result.

## Verify the draft

Run the applicable contract, type, build, and route checks from the docs contributor guide, plus `bun run check:agent-navigation` when changing agent entry points. Inspect the diff for scope, stale paths, and generated files. Stage only current-task files when creating a local commit.

## Prepare or publish a version

Use the requested semantic version for the release preview; derive a target only when the publish entry point supports that case. Keep documentation-only drafting separate from a version cut.

Follow the runbook's **Release dry-run and authorization** section. `make publish` owns the dated CHANGELOG cut, workspace/web versions, both lockfiles, and release commit. `DRY_RUN=1` previews the cut; `PREPARE_ONLY=1` prepares locally without pushing or tagging. After preparation, check versioned documentation examples and rerun their contracts before authorized publication. Use this entry point instead of a separate hand-written version-update or release-commit sequence.

Use the user's existing authorization for external actions. VPS trials and distribution checks follow the runbook's applicable gates; a documentation update by itself does not authorize publishing.
