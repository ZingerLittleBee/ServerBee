import {
  LockIcon,
  MoveDiagonal2Icon,
  MoveHorizontalIcon,
  MoveVerticalIcon,
  PencilIcon,
  TrashIcon,
  UnlockIcon
} from 'lucide-react'
import { type ReactNode, type Ref, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  GridLayout,
  getCompactor,
  type Layout,
  type LayoutItem,
  type ResizeHandleAxis,
  useContainerWidth
} from 'react-grid-layout'
import 'react-grid-layout/css/styles.css'
import { Button } from '@/components/ui/button'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { cn } from '@/lib/utils'
import type { DashboardWidget } from '@/lib/widget-types'
import { layoutToPatch, widgetsToLayout } from './dashboard-layout'
import { VisibilityGate } from './visibility-gate'
import { WidgetRenderer } from './widget-renderer'

interface DashboardGridProps {
  isEditing: boolean
  onLayoutChange: (updates: { id: string; grid_x: number; grid_y: number; grid_w: number; grid_h: number }[]) => void
  onWidgetDelete: (widgetId: string) => void
  onWidgetEdit: (widgetId: string) => void
  onWidgetToggleStatic?: (widgetId: string) => void
  servers: ServerMetrics[]
  widgets: DashboardWidget[]
}

function isWidgetStatic(configJson: string): boolean {
  try {
    return JSON.parse(configJson)?.is_static === true
  } catch {
    return false
  }
}

const COLS = 12
// Vertical fine-grain factor: persisted (coarse) grid rows are split into SCALE
// finer rows so content-sized widgets quantize to ~ROW_HEIGHT px instead of a
// whole coarse row. Invariant for pixel-identical legacy widgets: the legacy
// per-row step (80 + 16) must equal SCALE * (ROW_HEIGHT + MARGIN_Y) → 4*(8+16)=96.
const SCALE = 4
const ROW_HEIGHT = 8
const MARGIN: [number, number] = [16, 16]
const MARGIN_Y = MARGIN[1]
// Legacy coarse row pixel height, used only for the mobile single-column min-height.
const MOBILE_ROW_PX = 80
const MOBILE_BREAKPOINT = 768

// Widgets whose grid cell height should follow their measured content height
// instead of a fixed/estimated number of rows.
const AUTO_HEIGHT_TYPES = new Set(['top-n'])

// Widgets that must stay square (1:1 in coarse grid units). Width and height
// are locked together during resize so the radial visual stays balanced.
const SQUARE_TYPES = new Set(['gauge'])

function pxToGridUnits(px: number): number {
  return Math.max(2, Math.ceil((px + MARGIN_Y) / (ROW_HEIGHT + MARGIN_Y)))
}

function rectsOverlap(a: Layout[number], b: Layout[number]): boolean {
  return a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y
}

// The grid uses an identity (no-op) compactor so widgets can be freely placed
// and aligned across columns. That means a stale persisted layout with
// overlapping widgets would render overlapped. This sweeps top-to-bottom and
// pushes only the genuinely-overlapping widgets straight down, leaving every
// non-colliding widget exactly where it was.
function deoverlapLayout(layout: Layout): Layout {
  const placed: LayoutItem[] = []
  const sorted = [...layout].sort((a, b) => a.y - b.y || a.x - b.x)
  for (const original of sorted) {
    const item = { ...original }
    let collided = true
    while (collided) {
      collided = false
      for (const p of placed) {
        if (rectsOverlap(item, p)) {
          item.y = p.y + p.h
          collided = true
        }
      }
    }
    placed.push(item)
  }
  const byId = new Map(placed.map((it) => [it.i, it]))
  return layout.map((it) => byId.get(it.i) ?? it)
}

type InteractionState = 'dragging' | 'idle' | 'resizing'

const resizeHandleIconMap: Record<ResizeHandleAxis, typeof MoveDiagonal2Icon> = {
  n: MoveVerticalIcon,
  ne: MoveDiagonal2Icon,
  e: MoveHorizontalIcon,
  se: MoveDiagonal2Icon,
  s: MoveVerticalIcon,
  sw: MoveDiagonal2Icon,
  w: MoveHorizontalIcon,
  nw: MoveDiagonal2Icon
}

