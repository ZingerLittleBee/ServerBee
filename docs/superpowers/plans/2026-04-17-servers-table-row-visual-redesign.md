# Servers Table Row Visual Redesign Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild every row in `/servers?view=table` so each metric cell is a two-line block (icon + bar + % on top, compact sub-data below), replace the status text badge with a pulsing dot column, show `server_tags` under the server name, and surface monthly traffic quota usage as a bar in the Network cell.

**Architecture:** Phase A is purely frontend — a new shared `lib/traffic.ts` primitive, a `<MetricBarRow />` primitive in the servers route, and a rewrite of every cell in `index.cells.tsx`. Phase B is additive backend — `tags` + `cpu_cores` are added to the `ServerStatus` WS payload and a small `server_tags` REST surface is exposed; the frontend gains a `useServerTags` hook and a tag editor inside `ServerEditDialog`. No database migration is required (both `servers.cpu_cores` and the `server_tags` table already exist).

**Tech Stack:** React 19, TanStack Router + Query, shadcn/ui, lucide-react icons, Vitest + @testing-library/react, Axum 0.8, sea-orm, utoipa, tokio_tungstenite (integration tests).

**Spec:** `docs/superpowers/specs/2026-04-17-servers-table-row-visual-redesign-design.md`

---

## File Map

### Phase A — frontend (no backend changes)

| File | Action | Responsibility |
|------|--------|----------------|
| `apps/web/src/locales/en/servers.json` | Modify | Add tag / uptime / validation keys |
| `apps/web/src/locales/zh/servers.json` | Modify | Same keys in Chinese |
| `apps/web/src/lib/traffic.ts` | Create | `DEFAULT_TRAFFIC_LIMIT_BYTES` + `computeTrafficQuota` helper |
| `apps/web/src/lib/traffic.test.ts` | Create | Unit tests for the helper |
| `apps/web/src/components/server/server-card.tsx` | Modify | Consume `computeTrafficQuota` instead of inlined logic |
| `apps/web/src/hooks/use-servers-ws.ts` | Modify | Add `tags`, `cpu_cores` to `ServerMetrics`; extend `STATIC_FIELDS` + default guard to cover `[]` arrays |
| `apps/web/src/hooks/use-servers-ws.test.ts` | Create | Unit tests for `mergeServerUpdate` guard |
| `apps/web/src/components/server/status-dot.tsx` | Create | `<StatusDot online />` — pulsing/muted dot |
| `apps/web/src/components/server/status-dot.test.tsx` | Create | Unit test |
| `apps/web/src/components/server/tag-chip.tsx` | Create | `<TagChipRow tags>` with stable-hash palette |
| `apps/web/src/components/server/tag-chip.test.tsx` | Create | Unit test |
| `apps/web/src/routes/_authed/servers/index.cells.tsx` | Rewrite | New `MetricBarRow`, `CpuCell`, `MemoryCell`, `DiskCell`, `NetworkCell`, `UptimeCell`, `NameCell` |
| `apps/web/src/routes/_authed/servers/index.cells.test.tsx` | Rewrite | New cell tests (keeps the file path, replaces old tests) |
| `apps/web/src/routes/_authed/servers/index.tsx` | Modify | New column set (status-dot first, status text column dropped), `useTrafficOverview` wired to `NetworkCell` |

### Phase B — backend + editor

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/common/src/types.rs` | Modify | Add `tags: Vec<String>` and `cpu_cores: Option<i32>` to `ServerStatus` |
| `crates/server/src/router/ws/browser.rs` | Modify | `build_full_sync` populates both fields (single grouped query for tags) |
| `crates/server/src/service/server_tag.rs` | Create | Validation + CRUD service (`list_tags`, `set_tags`) |
| `crates/server/src/service/mod.rs` | Modify | `pub mod server_tag;` |
| `crates/server/src/router/api/server_tag.rs` | Create | REST router: `GET /api/servers/:id/tags`, `PUT /api/servers/:id/tags` |
| `crates/server/src/router/api/mod.rs` | Modify | Mount new read/write sub-routers |
| `crates/server/tests/server_tags.rs` | Create | Integration test: RBAC + validation + full_sync payload shape |
| `apps/web/src/locales/en/servers.json` | Modify | Additional tag-editor keys (save/revert toasts) |
| `apps/web/src/locales/zh/servers.json` | Modify | Same |
| `apps/web/src/hooks/use-server-tags.ts` | Create | `useServerTags(id)` + `useUpdateServerTags(id)` with optimistic cache update |
| `apps/web/src/components/server/server-edit-dialog.tsx` | Modify | Tags editor + sequential PATCH-then-PUT save |
| `tests/servers/table-row-visual-redesign.md` | Create | Manual QA checklist |

---

## Chunk 1: Shared primitives & merge guard

### Task 1: Add i18n keys for new labels

**Files:**
- Modify: `apps/web/src/locales/en/servers.json`
- Modify: `apps/web/src/locales/zh/servers.json`

- [ ] **Step 1: Add English keys**

Insert after the `"edit_failed": "Failed to update server"` line:

```json
"tags_label": "Tags",
"tags_hint": "Comma or space separated, up to 8 tags, 16 chars each",
"tags_placeholder": "prod, db, web",
"tags_validation_too_many": "At most 8 tags",
"tags_validation_too_long": "Each tag must be ≤16 chars",
"tags_validation_invalid_char": "Only letters, digits, and ._- are allowed",
"tags_save_failed": "Failed to save tags",
"last_seen_ago": "last seen {{time}}",
"offline_label": "offline",
```

- [ ] **Step 2: Add Chinese keys**

Insert after the `"edit_failed"` line in `zh/servers.json`:

```json
"tags_label": "标签",
"tags_hint": "使用逗号或空格分隔，最多 8 个标签，每个 16 字符以内",
"tags_placeholder": "prod, db, web",
"tags_validation_too_many": "最多 8 个标签",
"tags_validation_too_long": "单个标签最多 16 字符",
"tags_validation_invalid_char": "只允许字母、数字、`._-`",
"tags_save_failed": "保存标签失败",
"last_seen_ago": "最后上线 {{time}}",
"offline_label": "离线",
```

- [ ] **Step 3: Verify JSON is valid**

Run: `bun run --cwd apps/web typecheck` (TypeScript resource files are validated by the build; if the project uses `bun x tsc --noEmit` it will also surface JSON parse errors through imports)

Expected: exit 0.

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/locales/en/servers.json apps/web/src/locales/zh/servers.json
git commit -m "feat(web): add i18n keys for servers table tags and uptime labels"
```

---

### Task 2: Shared `lib/traffic.ts` primitive (TDD)

**Files:**
- Create: `apps/web/src/lib/traffic.ts`
- Create: `apps/web/src/lib/traffic.test.ts`

- [ ] **Step 1: Write failing tests**

Create `apps/web/src/lib/traffic.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import type { TrafficOverviewItem } from '@/hooks/use-traffic-overview'
import { computeTrafficQuota, DEFAULT_TRAFFIC_LIMIT_BYTES } from './traffic'

const GB = 1024 ** 3
const TB = 1024 ** 4

function entry(overrides: Partial<TrafficOverviewItem>): TrafficOverviewItem {
  return {
    billing_cycle: null,
    cycle_in: 0,
    cycle_out: 0,
    days_remaining: null,
    name: 'srv',
    percent_used: null,
    server_id: 'srv-1',
    traffic_limit: null,
    ...overrides
  }
}

describe('computeTrafficQuota', () => {
  it('uses cycle_in + cycle_out when entry present', () => {
    const result = computeTrafficQuota({
      entry: entry({ cycle_in: 50 * GB, cycle_out: 43.2 * GB, traffic_limit: 1 * TB }),
      netInTransfer: 999,
      netOutTransfer: 999
    })
    expect(result.used).toBe(50 * GB + 43.2 * GB)
    expect(result.limit).toBe(1 * TB)
    expect(result.pct).toBeCloseTo(((50 + 43.2) / 1024) * 100, 1)
  })

  it('falls back to net_in_transfer + net_out_transfer when entry is undefined', () => {
    const result = computeTrafficQuota({
      entry: undefined,
      netInTransfer: 10 * GB,
      netOutTransfer: 5 * GB
    })
    expect(result.used).toBe(15 * GB)
    expect(result.limit).toBe(DEFAULT_TRAFFIC_LIMIT_BYTES)
    expect(DEFAULT_TRAFFIC_LIMIT_BYTES).toBe(TB)
  })

  it('falls back to default limit when traffic_limit is null', () => {
    const result = computeTrafficQuota({
      entry: entry({ traffic_limit: null }),
      netInTransfer: 0,
      netOutTransfer: 0
    })
    expect(result.limit).toBe(DEFAULT_TRAFFIC_LIMIT_BYTES)
  })

  it('falls back to default limit when traffic_limit <= 0', () => {
    const result = computeTrafficQuota({
      entry: entry({ traffic_limit: 0 }),
      netInTransfer: 0,
      netOutTransfer: 0
    })
    expect(result.limit).toBe(DEFAULT_TRAFFIC_LIMIT_BYTES)

    const negative = computeTrafficQuota({
      entry: entry({ traffic_limit: -1 }),
      netInTransfer: 0,
      netOutTransfer: 0
    })
    expect(negative.limit).toBe(DEFAULT_TRAFFIC_LIMIT_BYTES)
  })

  it('clamps pct to 100 when used exceeds limit', () => {
    const result = computeTrafficQuota({
      entry: entry({ cycle_in: 2 * TB, cycle_out: 0, traffic_limit: 1 * TB }),
      netInTransfer: 0,
      netOutTransfer: 0
    })
    expect(result.pct).toBe(100)
  })

  it('returns 0 pct when limit resolves to the default and used is 0', () => {
    const result = computeTrafficQuota({
      entry: undefined,
      netInTransfer: 0,
      netOutTransfer: 0
    })
    expect(result.pct).toBe(0)
  })
})
```

