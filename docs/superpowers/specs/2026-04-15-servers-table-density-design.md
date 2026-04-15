# Servers Table Density & Disk I/O Display Design

**Date:** 2026-04-15
**Scope:** `apps/web/src/routes/_authed/servers/index.tsx` (table view only)

## Problem

The `/servers?view=table` page has low information density. Each metric cell (CPU / Memory / Disk) is 160px wide but shows only a 1.5px-high progress bar plus a percentage and a single sub-line (`formatBytes(used)`). Disk I/O is collected by the agent (`disk_read_bytes_per_sec`, `disk_write_bytes_per_sec` are already on the `ServerMetrics` WebSocket payload) but not surfaced anywhere in the list view.

Goals:

1. Surface Disk I/O in the table.
2. Increase per-cell information density without adding new columns.
3. Keep layout stable across breakpoints (lg / xl).

Non-goals:

- Grid view (`ServerCard`) — untouched.
- Adding new columns — the `xl` layout already has 8 columns and is tight.
- Sparkline / mini-chart visualizations — out of scope; we meet density goals with text.

## Design

### Cell layouts (column widths unchanged)

**CPU** (2 rows, 160px)

```
▓▓▓▓▓░░░░░  45%
load 1.23
```

Sub-line shows `load {load1.toFixed(2)}`. `cpu_cores` is not available on the `ServerMetrics` list payload (only on the detail DTO), so we do not display core count here.

**Memory** (2 rows, 160px)

```
▓▓▓▓▓░░░░░  45%
3.2GB / 8.0GB
```

Sub-line upgraded from `formatBytes(used)` to `formatBytes(used) / formatBytes(total)`.

**Disk** (3 rows, 160px)

```
▓▓▓▓▓░░░░░  45%
120G / 500G
↺ 2.1MB/s  ↻ 1.2MB/s
```

Third row shows I/O. Rendered **only when** `disk_read_bytes_per_sec !== undefined || disk_write_bytes_per_sec !== undefined`. Missing side shows `0B/s` so the row stays fixed-width. The arrow glyphs (`↺` read, `↻` write) match the existing `ServerCard` convention (see `servers.json` `card_disk_read` / `card_disk_write`).

**Network** (2 rows, 160px, stays `hidden lg:table-cell`)

```
↓ 1.2MB/s   ↑ 340KB/s
Σ ↓12GB  ↑3.4GB
```

Sub-line shows cumulative transfer using `net_in_transfer` / `net_out_transfer`, prefixed with `Σ` to distinguish from the live speed row.

### Component changes

**`MiniBar`** (in `apps/web/src/routes/_authed/servers/index.tsx`)

- `sub` prop type changes from `string | undefined` to `ReactNode | undefined`.
- Rendering: `sub` is wrapped in a single `<div>`; consumers can pass a fragment with multiple `<span>` / `<p>` children for multi-line sub content.
- Sub styling stays `text-[10px] text-muted-foreground tabular-nums`. Multi-row cases use `flex flex-col gap-0.5`.

**CPU column cell**

```tsx
<MiniBar pct={s.cpu} sub={<span>load {s.load1.toFixed(2)}</span>} />
```

**Memory column cell**

```tsx
<MiniBar pct={memPct} sub={<span>{formatBytes(s.mem_used)} / {formatBytes(s.mem_total)}</span>} />
```

**Disk column cell**

```tsx
const hasIo = s.disk_read_bytes_per_sec !== undefined || s.disk_write_bytes_per_sec !== undefined
<MiniBar
  pct={diskPct}
  sub={
    <div className="flex flex-col gap-0.5">
      <span>{formatBytes(s.disk_used)} / {formatBytes(s.disk_total)}</span>
      {hasIo && (
        <span>
          ↺ {formatSpeed(s.disk_read_bytes_per_sec ?? 0)}  ↻ {formatSpeed(s.disk_write_bytes_per_sec ?? 0)}
        </span>
      )}
    </div>
  }
/>
```

**Network column cell** (inline, no `MiniBar`)

```tsx
<div className="flex flex-col gap-0.5 font-mono text-muted-foreground text-xs tabular-nums">
  <span>
    <span className="inline-block min-w-[64px]">↓{formatSpeed(s.net_in_speed)}</span>
    <span className="ml-2 inline-block min-w-[64px]">↑{formatSpeed(s.net_out_speed)}</span>
  </span>
  <span className="text-[10px]">
    Σ ↓{formatBytes(s.net_in_transfer)}  ↑{formatBytes(s.net_out_transfer)}
  </span>
</div>
```

### Row height

Current row height ≈ 48px. After change:

- Rows with I/O data (most rows): ≈ 64px (Disk cell has 3 rows).
- Rows without I/O (offline / legacy agents): ≈ 52px (Disk cell has 2 rows).

This is acceptable given the user's explicit approval to grow row height.

## Edge cases

| Case | Behavior |
|------|----------|
| `disk_total === 0` | Existing logic: pct = 0, sub shows `0B / 0B`. No I/O row unless `disk_read_bytes_per_sec` / `disk_write_bytes_per_sec` present. |
| `disk_read_bytes_per_sec === undefined` and `disk_write_bytes_per_sec === undefined` | No I/O row (legacy agent compatibility). |
| Only one I/O side defined | Both shown, missing side renders as `0B/s`. |
| Offline server | Network live speeds become 0; cumulative transfer still shows last known values. Matches existing behavior. |
| `mem_total === 0` / `disk_total === 0` | Sub-line renders `0B / 0B`. `formatBytes(0)` returns `0B`. |

## Testing

- No new unit tests for the page itself (existing pattern: table logic lives inline in the route file without tests).
- Manual check: offline agent, legacy agent (no I/O fields), normal agent — verify I/O row visibility and row-height consistency.
- Lint: `bun x ultracite check` and `bun run typecheck`.
- Visual: 1789×963 viewport (user's reported size) and narrow viewport to confirm `hidden lg:` / `hidden xl:` breakpoints still work.

## Rejected alternatives

- **Separate Disk I/O column (option A)**: xl layout is already dense; adding a column squeezes `name` or `group`.
- **Single-line disk sub (`120G/500G · ↺2M ↻1M`)**: does not fit in 160px at 10px monospace (~35 chars ≈ 210px).
- **Widen Disk column to 200px** to fit single-line: disrupts existing column balance; forced us to shrink name/group.
- **Sparkline for CPU/Memory/Disk**: out of scope; meets density goal without additional dependencies or renders.
