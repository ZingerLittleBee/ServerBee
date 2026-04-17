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
Load 1.23
```

Sub-line shows `{t('card_load')} {load1.toFixed(2)}` (reuses existing `card_load` key in `servers.json` — "Load" / "负载"). `cpu_cores` is not available on the `ServerMetrics` list payload (only on the detail DTO), so we do not display core count here.

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

Third row shows I/O. Rendered **only when `server.online === true`**. We cannot distinguish "legacy agent that never reports I/O" from "modern agent reporting 0" on the browser, because `crates/common/src/types.rs:172` declares `disk_read_bytes_per_sec: u64` with `#[serde(default)]` — missing fields deserialize to 0 on the server and are re-emitted as numbers to the browser. Offline rows hide the I/O line (value would be a stale last frame); online rows always show it, with legacy / idle agents rendering `↺ 0B/s  ↻ 0B/s`. The arrow glyphs (`↺` read, `↻` write) match the existing `ServerCard` convention (see `servers.json` `card_disk_read` / `card_disk_write`). The TypeScript type in `apps/web/src/hooks/use-servers-ws.ts` should be tightened from `disk_read_bytes_per_sec?: number` to `disk_read_bytes_per_sec: number` (non-optional, default 0) to reflect the wire reality.

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
<MiniBar pct={s.cpu} sub={<span>{t('card_load')} {s.load1.toFixed(2)}</span>} />
```

**Memory column cell**

```tsx
<MiniBar pct={memPct} sub={<span>{formatBytes(s.mem_used)} / {formatBytes(s.mem_total)}</span>} />
```

**Disk column cell**

```tsx
<MiniBar
  pct={diskPct}
  sub={
    <div className="flex flex-col gap-0.5">
      <span>{formatBytes(s.disk_used)} / {formatBytes(s.disk_total)}</span>
      {s.online && (
        <span>
          ↺ {formatSpeed(s.disk_read_bytes_per_sec)}  ↻ {formatSpeed(s.disk_write_bytes_per_sec)}
        </span>
      )}
    </div>
  }
/>
```

The visibility gate is `s.online` (not `disk_*` field presence) — see the HIGH rationale under "Cell layouts / Disk" above.

**Network column cell** (inline, no `MiniBar`)

```tsx
const inSpeed = s.online ? s.net_in_speed : 0
const outSpeed = s.online ? s.net_out_speed : 0
<div className="flex flex-col gap-0.5 font-mono text-muted-foreground text-xs tabular-nums">
  <span>
    <span className="inline-block min-w-[64px]">↓{formatSpeed(inSpeed)}</span>
    <span className="ml-2 inline-block min-w-[64px]">↑{formatSpeed(outSpeed)}</span>
  </span>
  <span className="text-[10px]">
    Σ ↓{formatBytes(s.net_in_transfer)}  ↑{formatBytes(s.net_out_transfer)}
  </span>
</div>
```

Live speed is explicitly zeroed when `!s.online`, because `use-servers-ws.ts` keeps the last-frame `net_*_speed` values on `server_offline` (only flips the `online` boolean). Without the zeroing, offline rows would look like they are still pushing traffic. Cumulative `net_*_transfer` keeps its last value intentionally — historical totals do not expire.

### Row height

Current row height ≈ 48px. After change:

- Online rows: ≈ 64px (Disk cell has 3 rows — bar, `used/total`, I/O).
- Offline rows: ≈ 52px (Disk cell drops the I/O row).

The ≈12px in-table jump between online and offline rows is acceptable; offline rows are rare in steady state. User has explicitly approved row-height growth.

## Edge cases

| Case | Behavior |
|------|----------|
| `disk_total === 0` | pct = 0, sub shows `0B / 0B`. I/O row shown if online (as `↺ 0B/s ↻ 0B/s`). |
| Legacy agent (never sends `disk_*_bytes_per_sec`) | Server's `#[serde(default)]` lands 0; browser sees `0` and renders `↺ 0B/s ↻ 0B/s` when the server is online. Indistinguishable from a truly idle disk — this is intentional given the protocol. |
| Offline server — Disk I/O row | Hidden (would be stale last-frame). Disk `used/total` stays (stored value). |
| Offline server — Network live speeds | Rendered as `↓0B/s ↑0B/s` (explicitly zeroed in the cell, since `use-servers-ws.ts` does not clear the fields on `server_offline`). |
| Offline server — Network cumulative | Unchanged (Σ ↓.. ↑..), historical totals do not expire. |
| `mem_total === 0` / `disk_total === 0` | Sub-line renders `0B / 0B`. `formatBytes(0)` returns `0B`. |

## Testing

The change's two riskiest behaviors — conditional I/O row rendering and offline speed zeroing — must have regression coverage. Visual QA alone is not enough.

**Refactor for testability**: extract the metric cell renderers from the inline `columns` array into named exports in `apps/web/src/routes/_authed/servers/index.cells.tsx`:

- `CpuCell({ server })`
- `MemoryCell({ server })`
- `DiskCell({ server })`
- `NetworkCell({ server })`

The `columns` definition then wires `cell: ({ row }) => <DiskCell server={row.original} />`. `MiniBar` and `UpgradeBadgeCell` stay in `index.tsx`.

**Unit tests** in `apps/web/src/routes/_authed/servers/index.cells.test.tsx` (vitest + RTL, wrapped in `I18nextProvider` per existing `server-card.test.tsx` pattern):

1. `DiskCell` — online server: I/O row is present and shows both `↺` and `↻` values.
2. `DiskCell` — offline server: I/O row is not rendered (assert neither arrow glyph appears).
3. `DiskCell` — online with `disk_read_bytes_per_sec === 0` and `disk_write_bytes_per_sec === 0`: I/O row still renders as `↺ 0B/s  ↻ 0B/s` (documents the "legacy agent ≡ idle disk" behavior).
4. `NetworkCell` — offline server: live speed row shows `↓0B/s ↑0B/s` regardless of the numeric `net_in_speed` / `net_out_speed` fields on the record; cumulative row shows the stored `net_*_transfer` values.
5. `NetworkCell` — online server with non-zero speeds: live speed row reflects the fields.

**Manual checks**:

- 1789×963 viewport (user's reported size) — verify Disk row does not overflow at 160px, row height increase is acceptable.
- `hidden lg:` / `hidden xl:` breakpoints — narrow viewport still hides Network / Group / Uptime correctly.

**Lint / typecheck**: `bun x ultracite check` and `bun run typecheck`. The latter will flag the `disk_read_bytes_per_sec?: number` → `disk_read_bytes_per_sec: number` type tightening if any consumer was relying on `undefined`.

## Rejected alternatives

- **Separate Disk I/O column (option A)**: xl layout is already dense; adding a column squeezes `name` or `group`.
- **Single-line disk sub (`120G/500G · ↺2M ↻1M`)**: does not fit in 160px at 10px monospace (~35 chars ≈ 210px).
- **Widen Disk column to 200px** to fit single-line: disrupts existing column balance; forced us to shrink name/group.
- **Sparkline for CPU/Memory/Disk**: out of scope; meets density goal without additional dependencies or renders.