const resizeHandleIconRotationClassMap: Record<ResizeHandleAxis, string | undefined> = {
  n: 'rotate-[135deg]',
  ne: 'rotate-90',
  e: 'rotate-45',
  se: undefined,
  s: '-rotate-45',
  sw: '-rotate-90',
  w: '-rotate-[135deg]',
  nw: 'rotate-180'
}

function renderResizeHandle(axis: ResizeHandleAxis, ref: Ref<HTMLElement>) {
  const Icon = resizeHandleIconMap[axis]

  return (
    <div
      aria-hidden="true"
      className={cn(
        `react-resizable-handle react-resizable-handle-${axis}`,
        'dashboard-resize-handle flex touch-none select-none items-center justify-center',
        axis === 'n' && 'pt-1',
        axis === 'ne' && 'justify-end pt-1 pr-1',
        axis === 'e' && 'justify-end pr-1',
        axis === 'se' && 'items-end justify-end pr-1 pb-1',
        axis === 's' && 'items-end pb-1',
        axis === 'sw' && 'items-end justify-start pb-1 pl-1',
        axis === 'w' && 'justify-start pl-1',
        axis === 'nw' && 'justify-start pt-1 pl-1'
      )}
      ref={ref as Ref<HTMLDivElement>}
    >
      <span className="flex size-4 items-center justify-center rounded-md border border-border bg-background/95 shadow-sm ring-1 ring-black/5 backdrop-blur-sm dark:ring-white/10">
        <Icon
          className={cn('size-2.5 text-muted-foreground', resizeHandleIconRotationClassMap[axis])}
          strokeWidth={2.25}
        />
      </span>
    </div>
  )
}

function useIsMobile(): boolean {
  const [isMobile, setIsMobile] = useState(() =>
    typeof window !== 'undefined' ? window.innerWidth < MOBILE_BREAKPOINT : false
  )

  useEffect(() => {
    if (typeof window.matchMedia !== 'function') {
      return
    }
    const mql = window.matchMedia(`(max-width: ${MOBILE_BREAKPOINT - 1}px)`)
    const handler = (e: MediaQueryListEvent) => setIsMobile(e.matches)
    mql.addEventListener('change', handler)
    return () => mql.removeEventListener('change', handler)
  }, [])

  return isMobile
}