- [ ] **Step 2: Run the tests (should fail with import error)**

Run: `bun run --cwd apps/web test src/lib/traffic.test.ts`
Expected: FAIL with "Failed to resolve './traffic'" (module not found).

- [ ] **Step 3: Create the primitive**

Create `apps/web/src/lib/traffic.ts`:

```ts
import type { TrafficOverviewItem } from '@/hooks/use-traffic-overview'

export const DEFAULT_TRAFFIC_LIMIT_BYTES = 1024 ** 4

export interface TrafficQuota {
  used: number
  limit: number
  pct: number
}

interface ComputeInput {
  entry: TrafficOverviewItem | undefined
  netInTransfer: number
  netOutTransfer: number
}

export function computeTrafficQuota({ entry, netInTransfer, netOutTransfer }: ComputeInput): TrafficQuota {
  const used = entry ? entry.cycle_in + entry.cycle_out : netInTransfer + netOutTransfer
  const rawLimit = entry?.traffic_limit ?? null
  const limit = rawLimit != null && rawLimit > 0 ? rawLimit : DEFAULT_TRAFFIC_LIMIT_BYTES
  const rawPct = limit > 0 ? (used / limit) * 100 : 0
  const pct = Math.min(rawPct, 100)
  return { used, limit, pct }
}
```

- [ ] **Step 4: Run the tests (should pass)**

Run: `bun run --cwd apps/web test src/lib/traffic.test.ts`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/lib/traffic.ts apps/web/src/lib/traffic.test.ts
git commit -m "feat(web): add shared traffic quota helper"
```

---

### Task 3: Refactor `ServerCard` to use the shared helper

**Files:**
- Modify: `apps/web/src/components/server/server-card.tsx`

- [ ] **Step 1: Replace inlined traffic math**

In `apps/web/src/components/server/server-card.tsx`:

- Delete the line `const DEFAULT_TRAFFIC_LIMIT_BYTES = 1024 ** 4 // 1 TiB fallback when no quota configured`
- Replace the block that starts with `const trafficEntry = trafficOverview?.find(...)` through `const trafficRingPct = Math.min(trafficRawPct, 100)` with:

```tsx
const trafficEntry = trafficOverview?.find((entry) => entry.server_id === server.id)
const { used: trafficUsed, limit: trafficLimit, pct: trafficRingPct } = computeTrafficQuota({
  entry: trafficEntry,
  netInTransfer: server.net_in_transfer,
  netOutTransfer: server.net_out_transfer
})
const trafficDaysRemaining = trafficEntry?.days_remaining ?? null
```

- Add an import at the top of the file:

```tsx
import { computeTrafficQuota } from '@/lib/traffic'
```

- [ ] **Step 2: Run the existing ServerCard tests**

Run: `bun run --cwd apps/web test server-card`
Expected: PASS (no behavior change).

- [ ] **Step 3: Run the full frontend test suite**

Run: `bun run --cwd apps/web test`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/components/server/server-card.tsx
git commit -m "refactor(web): ServerCard consumes shared computeTrafficQuota helper"
```

---

### Task 4: Extend `mergeServerUpdate` guard and `ServerMetrics` interface

**Files:**
- Modify: `apps/web/src/hooks/use-servers-ws.ts`
- Create: `apps/web/src/hooks/use-servers-ws.test.ts`

- [ ] **Step 1: Write failing tests**

Create `apps/web/src/hooks/use-servers-ws.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import { mergeServerUpdate, type ServerMetrics } from './use-servers-ws'

function baseServer(overrides: Partial<ServerMetrics> = {}): ServerMetrics {
  return {
    id: 'srv-1',
    name: 'srv',
    online: true,
    country_code: null,
    cpu: 0,
    cpu_name: null,
    cpu_cores: null,
    disk_read_bytes_per_sec: 0,
    disk_total: 0,
    disk_used: 0,
    disk_write_bytes_per_sec: 0,
    group_id: null,
    last_active: 0,
    load1: 0,
    load5: 0,
    load15: 0,
    mem_total: 0,
    mem_used: 0,
    net_in_speed: 0,
    net_in_transfer: 0,
    net_out_speed: 0,
    net_out_transfer: 0,
    os: null,
    process_count: 0,
    region: null,
    swap_total: 0,
    swap_used: 0,
    tags: [],
    tcp_conn: 0,
    udp_conn: 0,
    uptime: 0,
    features: [],
    ...overrides
  }
}

describe('mergeServerUpdate static-fields guard', () => {
  it('preserves prior tags when incoming frame carries tags: []', () => {
    const prev = [baseServer({ tags: ['prod', 'web'] })]
    const incoming = [baseServer({ tags: [], cpu: 42 })]
    const result = mergeServerUpdate(prev, incoming)
    expect(result[0].tags).toEqual(['prod', 'web'])
    expect(result[0].cpu).toBe(42)
  })

  it('preserves prior features when incoming frame carries features: []', () => {
    const prev = [baseServer({ features: ['docker'] })]
    const incoming = [baseServer({ features: [], cpu: 10 })]
    const result = mergeServerUpdate(prev, incoming)
    expect(result[0].features).toEqual(['docker'])
    expect(result[0].cpu).toBe(10)
  })

  it('preserves prior cpu_cores when incoming frame carries cpu_cores: null', () => {
    const prev = [baseServer({ cpu_cores: 8 })]
    const incoming = [baseServer({ cpu_cores: null, cpu: 5 })]
    const result = mergeServerUpdate(prev, incoming)
    expect(result[0].cpu_cores).toBe(8)
  })

  it('overwrites prior tags with non-empty incoming array', () => {
    const prev = [baseServer({ tags: ['old'] })]
    const incoming = [baseServer({ tags: ['new-a', 'new-b'] })]
    const result = mergeServerUpdate(prev, incoming)
    expect(result[0].tags).toEqual(['new-a', 'new-b'])
  })
})
```

- [ ] **Step 2: Run the tests (expected to fail)**

Run: `bun run --cwd apps/web test use-servers-ws`
Expected: FAIL — existing `ServerMetrics` is missing `tags` / `cpu_cores` fields; guard does not cover `[]`.

- [ ] **Step 3: Extend `ServerMetrics` and `STATIC_FIELDS`**

In `apps/web/src/hooks/use-servers-ws.ts`:

- Inside `interface ServerMetrics { ... }`, add these fields (alphabetically in the interface, respecting existing ordering):

```ts
  cpu_cores?: number | null
  features?: string[]   // already declared? confirm; if present, ensure optional
  tags?: string[]
```

Note: `features?: string[]` is **already declared**; do not duplicate. Only add `cpu_cores` and `tags`.

- Extend `STATIC_FIELDS`:

```ts
const STATIC_FIELDS = new Set([
  'mem_total',
  'swap_total',
  'disk_total',
  'cpu_name',
  'cpu_cores',
  'os',
  'region',
  'country_code',
  'group_id',
  'tags',
  'features'
])
```

- Extend `mergeServerUpdate` default-value guard:

```ts
const isStaticDefault =
  STATIC_FIELDS.has(key) &&
  (value === null ||
    value === 0 ||
    (Array.isArray(value) && value.length === 0))
```

- [ ] **Step 4: Run the tests (expected to pass)**

Run: `bun run --cwd apps/web test use-servers-ws`
Expected: PASS (4 tests).

- [ ] **Step 5: Run the full frontend test suite**

Run: `bun run --cwd apps/web test`
Expected: PASS (no regressions).

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/hooks/use-servers-ws.ts apps/web/src/hooks/use-servers-ws.test.ts
git commit -m "feat(web): guard static array fields in ServerMetrics merge"
```

---

## Chunk 2: Cell primitives (Phase A rewrites)

### Task 5: `<StatusDot />` component (TDD)

**Files:**
- Create: `apps/web/src/components/server/status-dot.tsx`
- Create: `apps/web/src/components/server/status-dot.test.tsx`

- [ ] **Step 1: Write failing tests**

```tsx
import { render } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { StatusDot } from './status-dot'

describe('StatusDot', () => {
  it('renders pulsing emerald dot when online', () => {
    const { container } = render(<StatusDot online />)
    const el = container.querySelector('[data-slot="status-dot"]')
    expect(el?.className).toMatch(/animate-pulse/)
    expect(el?.className).toMatch(/bg-emerald-500/)
  })

  it('renders muted dot without pulse when offline', () => {
    const { container } = render(<StatusDot online={false} />)
    const el = container.querySelector('[data-slot="status-dot"]')
    expect(el?.className).not.toMatch(/animate-pulse/)
    expect(el?.className).toMatch(/bg-muted-foreground/)
  })
})
```

- [ ] **Step 2: Run (fail: module missing)**

Run: `bun run --cwd apps/web test status-dot`
Expected: FAIL.

- [ ] **Step 3: Implement**

```tsx
import { cn } from '@/lib/utils'

interface StatusDotProps {
  className?: string
  online: boolean
}

export function StatusDot({ online, className }: StatusDotProps) {
  return (
    <span
      aria-label={online ? 'online' : 'offline'}
      className={cn(
        'inline-block size-2 rounded-full',
        online
          ? 'bg-emerald-500 shadow-[0_0_0_3px_rgba(16,185,129,0.18)] animate-pulse'
          : 'bg-muted-foreground/60',
        className
      )}
      data-slot="status-dot"
      role="img"
    />
  )
}
```

- [ ] **Step 4: Run (pass)**

Run: `bun run --cwd apps/web test status-dot`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/components/server/status-dot.tsx apps/web/src/components/server/status-dot.test.tsx
git commit -m "feat(web): add StatusDot pulsing indicator"
```

---

### Task 6: `<TagChipRow />` component (TDD)

**Files:**
- Create: `apps/web/src/components/server/tag-chip.tsx`
- Create: `apps/web/src/components/server/tag-chip.test.tsx`

- [ ] **Step 1: Write failing tests**

```tsx
import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { TagChipRow } from './tag-chip'

