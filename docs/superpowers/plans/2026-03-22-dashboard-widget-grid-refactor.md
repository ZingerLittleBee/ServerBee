# Dashboard Widget Grid Refactor Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stabilize dashboard widget drag/resize interactions while keeping `react-grid-layout`, preserving current widget features, and separating layout-edit state from page orchestration.

**Architecture:** Introduce pure layout utilities and a dedicated dashboard editor hook so business draft state has a single source of truth. Keep `react-grid-layout` as the desktop layout engine, but make `DashboardGrid` own drag-time `liveLayout` and only commit layout patches on `drag/resize stop`. Extract a testable `DashboardEditorView` to orchestrate editing flows and update React Query caches with the server response after save.

**Tech Stack:** React 19, TypeScript, TanStack Query, TanStack Router, react-grid-layout, Vitest, Testing Library

**Spec:** `docs/superpowers/specs/2026-03-22-dashboard-widget-grid-refactor-design.md`

**Worker Notes:** Use `@superpowers:test-driven-development` for each task and `@superpowers:verification-before-completion` before claiming the refactor is done.

---

## File Map

### New Files

- `apps/web/src/components/dashboard/dashboard-layout.ts` — Pure helpers for `widgets <-> layout` conversion, diffing, and patch application.
- `apps/web/src/components/dashboard/dashboard-layout.test.ts` — Unit tests for layout conversion and patch merge behavior.
- `apps/web/src/hooks/use-dashboard-editor.ts` — Canonical widget draft state and editing commands.
- `apps/web/src/hooks/use-dashboard-editor.test.tsx` — Hook tests for draft behavior and save payload generation.
- `apps/web/src/components/dashboard/dashboard-editor-view.tsx` — Testable dashboard editing orchestration component used by the route.
- `apps/web/src/components/dashboard/dashboard-editor-view.test.tsx` — Flow tests for edit, cancel, add, delete, and save behavior.

### Modified Files

- `apps/web/src/components/dashboard/dashboard-grid.tsx` — Move to `liveLayout` + `interactionState` model and commit-on-stop callbacks.
- `apps/web/src/components/dashboard/dashboard-grid.test.tsx` — Update tests for drag-time suppression and commit timing.
- `apps/web/src/routes/_authed/index.tsx` — Shrink route to data loading and prop wiring for `DashboardEditorView`.
- `apps/web/src/hooks/use-dashboard.ts` — Sync detail/default dashboard caches with mutation results.
- `apps/web/src/hooks/use-dashboard.test.tsx` — Add cache synchronization assertions for `useUpdateDashboard`.
- `TESTING.md` — Update front-end test counts and dashboard-related verification notes after the implementation lands.

---

## Chunk 1: Layout Core

### Task 1: Add pure dashboard layout helpers

**Files:**
- Create: `apps/web/src/components/dashboard/dashboard-layout.ts`
- Test: `apps/web/src/components/dashboard/dashboard-layout.test.ts`

- [ ] **Step 1: Write the failing layout tests**

Create `apps/web/src/components/dashboard/dashboard-layout.test.ts` with focused tests for the four pure helpers:

