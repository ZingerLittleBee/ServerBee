# Manual QA — Servers Table Row Visual Redesign

**Spec:** `docs/superpowers/specs/2026-04-17-servers-table-row-visual-redesign-design.md`

## Prereqs
- Admin login.
- At least 3 servers with mixed state: online + traffic configured, online + no traffic limit, offline.

## Checks

- [ ] `/servers?view=table` — first column renders a pulsing green dot for online, a muted grey dot for offline.
- [ ] The text-badge `Status` column is gone; the filter pill in the toolbar still offers `Online/Offline`.
- [ ] CPU cell: bar + `%` on top (colored by threshold), `{N} cores · load X.XX` below. If `cpu_cores` is not yet exposed (legacy agent), falls back to `load X.XX`.
- [ ] Memory cell: `{used} / {total} · swap X%`. Swap color reflects threshold.
- [ ] Disk cell: bar + `%`, `↓ {read} ↑ {write}` below.
- [ ] Network cell: traffic quota bar (uses `/api/traffic/overview`), `{used} / {limit} · ↓in ↑out`. If no quota configured, uses 1 TiB fallback.
- [ ] Offline row: metric cells show `—`, Network quota bar still visible, Uptime shows `offline` + `last seen Xh ago`. Tag chips still visible.
- [ ] Name cell: flag + name + UpgradeBadge on line 1, tag chips on line 2 when tags are set.
- [ ] Edit dialog: type `prod, db, web` → save → chips appear in the row.
- [ ] Edit dialog validations: 9 tags / 17-char tag / `has space` → error toast, no PUT fires.
- [ ] Edit dialog client-validation blocks submit: set a name + client-invalid tags (e.g. `bad space` or 17-char tag) → a validation toast fires, **no PATCH and no PUT are issued** (verify in browser devtools Network tab), the dialog stays open.
- [ ] Edit dialog partial failure (PATCH ok, PUT fails): set a name + valid tags, force `PUT /api/servers/:id/tags` to return 500 (e.g. via browser devtools "block request URL" or a mock worker) → PATCH persists (server list shows the new name after dialog closes), tag input reverts to the last-known tags, `tags_save_failed` toast fires, dialog stays open.
- [ ] Breakpoints: network column hides below `lg:`, group/uptime hide below `xl:`.
- [ ] Viewport 1920×963 screenshot matches spec mockup proportions.
- [ ] `bun run test` green; `cargo test --workspace` green; `cargo clippy --workspace -- -D warnings` clean; `bun x ultracite check` clean; `bun run typecheck` clean.