describe('TagChipRow', () => {
  it('renders nothing when tags is empty', () => {
    const { container } = render(<TagChipRow tags={[]} />)
    expect(container.firstChild).toBeNull()
  })

  it('renders nothing when tags is undefined', () => {
    const { container } = render(<TagChipRow tags={undefined} />)
    expect(container.firstChild).toBeNull()
  })

  it('renders a chip per tag', () => {
    render(<TagChipRow tags={['prod', 'web']} />)
    expect(screen.getByText('prod')).toBeDefined()
    expect(screen.getByText('web')).toBeDefined()
  })

  it('assigns the same palette color to the same tag across renders', () => {
    const { container, rerender } = render(<TagChipRow tags={['prod']} />)
    const first = container.querySelector('[data-slot="tag-chip"]')?.className
    rerender(<TagChipRow tags={['prod']} />)
    const second = container.querySelector('[data-slot="tag-chip"]')?.className
    expect(first).toBe(second)
  })

  it('adds title attr on the chip element for tooltip / truncate fallback', () => {
    render(<TagChipRow tags={['long-tag-value']} />)
    const chip = screen.getByText('long-tag-value')
    expect(chip.getAttribute('title')).toBe('long-tag-value')
  })
})
```

- [ ] **Step 2: Run (fail: module missing)**

Run: `bun run --cwd apps/web test tag-chip`
Expected: FAIL.

- [ ] **Step 3: Implement**

```tsx
import { cn } from '@/lib/utils'

const PALETTE = [
  'bg-emerald-500/15 text-emerald-700 dark:text-emerald-400',
  'bg-sky-500/15 text-sky-700 dark:text-sky-400',
  'bg-amber-500/15 text-amber-700 dark:text-amber-400',
  'bg-rose-500/15 text-rose-700 dark:text-rose-400',
  'bg-violet-500/15 text-violet-700 dark:text-violet-400',
  'bg-slate-500/15 text-slate-700 dark:text-slate-300'
] as const

function hashTag(tag: string): number {
  let h = 0
  for (let i = 0; i < tag.length; i++) {
    h = (h * 31 + tag.charCodeAt(i)) | 0
  }
  return Math.abs(h) % PALETTE.length
}

interface TagChipRowProps {
  className?: string
  tags: string[] | undefined
}

export function TagChipRow({ tags, className }: TagChipRowProps) {
  if (!tags || tags.length === 0) {
    return null
  }
  return (
    <div className={cn('mt-1 flex flex-wrap gap-1', className)}>
      {tags.map((tag) => (
        <span
          className={cn(
            'inline-flex max-w-[80px] items-center truncate rounded px-1.5 py-0.5 text-[10px] font-medium leading-tight',
            PALETTE[hashTag(tag)]
          )}
          data-slot="tag-chip"
          key={tag}
          title={tag}
        >
          {tag}
        </span>
      ))}
    </div>
  )
}
```

- [ ] **Step 4: Run (pass)**

Run: `bun run --cwd apps/web test tag-chip`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/components/server/tag-chip.tsx apps/web/src/components/server/tag-chip.test.tsx
git commit -m "feat(web): add TagChipRow with stable-hash palette"
```

---

### Task 7: Rewrite `index.cells.tsx` — `MetricBarRow` primitive + tests

**Files:**
- Modify: `apps/web/src/routes/_authed/servers/index.cells.tsx`
- Modify: `apps/web/src/routes/_authed/servers/index.cells.test.tsx`

Because the cells are all inter-dependent and the existing tests reference the old rendering (`card_load X.YY`, `↺ 2.0 MB/s`, `Σ ↓...`), we rewrite both the component file and the test file in one atomic task first (new `MetricBarRow` + the CpuCell, MemoryCell, DiskCell, NetworkCell rewrites all happen here), then extend the tests for the new cells in the next task.

- [ ] **Step 1: Back up intent — note the current exports**

The current `index.cells.tsx` exports: `MiniBar`, `CpuCell`, `MemoryCell`, `DiskCell`, `NetworkCell`. The rewrite must keep exporting `CpuCell`, `MemoryCell`, `DiskCell`, `NetworkCell` (consumed in `index.tsx`). `MiniBar` is no longer used elsewhere in `src/` (`rg -n "from.*index.cells" apps/web/src` will show only `index.tsx`); it will be removed.

Run: `rg -n "import.*MiniBar" apps/web/src`
Expected: empty (no other callers).

- [ ] **Step 2: Write the failing test skeleton for `MetricBarRow`**

Replace the entire content of `apps/web/src/routes/_authed/servers/index.cells.test.tsx` with:

```tsx
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { MetricBarRow } from './index.cells'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key })
}))

export function makeServer(overrides: Partial<ServerMetrics> = {}): ServerMetrics {
  return {
    id: 'srv-1',
    name: 'test-server',
    online: true,
    country_code: null,
    cpu: 0,
    cpu_cores: null,
    cpu_name: null,
    disk_read_bytes_per_sec: 0,
    disk_total: 500_000_000_000,
    disk_used: 120_000_000_000,
    disk_write_bytes_per_sec: 0,
    features: [],
    group_id: null,
    last_active: 0,
    load1: 0,
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
    tags: [],
    tcp_conn: 0,
    udp_conn: 0,
    uptime: 0,
    ...overrides
  }
}

describe('MetricBarRow', () => {
  it('renders green bar below 70%', () => {
    const { container } = render(<MetricBarRow icon={null} pct={50} />)
    const fill = container.querySelector('[data-slot="metric-bar-fill"]')
    expect(fill?.className).toMatch(/bg-emerald-500/)
  })

  it('renders amber bar at 70% and below 90%', () => {
    const { container } = render(<MetricBarRow icon={null} pct={70.5} />)
    const fill = container.querySelector('[data-slot="metric-bar-fill"]')
    expect(fill?.className).toMatch(/bg-amber-500/)
  })

  it('renders red bar at 90%+', () => {
    const { container } = render(<MetricBarRow icon={null} pct={92} />)
    const fill = container.querySelector('[data-slot="metric-bar-fill"]')
    expect(fill?.className).toMatch(/bg-red-500/)
  })

  it('rounds the percentage to 0 decimals', () => {
    render(<MetricBarRow icon={null} pct={42.67} />)
    expect(screen.getByText('43%')).toBeDefined()
  })

  it('clamps percentage to [0, 100]', () => {
    render(<MetricBarRow icon={null} pct={150} />)
    expect(screen.getByText('100%')).toBeDefined()
    render(<MetricBarRow icon={null} pct={-5} />)
    expect(screen.getByText('0%')).toBeDefined()
  })

  it('renders the supplied icon slot', () => {
    render(<MetricBarRow icon={<span data-testid="cpu-icon" />} pct={10} />)
    expect(screen.getByTestId('cpu-icon')).toBeDefined()
  })
})
```

- [ ] **Step 3: Run (fail: `MetricBarRow` not exported)**

Run: `bun run --cwd apps/web test index.cells`
Expected: FAIL with "MetricBarRow is not exported" (or similar).

- [ ] **Step 4: Introduce the new `MetricBarRow` primitive**

Replace the entire content of `apps/web/src/routes/_authed/servers/index.cells.tsx` with:

```tsx
import type { ReactNode } from 'react'
import { cn } from '@/lib/utils'

export function getBarColor(pct: number): string {
  if (pct > 90) return 'bg-red-500'
  if (pct > 70) return 'bg-amber-500'
  return 'bg-emerald-500'
}

export function getBarTextColor(pct: number): string {
  if (pct > 90) return 'text-red-600 dark:text-red-400'
  if (pct > 70) return 'text-amber-600 dark:text-amber-400'
  return 'text-foreground'
}

interface MetricBarRowProps {
  ariaLabel?: string
  icon: ReactNode
  pct: number
  valueClassName?: string
}

export function MetricBarRow({ icon, pct, ariaLabel, valueClassName }: MetricBarRowProps) {
  const clamped = Math.min(100, Math.max(0, pct))
  const colorBg = getBarColor(clamped)
  const colorText = getBarTextColor(clamped)
  return (
    <div className="flex items-center gap-1.5" role="img" aria-label={ariaLabel}>
      {icon !== null && <span className="inline-flex size-3.5 flex-none text-muted-foreground">{icon}</span>}
      <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-muted">
        <div className={cn('h-full rounded-full', colorBg)} data-slot="metric-bar-fill" style={{ width: `${clamped}%` }} />
      </div>
      <span
        className={cn('w-10 text-right font-mono text-xs font-semibold tabular-nums', colorText, valueClassName)}
      >
        {Math.round(clamped)}%
      </span>
    </div>
  )
}
```

The remainder of `index.cells.tsx` (the `CpuCell`, `MemoryCell`, `DiskCell`, `NetworkCell`, plus the new `UptimeCell` and `NameCell`) will be implemented in subsequent tasks on top of this primitive. For now, leave placeholders that preserve the existing column integration by temporarily re-exporting the old cells — **however**, to avoid breaking the route, also delete the old `MiniBar`-based implementations immediately and supply stubs that will be replaced in Tasks 8–13.

Append to `index.cells.tsx`:

```tsx
import type { ServerMetrics } from '@/hooks/use-servers-ws'

// Temporary stubs — replaced in Tasks 8–13.
export function CpuCell(_: { server: ServerMetrics }) { return <MetricBarRow icon={null} pct={0} /> }
export function MemoryCell(_: { server: ServerMetrics }) { return <MetricBarRow icon={null} pct={0} /> }
export function DiskCell(_: { server: ServerMetrics }) { return <MetricBarRow icon={null} pct={0} /> }
export function NetworkCell(_: { server: ServerMetrics }) { return <MetricBarRow icon={null} pct={0} /> }
```

- [ ] **Step 5: Run `MetricBarRow` tests (pass), other cell tests (fail)**