```ts
import { describe, expect, it } from 'vitest'
import type { DashboardWidget } from '@/lib/widget-types'
import {
  layoutToPatch,
  mergeLayoutPatch,
  normalizeNewWidgetPlacement,
  widgetsToLayout
} from './dashboard-layout'

const widgets: DashboardWidget[] = [
  {
    id: 'w-1',
    dashboard_id: 'dash-1',
    widget_type: 'stat-number',
    title: 'CPU',
    config_json: '{"metric":"avg_cpu"}',
    grid_x: 0,
    grid_y: 0,
    grid_w: 2,
    grid_h: 2,
    sort_order: 0,
    created_at: '2026-03-20T00:00:00Z'
  },
  {
    id: 'w-2',
    dashboard_id: 'dash-1',
    widget_type: 'gauge',
    title: 'Gauge',
    config_json: '{"metric":"cpu"}',
    grid_x: 2,
    grid_y: 0,
    grid_w: 3,
    grid_h: 3,
    sort_order: 1,
    created_at: '2026-03-20T00:00:00Z'
  }
]

describe('dashboard-layout', () => {
  it('widgetsToLayout adds min constraints from widget definitions', () => {
    const layout = widgetsToLayout(widgets)
    expect(layout[0]).toMatchObject({ i: 'w-1', x: 0, y: 0, w: 2, h: 2, minW: 2, minH: 2 })
    expect(layout[1]).toMatchObject({ i: 'w-2', x: 2, y: 0, w: 3, h: 3, minW: 2, minH: 2 })
  })

  it('layoutToPatch only returns changed widgets', () => {
    const patch = layoutToPatch(
      [
        { i: 'w-1', x: 1, y: 0, w: 2, h: 2 },
        { i: 'w-2', x: 2, y: 0, w: 3, h: 3 }
      ],
      widgets
    )

    expect(patch).toEqual([{ id: 'w-1', grid_x: 1, grid_y: 0, grid_w: 2, grid_h: 2 }])
  })

  it('mergeLayoutPatch only updates layout fields', () => {
    const updated = mergeLayoutPatch(widgets, [{ id: 'w-2', grid_x: 4, grid_y: 1, grid_w: 3, grid_h: 4 }])
    expect(updated[1]).toMatchObject({
      id: 'w-2',
      title: 'Gauge',
      config_json: '{"metric":"cpu"}',
      sort_order: 1,
      grid_x: 4,
      grid_y: 1,
      grid_w: 3,
      grid_h: 4
    })
  })

  it('normalizeNewWidgetPlacement keeps safe defaults for newly added widgets', () => {
    const newWidget = {
      ...widgets[0],
      id: 'temp-1',
      title: null,
      grid_x: 0,
      grid_y: Number.POSITIVE_INFINITY,
      grid_w: 4,
      grid_h: 3,
      sort_order: 2
    }

    const normalized = normalizeNewWidgetPlacement(widgets, newWidget)
    expect(normalized.at(-1)).toMatchObject({ id: 'temp-1', grid_x: 0, grid_w: 4, grid_h: 3 })
  })
})
```

- [ ] **Step 2: Run the new test file and confirm it fails**

Run: `cd apps/web && bunx vitest run src/components/dashboard/dashboard-layout.test.ts`

Expected: FAIL because `dashboard-layout.ts` does not exist yet.

- [ ] **Step 3: Implement the pure helpers**

Create `apps/web/src/components/dashboard/dashboard-layout.ts` with a focused API:

```ts
import type { Layout, LayoutItem } from 'react-grid-layout'
import type { DashboardWidget } from '@/lib/widget-types'
import { WIDGET_TYPES } from '@/lib/widget-types'

export interface LayoutPatch {
  id: string
  grid_h: number
  grid_w: number
  grid_x: number
  grid_y: number
}

const WIDGET_TYPE_MAP = new Map(WIDGET_TYPES.map((widget) => [widget.id, widget]))

function getMinConstraints(widgetType: string) {
  const definition = WIDGET_TYPE_MAP.get(widgetType)
  return { minW: definition?.minW ?? 2, minH: definition?.minH ?? 2 }
}

export function widgetsToLayout(widgets: DashboardWidget[]): Layout {
  return widgets.map((widget) => {
    const { minW, minH } = getMinConstraints(widget.widget_type)
    return {
      i: widget.id,
      x: widget.grid_x,
      y: widget.grid_y,
      w: widget.grid_w,
      h: widget.grid_h,
      minW,
      minH
    }
  })
}

export function layoutToPatch(layout: Pick<LayoutItem, 'i' | 'x' | 'y' | 'w' | 'h'>[], widgets: DashboardWidget[]): LayoutPatch[] {
  const widgetMap = new Map(widgets.map((widget) => [widget.id, widget]))
  return layout.flatMap((item) => {
    const widget = widgetMap.get(item.i)
    if (!widget) {
      return []
    }
    if (
      item.x === widget.grid_x &&
      item.y === widget.grid_y &&
      item.w === widget.grid_w &&
      item.h === widget.grid_h
    ) {
      return []
    }
    return [{ id: item.i, grid_x: item.x, grid_y: item.y, grid_w: item.w, grid_h: item.h }]
  })
}

export function mergeLayoutPatch(widgets: DashboardWidget[], patch: LayoutPatch[]): DashboardWidget[] {
  const patchMap = new Map(patch.map((item) => [item.id, item]))
  return widgets.map((widget) => {
    const next = patchMap.get(widget.id)
    return next
      ? { ...widget, grid_x: next.grid_x, grid_y: next.grid_y, grid_w: next.grid_w, grid_h: next.grid_h }
      : widget
  })
}

export function normalizeNewWidgetPlacement(widgets: DashboardWidget[], newWidget: DashboardWidget): DashboardWidget[] {
  return [...widgets, { ...newWidget, sort_order: widgets.length }]
}
```