export function DashboardGrid({
  widgets,
  isEditing,
  onLayoutChange,
  onWidgetEdit,
  onWidgetDelete,
  onWidgetToggleStatic,
  servers
}: DashboardGridProps) {
  const isMobile = useIsMobile()
  const { width, containerRef, mounted } = useContainerWidth()

  const [autoUnits, setAutoUnits] = useState<Record<string, number>>({})

  const handleMeasure = useCallback((id: string, px: number) => {
    const units = pxToGridUnits(px)
    setAutoUnits((prev) => (prev[id] === units ? prev : { ...prev, [id]: units }))
  }, [])

  const squareIdSet = useMemo(
    () => new Set(widgets.filter((w) => SQUARE_TYPES.has(w.widget_type)).map((w) => w.id)),
    [widgets]
  )

  // Square widgets persist as `w_coarse === h_coarse`, but coarse rows are
  // taller than columns (96px vs ~85px), so the rendered cell is rectangular.
  // Override the render height in fine units so the cell is pixel-square. The
  // persisted coarse height is left untouched — only the rendered height
  // changes, and a drag commit re-derives both from `w`.
  const visualSquareHFine = useCallback(
    (wCoarse: number): number => {
      if (width <= 0) {
        return wCoarse * SCALE
      }
      const colStepPx = (width + MARGIN[0]) / COLS
      const fineRowStepPx = ROW_HEIGHT + MARGIN_Y
      return Math.max(1, Math.round((wCoarse * colStepPx) / fineRowStepPx))
    },
    [width]
  )

  // Persisted grid units are coarse (1 row == ROW_HEIGHT*SCALE px). The grid
  // itself runs at a SCALE-times finer row so content-sized widgets can hug
  // their content within ~ROW_HEIGHT px instead of a full coarse row. Scale the
  // vertical axis up here and divide back down on commit.
  const baseLayout = useMemo(() => {
    const layout = widgetsToLayout(widgets)
    for (const item of layout) {
      item.y *= SCALE
      item.h *= SCALE
      if (item.minH !== undefined) {
        item.minH *= SCALE
      }
      if (item.maxH !== undefined) {
        item.maxH *= SCALE
      }
      const units = autoUnits[item.i]
      if (units !== undefined) {
        // Auto-height widgets fit their content exactly: height is locked to the
        // measured content and cannot be adjusted. Only horizontal resize stays.
        item.minH = units
        item.maxH = units
        item.h = units
        item.resizeHandles = ['e']
      }
      if (squareIdSet.has(item.i)) {
        // Square widgets resize via the SE corner so the user can grow by
        // dragging either edge of the cell. Idle h is the pixel-square value
        // (visualSquareHFine); minH/maxH are also derived so RGL doesn't clamp
        // h back to the un-overridden coarse-fine value.
        item.resizeHandles = ['se']
        item.h = visualSquareHFine(item.w)
        const minW = item.minW ?? 2
        const maxW = item.maxW
        item.minH = visualSquareHFine(minW)
        item.maxH = maxW !== undefined ? visualSquareHFine(maxW) : undefined
      }
    }
    return deoverlapLayout(layout)
  }, [widgets, autoUnits, squareIdSet, visualSquareHFine])

  const [liveLayout, setLiveLayout] = useState<Layout>(baseLayout)
  const [interactionState, setInteractionState] = useState<InteractionState>('idle')
  // Mirror interactionState into a ref so updateLiveLayout's callback (called
  // on every RGL echo) can branch on "currently resizing" without forcing a
  // re-bind every state transition.
  const interactionStateRef = useRef(interactionState)
  interactionStateRef.current = interactionState

  // While editing (or actively dragging/resizing), freeze the servers snapshot fed
  // to widgets. Otherwise every websocket tick swaps the servers array reference
  // and re-renders all Recharts widgets, which janks drag and makes resize handles
  // flicker over the moving chart.
  const isInteracting = interactionState !== 'idle'
  const shouldFreeze = isInteracting || isEditing
  const frozenServersRef = useRef(servers)
  if (!shouldFreeze) {
    frozenServersRef.current = servers
  }
  const widgetServers = shouldFreeze ? frozenServersRef.current : servers

  const autoIdSet = useMemo(
    () => new Set(widgets.filter((w) => AUTO_HEIGHT_TYPES.has(w.widget_type)).map((w) => w.id)),
    [widgets]
  )

  // No auto-compaction (widgets stay exactly where dropped, so items in
  // different columns can be aligned freely) and preventCollision blocks
  // dropping onto another widget (it snaps back), so widgets never overlap.
  const compactor = useMemo(() => getCompactor(null, false, true), [])

  // Manual position/size persists in coarse units, so snap the live layout to
  // whole coarse rows (SCALE-aligned) while dragging/resizing. Otherwise the
  // dropped fine position gets rounded on commit and the widget visibly snaps
  // back, then ping-pongs with RGL's onLayoutChange echo (the "jitter").
  // Auto-height widgets keep their measured fine height untouched.
  const updateLiveLayout = useCallback(
    (nextLayout: Layout) => {
      const snapped = nextLayout.map((item) => {
        const base = {
          ...item,
          y: Math.round(item.y / SCALE) * SCALE,
          h: autoIdSet.has(item.i) ? item.h : Math.max(SCALE, Math.round(item.h / SCALE) * SCALE)
        }
        if (squareIdSet.has(item.i)) {
          // While the user is actively resizing, h follows w in coarse units so
          // react-resizable's cursor delta tracking stays consistent. On the
          // idle echo (RGL re-emitting the rendered layout) keep the rendered
          // pixel-square value so we don't undo the baseLayout override.
          base.h = interactionStateRef.current === 'resizing' ? base.w * SCALE : visualSquareHFine(base.w)
        }
        return base
      })
      // De-overlap every frame: RGL's identity compactor never resolves
      // overlaps, and its idle onLayoutChange echo would otherwise re-apply the
      // raw (overlapping) persisted positions over the de-overlapped layout.
      setLiveLayout(deoverlapLayout(snapped))
    },
    [autoIdSet, squareIdSet, visualSquareHFine]
  )

  // Resync the rendered layout to the widgets-derived one the moment `widgets`
  // change while idle (e.g. after a save swaps temp ids for server ids). Doing
  // this in render (guarded) instead of an effect avoids the stale frame that
  // snapped widgets back to their initial positions and flickered.
  const prevBaseRef = useRef(baseLayout)
  if (!isInteracting && prevBaseRef.current !== baseLayout) {
    prevBaseRef.current = baseLayout
    setLiveLayout(baseLayout)
  }

  useEffect(() => {
    if (isMobile) {
      setInteractionState('idle')
    }
  }, [isMobile])

  const handleLayoutChange = useCallback(
    (newLayout: Layout) => {
      updateLiveLayout(newLayout)
    },
    [updateLiveLayout]
  )

  const commitLayoutChange = useCallback(
    (finalLayout: Layout) => {
      setInteractionState('idle')
      // Convert the fine grid back to coarse persisted units. Auto-height widgets
      // persist their height too so a user-grown size sticks (the measured
      // content height still acts as the floor on the next render).
      // Snap to coarse rows then resolve any residual penetration. preventCollision
      // can block the move so no patch is emitted; without this the live layout
      // would keep the penetrating drag position until the next widgets change.
      const snapped = finalLayout.map((item) => {
        const base = {
          ...item,
          y: Math.round(item.y / SCALE) * SCALE,
          h: autoIdSet.has(item.i) ? item.h : Math.max(SCALE, Math.round(item.h / SCALE) * SCALE)
        }
        if (squareIdSet.has(item.i)) {
          base.h = visualSquareHFine(base.w)
        }
        return base
      })
      const resolved = deoverlapLayout(snapped)
      setLiveLayout(resolved)
      const coarseLayout = resolved.map((item) => ({
        ...item,
        y: Math.round(item.y / SCALE),
        // Squares persist as h_coarse = w_coarse regardless of the visual
        // fine-height override (which stays a pure render-time concern).
        h: squareIdSet.has(item.i) ? item.w : Math.round(item.h / SCALE)
      }))
      const patch = layoutToPatch(coarseLayout, widgets)
      if (patch.length > 0) {
        onLayoutChange(patch)
      }
    },
    [autoIdSet, squareIdSet, onLayoutChange, widgets, visualSquareHFine]
  )

  const sortedWidgets = useMemo(() => {
    return [...widgets].sort((a, b) => a.sort_order - b.sort_order)
  }, [widgets])

  if (isMobile) {
    return (
      <div className="space-y-4">
        {sortedWidgets.map((widget) => {
          const isAuto = AUTO_HEIGHT_TYPES.has(widget.widget_type)
          return (
            <div className="relative" key={widget.id}>
              {isEditing && (
                <EditOverlay
                  forceVisible
                  isStatic={isWidgetStatic(widget.config_json)}
                  onDelete={() => onWidgetDelete(widget.id)}
                  onEdit={() => onWidgetEdit(widget.id)}
                  onToggleStatic={onWidgetToggleStatic ? () => onWidgetToggleStatic(widget.id) : undefined}
                />
              )}
              <div
                className={isEditing ? 'pointer-events-none' : undefined}
                style={{ minHeight: widget.grid_h * MOBILE_ROW_PX }}
              >
                <VisibilityGate disabled={isEditing || isAuto}>
                  <WidgetRenderer servers={widgetServers} widget={widget} />
                </VisibilityGate>
              </div>
            </div>
          )
        })}
      </div>
    )
  }

  return (
    <div ref={containerRef}>
      {mounted && (
        <GridLayout
          autoSize
          className={cn('dashboard-grid', isEditing && 'dashboard-grid--editing')}
          compactor={compactor}
          dragConfig={{ enabled: isEditing, bounded: false, threshold: 3 }}
          gridConfig={{ cols: COLS, rowHeight: ROW_HEIGHT, margin: MARGIN }}
          layout={liveLayout}
          onDrag={updateLiveLayout}
          onDragStart={() => setInteractionState('dragging')}
          onDragStop={commitLayoutChange}
          onLayoutChange={handleLayoutChange}
          onResize={updateLiveLayout}
          onResizeStart={() => setInteractionState('resizing')}
          onResizeStop={commitLayoutChange}
          resizeConfig={{ enabled: isEditing, handleComponent: renderResizeHandle, handles: ['s', 'e', 'se'] }}
          width={width}
        >
          {widgets.map((widget) => {
            const isAuto = AUTO_HEIGHT_TYPES.has(widget.widget_type)
            return (
              <div className="relative h-full" key={widget.id}>
                {isEditing && (
                  <EditOverlay
                    isStatic={isWidgetStatic(widget.config_json)}
                    onDelete={() => onWidgetDelete(widget.id)}
                    onEdit={() => onWidgetEdit(widget.id)}
                    onToggleStatic={onWidgetToggleStatic ? () => onWidgetToggleStatic(widget.id) : undefined}
                  />
                )}
                {isAuto ? (
                  <div className={cn('flex h-full flex-col justify-center', isEditing && 'pointer-events-none')}>
                    <AutoHeightItem onMeasure={handleMeasure} widgetId={widget.id}>
                      <WidgetRenderer servers={widgetServers} widget={widget} />
                    </AutoHeightItem>
                  </div>
                ) : (
                  <div className={isEditing ? 'pointer-events-none h-full' : 'h-full'}>
                    <VisibilityGate disabled={isEditing}>
                      <WidgetRenderer servers={widgetServers} widget={widget} />
                    </VisibilityGate>
                  </div>
                )}
              </div>
            )
          })}
        </GridLayout>
      )}
    </div>
  )
}