Run: `bun run --cwd apps/web test index.cells`
Expected: `MetricBarRow` PASS (6 tests); other cell describes from the original file are now removed so only `MetricBarRow` runs.

- [ ] **Step 6: Run app-wide lint / typecheck**

Run: `bun run --cwd apps/web typecheck && bun x ultracite check apps/web/src/routes/_authed/servers/index.cells.tsx`
Expected: no new errors (stubs are typed).

- [ ] **Step 7: Commit**

```bash
git add apps/web/src/routes/_authed/servers/index.cells.tsx apps/web/src/routes/_authed/servers/index.cells.test.tsx
git commit -m "refactor(web): introduce MetricBarRow primitive in servers cells"
```

---

### Task 8: `<CpuCell />` rewrite (TDD)

**Files:**
- Modify: `apps/web/src/routes/_authed/servers/index.cells.tsx`
- Modify: `apps/web/src/routes/_authed/servers/index.cells.test.tsx`

- [ ] **Step 1: Append failing tests**

Append inside `index.cells.test.tsx`:

```tsx
import { CpuCell } from './index.cells'

describe('CpuCell', () => {
  it('renders cores + load when cpu_cores is present', () => {
    render(<CpuCell server={makeServer({ cpu: 12, cpu_cores: 8, load1: 1.234 })} />)
    expect(screen.getByText('12%')).toBeDefined()
    expect(screen.getByText(/8 cores · load 1\.23/)).toBeDefined()
  })

  it('falls back to load-only when cpu_cores is null (Phase A)', () => {
    render(<CpuCell server={makeServer({ cpu: 12, cpu_cores: null, load1: 1.23 })} />)
    expect(screen.queryByText(/cores/)).toBeNull()
    expect(screen.getByText(/load 1\.23/)).toBeDefined()
  })

  it('hides sub-line when offline', () => {
    render(<CpuCell server={makeServer({ online: false, cpu_cores: 8, load1: 1.23 })} />)
    expect(screen.queryByText(/cores/)).toBeNull()
    expect(screen.queryByText(/load/)).toBeNull()
  })
})
```

- [ ] **Step 2: Run (fail: stub returns 0% / no load text)**

Run: `bun run --cwd apps/web test index.cells`
Expected: CpuCell tests FAIL.

- [ ] **Step 3: Replace the CpuCell stub**

Add the `Cpu` import at the top of `index.cells.tsx`:

```tsx
import { Cpu } from 'lucide-react'
```

Replace the `CpuCell` stub with:

```tsx
export function CpuCell({ server }: { server: ServerMetrics }) {
  if (!server.online) {
    return <span className="text-muted-foreground">—</span>
  }
  const cores = server.cpu_cores ?? null
  return (
    <div className="flex flex-col gap-1">
      <MetricBarRow icon={<Cpu className="size-3.5" aria-hidden="true" />} pct={server.cpu} />
      <div className="pl-5 font-mono text-[10px] tabular-nums text-muted-foreground">
        {cores != null && (
          <>
            <span className="text-foreground font-medium">{cores}</span> cores
            <span className="opacity-50 px-1">·</span>
          </>
        )}
        load <span className="text-foreground font-medium">{server.load1.toFixed(2)}</span>
      </div>
    </div>
  )
}
```

- [ ] **Step 4: Run (pass)**

Run: `bun run --cwd apps/web test index.cells`
Expected: CpuCell PASS (3 tests) + MetricBarRow still PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/routes/_authed/servers/index.cells.tsx apps/web/src/routes/_authed/servers/index.cells.test.tsx
git commit -m "feat(web): CpuCell shows cores + load with Phase A fallback"
```

---

### Task 9: `<MemoryCell />` rewrite (TDD)

- [ ] **Step 1: Append failing tests**

```tsx
import { MemoryCell } from './index.cells'

describe('MemoryCell', () => {
  it('renders used/total + swap pct', () => {
    render(
      <MemoryCell
        server={makeServer({
          mem_used: 7.2 * 1024 ** 3,
          mem_total: 16 * 1024 ** 3,
          swap_used: 0.1 * 1024 ** 3,
          swap_total: 4 * 1024 ** 3
        })}
      />
    )
    expect(screen.getByText(/7\.2 GB \/ 16\.0 GB/)).toBeDefined()
    expect(screen.getByText(/swap/)).toBeDefined()
    expect(screen.getByText(/3%/)).toBeDefined()
  })

  it('renders 0% swap when swap_total is 0', () => {
    render(
      <MemoryCell
        server={makeServer({ mem_used: 100, mem_total: 200, swap_used: 0, swap_total: 0 })}
      />
    )
    expect(screen.getByText(/swap 0%/)).toBeDefined()
  })

  it('hides sub-line when offline', () => {
    render(<MemoryCell server={makeServer({ online: false })} />)
    expect(screen.queryByText(/swap/)).toBeNull()
  })
})
```

- [ ] **Step 2: Run (fail)**
- [ ] **Step 3: Replace the `MemoryCell` stub**

Import `MemoryStick` in the lucide import line:

```tsx
import { Cpu, MemoryStick } from 'lucide-react'
```

Add the helper `formatBytes` import near the top:

```tsx
import { formatBytes } from '@/lib/utils'
```

Replace the stub:

```tsx
export function MemoryCell({ server }: { server: ServerMetrics }) {
  if (!server.online) {
    return <span className="text-muted-foreground">—</span>
  }
  const pct = server.mem_total > 0 ? (server.mem_used / server.mem_total) * 100 : 0
  const swapPct = server.swap_total > 0 ? (server.swap_used / server.swap_total) * 100 : 0
  const swapColor = getBarTextColor(swapPct)
  return (
    <div className="flex flex-col gap-1">
      <MetricBarRow icon={<MemoryStick className="size-3.5" aria-hidden="true" />} pct={pct} />
      <div className="pl-5 font-mono text-[10px] tabular-nums text-muted-foreground">
        <span className="text-foreground font-medium">{formatBytes(server.mem_used)}</span> /{' '}
        {formatBytes(server.mem_total)}
        <span className="opacity-50 px-1">·</span>
        swap <span className={cn('font-medium', swapColor)}>{Math.round(swapPct)}%</span>
      </div>
    </div>
  )
}
```

- [ ] **Step 4: Run (pass)**
- [ ] **Step 5: Commit**

```bash
git add apps/web/src/routes/_authed/servers/index.cells.tsx apps/web/src/routes/_authed/servers/index.cells.test.tsx
git commit -m "feat(web): MemoryCell shows used/total + swap pct"
```

---

### Task 10: `<DiskCell />` rewrite (TDD)

- [ ] **Step 1: Append failing tests**

```tsx
import { DiskCell } from './index.cells'

describe('DiskCell', () => {
  it('shows usage bar + r/w speeds when online', () => {
    render(
      <DiskCell
        server={makeServer({
          online: true,
          disk_used: 60_000_000_000,
          disk_total: 100_000_000_000,
          disk_read_bytes_per_sec: 2_100_000,
          disk_write_bytes_per_sec: 512_000
        })}
      />
    )
    expect(screen.getByText('60%')).toBeDefined()
    expect(screen.getByText(/2\.0 MB\/s/)).toBeDefined()
    expect(screen.getByText(/500\.0 KB\/s/)).toBeDefined()
  })

  it('hides r/w sub when offline', () => {
    render(
      <DiskCell
        server={makeServer({ online: false, disk_read_bytes_per_sec: 999, disk_write_bytes_per_sec: 999 })}
      />
    )
    expect(screen.queryByText(/KB\/s/)).toBeNull()
  })

  it('renders 0% when disk_total is 0', () => {
    render(<DiskCell server={makeServer({ disk_total: 0, disk_used: 0 })} />)
    expect(screen.getByText('0%')).toBeDefined()
  })
})
```

- [ ] **Step 2: Run (fail)**
- [ ] **Step 3: Replace the `DiskCell` stub**

Add `HardDrive`, `ArrowDown`, `ArrowUp` imports:

```tsx
import { ArrowDown, ArrowUp, Cpu, HardDrive, MemoryStick } from 'lucide-react'
```

Add `formatSpeed` to the utils import:

```tsx
import { formatBytes, formatSpeed } from '@/lib/utils'
```

Replace the stub:

```tsx
export function DiskCell({ server }: { server: ServerMetrics }) {
  if (!server.online) {
    return <span className="text-muted-foreground">—</span>
  }
  const pct = server.disk_total > 0 ? (server.disk_used / server.disk_total) * 100 : 0
  return (
    <div className="flex flex-col gap-1">
      <MetricBarRow icon={<HardDrive className="size-3.5" aria-hidden="true" />} pct={pct} />
      <div className="pl-5 flex items-center gap-2 font-mono text-[10px] tabular-nums text-muted-foreground">
        <span className="inline-flex items-center gap-1">
          <ArrowDown aria-hidden="true" className="size-2.5" />
          <span className="text-foreground font-medium">{formatSpeed(server.disk_read_bytes_per_sec)}</span>
        </span>
        <span className="inline-flex items-center gap-1">
          <ArrowUp aria-hidden="true" className="size-2.5" />
          <span className="text-foreground font-medium">{formatSpeed(server.disk_write_bytes_per_sec)}</span>
        </span>
      </div>
    </div>
  )
}
```

- [ ] **Step 4: Run (pass)**
- [ ] **Step 5: Commit**

```bash
git add apps/web/src/routes/_authed/servers/index.cells.tsx apps/web/src/routes/_authed/servers/index.cells.test.tsx
git commit -m "feat(web): DiskCell shows usage + disk I/O with lucide icons"
```

---

### Task 11: `<NetworkCell />` rewrite (TDD)

**Design note:** this is the only cell that takes external data (`TrafficOverviewItem | undefined`). We lift `useTrafficOverview` to the page level (`index.tsx`) and pass the per-row entry through a prop.

- [ ] **Step 1: Append failing tests**

```tsx
import { NetworkCell } from './index.cells'
import type { TrafficOverviewItem } from '@/hooks/use-traffic-overview'