- [ ] **Step 4: Re-run the layout tests**

Run: `cd apps/web && bunx vitest run src/components/dashboard/dashboard-layout.test.ts`

Expected: PASS.

- [ ] **Step 5: Commit the layout helper slice**

```bash
git add apps/web/src/components/dashboard/dashboard-layout.ts apps/web/src/components/dashboard/dashboard-layout.test.ts
git commit -m "refactor(web): add dashboard layout helper module"
```

### Task 2: Add a dedicated dashboard editor hook

**Files:**
- Create: `apps/web/src/hooks/use-dashboard-editor.ts`
- Test: `apps/web/src/hooks/use-dashboard-editor.test.tsx`

- [ ] **Step 1: Write the failing hook tests**

Create `apps/web/src/hooks/use-dashboard-editor.test.tsx` with tests for draft initialization, layout patch merge, content edits, add/delete, and save payload generation:

```tsx
import { renderHook } from '@testing-library/react'
import { act } from 'react'
import { describe, expect, it } from 'vitest'
import type { DashboardWidget } from '@/lib/widget-types'
import { useDashboardEditor } from './use-dashboard-editor'

const widgets: DashboardWidget[] = [
  {
    id: 'w-1',
    dashboard_id: 'dash-1',
    widget_type: 'stat-number',
    title: 'CPU',
    config_json: '{"metric":"avg_cpu"}',
    grid_x: 0,
    grid_y: 0,
    grid_w: 2,
    grid_h: 2,
    sort_order: 0,
    created_at: '2026-03-20T00:00:00Z'
  }
]

describe('useDashboardEditor', () => {
  it('starts editing from a cloned widget draft', () => {
    const { result } = renderHook(() => useDashboardEditor())
    act(() => result.current.startEditing(widgets))
    expect(result.current.isEditing).toBe(true)
    expect(result.current.draftWidgets).toEqual(widgets)
    expect(result.current.draftWidgets).not.toBe(widgets)
  })

  it('commitLayoutPatch only updates layout fields', () => {
    const { result } = renderHook(() => useDashboardEditor())
    act(() => result.current.startEditing(widgets))
    act(() => result.current.commitLayoutPatch([{ id: 'w-1', grid_x: 4, grid_y: 1, grid_w: 3, grid_h: 2 }]))
    expect(result.current.draftWidgets[0]).toMatchObject({
      title: 'CPU',
      config_json: '{"metric":"avg_cpu"}',
      grid_x: 4,
      grid_y: 1,
      grid_w: 3,
      grid_h: 2
    })
  })

  it('updateWidget leaves layout untouched', () => {
    const { result } = renderHook(() => useDashboardEditor())
    act(() => result.current.startEditing(widgets))
    act(() => result.current.updateWidget('w-1', { title: 'Memory', config_json: '{"metric":"avg_mem"}' }))
    expect(result.current.draftWidgets[0]).toMatchObject({
      title: 'Memory',
      config_json: '{"metric":"avg_mem"}',
      grid_x: 0,
      grid_y: 0
    })
  })

  it('buildSaveInput keeps sort_order stable and strips temp ids', () => {
    const { result } = renderHook(() => useDashboardEditor())
    act(() => result.current.startEditing([{ ...widgets[0], id: 'temp-1', sort_order: 0 }]))
    expect(result.current.buildSaveInput()[0]).toMatchObject({
      id: undefined,
      widget_type: 'stat-number',
      sort_order: 0
    })
  })
})
```

- [ ] **Step 2: Run the hook tests and confirm they fail**

Run: `cd apps/web && bunx vitest run src/hooks/use-dashboard-editor.test.tsx`

Expected: FAIL because the hook does not exist yet.

- [ ] **Step 3: Implement the editor hook**

Create `apps/web/src/hooks/use-dashboard-editor.ts`:

```ts
import { useMemo, useState } from 'react'
import type { DashboardWidget } from '@/lib/widget-types'
import type { WidgetInput } from './use-dashboard'
import { mergeLayoutPatch, type LayoutPatch } from '@/components/dashboard/dashboard-layout'

interface UpdateWidgetChanges {
  config_json?: string
  title?: string | null
}

export function useDashboardEditor() {
  const [baseWidgets, setBaseWidgets] = useState<DashboardWidget[]>([])
  const [draftWidgets, setDraftWidgets] = useState<DashboardWidget[]>([])
  const [isEditing, setIsEditing] = useState(false)

  function startEditing(widgets: DashboardWidget[]) {
    const cloned = widgets.map((widget) => ({ ...widget }))
    setBaseWidgets(cloned)
    setDraftWidgets(cloned)
    setIsEditing(true)
  }

  function cancelEditing() {
    setDraftWidgets([])
    setBaseWidgets([])
    setIsEditing(false)
  }

  function commitLayoutPatch(patch: LayoutPatch[]) {
    if (patch.length === 0) {
      return
    }
    setDraftWidgets((current) => mergeLayoutPatch(current, patch))
  }

  function addWidget(widget: DashboardWidget) {
    setDraftWidgets((current) => [...current, { ...widget, sort_order: current.length }])
  }

  function updateWidget(id: string, changes: UpdateWidgetChanges) {
    setDraftWidgets((current) =>
      current.map((widget) => (widget.id === id ? { ...widget, ...changes } : widget))
    )
  }

  function deleteWidget(id: string) {
    setDraftWidgets((current) =>
      current.filter((widget) => widget.id !== id).map((widget, index) => ({ ...widget, sort_order: index }))
    )
  }

  function buildSaveInput(): WidgetInput[] {
    return draftWidgets.map((widget) => ({
      id: widget.id.startsWith('temp-') ? undefined : widget.id,
      widget_type: widget.widget_type,
      title: widget.title,
      config_json: JSON.parse(widget.config_json),
      grid_x: widget.grid_x,
      grid_y: widget.grid_y,
      grid_w: widget.grid_w,
      grid_h: widget.grid_h,
      sort_order: widget.sort_order
    }))
  }

  const isDirty = useMemo(() => JSON.stringify(baseWidgets) !== JSON.stringify(draftWidgets), [baseWidgets, draftWidgets])

  return {
    addWidget,
    buildSaveInput,
    cancelEditing,
    commitLayoutPatch,
    deleteWidget,
    draftWidgets,
    isDirty,
    isEditing,
    startEditing,
    updateWidget
  }
}
```

- [ ] **Step 4: Re-run the hook tests**

Run: `cd apps/web && bunx vitest run src/hooks/use-dashboard-editor.test.tsx`

Expected: PASS.

- [ ] **Step 5: Commit the editor hook slice**

```bash
git add apps/web/src/hooks/use-dashboard-editor.ts apps/web/src/hooks/use-dashboard-editor.test.tsx
git commit -m "refactor(web): add dashboard editor hook"
```

---

## Chunk 2: Grid Interaction

### Task 3: Refactor `DashboardGrid` to own drag-time layout state

**Files:**
- Modify: `apps/web/src/components/dashboard/dashboard-grid.tsx`
- Modify: `apps/web/src/components/dashboard/dashboard-grid.test.tsx`
- Reference: `apps/web/src/components/dashboard/dashboard-layout.ts`

- [ ] **Step 1: Expand the Grid tests to capture the new behavior**

Update `apps/web/src/components/dashboard/dashboard-grid.test.tsx` so the `react-grid-layout` mock exposes props and allows tests to fire layout callbacks directly:

```tsx
let latestGridProps: Record<string, unknown> = {}

vi.mock('react-grid-layout', () => ({
  GridLayout: ({ children, ...props }: { children: ReactNode }) => {
    latestGridProps = props
    return <div data-testid="grid-layout">{children}</div>
  },
  useContainerWidth: () => ({ width: 1200, containerRef: { current: null }, mounted: true })
}))

it('does not commit layout changes during drag-time onLayoutChange', () => {
  const onLayoutCommit = vi.fn()
  render(
    <DashboardGrid
      isEditing
      onAddWidget={noop}
      onLayoutCommit={onLayoutCommit}
      onWidgetDelete={noop}
      onWidgetEdit={noop}
      servers={[]}
      widgets={widgets}
    />
  )

  act(() => {
    latestGridProps.onLayoutChange?.([
      { i: 'w-1', x: 1, y: 0, w: 2, h: 2 },
      { i: 'w-2', x: 2, y: 0, w: 3, h: 3 }
    ])
  })

  expect(onLayoutCommit).not.toHaveBeenCalled()
})

it('commits the final patch on drag stop', () => {
  const onLayoutCommit = vi.fn()
  render(/* same as above */)

  act(() => {
    latestGridProps.onDragStart?.()
    latestGridProps.onDragStop?.([
      { i: 'w-1', x: 1, y: 0, w: 2, h: 2 },
      { i: 'w-2', x: 2, y: 0, w: 3, h: 3 }
    ])
  })

  expect(onLayoutCommit).toHaveBeenCalledWith([{ id: 'w-1', grid_x: 1, grid_y: 0, grid_w: 2, grid_h: 2 }])
})
```

