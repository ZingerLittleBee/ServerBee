# Servers Table Density & Disk I/O Display Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Increase information density of the `/servers` table view and surface Disk I/O per row, implementing the design in `docs/superpowers/specs/2026-04-15-servers-table-density-design.md`.

**Architecture:** Extract the CPU / Memory / Disk / Network cell renderers from the inline `columns` array in `apps/web/src/routes/_authed/servers/index.tsx` into named, testable components in a sibling `index.cells.tsx` file. Extend `MiniBar` to accept a `ReactNode` sub slot so cells can stack multiple sub-lines. Gate Disk I/O row on `server.online` (protocol cannot distinguish legacy vs idle). Zero live network speeds when offline.

**Tech Stack:** React 19, TypeScript, vitest + @testing-library/react, react-i18next, TanStack Table, Tailwind v4.

---

## File Structure

- **Modify** `apps/web/src/hooks/use-servers-ws.ts` — tighten `disk_*_bytes_per_sec` types from `?: number` to `: number`.
- **Modify** `apps/web/src/routes/_authed/servers/index.tsx`:
  - Extend `MiniBar`'s `sub` prop from `string` to `ReactNode`.
  - Replace inline cell render functions in `columns` with `<CpuCell server={...} />` etc.
- **Create** `apps/web/src/routes/_authed/servers/index.cells.tsx` — named exports: `CpuCell`, `MemoryCell`, `DiskCell`, `NetworkCell`. Imports `MiniBar` from `./index`.
- **Create** `apps/web/src/routes/_authed/servers/index.cells.test.tsx` — vitest + RTL tests covering the two riskiest behaviors: conditional Disk I/O row and offline network zeroing.