const GB = 1024 ** 3
const TB = 1024 ** 4

function makeEntry(overrides: Partial<TrafficOverviewItem>): TrafficOverviewItem {
  return {
    billing_cycle: null,
    cycle_in: 0,
    cycle_out: 0,
    days_remaining: null,
    name: 'srv',
    percent_used: null,
    server_id: 'srv-1',
    traffic_limit: null,
    ...overrides
  }
}

describe('NetworkCell', () => {
  it('renders traffic-quota bar + used/limit + live ↓↑ when online', () => {
    render(
      <NetworkCell
        entry={makeEntry({ cycle_in: 50 * GB, cycle_out: 43.2 * GB, traffic_limit: 1 * TB })}
        server={makeServer({ online: true, net_in_speed: 1_153_434, net_out_speed: 339_968 })}
      />
    )
    expect(screen.getByText('9%')).toBeDefined()
    expect(screen.getByText(/93\.2 GB \/ 1\.0 TB/)).toBeDefined()
    expect(screen.getByText(/1\.1 MB\/s/)).toBeDefined()
    expect(screen.getByText(/332\.0 KB\/s/)).toBeDefined()
  })

  it('falls back to net_in_transfer + 1 TiB default when entry is undefined', () => {
    render(
      <NetworkCell
        entry={undefined}
        server={makeServer({ online: true, net_in_transfer: 2 * GB, net_out_transfer: 1 * GB })}
      />
    )
    // 3 GB / 1 TiB ≈ 0.29% → rounds to 0%
    expect(screen.getByText('0%')).toBeDefined()
    expect(screen.getByText(/3\.0 GB \/ 1\.0 TB/)).toBeDefined()
  })

  it('renders traffic-quota bar even when offline (server-level data)', () => {
    render(
      <NetworkCell
        entry={makeEntry({ cycle_in: 50 * GB, cycle_out: 50 * GB, traffic_limit: 1 * TB })}
        server={makeServer({ online: false })}
      />
    )
    expect(screen.getByText(/10%/)).toBeDefined()
    expect(screen.getByText(/100\.0 GB \/ 1\.0 TB/)).toBeDefined()
    expect(screen.queryByText(/MB\/s/)).toBeNull()
    expect(screen.queryByText(/KB\/s/)).toBeNull()
  })

  it('treats traffic_limit <= 0 as fallback to default', () => {
    render(
      <NetworkCell entry={makeEntry({ traffic_limit: 0 })} server={makeServer({ online: true })} />
    )
    expect(screen.getByText(/1\.0 TB/)).toBeDefined()
  })
})
```

- [ ] **Step 2: Run (fail)**
- [ ] **Step 3: Replace the `NetworkCell` stub**

Add `Network` to the lucide import:

```tsx
import { ArrowDown, ArrowUp, Cpu, HardDrive, MemoryStick, Network } from 'lucide-react'
```

Add traffic import:

```tsx
import { computeTrafficQuota } from '@/lib/traffic'
import type { TrafficOverviewItem } from '@/hooks/use-traffic-overview'
```

Replace the stub:

```tsx
interface NetworkCellProps {
  entry: TrafficOverviewItem | undefined
  server: ServerMetrics
}

export function NetworkCell({ server, entry }: NetworkCellProps) {
  const { used, limit, pct } = computeTrafficQuota({
    entry,
    netInTransfer: server.net_in_transfer,
    netOutTransfer: server.net_out_transfer
  })
  return (
    <div className="flex flex-col gap-1">
      <MetricBarRow icon={<Network className="size-3.5" aria-hidden="true" />} pct={pct} />
      <div className="pl-5 flex flex-wrap items-center gap-x-2 gap-y-0.5 font-mono text-[10px] tabular-nums text-muted-foreground">
        <span>
          <span className="text-foreground font-medium">{formatBytes(used)}</span> / {formatBytes(limit)}
        </span>
        {server.online && (
          <>
            <span className="opacity-50">·</span>
            <span className="inline-flex items-center gap-1">
              <ArrowDown aria-hidden="true" className="size-2.5" />
              <span className="text-foreground font-medium">{formatSpeed(server.net_in_speed)}</span>
            </span>
            <span className="inline-flex items-center gap-1">
              <ArrowUp aria-hidden="true" className="size-2.5" />
              <span className="text-foreground font-medium">{formatSpeed(server.net_out_speed)}</span>
            </span>
          </>
        )}
      </div>
    </div>
  )
}
```

- [ ] **Step 4: Run (pass)**
- [ ] **Step 5: Commit**

```bash
git add apps/web/src/routes/_authed/servers/index.cells.tsx apps/web/src/routes/_authed/servers/index.cells.test.tsx
git commit -m "feat(web): NetworkCell shows traffic quota bar + live speeds"
```

---

### Task 12: `<UptimeCell />` + `<NameCell />` (TDD)

**Files:**
- Modify: `apps/web/src/routes/_authed/servers/index.cells.tsx`
- Modify: `apps/web/src/routes/_authed/servers/index.cells.test.tsx`

- [ ] **Step 1: Append failing tests**

```tsx
import { NameCell, UptimeCell } from './index.cells'

describe('UptimeCell', () => {
  const NOW = 1_700_000_000
  const _originalNow = Date.now
  beforeEach(() => {
    Date.now = () => NOW * 1000
  })
  afterEach(() => {
    Date.now = _originalNow
  })

  it('shows uptime + OS line when online', () => {
    render(
      <UptimeCell
        server={makeServer({ online: true, uptime: 23 * 86400, os: 'Ubuntu 22.04', last_active: NOW })}
      />
    )
    expect(screen.getByText(/23d/)).toBeDefined()
    expect(screen.getByText(/Ubuntu 22\.04/)).toBeDefined()
  })

  it('shows offline + last-seen relative when offline', () => {
    render(
      <UptimeCell
        server={makeServer({ online: false, uptime: 0, os: 'Ubuntu 22.04', last_active: NOW - 7200 })}
      />
    )
    expect(screen.getByText(/offline/i)).toBeDefined()
    expect(screen.getByText(/last_seen_ago/)).toBeDefined()
  })
})

describe('NameCell', () => {
  it('renders single-line layout when no tags', () => {
    const { container } = render(
      <NameCell server={makeServer({ name: 'tokyo-1', tags: [] })} />
    )
    expect(screen.getByText('tokyo-1')).toBeDefined()
    expect(container.querySelector('[data-slot="tag-chip"]')).toBeNull()
  })

  it('renders chips under the name when tags present', () => {
    render(<NameCell server={makeServer({ name: 'tokyo-1', tags: ['prod', 'web'] })} />)
    expect(screen.getByText('prod')).toBeDefined()
    expect(screen.getByText('web')).toBeDefined()
  })
})
```

Note: the test file will need `beforeEach`/`afterEach` imports at the top. Update the top import:

```tsx
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
```

- [ ] **Step 2: Run (fail)**
- [ ] **Step 3: Implement `UptimeCell` and `NameCell`**

Add imports to `index.cells.tsx`:

```tsx
import { Link } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'
import { Clock } from 'lucide-react'  // extend the existing lucide import
import { TagChipRow } from '@/components/server/tag-chip'
import { countryCodeToFlag, formatUptime } from '@/lib/utils'
```

Add at the bottom of `index.cells.tsx`:

```tsx
function osEmoji(os: string | null): string {
  if (!os) return ''
  const l = os.toLowerCase()
  if (l.includes('ubuntu') || l.includes('debian') || l.includes('linux')) return '🐧'
  if (l.includes('windows')) return '🪟'
  if (l.includes('macos') || l.includes('darwin')) return '🍎'
  if (l.includes('freebsd') || l.includes('openbsd')) return '😈'
  return ''
}

function relativeTime(thenSec: number, nowMs = Date.now()): string {
  const diffSec = Math.max(0, Math.floor(nowMs / 1000) - thenSec)
  if (diffSec < 60) return `${diffSec}s ago`
  if (diffSec < 3600) return `${Math.floor(diffSec / 60)}m ago`
  if (diffSec < 86400) return `${Math.floor(diffSec / 3600)}h ago`
  return `${Math.floor(diffSec / 86400)}d ago`
}

export function UptimeCell({ server }: { server: ServerMetrics }) {
  const { t } = useTranslation(['servers'])
  const emoji = osEmoji(server.os)
  if (!server.online) {
    return (
      <div className="flex flex-col">
        <span className="text-muted-foreground text-xs">{t('offline_label')}</span>
        <span className="font-mono text-[10px] tabular-nums text-muted-foreground">
          {t('last_seen_ago', { time: relativeTime(server.last_active) })}
        </span>
      </div>
    )
  }
  return (
    <div className="flex flex-col">
      <span className="inline-flex items-center gap-1 font-mono text-xs tabular-nums text-muted-foreground">
        <Clock aria-hidden="true" className="size-3" />
        {formatUptime(server.uptime)}
      </span>
      {server.os && (
        <span className="font-mono text-[10px] tabular-nums text-muted-foreground">
          {emoji && <span className="mr-1">{emoji}</span>}
          {server.os}
        </span>
      )}
    </div>
  )
}

