# Release Documentation Updater

Update version numbers, CHANGELOG.md, README.md, README.zh-CN.md, and Fumadocs based on the current branch's changes vs main.

## Arguments

The user MUST provide a version number (e.g., `v0.2.2` or `0.2.2`). If not provided, ask the user to specify one.

## Process

### Step 1: Determine and sync version

```
1. Parse version from argument, strip leading 'v' if present (e.g., "v0.2.2" -> "0.2.2")
2. Read current version from Cargo.toml [workspace.package] version field
3. Read current version from package.json "version" field (if exists, may not have one)
4. If the new version differs from Cargo.toml version:
   - Update Cargo.toml [workspace.package] version to the new version
   - Update package.json "version" to the new version (if field exists)
   - Run `cargo check --workspace` to regenerate Cargo.lock
   - Note: Do NOT add 'v' prefix — Cargo.toml and package.json use bare semver (e.g., "0.2.2")
5. If versions already match, skip this step
```

### Step 2: Gather change context

Run these commands to understand what changed on this branch:

```bash
# Commit history since diverging from main
git log --oneline main..HEAD

# Full diff summary (files changed)
git diff --stat main...HEAD

# Full diff for understanding changes
git diff main...HEAD
```

Also read ALL files in these directories for feature context:
- `docs/superpowers/specs/` — design specs (read each file's title and overview)
- `docs/superpowers/plans/` — implementation plans (read each file's title)
- `docs/superpowers/plans/PROGRESS.md` — current progress

### Step 3: Analyze changes

Categorize all changes into:
- **Added** — new features, new capabilities
- **Changed** — modified behavior, updated defaults
- **Fixed** — bug fixes, corrections
- **Testing** — new tests, updated test counts
- **Documentation** — doc updates (don't list in CHANGELOG, these ARE the docs)

### Step 4: Update CHANGELOG.md

Read the existing `CHANGELOG.md` to understand the format and style.

Add a new version section at the top (after the header, before the previous version). Follow the exact same format as existing entries:
- Use `## [version] - YYYY-MM-DD` header with today's date
- Group changes under `### Added`, `### Changed`, `### Fixed`, `### Testing` subsections
- Each item starts with `- **Feature name** -- description`
- Be specific about what was added/changed, referencing concrete metrics (test counts, endpoint counts, etc.)
- Match the writing style and detail level of existing entries

Do NOT duplicate entries that already exist in previous versions.

### Step 5: Update README.md

Read the existing `README.md`. Update the **Features** section to include any new user-facing features. Follow the existing bullet point format:
- `- **Feature Name** -- Brief description`

Also update:
- Test counts in the development section if they changed
- Configuration examples if new config options were added
- Any other sections affected by the changes

### Step 6: Update README.zh-CN.md

Apply the same changes as README.md but in Chinese. Read the existing `README.zh-CN.md` to match its translation style. The Chinese README should be a mirror of the English one with all content translated.

### Step 7: Update Fumadocs (if applicable)

If the changes include new user-facing features, configuration options, or architectural changes:

Check `apps/docs/content/docs/en/` and `apps/docs/content/docs/cn/` for relevant MDX files that need updating. Common files to check:
- `configuration.mdx` — if new env vars or config options added
- `architecture.mdx` — if architectural changes
- `monitoring.mdx` — if monitoring features changed
- Feature-specific pages if they exist

Only update docs pages that are directly affected by the changes. Don't create new doc pages unless the feature is significant enough to warrant its own page.

### Step 8: Verify and commit

```bash
# Verify the changes look correct
git diff --stat

# Stage all changed files (version files + docs)
git add Cargo.toml Cargo.lock package.json CHANGELOG.md README.md README.zh-CN.md apps/docs/
git commit -m "release: v{version} — update version and documentation"
```

## Important Notes

- Always read existing files BEFORE modifying them to match style
- The CHANGELOG follows [Keep a Changelog](https://keepachangelog.com/) format
- README features should be concise (one line each)
- Chinese translations should be natural, not machine-translated
- Don't add features to README/CHANGELOG that were already listed in previous versions
- Today's date should be used for the CHANGELOG entry
- If unsure whether a change is user-facing, err on the side of including it in CHANGELOG but NOT in README (README is for feature highlights only)