function EditOverlay({
  forceVisible,
  isStatic,
  onEdit,
  onDelete,
  onToggleStatic
}: {
  forceVisible?: boolean
  isStatic?: boolean
  onDelete: () => void
  onEdit: () => void
  onToggleStatic?: () => void
}) {
  return (
    <div
      className={cn(
        'absolute top-1 right-1 z-10 flex gap-1 transition-opacity [div:hover>&]:opacity-100',
        forceVisible ? 'opacity-100' : 'opacity-0'
      )}
    >
      {onToggleStatic && (
        <Button
          className="size-7"
          onClick={(e) => {
            e.stopPropagation()
            onToggleStatic()
          }}
          size="icon-sm"
          variant="outline"
        >
          {isStatic ? <LockIcon className="size-3.5" /> : <UnlockIcon className="size-3.5" />}
        </Button>
      )}
      <Button
        className="size-7"
        onClick={(e) => {
          e.stopPropagation()
          onEdit()
        }}
        size="icon-sm"
        variant="outline"
      >
        <PencilIcon className="size-3.5" />
      </Button>
      <Button
        className="size-7"
        onClick={(e) => {
          e.stopPropagation()
          onDelete()
        }}
        size="icon-sm"
        variant="destructive"
      >
        <TrashIcon className="size-3.5" />
      </Button>
    </div>
  )
}

// Measures its content height (the widget card hugs its content, so this is the
// real height) and reports it so the grid cell can be sized to fit exactly.
function AutoHeightItem({
  widgetId,
  onMeasure,
  children
}: {
  widgetId: string
  onMeasure: (id: string, px: number) => void
  children: ReactNode
}) {
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const root = ref.current
    if (!root) {
      return
    }
    // Observe the inner [data-measure] element: its height is the card's
    // natural content height (incl. padding), independent of the h-full card
    // that stretches to fill the cell. Measuring the card itself would be
    // circular once it fills the resized cell.
    const target = root.querySelector<HTMLElement>('[data-measure]') ?? root
    const observer = new ResizeObserver(() => {
      const px = target.offsetHeight
      if (px > 0) {
        onMeasure(widgetId, px)
      }
    })
    observer.observe(target)
    return () => observer.disconnect()
  }, [widgetId, onMeasure])

  return (
    <div className="h-full w-full" ref={ref}>
      {children}
    </div>
  )
}