export function NameCell({ server }: { server: ServerMetrics }) {
  const flag = countryCodeToFlag(server.country_code)
  return (
    <div className="flex min-w-0 flex-col">
      <Link
        className="group/link flex min-w-0 items-center gap-1.5"
        params={{ id: server.id }}
        search={{ range: 'realtime' }}
        to="/servers/$id"
      >
        {flag && <span className="text-xs">{flag}</span>}
        <span className="truncate font-medium group-hover/link:underline">{server.name}</span>
      </Link>
      <TagChipRow tags={server.tags} />
    </div>
  )
}
```

- [ ] **Step 4: Run (pass)**
- [ ] **Step 5: Commit**

```bash
git add apps/web/src/routes/_authed/servers/index.cells.tsx apps/web/src/routes/_authed/servers/index.cells.test.tsx
git commit -m "feat(web): UptimeCell and NameCell with tags support"
```

---

### Task 13: Update `index.tsx` columns (status-dot first, Network+Uptime+Name use new cells)

**Files:**
- Modify: `apps/web/src/routes/_authed/servers/index.tsx`

- [ ] **Step 1: Wire the new cells and add traffic-overview query**

At the top of the file, add:

```tsx
import { CircleDot } from 'lucide-react'
import { useTrafficOverview } from '@/hooks/use-traffic-overview'
import { StatusDot } from '@/components/server/status-dot'
import { CpuCell, DiskCell, MemoryCell, NameCell, NetworkCell, UptimeCell } from './index.cells'
```

Remove the following imports (no longer used):
- `StatusBadge` (replaced by `StatusDot`)
- `CircleDot` **keep** — still used for filter icon in the new dot column meta
- Individual cell imports from `./index.cells` were already present; update to the new list

Inside `ServersListPage`, near the other queries:

```tsx
const { data: trafficOverview = [] } = useTrafficOverview()
```

Inside `useMemo` for `columns`, replace:

```tsx
      {
        id: 'select',
        ...
```

- Keep the `select` column unchanged.
- **Insert a new `status-dot` column immediately after `select` and before `name`:**

```tsx
      {
        id: 'status-dot',
        accessorFn: (row) => (row.online ? 'online' : 'offline'),
        enableSorting: false,
        header: () => null,
        cell: ({ row }) => <StatusDot online={row.original.online} />,
        filterFn: arrayIncludesFilter,
        enableColumnFilter: true,
        size: 36,
        meta: {
          className: 'w-9',
          label: t('col_status'),
          variant: 'select',
          options: statusOptions,
          icon: CircleDot
        }
      },
```

- **Delete the old `status` column** (`id: 'status'` with the `StatusBadge` cell).

- Replace the `name` column's `cell` with:

```tsx
        cell: ({ row }) => <NameCell server={row.original} />,
```

(`UpgradeBadgeCell` must still render somewhere — append it inside `NameCell`'s link row; see Task 13 addendum below.)

- Replace the `network` column's `cell` with:

```tsx
        cell: ({ row }) => {
          const entry = trafficOverview.find((e) => e.server_id === row.original.id)
          return <NetworkCell server={row.original} entry={entry} />
        },
```

- Replace the `uptime` column's `cell` with:

```tsx
        cell: ({ row }) => <UptimeCell server={row.original} />,
```

- **Addendum: move `UpgradeBadgeCell` into `NameCell`.** Because `NameCell` now owns the Name layout, modify `NameCell` (in `index.cells.tsx`) to accept and render an optional right-side slot, OR pass `UpgradeBadgeCell` via composition. Simplest: add a `rightSlot` prop to `NameCell` and, in `index.tsx`'s column cell, pass `<UpgradeBadgeCell serverId={row.original.id} />` as `rightSlot`.

Update `NameCell` signature in `index.cells.tsx`:

```tsx
export function NameCell({ server, rightSlot }: { server: ServerMetrics; rightSlot?: ReactNode }) {
  ...
  return (
    <div className="flex min-w-0 flex-col">
      <div className="flex items-center gap-1.5 min-w-0">
        <Link ...>
          {flag && <span className="text-xs">{flag}</span>}
          <span className="truncate font-medium group-hover/link:underline">{server.name}</span>
        </Link>
        {rightSlot}
      </div>
      <TagChipRow tags={server.tags} />
    </div>
  )
}
```

In `index.tsx`, the `name` column cell becomes:

```tsx
        cell: ({ row }) => <NameCell server={row.original} rightSlot={<UpgradeBadgeCell serverId={row.original.id} />} />,
```

- [ ] **Step 2: Run the frontend test suite**

Run: `bun run --cwd apps/web test`
Expected: PASS (cells + existing route tests).

- [ ] **Step 3: Run ultracite**

Run: `bun x ultracite check apps/web/src/routes/_authed/servers/`
Expected: no errors.

- [ ] **Step 4: Run typecheck**

Run: `bun run --cwd apps/web typecheck`
Expected: exit 0.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/routes/_authed/servers/index.tsx apps/web/src/routes/_authed/servers/index.cells.tsx
git commit -m "feat(web): servers table adopts new cells with status-dot column"
```

---

### Task 14: Phase A green — full lint / typecheck / test

- [ ] **Step 1: Frontend tests**

Run: `bun run --cwd apps/web test`
Expected: all green.

- [ ] **Step 2: Ultracite**

Run: `bun x ultracite check`
Expected: clean.

- [ ] **Step 3: Typecheck**

Run: `bun run typecheck`
Expected: clean.

- [ ] **Step 4: Manual smoke (if dev env available)**

Run `make web-dev-prod` (if configured) or run the local server and visit `/servers?view=table`. Visually verify pulsing dot, dual-line cells, traffic bar, tag row absent (Phase A, no tags pushed).

- [ ] **Step 5: Mark Phase A done**

```bash
git tag --annotate phase-a-complete -m "servers table visual refactor Phase A"
```

(Tag is local; push is explicit and not part of this plan.)

---

## Chunk 3: Backend — tags and cpu_cores on the wire (Phase B)

### Task 15: Add `tags` and `cpu_cores` to `ServerStatus`

**Files:**
- Modify: `crates/common/src/types.rs`

- [ ] **Step 1: Edit the struct**

In `crates/common/src/types.rs`, locate the `ServerStatus` struct (around line 141) and add the two new fields at the end (before the closing brace), with `#[serde(default)]`:

```rust
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub cpu_cores: Option<i32>,
```

- [ ] **Step 2: Update the existing deserialization test for backward-compat (if any)**

Run: `cargo test -p serverbee-common`
Expected: PASS — `#[serde(default)]` guarantees old payloads still deserialize.

- [ ] **Step 3: Commit**

```bash
git add crates/common/src/types.rs
git commit -m "feat(common): extend ServerStatus with tags and cpu_cores"
```

---

### Task 16: Fetch + populate in `build_full_sync`

**Files:**
- Modify: `crates/server/src/router/ws/browser.rs`

- [ ] **Step 1: Add imports**

Near the top of `browser.rs`, add:

```rust
use std::collections::HashMap;
use sea_orm::{EntityTrait, QueryOrder};
use crate::entity::server_tag;
```

(Confirm existing imports; only add what's missing.)

- [ ] **Step 2: Group-query tags once**

Inside `build_full_sync`, after reading the servers list and before the per-server `ServerStatus` construction loop, add:

```rust
let tags_rows = server_tag::Entity::find()
    .order_by_asc(server_tag::Column::ServerId)
    .order_by_asc(server_tag::Column::Tag)
    .all(&state.db)
    .await
    .unwrap_or_default();
let mut tags_by_server: HashMap<String, Vec<String>> = HashMap::new();
for row in tags_rows {
    tags_by_server.entry(row.server_id).or_default().push(row.tag);
}
```

- [ ] **Step 3: Populate the new fields inside the `ServerStatus { ... }` literal**

Inside the struct-literal inside `build_full_sync`, add:

```rust
                tags: tags_by_server.remove(&server.id).unwrap_or_default(),
                cpu_cores: server.cpu_cores,
```

(Place them alphabetically within the literal to match project style if possible.)

- [ ] **Step 4: Also zero them out in `update_report` in `agent_manager.rs`**

In `crates/server/src/service/agent_manager.rs::update_report`, inside the `ServerStatus { ... }` literal that constructs the incremental-update payload, add (if not already present):

```rust
                tags: Vec::new(),
                cpu_cores: None,
```

- [ ] **Step 5: Build**

Run: `cargo build --workspace`
Expected: success.

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/router/ws/browser.rs crates/server/src/service/agent_manager.rs
git commit -m "feat(server): include tags and cpu_cores in ServerStatus full_sync"
```

---

### Task 17: `service/server_tag.rs` — validation and CRUD service (TDD)

**Files:**
- Create: `crates/server/src/service/server_tag.rs`
- Modify: `crates/server/src/service/mod.rs`

- [ ] **Step 1: Register the module**

In `crates/server/src/service/mod.rs`, add:

```rust
pub mod server_tag;
```

- [ ] **Step 2: Write the unit tests first**

Create `crates/server/src/service/server_tag.rs` with the tests at the bottom:

```rust
use sea_orm::{DatabaseConnection, EntityTrait, QueryOrder, Set, TransactionTrait};
use crate::entity::server_tag;
use crate::error::AppError;

pub const MAX_TAGS: usize = 8;
pub const MAX_TAG_LEN: usize = 16;

pub fn validate_tags(raw: &[String]) -> Result<Vec<String>, AppError> {
    if raw.len() > MAX_TAGS {
        return Err(AppError::Validation(format!(
            "at most {MAX_TAGS} tags"
        )));
    }
    let mut seen = std::collections::BTreeSet::new();
    for tag in raw {
        let trimmed = tag.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.chars().count() > MAX_TAG_LEN {
            return Err(AppError::Validation(format!(
                "tag '{trimmed}' exceeds {MAX_TAG_LEN} chars"
            )));
        }
        if !trimmed.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')) {
            return Err(AppError::Validation(format!(
                "tag '{trimmed}' contains invalid characters"
            )));
        }
        seen.insert(trimmed);
    }
    Ok(seen.into_iter().collect())
}

pub async fn list_tags(db: &DatabaseConnection, server_id: &str) -> Result<Vec<String>, AppError> {
    let rows = server_tag::Entity::find()
        .filter(server_tag::Column::ServerId.eq(server_id))
        .order_by_asc(server_tag::Column::Tag)
        .all(db)
        .await?;
    Ok(rows.into_iter().map(|r| r.tag).collect())
}

pub async fn set_tags(
    db: &DatabaseConnection,
    server_id: &str,
    tags: Vec<String>,
) -> Result<Vec<String>, AppError> {
    let normalized = validate_tags(&tags)?;
    let txn = db.begin().await?;
    server_tag::Entity::delete_many()
        .filter(server_tag::Column::ServerId.eq(server_id))
        .exec(&txn)
        .await?;
    for tag in &normalized {
        server_tag::ActiveModel {
            server_id: Set(server_id.to_string()),
            tag: Set(tag.clone()),
        }
        .insert(&txn)
        .await?;
    }
    txn.commit().await?;
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_too_many() {
        let tags: Vec<String> = (0..9).map(|i| format!("t{i}")).collect();
        assert!(validate_tags(&tags).is_err());
    }

    #[test]
    fn validate_rejects_too_long() {
        let tags = vec!["a".repeat(17)];
        assert!(validate_tags(&tags).is_err());
    }

    #[test]
    fn validate_rejects_invalid_chars() {
        assert!(validate_tags(&vec!["bad space".into()]).is_err());
        assert!(validate_tags(&vec!["bad/slash".into()]).is_err());
    }

    #[test]
    fn validate_trims_and_dedupes_and_sorts() {
        let got = validate_tags(&vec!["  b ".into(), "a".into(), "b".into()]).unwrap();
        assert_eq!(got, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn validate_skips_empty_after_trim() {
        let got = validate_tags(&vec!["  ".into(), "a".into()]).unwrap();
        assert_eq!(got, vec!["a".to_string()]);
    }

    #[test]
    fn validate_allows_underscore_dash_dot() {
        assert!(validate_tags(&vec!["db_primary".into(), "db-secondary".into(), "v1.0".into()]).is_ok());
    }
}
```

- [ ] **Step 3: Add the missing `ColumnTrait` import (sea-orm filtering)**

At the top of `server_tag.rs`:

```rust
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set, TransactionTrait};
```

(Replace the earlier imports if duplicate.)

- [ ] **Step 4: Run tests**

Run: `cargo test -p serverbee-server --lib service::server_tag`
Expected: PASS (6 unit tests).

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/server_tag.rs crates/server/src/service/mod.rs
git commit -m "feat(server): add server_tag service with validation"
```

---

### Task 18: REST router for tags

**Files:**
- Create: `crates/server/src/router/api/server_tag.rs`
- Modify: `crates/server/src/router/api/mod.rs`

- [ ] **Step 1: Create the handlers**

Create `crates/server/src/router/api/server_tag.rs`:

```rust
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::Deserialize;

use crate::error::{ApiResponse, AppError, ok};
use crate::service::server_tag;
use crate::state::AppState;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SetTagsRequest {
    tags: Vec<String>,
}

/// Read router — all authenticated users.
pub fn read_router() -> Router<Arc<AppState>> {
    Router::new().route("/servers/{id}/tags", get(get_tags))
}

/// Write router — admin only (mounted under the require_admin layer in api::mod).
pub fn write_router() -> Router<Arc<AppState>> {
    Router::new().route("/servers/{id}/tags", put(put_tags))
}

#[utoipa::path(
    get,
    path = "/api/servers/{id}/tags",
    operation_id = "get_server_tags",
    tag = "server-tags",
    params(("id" = String, Path, description = "Server ID")),
    responses(
        (status = 200, description = "Tags for the server", body = Vec<String>),
        (status = 401, description = "Unauthenticated"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn get_tags(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Vec<String>>>, AppError> {
    let tags = server_tag::list_tags(&state.db, &id).await?;
    ok(tags)
}

#[utoipa::path(
    put,
    path = "/api/servers/{id}/tags",
    operation_id = "set_server_tags",
    tag = "server-tags",
    params(("id" = String, Path, description = "Server ID")),
    request_body = SetTagsRequest,
    responses(
        (status = 200, description = "Canonical tag list after update", body = Vec<String>),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden (non-admin)"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn put_tags(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<SetTagsRequest>,
) -> Result<Json<ApiResponse<Vec<String>>>, AppError> {
    let normalized = server_tag::set_tags(&state.db, &id, body.tags).await?;
    ok(normalized)
}
```

- [ ] **Step 2: Mount the router**

In `crates/server/src/router/api/mod.rs`:

- Add `pub mod server_tag;` at the top (alphabetically — after `server_group`).
- Mount `read_router` alongside other `read_router`s:

```rust
                .merge(server_tag::read_router())
```

- Mount `write_router` alongside other `write_router`s (inside the `require_admin` layer):

```rust
                        .merge(server_tag::write_router())
```

- [ ] **Step 3: Build and clippy**

Run: `cargo clippy -p serverbee-server -- -D warnings`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/router/api/server_tag.rs crates/server/src/router/api/mod.rs
git commit -m "feat(server): add /api/servers/:id/tags read/write routes"
```

---

### Task 19: Integration tests for tags + full_sync

**Files:**
- Create: `crates/server/tests/server_tags.rs`

- [ ] **Step 1: Reuse the test helpers**

The existing `crates/server/tests/integration.rs` defines `start_test_server`, `http_client`, `login_admin`, `register_agent`. Because those helpers are `async fn` in a separate test binary, copy-paste the minimal set needed into the new `server_tags.rs` (Rust integration tests don't share modules across files).

Create `crates/server/tests/server_tags.rs`:

```rust
// Copy `start_test_server`, `http_client`, `login_admin`, `register_agent` verbatim
// from tests/integration.rs. (Integration test binaries don't share modules.)

// ...helpers above...

#[tokio::test]
async fn unauthenticated_get_tags_returns_401() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    let resp = client
        .get(format!("{}/api/servers/unknown/tags", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn unauthenticated_put_tags_returns_401() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    let resp = client
        .put(format!("{}/api/servers/unknown/tags", base_url))
        .json(&serde_json::json!({"tags": ["a"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn admin_put_then_get_roundtrips() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    // Use a known server id (the agent-register flow creates one)
    let (server_id, _token) = register_agent(&admin, &base_url).await;

    let resp = admin
        .put(format!("{}/api/servers/{server_id}/tags", base_url))
        .json(&serde_json::json!({"tags": ["b", "a", "b", " c "]}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let data: Vec<String> = serde_json::from_value(body["data"].clone()).unwrap();
    assert_eq!(data, vec!["a", "b", "c"]);

    let resp = admin.get(format!("{}/api/servers/{server_id}/tags", base_url)).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let data: Vec<String> = serde_json::from_value(body["data"].clone()).unwrap();
    assert_eq!(data, vec!["a", "b", "c"]);
}

#[tokio::test]
async fn admin_put_rejects_too_many_tags() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let (server_id, _token) = register_agent(&admin, &base_url).await;
    let many: Vec<String> = (0..9).map(|i| format!("t{i}")).collect();
    let resp = admin
        .put(format!("{}/api/servers/{server_id}/tags", base_url))
        .json(&serde_json::json!({"tags": many}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn admin_put_rejects_invalid_char() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let (server_id, _token) = register_agent(&admin, &base_url).await;
    let resp = admin
        .put(format!("{}/api/servers/{server_id}/tags", base_url))
        .json(&serde_json::json!({"tags": ["has space"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}
```

Add a second test binary file for full_sync inclusion; but to minimize new files, extend the test above with a full_sync WebSocket open and assertion. Alternatively, use the existing `integration.rs` test binary — **prefer** adding a test to `integration.rs` (same binary, shared helpers) rather than duplicating helpers.

Simpler: **put the full_sync assertion directly in `integration.rs`** (not in the new `server_tags.rs` file). Add to `integration.rs`:

```rust
#[tokio::test]
async fn browser_ws_full_sync_includes_tags_and_cpu_cores() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let (server_id, _token) = register_agent(&admin, &base_url).await;

    // Seed tags
    admin
        .put(format!("{}/api/servers/{server_id}/tags", base_url))
        .json(&serde_json::json!({"tags": ["alpha", "beta"]}))
        .send()
        .await
        .unwrap();

    // Open the browser WS; use the existing session cookie from `admin`
    // (extract cookie; copy the pattern used for other browser WS tests in this file).
    // The first message should be `full_sync` and each server should include `tags`.

    // ...copy-paste the pattern from an existing browser-ws test in this file and
    // assert: json["servers"][0]["tags"] == ["alpha","beta"] and json["servers"][0]["cpu_cores"] is null or an integer.
}
```

(The reason for splitting: integration.rs already has the browser-ws helpers; the new server_tags.rs only exercises REST. If browser-ws helpers are not already present in integration.rs, inline the `tokio_tungstenite::connect_async` call here with the session cookie header.)

- [ ] **Step 2: Run**

Run: `cargo test -p serverbee-server --test server_tags`
Run: `cargo test -p serverbee-server --test integration browser_ws_full_sync_includes_tags_and_cpu_cores`
Expected: all PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/server/tests/server_tags.rs crates/server/tests/integration.rs
git commit -m "test(server): cover tags CRUD + RBAC and full_sync payload inclusion"
```

---

### Task 20: Backend green — clippy + tests

- [ ] **Step 1:** `cargo clippy --workspace -- -D warnings`
- [ ] **Step 2:** `cargo test --workspace`
- [ ] **Step 3:** Expected: all PASS.

---

## Chunk 4: Frontend — tag editor in ServerEditDialog (Phase B)

### Task 21: `use-server-tags.ts` hook (TDD)

**Files:**
- Create: `apps/web/src/hooks/use-server-tags.ts`
- Create: `apps/web/src/hooks/use-server-tags.test.ts`

- [ ] **Step 1: Write failing tests**

```ts
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import { http, HttpResponse } from 'msw'
import { setupServer } from 'msw/node'
import { afterAll, afterEach, beforeAll, describe, expect, it } from 'vitest'
import { useServerTags, useUpdateServerTags } from './use-server-tags'

const server = setupServer()
beforeAll(() => server.listen())
afterEach(() => server.resetHandlers())
afterAll(() => server.close())

function wrapper() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return {
    qc,
    wrapper: ({ children }: { children: React.ReactNode }) => (
      <QueryClientProvider client={qc}>{children}</QueryClientProvider>
    )
  }
}

describe('useServerTags', () => {
  it('fetches GET /api/servers/:id/tags', async () => {
    server.use(
      http.get('/api/servers/srv-1/tags', () => HttpResponse.json({ data: ['a', 'b'] }))
    )
    const { wrapper: Wrapper } = wrapper()
    const { result } = renderHook(() => useServerTags('srv-1'), { wrapper: Wrapper })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(result.current.data).toEqual(['a', 'b'])
  })
})

describe('useUpdateServerTags', () => {
  it('PUTs tags and optimistically patches both caches', async () => {
    server.use(
      http.put('/api/servers/srv-1/tags', async ({ request }) => {
        const body = (await request.json()) as { tags: string[] }
        return HttpResponse.json({ data: body.tags.toSorted() })
      })
    )
    const { qc, wrapper: Wrapper } = wrapper()
    qc.setQueryData(['server-tags', 'srv-1'], ['old'])
    qc.setQueryData(['servers'], [{ id: 'srv-1', tags: ['old'] }])
    const { result } = renderHook(() => useUpdateServerTags('srv-1'), { wrapper: Wrapper })
    await result.current.mutateAsync(['b', 'a'])
    expect(qc.getQueryData(['server-tags', 'srv-1'])).toEqual(['a', 'b'])
    expect((qc.getQueryData(['servers']) as Array<{ id: string; tags: string[] }>)[0].tags).toEqual(['a', 'b'])
  })
})
```

- [ ] **Step 2: Run (fail — module missing; msw may also require setup)**

Run: `bun run --cwd apps/web test use-server-tags`
Expected: FAIL.

If `msw/node` isn't installed, skip the MSW-based test and use a lightweight `vi.fn()` for `fetch` instead (the existing test setup probably uses this pattern — check `apps/web/src/test-setup.ts` and mirror it).

- [ ] **Step 3: Implement the hook**

Create `apps/web/src/hooks/use-server-tags.ts`:

```ts
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { api } from '@/lib/api-client'

export function useServerTags(serverId: string, enabled = true) {
  return useQuery<string[]>({
    queryKey: ['server-tags', serverId],
    queryFn: () => api.get<string[]>(`/api/servers/${serverId}/tags`),
    enabled: enabled && !!serverId,
    staleTime: 60_000
  })
}

export function useUpdateServerTags(serverId: string) {
  const queryClient = useQueryClient()
  return useMutation<string[], Error, string[]>({
    mutationFn: (tags) => api.put<string[]>(`/api/servers/${serverId}/tags`, { tags }),
    onSuccess: (data) => {
      queryClient.setQueryData<string[]>(['server-tags', serverId], data)
      queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) =>
        prev?.map((s) => (s.id === serverId ? { ...s, tags: data } : s))
      )
    }
  })
}
```

- [ ] **Step 4: Run (pass)**

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/hooks/use-server-tags.ts apps/web/src/hooks/use-server-tags.test.ts
git commit -m "feat(web): useServerTags + useUpdateServerTags with optimistic cache"
```

---

### Task 22: Tags editor in `ServerEditDialog`

**Files:**
- Modify: `apps/web/src/components/server/server-edit-dialog.tsx`

- [ ] **Step 1: Add tag editor state and query**

Inside `ServerEditDialog`:

```tsx
import { useServerTags, useUpdateServerTags } from '@/hooks/use-server-tags'

// ... inside the component
const [tagsInput, setTagsInput] = useState('')
const [tagsDirty, setTagsDirty] = useState(false)
const { data: initialTags } = useServerTags(server.id, open)
const tagsMutation = useUpdateServerTags(server.id)

useEffect(() => {
  if (open && initialTags) {
    setTagsInput(initialTags.join(', '))
    setTagsDirty(false)
  }
}, [open, initialTags])
```

- [ ] **Step 2: Parse and validate on the client**

Add a pure helper inside the file (not exported):

```tsx
function parseTagsInput(raw: string): { tags: string[]; error: string | null } {
  const parts = raw.split(/[\s,]+/).map((t) => t.trim()).filter(Boolean)
  const seen = new Set<string>()
  const deduped: string[] = []
  for (const tag of parts) {
    if (tag.length > 16) return { tags: [], error: 'tags_validation_too_long' }
    if (!/^[A-Za-z0-9_.\-]+$/.test(tag)) return { tags: [], error: 'tags_validation_invalid_char' }
    if (seen.has(tag)) continue
    seen.add(tag)
    deduped.push(tag)
  }
  if (deduped.length > 8) return { tags: [], error: 'tags_validation_too_many' }
  return { tags: deduped.sort(), error: null }
}
```

- [ ] **Step 3: Render the editor block inside the Basic fieldset**

Insert after the `Public Remark` field:

```tsx
<Field label={t('tags_label')}>
  <Input
    aria-label={t('tags_label')}
    name="tags"
    onChange={(e) => {
      setTagsInput(e.target.value)
      setTagsDirty(true)
    }}
    placeholder={t('tags_placeholder')}
    type="text"
    value={tagsInput}
  />
  <p className="mt-1 text-[11px] text-muted-foreground">{t('tags_hint')}</p>
</Field>
```

- [ ] **Step 4: Sequential save in `handleSubmit`**

Modify `handleSubmit`:

```tsx
const handleSubmit = async (e: FormEvent) => {
  e.preventDefault()
  const parsed = parseTagsInput(tagsInput)
  if (parsed.error) {
    toast.error(t(parsed.error as never))
    return
  }
  const payload: UpdateServerInput = { /* ...existing... */ }
  try {
    await mutation.mutateAsync(payload)
  } catch (err) {
    toast.error(err instanceof Error ? err.message : t('edit_failed'))
    return
  }
  if (tagsDirty) {
    try {
      await tagsMutation.mutateAsync(parsed.tags)
    } catch (err) {
      // Revert the input so UX reflects the rollback; the PATCH stays committed.
      if (initialTags) setTagsInput(initialTags.join(', '))
      toast.error(err instanceof Error ? err.message : t('tags_save_failed'))
      return
    }
  }
  toast.success(t('edit_success', { defaultValue: 'Server updated successfully' }))
  onClose()
}
```

- [ ] **Step 5: Update the Save button disabled state**

```tsx
<Button disabled={mutation.isPending || tagsMutation.isPending} type="submit">
  {mutation.isPending || tagsMutation.isPending ? t('common:saving') : t('common:save')}
</Button>
```

- [ ] **Step 6: Run full frontend tests**

Run: `bun run --cwd apps/web test`
Expected: all PASS.

Run: `bun x ultracite check apps/web/src/components/server/server-edit-dialog.tsx`
Run: `bun run --cwd apps/web typecheck`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add apps/web/src/components/server/server-edit-dialog.tsx
git commit -m "feat(web): ServerEditDialog tags editor with sequential save"
```

---

### Task 23: Manual QA checklist

**Files:**
- Create: `tests/servers/table-row-visual-redesign.md`

- [ ] **Step 1: Write the checklist**

```markdown
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
- [ ] Edit dialog partial failure: set a name + invalid tags → only name change persists (expected because tags didn't change); set a name + valid tags but force PUT 500 (via browser devtools network throttling) → PATCH persists, tag input reverts, `tags_save_failed` toast fires.
- [ ] Breakpoints: network column hides below `lg:`, group/uptime hide below `xl:`.
- [ ] Viewport 1920×963 screenshot matches spec mockup proportions.
- [ ] `bun run test` green; `cargo test --workspace` green; `cargo clippy --workspace -- -D warnings` clean; `bun x ultracite check` clean; `bun run typecheck` clean.
```

- [ ] **Step 2: Commit**

```bash
git add tests/servers/table-row-visual-redesign.md
git commit -m "test(servers): manual QA checklist for table row redesign"
```

---

### Task 24: Final green — all checks

- [ ] **Step 1: Rust**

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

Expected: all PASS.

- [ ] **Step 2: Frontend**

```bash
bun run --cwd apps/web test
bun x ultracite check
bun run typecheck
```

Expected: all PASS.

- [ ] **Step 3: Build**

```bash
cargo build --workspace
cd apps/web && bun run build
```

Expected: both succeed.

- [ ] **Step 4: Announce readiness**

Open a PR with body summarizing Phase A (frontend visual refactor) and Phase B (tags backend + editor). Link to the spec at `docs/superpowers/specs/2026-04-17-servers-table-row-visual-redesign-design.md` and to this plan.

---

## Notes for the Implementer

- **Test file rewrite warning (Task 7):** the existing `index.cells.test.tsx` contains test data and regex constants that no longer apply once cells are rewritten. The instruction is to **replace the entire file content** — do not try to merge old and new assertions.
- **`UpgradeBadgeCell` relocation:** previously rendered inline inside the `name` column cell. After the rewrite it lives inside `NameCell` via the `rightSlot` prop. Don't let it go missing — it is the tiny "upgrade in progress" badge and removing it breaks an existing feature.
- **`MiniBar` removal:** confirm via `rg -n "MiniBar" apps/web/src` that no callers remain after the rewrite (the old `index.cells.tsx` was the sole exporter).
- **Phase A → Phase B ordering:** Phase A's frontend-only changes depend on the `ServerMetrics` TS interface being extended with optional `tags?` / `cpu_cores?`. Do NOT add those fields as required. Phase B's backend additions use `#[serde(default)]`, guaranteeing old WebSocket payloads parse cleanly even before the frontend is deployed.
- **Phase C (live tag propagation):** explicitly out of scope; do not add a `tags_changed` WS event in this plan.
- **i18n keys added:** `tags_label`, `tags_hint`, `tags_placeholder`, `tags_validation_too_many`, `tags_validation_too_long`, `tags_validation_invalid_char`, `tags_save_failed`, `last_seen_ago`, `offline_label`. Both `en` and `zh`.

---