Also add a test that rerendering with new `widgets` while dragging does not overwrite the in-progress `liveLayout`.

- [ ] **Step 2: Run the Grid tests and confirm they fail**

Run: `cd apps/web && bunx vitest run src/components/dashboard/dashboard-grid.test.tsx`

Expected: FAIL because the component still uses `onLayoutChange` as an eager parent callback.

- [ ] **Step 3: Implement `liveLayout` + `interactionState` in `dashboard-grid.tsx`**

Refactor `apps/web/src/components/dashboard/dashboard-grid.tsx` along these lines:

```tsx
import { useCallback, useEffect, useMemo, useState } from 'react'
import { GridLayout, type Layout } from 'react-grid-layout'
import { layoutToPatch, widgetsToLayout } from './dashboard-layout'

type InteractionState = 'idle' | 'dragging' | 'resizing'

interface DashboardGridProps {
  isEditing: boolean
  onAddWidget?: () => void
  onLayoutCommit: (patch: { id: string; grid_x: number; grid_y: number; grid_w: number; grid_h: number }[]) => void
  onWidgetDelete: (widgetId: string) => void
  onWidgetEdit: (widgetId: string) => void
  servers: ServerMetrics[]
  widgets: DashboardWidget[]
}

export function DashboardGrid(props: DashboardGridProps) {
  const baseLayout = useMemo(() => widgetsToLayout(props.widgets), [props.widgets])
  const [liveLayout, setLiveLayout] = useState<Layout>(baseLayout)
  const [interactionState, setInteractionState] = useState<InteractionState>('idle')

  useEffect(() => {
    if (interactionState === 'idle') {
      setLiveLayout(baseLayout)
    }
  }, [baseLayout, interactionState])

  const handleLayoutChange = useCallback((nextLayout: Layout) => {
    setLiveLayout(nextLayout)
  }, [])

  const commitLayout = useCallback(
    (nextLayout: Layout) => {
      setLiveLayout(nextLayout)
      setInteractionState('idle')
      const patch = layoutToPatch(nextLayout, props.widgets)
      if (patch.length > 0) {
        props.onLayoutCommit(patch)
      }
    },
    [props.onLayoutCommit, props.widgets]
  )

  return (
    <GridLayout
      layout={liveLayout}
      onLayoutChange={handleLayoutChange}
      onDragStart={() => setInteractionState('dragging')}
      onDragStop={commitLayout}
      onResizeStart={() => setInteractionState('resizing')}
      onResizeStop={commitLayout}
      /* preserve current width, rowHeight, margin, edit gating */
    >
      {/* existing widget content */}
    </GridLayout>
  )
}
```

Important implementation rules:

- Keep mobile rendering as a plain stacked list.
- When mobile mode activates, reset `interactionState` to `'idle'`.
- Do not update parent draft state from `onLayoutChange`.
- Preserve add/edit/delete overlays and current styling.

- [ ] **Step 4: Re-run the Grid tests**

Run: `cd apps/web && bunx vitest run src/components/dashboard/dashboard-grid.test.tsx`

Expected: PASS.

- [ ] **Step 5: Commit the Grid refactor slice**

```bash
git add apps/web/src/components/dashboard/dashboard-grid.tsx apps/web/src/components/dashboard/dashboard-grid.test.tsx
git commit -m "refactor(web): stabilize dashboard grid drag state"
```

### Task 4: Keep dashboard detail caches aligned with save results

**Files:**
- Modify: `apps/web/src/hooks/use-dashboard.ts`
- Modify: `apps/web/src/hooks/use-dashboard.test.tsx`

- [ ] **Step 1: Add failing cache sync tests for `useUpdateDashboard`**

Extend `apps/web/src/hooks/use-dashboard.test.tsx` with a test that seeds detail queries, runs `useUpdateDashboard`, and expects the updated dashboard to be written back into both `['dashboards', id]` and `['dashboards', 'default']` when appropriate:

```tsx
it('updates detail and default dashboard caches after save', async () => {
  mockFetchResponse({
    data: {
      ...mockDashboardWithWidgets,
      widgets: [
        {
          ...mockDashboardWithWidgets.widgets[0],
          grid_x: 4,
          grid_y: 1
        }
      ]
    }
  })

  const wrapper = createWrapper()
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false }
    }
  })

  queryClient.setQueryData(['dashboards', 'dash-1'], mockDashboardWithWidgets)
  queryClient.setQueryData(['dashboards', 'default'], mockDashboardWithWidgets)

  const { result } = renderHook(() => useUpdateDashboard(), {
    wrapper: ({ children }) => <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  })

  act(() => {
    result.current.mutate({ id: 'dash-1', widgets: [] })
  })

  await waitFor(() => expect(result.current.isSuccess).toBe(true))

  expect(queryClient.getQueryData(['dashboards', 'dash-1'])).toMatchObject({
    widgets: [expect.objectContaining({ grid_x: 4, grid_y: 1 })]
  })
  expect(queryClient.getQueryData(['dashboards', 'default'])).toMatchObject({
    widgets: [expect.objectContaining({ grid_x: 4, grid_y: 1 })]
  })
})
```

- [ ] **Step 2: Run the hook test file and confirm the new test fails**

Run: `cd apps/web && bunx vitest run src/hooks/use-dashboard.test.tsx`

Expected: FAIL because `useUpdateDashboard` currently only invalidates `['dashboards']`.

- [ ] **Step 3: Implement cache updates in `use-dashboard.ts`**

Change the mutation handlers in `apps/web/src/hooks/use-dashboard.ts`:

```ts
export function useUpdateDashboard() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, ...input }: { id: string } & UpdateDashboardInput) =>
      api.put<DashboardWithWidgets>(`/api/dashboards/${id}`, input),
    onSuccess: (updated) => {
      queryClient.setQueryData(['dashboards', updated.id], updated)
      if (updated.is_default) {
        queryClient.setQueryData(['dashboards', 'default'], updated)
      }
      queryClient.invalidateQueries({ queryKey: ['dashboards'] })
      toast.success('Dashboard updated')
    }
  })
}
```

Also keep `useCreateDashboard` / `useDeleteDashboard` behavior intact unless a test shows a concrete regression.

- [ ] **Step 4: Re-run the hook tests**

Run: `cd apps/web && bunx vitest run src/hooks/use-dashboard.test.tsx`

Expected: PASS.

- [ ] **Step 5: Commit the cache sync slice**

```bash
git add apps/web/src/hooks/use-dashboard.ts apps/web/src/hooks/use-dashboard.test.tsx
git commit -m "fix(web): sync dashboard caches after save"
```

---

## Chunk 3: Page Wiring And Verification

### Task 5: Extract a testable `DashboardEditorView` and wire the route to it

**Files:**
- Create: `apps/web/src/components/dashboard/dashboard-editor-view.tsx`
- Create: `apps/web/src/components/dashboard/dashboard-editor-view.test.tsx`
- Modify: `apps/web/src/routes/_authed/index.tsx`
- Reference: `apps/web/src/components/dashboard/dashboard-grid.tsx`
- Reference: `apps/web/src/hooks/use-dashboard-editor.ts`

- [ ] **Step 1: Write the failing editor view tests**

Create `apps/web/src/components/dashboard/dashboard-editor-view.test.tsx`. Mock child components so the tests only cover orchestration:

```tsx
vi.mock('./dashboard-grid', () => ({
  DashboardGrid: ({
    widgets,
    onLayoutCommit,
    onWidgetDelete,
    onWidgetEdit
  }: {
    widgets: DashboardWidget[]
    onLayoutCommit: (patch: unknown[]) => void
    onWidgetDelete: (id: string) => void
    onWidgetEdit: (id: string) => void
  }) => (
    <div>
      <div data-testid="grid-count">{widgets.length}</div>
      <button onClick={() => onLayoutCommit([{ id: 'w-1', grid_x: 5, grid_y: 1, grid_w: 2, grid_h: 2 }])}>
        commit-layout
      </button>
      <button onClick={() => onWidgetDelete('w-1')}>delete-widget</button>
      <button onClick={() => onWidgetEdit('w-1')}>edit-widget</button>
    </div>
  )
}))

it('keeps drag commits in draft state until save', async () => {
  const onSave = vi.fn().mockResolvedValue(undefined)

  render(
    <DashboardEditorView
      dashboard={dashboard}
      dashboards={[dashboard]}
      isAdmin
      isSaving={false}
      onSave={onSave}
      onSelectDashboard={vi.fn()}
      servers={[]}
    />
  )

  await user.click(screen.getByRole('button', { name: 'Edit' }))
  await user.click(screen.getByRole('button', { name: 'commit-layout' }))
  await user.click(screen.getByRole('button', { name: 'Save' }))

  expect(onSave).toHaveBeenCalledWith(
    expect.arrayContaining([expect.objectContaining({ id: 'w-1', grid_x: 5, grid_y: 1 })])
  )
})

it('cancel discards draft changes and returns to server widgets', async () => {
  // start edit -> delete widget -> cancel -> count returns to original
})
```

Add one more test for adding a new widget through the config dialog path. It is acceptable to mock `WidgetPicker` and `WidgetConfigDialog` with simple buttons that call `onSelect` / `onSubmit`.

- [ ] **Step 2: Run the editor view tests and confirm they fail**

Run: `cd apps/web && bunx vitest run src/components/dashboard/dashboard-editor-view.test.tsx`

Expected: FAIL because `DashboardEditorView` does not exist yet.

- [ ] **Step 3: Implement `DashboardEditorView`**

Create `apps/web/src/components/dashboard/dashboard-editor-view.tsx` and move the editing orchestration out of the route:

```tsx
import { PencilIcon, SaveIcon, XIcon } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import type { Dashboard, DashboardWithWidgets } from '@/lib/widget-types'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { WIDGET_TYPES } from '@/lib/widget-types'
import { useDashboardEditor } from '@/hooks/use-dashboard-editor'
import { DashboardGrid } from './dashboard-grid'
import { DashboardSwitcher } from './dashboard-switcher'
import { WidgetConfigDialog } from './widget-config-dialog'
import { WidgetPicker } from './widget-picker'

interface DashboardEditorViewProps {
  dashboard?: DashboardWithWidgets
  dashboards: Dashboard[]
  isAdmin: boolean
  isSaving: boolean
  onSave: (widgets: ReturnType<typeof useDashboardEditor>['buildSaveInput']) => Promise<void> | void
  onSelectDashboard: (id: string) => void
  servers: ServerMetrics[]
}

export function DashboardEditorView(props: DashboardEditorViewProps) {
  const editor = useDashboardEditor()
  const widgets = props.dashboard?.widgets ?? []
  const displayWidgets = editor.isEditing ? editor.draftWidgets : widgets

  function handleEdit() {
    editor.startEditing(widgets)
  }

  async function handleSave() {
    if (!props.dashboard) {
      return
    }
    await props.onSave(editor.buildSaveInput())
    editor.cancelEditing()
  }

  function handleCancel() {
    editor.cancelEditing()
  }

  // preserve current picker/config dialog behavior, but route all widget mutations through editor
  // preserve empty state UI for non-edit mode
}
```

Key rules while implementing:

- Keep existing button labels and localization keys.
- When `dashboard?.id` changes, ensure the editing state is reset before rendering the next dashboard.
- All `DashboardGrid` layout commits must go through `editor.commitLayoutPatch`.
- `handleConfigSubmit` must update existing widgets without touching layout fields.
- New widgets should still get `temp-${crypto.randomUUID()}` ids and safe placement defaults.

- [ ] **Step 4: Wire `routes/_authed/index.tsx` to the new view**

Refactor `apps/web/src/routes/_authed/index.tsx` so it only:

- loads `servers`, `dashboards`, `defaultDashboard`, `activeDashboard`
- tracks `selectedId`
- creates `updateDashboard`
- passes `dashboard`, `dashboards`, `servers`, and `onSave` into `DashboardEditorView`

Use a save callback shaped like this:

```tsx
async function handleSave(widgets: WidgetInput[]) {
  if (!dashboard) {
    return
  }

  await updateDashboard.mutateAsync({ id: dashboard.id, widgets })
}
```

Render the editor view with a stable key so dashboard switches get a hard reset:

```tsx
<DashboardEditorView
  key={dashboard?.id ?? 'no-dashboard'}
  dashboard={dashboard}
  dashboards={dashboards}
  isAdmin={isAdmin}
  isSaving={updateDashboard.isPending}
  onSave={handleSave}
  onSelectDashboard={handleDashboardSelect}
  servers={servers}
/>
```