To avoid a circular-import smell (`index.cells.tsx` needs `MiniBar`; `index.tsx` imports `*Cell` back), `MiniBar` will be moved out of `index.tsx` into `index.cells.tsx` and re-imported by the route file. This keeps all presentational pieces in one place. `UpgradeBadgeCell` stays in `index.tsx` (it's wired to a specific column only).

---

## Task 1: Tighten `ServerMetrics` disk I/O types

**Files:**
- Modify: `apps/web/src/hooks/use-servers-ws.ts:20,23`

**Rationale:** `crates/common/src/types.rs:172` declares `disk_read_bytes_per_sec: u64` with `#[serde(default)]`. Missing fields from legacy agents deserialize to 0 on the server and are re-emitted as numbers. The browser never sees `undefined`, so the `?` on these fields is misleading and will cause `hasIo` checks based on `!== undefined` to always be true.

- [ ] **Step 1: Make the disk I/O fields non-optional**

Change `apps/web/src/hooks/use-servers-ws.ts` lines 20 and 23:

```ts
// Before:
  disk_read_bytes_per_sec?: number
  ...
  disk_write_bytes_per_sec?: number

// After:
  disk_read_bytes_per_sec: number
  ...
  disk_write_bytes_per_sec: number
```

- [ ] **Step 2: Verify typecheck passes**

Run: `cd apps/web && bun run typecheck`
Expected: PASS. If any existing consumer was treating these as optional (e.g., `s.disk_read_bytes_per_sec ?? 0`), the typecheck will surface it — those sites should be updated to read the field directly since it is now always defined.

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/hooks/use-servers-ws.ts
git commit -m "refactor(web): tighten disk i/o fields to non-optional on ServerMetrics"
```

---

## Task 2: Scaffold `index.cells.tsx` and move `MiniBar`

**Files:**
- Create: `apps/web/src/routes/_authed/servers/index.cells.tsx`
- Modify: `apps/web/src/routes/_authed/servers/index.tsx` (remove local `MiniBar`, import from new file; extend `sub` prop to `ReactNode`)

- [ ] **Step 1: Create `index.cells.tsx` with the extended `MiniBar`**

Create `apps/web/src/routes/_authed/servers/index.cells.tsx`:

```tsx
import type { ReactNode } from 'react'
import { cn } from '@/lib/utils'

function getBarColor(p: number): string {
  if (p > 90) {
    return 'bg-red-500'
  }
  if (p > 70) {
    return 'bg-amber-500'
  }
  return 'bg-emerald-500'
}

export function MiniBar({ pct, sub }: { pct: number; sub?: ReactNode }) {
  const p = Math.min(100, Math.max(0, pct))
  const color = getBarColor(p)
  return (
    <div className="min-w-[80px]">
      <div className="flex items-center gap-2">
        <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-muted">
          <div className={cn('h-full rounded-full', color)} style={{ width: `${p}%` }} />
        </div>
        <span className="w-10 text-right font-mono text-xs tabular-nums">{p.toFixed(0)}%</span>
      </div>
      {sub !== undefined && (
        <div className="mt-0.5 font-mono text-[10px] text-muted-foreground tabular-nums">{sub}</div>
      )}
    </div>
  )
}
```

Note: the old `sub` used a `<p>` with `formatBytes(used)` inline. The new form wraps sub in a generic `<div>` so callers may pass a fragment of multiple lines (`<div className="flex flex-col gap-0.5">…</div>`). Font-mono / tabular-nums / text-[10px] move up to the wrapper so individual sub-lines don't need to repeat the styling.

- [ ] **Step 2: Remove `MiniBar` and `getBarColor` from `index.tsx`, import from `./index.cells`**

In `apps/web/src/routes/_authed/servers/index.tsx`:

- Delete the `getBarColor` function (currently around line 468).
- Delete the `MiniBar` function (currently around line 478-492).
- Add an import near the other local imports:

```tsx
import { MiniBar } from './index.cells'
```

- [ ] **Step 3: Update existing inline cell calls to match the new API**

The three existing call sites (CPU, Memory, Disk columns) need the `sub` prop wrapped properly because the old `sub` was `string`. For this task, preserve the existing sub content but wrap the disk/memory sub in a fragment-compatible node. Concretely:

```tsx
// CPU cell (old):
cell: ({ row }) => <MiniBar pct={row.original.cpu} />,

// Memory cell (old inline):
return <MiniBar pct={memPct} sub={formatBytes(s.mem_used)} />

// Disk cell (old inline):
return <MiniBar pct={diskPct} sub={formatBytes(s.disk_used)} />
```

Change the memory and disk cells so the string becomes a node:

```tsx
// Memory cell (new, temporary — will be replaced in Task 4):
return <MiniBar pct={memPct} sub={<span>{formatBytes(s.mem_used)}</span>} />

// Disk cell (new, temporary — will be replaced in Task 5):
return <MiniBar pct={diskPct} sub={<span>{formatBytes(s.disk_used)}</span>} />
```

CPU has no sub today — leave it as is for now; Task 3 adds the load sub-line.

- [ ] **Step 4: Verify typecheck and existing tests still pass**

Run: `cd apps/web && bun run typecheck && bun run test -- --run`
Expected: PASS. No behavioral change yet — only a refactor and a `sub` type widening.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/routes/_authed/servers/index.tsx apps/web/src/routes/_authed/servers/index.cells.tsx
git commit -m "refactor(web): extract MiniBar to index.cells.tsx and accept ReactNode sub"
```

---

## Task 3: Implement and test `CpuCell` (with i18n load)

**Files:**
- Modify: `apps/web/src/routes/_authed/servers/index.cells.tsx` (add `CpuCell`)
- Create: `apps/web/src/routes/_authed/servers/index.cells.test.tsx`
- Modify: `apps/web/src/routes/_authed/servers/index.tsx` (wire `<CpuCell>` into the `cpu` column)

- [ ] **Step 1: Write the failing test**

Create `apps/web/src/routes/_authed/servers/index.cells.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { CpuCell } from './index.cells'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key })
}))

function makeServer(overrides: Partial<ServerMetrics> = {}): ServerMetrics {
  return {
    id: 'srv-1',
    name: 'test-server',
    online: true,
    country_code: null,
    cpu: 45,
    cpu_name: null,
    disk_read_bytes_per_sec: 0,
    disk_total: 500_000_000_000,
    disk_used: 120_000_000_000,
    disk_write_bytes_per_sec: 0,
    group_id: null,
    last_active: 0,
    load1: 1.23,
    load5: 0,
    load15: 0,
    mem_total: 8_000_000_000,
    mem_used: 3_200_000_000,
    net_in_speed: 0,
    net_in_transfer: 0,
    net_out_speed: 0,
    net_out_transfer: 0,
    os: null,
    process_count: 0,
    region: null,
    swap_total: 0,
    swap_used: 0,
    tcp_conn: 0,
    udp_conn: 0,
    uptime: 0,
    ...overrides
  }
}

describe('CpuCell', () => {
  it('shows cpu percentage and load1', () => {
    render(<CpuCell server={makeServer({ cpu: 45, load1: 1.23 })} />)
    expect(screen.getByText('45%')).toBeDefined()
    // Sub line contains the translated label key and load1 formatted to 2 decimals.
    expect(screen.getByText(/card_load\s+1\.23/)).toBeDefined()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd apps/web && bun run test -- --run src/routes/_authed/servers/index.cells.test.tsx`
Expected: FAIL with `CpuCell is not exported from ./index.cells` (or similar).

- [ ] **Step 3: Implement `CpuCell`**

Add to `apps/web/src/routes/_authed/servers/index.cells.tsx`:

```tsx
import { useTranslation } from 'react-i18next'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
```

Then append:

```tsx
export function CpuCell({ server }: { server: ServerMetrics }) {
  const { t } = useTranslation(['servers'])
  return (
    <MiniBar
      pct={server.cpu}
      sub={<span>{t('card_load')} {server.load1.toFixed(2)}</span>}
    />
  )
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd apps/web && bun run test -- --run src/routes/_authed/servers/index.cells.test.tsx`
Expected: PASS.

- [ ] **Step 5: Wire `CpuCell` into the table column**

In `apps/web/src/routes/_authed/servers/index.tsx`, update the cpu column cell:

```tsx
// Before:
{
  accessorKey: 'cpu',
  id: 'cpu',
  header: ({ column }) => <DataTableColumnHeader column={column} label={t('col_cpu')} />,
  cell: ({ row }) => <MiniBar pct={row.original.cpu} />,
  size: 160,
  meta: { className: 'w-[160px]' }
},

// After:
{
  accessorKey: 'cpu',
  id: 'cpu',
  header: ({ column }) => <DataTableColumnHeader column={column} label={t('col_cpu')} />,
  cell: ({ row }) => <CpuCell server={row.original} />,
  size: 160,
  meta: { className: 'w-[160px]' }
},
```

Add to the `./index.cells` import at the top of `index.tsx`:

```tsx
import { CpuCell, MiniBar } from './index.cells'
```

- [ ] **Step 6: Verify full web test suite still passes**

Run: `cd apps/web && bun run test -- --run && bun run typecheck`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add apps/web/src/routes/_authed/servers/index.tsx apps/web/src/routes/_authed/servers/index.cells.tsx apps/web/src/routes/_authed/servers/index.cells.test.tsx
git commit -m "feat(web): add CpuCell with load1 sub-line"
```

---

## Task 4: Implement and test `MemoryCell`

**Files:**
- Modify: `apps/web/src/routes/_authed/servers/index.cells.tsx`
- Modify: `apps/web/src/routes/_authed/servers/index.cells.test.tsx`
- Modify: `apps/web/src/routes/_authed/servers/index.tsx`

- [ ] **Step 1: Write the failing test**

Append to `apps/web/src/routes/_authed/servers/index.cells.test.tsx`:

```tsx
import { MemoryCell } from './index.cells'

describe('MemoryCell', () => {
  it('shows used/total with percentage', () => {
    render(
      <MemoryCell
        server={makeServer({ mem_used: 3_200_000_000, mem_total: 8_000_000_000 })}
      />
    )
    // 3.2GB / 8.0GB (formatBytes uses 1 decimal, units: B/KB/MB/GB/TB, base 1024)
    // 3_200_000_000 / 1024^3 ≈ 2.98 → "3.0 GB"
    // 8_000_000_000 / 1024^3 ≈ 7.45 → "7.5 GB"
    expect(screen.getByText('3.0 GB / 7.5 GB')).toBeDefined()
    // Pct: 3.2e9 / 8e9 = 40
    expect(screen.getByText('40%')).toBeDefined()
  })

  it('renders 0B / 0B when mem_total is zero', () => {
    render(<MemoryCell server={makeServer({ mem_used: 0, mem_total: 0 })} />)
    expect(screen.getByText('0 B / 0 B')).toBeDefined()
    expect(screen.getByText('0%')).toBeDefined()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd apps/web && bun run test -- --run src/routes/_authed/servers/index.cells.test.tsx`
Expected: FAIL with `MemoryCell is not exported from ./index.cells`.

- [ ] **Step 3: Implement `MemoryCell`**

Append to `apps/web/src/routes/_authed/servers/index.cells.tsx`:

```tsx
import { formatBytes } from '@/lib/utils'

// ...existing code...

export function MemoryCell({ server }: { server: ServerMetrics }) {
  const pct = server.mem_total > 0 ? (server.mem_used / server.mem_total) * 100 : 0
  return (
    <MiniBar
      pct={pct}
      sub={<span>{formatBytes(server.mem_used)} / {formatBytes(server.mem_total)}</span>}
    />
  )
}
```

(Consolidate the `formatBytes` import at the top of the file, not inside the cell.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd apps/web && bun run test -- --run src/routes/_authed/servers/index.cells.test.tsx`
Expected: PASS (all 3 cases).

- [ ] **Step 5: Wire `MemoryCell` into the table column**

In `apps/web/src/routes/_authed/servers/index.tsx`, update the memory column cell and import:

```tsx
import { CpuCell, MemoryCell, MiniBar } from './index.cells'
```

```tsx
// Before:
{
  accessorFn: (row) => (row.mem_total > 0 ? row.mem_used / row.mem_total : 0),
  id: 'memory',
  header: ({ column }) => <DataTableColumnHeader column={column} label={t('col_memory')} />,
  cell: ({ row }) => {
    const s = row.original
    const memPct = s.mem_total > 0 ? (s.mem_used / s.mem_total) * 100 : 0
    return <MiniBar pct={memPct} sub={<span>{formatBytes(s.mem_used)}</span>} />
  },
  size: 160,
  meta: { className: 'w-[160px]' }
},

// After:
{
  accessorFn: (row) => (row.mem_total > 0 ? row.mem_used / row.mem_total : 0),
  id: 'memory',
  header: ({ column }) => <DataTableColumnHeader column={column} label={t('col_memory')} />,
  cell: ({ row }) => <MemoryCell server={row.original} />,
  size: 160,
  meta: { className: 'w-[160px]' }
},
```

- [ ] **Step 6: Verify full test suite**

Run: `cd apps/web && bun run test -- --run && bun run typecheck`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add apps/web/src/routes/_authed/servers/index.cells.tsx apps/web/src/routes/_authed/servers/index.cells.test.tsx apps/web/src/routes/_authed/servers/index.tsx
git commit -m "feat(web): add MemoryCell showing used/total"
```

---

## Task 5: Implement and test `DiskCell` (with online-gated I/O row)

**Files:**
- Modify: `apps/web/src/routes/_authed/servers/index.cells.tsx`
- Modify: `apps/web/src/routes/_authed/servers/index.cells.test.tsx`
- Modify: `apps/web/src/routes/_authed/servers/index.tsx`

This is the most important task — the I/O row is the new feature and offline behavior is a correctness contract.

- [ ] **Step 1: Write the failing tests**

Append to `apps/web/src/routes/_authed/servers/index.cells.test.tsx`:

```tsx
import { DiskCell } from './index.cells'

describe('DiskCell', () => {
  it('shows used/total and I/O row when online', () => {
    render(
      <DiskCell
        server={makeServer({
          online: true,
          disk_used: 120_000_000_000,
          disk_total: 500_000_000_000,
          disk_read_bytes_per_sec: 2_100_000,
          disk_write_bytes_per_sec: 1_200_000
        })}
      />
    )
    // used/total line — 120e9 → 111.8 GB, 500e9 → 465.7 GB
    expect(screen.getByText('111.8 GB / 465.7 GB')).toBeDefined()
    // I/O row shows both arrows
    expect(screen.getByText(/↺.*2\.0 MB\/s.*↻.*1\.1 MB\/s/)).toBeDefined()
  })

  it('hides I/O row when offline', () => {
    render(
      <DiskCell
        server={makeServer({
          online: false,
          disk_used: 120_000_000_000,
          disk_total: 500_000_000_000,
          disk_read_bytes_per_sec: 2_100_000,
          disk_write_bytes_per_sec: 1_200_000
        })}
      />
    )
    expect(screen.getByText('111.8 GB / 465.7 GB')).toBeDefined()
    // Neither arrow glyph should appear
    expect(screen.queryByText(/↺/)).toBeNull()
    expect(screen.queryByText(/↻/)).toBeNull()
  })

  it('renders 0 B/s arrows when online with zero I/O (legacy agent or idle)', () => {
    render(
      <DiskCell
        server={makeServer({
          online: true,
          disk_read_bytes_per_sec: 0,
          disk_write_bytes_per_sec: 0
        })}
      />
    )
    // Regex allows one or more spaces between tokens
    expect(screen.getByText(/↺\s+0 B\/s\s+↻\s+0 B\/s/)).toBeDefined()
  })
})
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd apps/web && bun run test -- --run src/routes/_authed/servers/index.cells.test.tsx`
Expected: FAIL with `DiskCell is not exported from ./index.cells`.

- [ ] **Step 3: Implement `DiskCell`**

Append to `apps/web/src/routes/_authed/servers/index.cells.tsx`:

```tsx
import { formatSpeed } from '@/lib/utils'

// (add formatSpeed to the existing formatBytes import if already present:
//   import { formatBytes, formatSpeed } from '@/lib/utils')

export function DiskCell({ server }: { server: ServerMetrics }) {
  const pct = server.disk_total > 0 ? (server.disk_used / server.disk_total) * 100 : 0
  return (
    <MiniBar
      pct={pct}
      sub={
        <div className="flex flex-col gap-0.5">
          <span>{formatBytes(server.disk_used)} / {formatBytes(server.disk_total)}</span>
          {server.online && (
            <span>
              ↺ {formatSpeed(server.disk_read_bytes_per_sec)}  ↻ {formatSpeed(server.disk_write_bytes_per_sec)}
            </span>
          )}
        </div>
      }
    />
  )
}
```

Note the two spaces between `↺ ...` and `↻ ...` — this creates visual separation at 10px font size without needing a border or pipe.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd apps/web && bun run test -- --run src/routes/_authed/servers/index.cells.test.tsx`
Expected: PASS (all DiskCell cases).

- [ ] **Step 5: Wire `DiskCell` into the table column**

In `apps/web/src/routes/_authed/servers/index.tsx`, update the disk column and import:

```tsx
import { CpuCell, DiskCell, MemoryCell, MiniBar } from './index.cells'
```

```tsx
// Before:
{
  accessorFn: (row) => (row.disk_total > 0 ? row.disk_used / row.disk_total : 0),
  id: 'disk',
  header: ({ column }) => <DataTableColumnHeader column={column} label={t('col_disk')} />,
  cell: ({ row }) => {
    const s = row.original
    const diskPct = s.disk_total > 0 ? (s.disk_used / s.disk_total) * 100 : 0
    return <MiniBar pct={diskPct} sub={<span>{formatBytes(s.disk_used)}</span>} />
  },
  size: 160,
  meta: { className: 'w-[160px]' }
},

// After:
{
  accessorFn: (row) => (row.disk_total > 0 ? row.disk_used / row.disk_total : 0),
  id: 'disk',
  header: ({ column }) => <DataTableColumnHeader column={column} label={t('col_disk')} />,
  cell: ({ row }) => <DiskCell server={row.original} />,
  size: 160,
  meta: { className: 'w-[160px]' }
},
```

- [ ] **Step 6: Verify full test suite and types**

Run: `cd apps/web && bun run test -- --run && bun run typecheck`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add apps/web/src/routes/_authed/servers/index.cells.tsx apps/web/src/routes/_authed/servers/index.cells.test.tsx apps/web/src/routes/_authed/servers/index.tsx
git commit -m "feat(web): add DiskCell with online-gated i/o row"
```

---

## Task 6: Implement and test `NetworkCell` (with offline zeroing + cumulative)

**Files:**
- Modify: `apps/web/src/routes/_authed/servers/index.cells.tsx`
- Modify: `apps/web/src/routes/_authed/servers/index.cells.test.tsx`
- Modify: `apps/web/src/routes/_authed/servers/index.tsx`

- [ ] **Step 1: Write the failing tests**

Append to `apps/web/src/routes/_authed/servers/index.cells.test.tsx`:

```tsx
import { NetworkCell } from './index.cells'

describe('NetworkCell', () => {
  it('shows live speed and cumulative when online', () => {
    render(
      <NetworkCell
        server={makeServer({
          online: true,
          net_in_speed: 1_200_000,
          net_out_speed: 340_000,
          net_in_transfer: 12_000_000_000,
          net_out_transfer: 3_400_000_000
        })}
      />
    )
    // Live speeds
    expect(screen.getByText(/↓1\.1 MB\/s/)).toBeDefined()
    expect(screen.getByText(/↑332\.0 KB\/s/)).toBeDefined()
    // Cumulative row
    expect(screen.getByText(/Σ\s*↓11\.2 GB\s+↑3\.2 GB/)).toBeDefined()
  })

  it('zeroes live speed and keeps cumulative when offline', () => {
    render(
      <NetworkCell
        server={makeServer({
          online: false,
          net_in_speed: 1_200_000, // stale — should be ignored
          net_out_speed: 340_000,
          net_in_transfer: 12_000_000_000,
          net_out_transfer: 3_400_000_000
        })}
      />
    )
    // Live speed zeroed
    expect(screen.getByText(/↓0 B\/s/)).toBeDefined()
    expect(screen.getByText(/↑0 B\/s/)).toBeDefined()
    // Should NOT show the stale values
    expect(screen.queryByText(/↓1\.1 MB\/s/)).toBeNull()
    // Cumulative still present
    expect(screen.getByText(/Σ\s*↓11\.2 GB\s+↑3\.2 GB/)).toBeDefined()
  })
})
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd apps/web && bun run test -- --run src/routes/_authed/servers/index.cells.test.tsx`
Expected: FAIL with `NetworkCell is not exported from ./index.cells`.

- [ ] **Step 3: Implement `NetworkCell`**

Append to `apps/web/src/routes/_authed/servers/index.cells.tsx`:

```tsx
export function NetworkCell({ server }: { server: ServerMetrics }) {
  const inSpeed = server.online ? server.net_in_speed : 0
  const outSpeed = server.online ? server.net_out_speed : 0
  return (
    <div className="flex flex-col gap-0.5 font-mono text-muted-foreground text-xs tabular-nums">
      <span>
        <span className="inline-block min-w-[64px]">↓{formatSpeed(inSpeed)}</span>
        <span className="ml-2 inline-block min-w-[64px]">↑{formatSpeed(outSpeed)}</span>
      </span>
      <span className="text-[10px]">
        Σ ↓{formatBytes(server.net_in_transfer)}  ↑{formatBytes(server.net_out_transfer)}
      </span>
    </div>
  )
}
```

Note `NetworkCell` does not use `MiniBar` — it is a two-line inline text cell, not a progress bar.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd apps/web && bun run test -- --run src/routes/_authed/servers/index.cells.test.tsx`
Expected: PASS (all NetworkCell cases).

- [ ] **Step 5: Wire `NetworkCell` into the table column**

In `apps/web/src/routes/_authed/servers/index.tsx`, update the network column and import:

```tsx
import { CpuCell, DiskCell, MemoryCell, MiniBar, NetworkCell } from './index.cells'
```

```tsx
// Before:
{
  id: 'network',
  enableSorting: false,
  header: () => <span className="text-muted-foreground text-xs">{t('col_network')}</span>,
  cell: ({ row }) => {
    const s = row.original
    return (
      <span className="font-mono text-muted-foreground text-xs tabular-nums">
        <span className="inline-block min-w-[64px]">↓{formatSpeed(s.net_in_speed)}</span>
        <span className="ml-2 inline-block min-w-[64px]">↑{formatSpeed(s.net_out_speed)}</span>
      </span>
    )
  },
  size: 160,
  meta: { className: 'hidden lg:table-cell lg:w-[160px]' }
},

// After:
{
  id: 'network',
  enableSorting: false,
  header: () => <span className="text-muted-foreground text-xs">{t('col_network')}</span>,
  cell: ({ row }) => <NetworkCell server={row.original} />,
  size: 160,
  meta: { className: 'hidden lg:table-cell lg:w-[160px]' }
},
```

Now the top-level imports in `index.tsx` should no longer need `formatSpeed` or `formatBytes` at the route level (they are only used by cell components). Remove any now-unused imports — the existing `import { cn, countryCodeToFlag, formatBytes, formatSpeed, formatUptime } from '@/lib/utils'` should be trimmed based on what else uses them in the file. `countryCodeToFlag` and `formatUptime` are still used by the `name` and `uptime` columns respectively; `cn` may be unused after MiniBar moved out.

- [ ] **Step 6: Verify full test suite and types**

Run: `cd apps/web && bun run test -- --run && bun run typecheck && bun x ultracite check apps/web/src/routes/_authed/servers/`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add apps/web/src/routes/_authed/servers/index.cells.tsx apps/web/src/routes/_authed/servers/index.cells.test.tsx apps/web/src/routes/_authed/servers/index.tsx
git commit -m "feat(web): add NetworkCell with offline zeroing and cumulative row"
```

---

## Task 7: Final verification

**Files:** none — verification only.

- [ ] **Step 1: Run the full frontend test suite**

Run: `cd apps/web && bun run test -- --run`
Expected: PASS. Watch for any regressions in pre-existing tests that might be sensitive to MiniBar's sub-line DOM change (the `<p>` → `<div>` wrapper, font-mono / text-[10px] moved up from the sub-line to the wrapper). If any test queries sub content by tag or class, update it to match the new structure.

- [ ] **Step 2: Typecheck both web targets**

Run: `bun run typecheck`
Expected: PASS.

- [ ] **Step 3: Lint**

Run: `bun x ultracite check apps/web/src/routes/_authed/servers/ apps/web/src/hooks/use-servers-ws.ts`
Expected: no warnings/errors. Auto-fix with `bun x ultracite fix <paths>` if any formatting drift.

- [ ] **Step 4: Manual visual check**

Start the dev server against prod data for realistic content: `make web-dev-prod` (requires `SERVERBEE_PROD_URL` and `SERVERBEE_PROD_READONLY_API_KEY` per `CLAUDE.md`). Open `http://localhost:5173/servers?view=table`.

Verify at 1789×963 viewport:
- Each online row's Disk cell has 3 sub-lines: percentage row, `used/total`, I/O `↺ ↻`.
- Any offline row's Disk cell has only 2 sub-lines (no I/O row).
- Any offline row's Network cell shows `↓0 B/s ↑0 B/s` in the live-speed row, with the Σ cumulative row unchanged.
- CPU sub-line reads `Load 1.23` (or `负载 1.23` depending on locale), not `load 1.23`.
- Memory sub-line shows `used / total` (e.g. `3.0 GB / 7.5 GB`), not just `used`.
- Row height increase is visually acceptable (no layout break, no overflow).

Also resize the browser below the `lg` breakpoint (~<1024px) and confirm the Network column is hidden as before.

- [ ] **Step 5: Report completion**

If everything passes, the feature is done — no additional commit (no code changes in this task).

If manual check surfaces a visual issue (e.g., row content overflows 160px), file a follow-up rather than patching blindly; report what was seen and which cell/viewport.

---

## Self-Review Summary

**Spec coverage:**
- Disk I/O row with online gating — Task 5 ✓
- Memory used/total upgrade — Task 4 ✓
- CPU load1 with i18n — Task 3 ✓
- Network zero-on-offline + cumulative — Task 6 ✓
- TS type tightening — Task 1 ✓
- MiniBar ReactNode sub — Task 2 ✓
- Named cell extraction + tests — Tasks 3–6 ✓
- Lint/typecheck/manual QA — Task 7 ✓

**Not applicable:** Grid view (`ServerCard`) and Rust protocol changes are explicitly out of scope per the spec's non-goals.

**Placeholder scan:** None.

**Type consistency:** All cells take `{ server: ServerMetrics }`. `MiniBar` signature is consistent across tasks (`{ pct: number; sub?: ReactNode }`).