- [ ] **Step 5: Re-run the editor view tests**

Run: `cd apps/web && bunx vitest run src/components/dashboard/dashboard-editor-view.test.tsx`

Expected: PASS.

- [ ] **Step 6: Commit the page orchestration slice**

```bash
git add apps/web/src/components/dashboard/dashboard-editor-view.tsx apps/web/src/components/dashboard/dashboard-editor-view.test.tsx apps/web/src/routes/_authed/index.tsx
git commit -m "refactor(web): extract dashboard editor view"
```

### Task 6: Verify the whole front-end slice and update testing docs

**Files:**
- Modify: `TESTING.md`
- Verify: `apps/web/src/components/dashboard/dashboard-layout.test.ts`
- Verify: `apps/web/src/hooks/use-dashboard-editor.test.tsx`
- Verify: `apps/web/src/components/dashboard/dashboard-grid.test.tsx`
- Verify: `apps/web/src/components/dashboard/dashboard-editor-view.test.tsx`
- Verify: `apps/web/src/hooks/use-dashboard.test.tsx`

- [ ] **Step 1: Run targeted tests for the refactor**

Run:

```bash
cd apps/web && bunx vitest run \
  src/components/dashboard/dashboard-layout.test.ts \
  src/hooks/use-dashboard-editor.test.tsx \
  src/components/dashboard/dashboard-grid.test.tsx \
  src/components/dashboard/dashboard-editor-view.test.tsx \
  src/hooks/use-dashboard.test.tsx
```

Expected: PASS for all targeted dashboard refactor tests.

- [ ] **Step 2: Run project-level front-end verification**

Run:

```bash
cd apps/web && bun run test
cd apps/web && bun run typecheck
cd apps/web && bun x ultracite check
```

Expected:

- `bun run test`: PASS
- `bun run typecheck`: PASS
- `bun x ultracite check`: PASS

- [ ] **Step 3: Update `TESTING.md` with exact new test counts**

Update the following sections in `TESTING.md` using the actual numbers from the final passing test run:

- front-end total test count near the top summary
- `dashboard-grid.test.tsx` coverage description
- new `dashboard-layout.test.ts` coverage row
- new `use-dashboard-editor.test.tsx` coverage row
- new `dashboard-editor-view.test.tsx` coverage row

Do not guess counts. Use the observed Vitest output.

- [ ] **Step 4: Re-run the full front-end test command after the doc update**

Run: `cd apps/web && bun run test`

Expected: PASS again after any doc-only adjustments.

- [ ] **Step 5: Commit verification and docs**

```bash
git add TESTING.md apps/web/src/components/dashboard/dashboard-layout.test.ts apps/web/src/hooks/use-dashboard-editor.test.tsx apps/web/src/components/dashboard/dashboard-grid.test.tsx apps/web/src/components/dashboard/dashboard-editor-view.test.tsx apps/web/src/hooks/use-dashboard.test.tsx
git commit -m "test(web): cover dashboard grid refactor flows"
```

---

## Review Notes

- Review Chunk 1 after Task 2: validate the helper API is still minimal and not trying to solve breakpoint layouts yet.
- Review Chunk 2 after Task 4: validate `DashboardGrid` no longer emits parent updates during drag-time and that mutation cache updates only touch relevant keys.
- Review Chunk 3 after Task 6: validate route simplification, final verification results, and `TESTING.md` count updates.

## Risks To Watch

- `react-grid-layout` callback signatures can differ between versions; verify the `onDragStop` / `onResizeStop` argument shape before coding against assumptions.
- Avoid introducing deep equality checks over very large widget arrays in render paths; keep dirty checking inside the editor hook and out of hot drag paths.
- Do not let save success rely only on `invalidateQueries({ queryKey: ['dashboards'] })`; the detail cache must adopt the mutation response immediately.
- Keep the mobile branch simple; do not attempt to preserve desktop `liveLayout` through breakpoint switches.

## Done Criteria

- Desktop drag/resize no longer jitters or jumps while editing.
- Layout changes only commit on drag/resize stop.
- Add/edit/delete operations do not destabilize unrelated widgets.
- Cancel restores server-backed widgets.
- Save adopts the server response and leaves no stale detail cache behind.
- `TESTING.md` reflects the new dashboard-related tests and counts.
