import {
  LockIcon,
  MoveDiagonal2Icon,
  MoveHorizontalIcon,
  MoveVerticalIcon,
  PencilIcon,
  TrashIcon,
  UnlockIcon
} from 'lucide-react'
import {
  type ReactNode,
  type Ref,
  useCallback,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
  useSyncExternalStore
} from 'react'
import {
  GridLayout,
  getCompactor,
  type Layout,
  type LayoutItem,
  type ResizeHandleAxis,
  useContainerWidth
} from 'react-grid-layout'
import 'react-grid-layout/css/styles.css'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import type { ServerMetrics } from '@/lib/server-catalog'
import { cn } from '@/lib/utils'
import type { DashboardWidget, SizingStrategy, WidgetTypeDefinition } from '@/lib/widget-types'
import { WIDGET_TYPES } from '@/lib/widget-types'
import { layoutToPatch, widgetsToLayout } from './dashboard-layout'
import { COLS, MARGIN, MARGIN_Y, ROW_HEIGHT, SCALE } from './grid-constants'
import { applyCoarsePatch, applyStrategy, snapOnRelease } from './sizing-strategies'
import { normalizeRenderItem } from './sizing-strategies/normalize'
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

// Legacy coarse row pixel height, used only for the mobile single-column min-height.
const MOBILE_ROW_PX = 80
const MOBILE_BREAKPOINT = 768
const SINGLE_COLUMN_CONTENT_WIDTH = 900
const MOBILE_QUERY = `(max-width: ${MOBILE_BREAKPOINT - 1}px)`

const WIDGET_TYPE_MAP = new Map<string, WidgetTypeDefinition>(WIDGET_TYPES.map((widget) => [widget.id, widget]))

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
  const sorted = layout.toSorted((a, b) => a.y - b.y || a.x - b.x)
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
type ActiveInteractionState = Exclude<InteractionState, 'idle'>

interface LayoutRuntimeState {
  baseLayout: Layout
  interactionState: InteractionState
  liveLayout: Layout
}

type LayoutRuntimeAction =
  | { baseLayout: Layout; layout: Layout; state: ActiveInteractionState; type: 'start-interaction' }
  | { baseLayout: Layout; layout: Layout; type: 'commit-layout' }
  | { baseLayout: Layout; layout: Layout; type: 'set-live-layout' }
  | { type: 'set-idle' }

function createLayoutRuntimeState(layout: Layout): LayoutRuntimeState {
  return { baseLayout: layout, interactionState: 'idle', liveLayout: layout }
}

function layoutRuntimeReducer(state: LayoutRuntimeState, action: LayoutRuntimeAction): LayoutRuntimeState {
  switch (action.type) {
    case 'start-interaction':
      return { baseLayout: action.baseLayout, interactionState: action.state, liveLayout: action.layout }
    case 'commit-layout':
      return { baseLayout: action.baseLayout, interactionState: 'idle', liveLayout: action.layout }
    case 'set-live-layout':
      return { ...state, baseLayout: action.baseLayout, liveLayout: action.layout }
    case 'set-idle':
      return state.interactionState === 'idle' ? state : { ...state, interactionState: 'idle' }
    default:
      return state
  }
}

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

function getMobileSnapshot(): boolean {
  return typeof window !== 'undefined' ? window.innerWidth < MOBILE_BREAKPOINT : false
}

function subscribeToMobileChanges(onStoreChange: () => void): () => void {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
    return () => undefined
  }
  const mql = window.matchMedia(MOBILE_QUERY)
  mql.addEventListener('change', onStoreChange)
  return () => mql.removeEventListener('change', onStoreChange)
}

function useIsMobile(): boolean {
  return useSyncExternalStore(subscribeToMobileChanges, getMobileSnapshot, () => false)
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

  const widgetById = useMemo(() => new Map(widgets.map((w) => [w.id, w])), [widgets])

  const getStrategy = useCallback(
    (itemId: string): SizingStrategy => {
      const widget = widgetById.get(itemId)
      if (!widget) {
        return { kind: 'free' }
      }
      const def = WIDGET_TYPE_MAP.get(widget.widget_type)
      return def?.sizing ?? { kind: 'free' }
    },
    [widgetById]
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

      const strategy = getStrategy(item.i)
      const measured = autoUnits[item.i]

      // Layer A: idle h / minH / maxH per strategy.
      const normalized = normalizeRenderItem(item, strategy, {
        containerWidth: width,
        autoMeasuredFineH: measured
      })
      item.h = normalized.h
      item.minH = normalized.minH
      item.maxH = normalized.maxH

      // Layer B: resize-time constraints, handles, resizability.
      const desc = applyStrategy(strategy, measured)
      if (desc.constraints.length > 0) {
        item.constraints = desc.constraints
      }
      if (desc.resizeHandles) {
        item.resizeHandles = desc.resizeHandles
      }
      if (!desc.isResizable) {
        item.isResizable = false
      }
    }
    return deoverlapLayout(layout)
  }, [widgets, autoUnits, width, getStrategy])

  const [{ baseLayout: liveBaseLayout, liveLayout, interactionState }, dispatchLayoutRuntime] = useReducer(
    layoutRuntimeReducer,
    baseLayout,
    createLayoutRuntimeState
  )

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

  // No auto-compaction (widgets stay exactly where dropped, so items in
  // different columns can be aligned freely) and preventCollision blocks
  // dropping onto another widget (it snaps back), so widgets never overlap.
  //
  // Resize is the exception: RGL's preventCollision path REVERTS the item's
  // size on any collision instead of pushing neighbors, so growing a widget
  // toward a neighbor visually stretched (react-resizable inline style) and
  // snapped back to the old size on release. During a resize we swap in an
  // allowOverlap compactor so RGL accepts the new size; the transient overlap
  // is resolved by deoverlapLayout (every frame and on commit), which pushes
  // the widgets below down. Dragging keeps the strict compactor.
  const compactor = useMemo(
    () => (interactionState === 'resizing' ? getCompactor(null, true, false) : getCompactor(null, false, true)),
    [interactionState]
  )

  // Manual position/size persists in coarse units, so the live layout is
  // aligned to whole coarse rows (SCALE-aligned) while dragging/resizing.
  // Otherwise the dropped fine position gets rounded on commit and the widget
  // visibly snaps back. content-height widgets keep their measured fine height.
  // Only ever called mid-interaction — idle echoes are dropped in
  // handleLayoutChange (see the comment there).
  const updateLiveLayout = useCallback(
    (nextLayout: Layout) => {
      const next = nextLayout.map((item) => {
        const strategy = getStrategy(item.i)
        const base = {
          ...item,
          y: Math.round(item.y / SCALE) * SCALE
        }
        // Snap h to SCALE multiples only for strategies that operate at coarse h.
        // aspect-square: h is fine pixel-square (RGL constraints handle resize); leave it.
        // content-height: h locked to measurement; leave it.
        if (strategy.kind === 'free' || strategy.kind === 'fixed') {
          base.h = Math.max(SCALE, Math.round(item.h / SCALE) * SCALE)
        }
        return base
      })
      // De-overlap every frame: RGL's identity compactor never resolves
      // overlaps, and its idle onLayoutChange echo would otherwise re-apply the
      // raw (overlapping) persisted positions over the de-overlapped layout.
      dispatchLayoutRuntime({ type: 'set-live-layout', baseLayout, layout: deoverlapLayout(next) })
    },
    [baseLayout, getStrategy]
  )

  useEffect(() => {
    if (isMobile) {
      dispatchLayoutRuntime({ type: 'set-idle' })
    }
  }, [isMobile])

  const startInteraction = useCallback(
    (state: ActiveInteractionState) => {
      dispatchLayoutRuntime({ type: 'start-interaction', state, baseLayout, layout: baseLayout })
    },
    [baseLayout]
  )

  const handleLayoutChange = useCallback(
    (newLayout: Layout) => {
      // Live previews arrive via the onDrag/onResize props, and commits via
      // onDragStop/onResizeStop, so everything onLayoutChange delivers here is
      // an idle "echo" of RGL's internal state (mount correction, post-stop
      // re-emit, sync after a save or content-height measurement). Adopting an
      // echo into liveLayout feeds RGL's state back into the controlled
      // `layout` prop, and any mismatch with what we committed (e.g. the
      // unsnapped post-resize layout) ping-pongs forever — the layout jitters
      // and burns CPU. This grid is controlled: our layout is the source of
      // truth, so idle echoes are dropped and RGL re-syncs from props instead.
      if (isInteracting) {
        updateLiveLayout(newLayout)
      }
    },
    [isInteracting, updateLiveLayout]
  )

  const commitLayoutChange = useCallback(
    (finalLayout: Layout) => {
      // Per-strategy snap. For free/fixed: snap h to coarse multiples. For
      // aspect-square: apply snapOnRelease's coarse SnapPatch via applyCoarsePatch
      // (sets w and re-derives fine h via SCALE). Then re-normalize so the live
      // layout matches what the next baseLayout render will produce.
      const snapped = finalLayout.map((item) => {
        const strategy = getStrategy(item.i)
        const measured = autoUnits[item.i]

        let base = {
          ...item,
          y: Math.round(item.y / SCALE) * SCALE
        }
        if (strategy.kind === 'free' || strategy.kind === 'fixed') {
          base.h = Math.max(SCALE, Math.round(item.h / SCALE) * SCALE)
        }

        const snap = snapOnRelease(base, strategy, { containerWidth: width })
        base = applyCoarsePatch(base, snap)
        return normalizeRenderItem(base, strategy, {
          containerWidth: width,
          autoMeasuredFineH: measured
        })
      })
      const resolved = deoverlapLayout(snapped)
      dispatchLayoutRuntime({ type: 'commit-layout', baseLayout, layout: resolved })

      const coarseLayout = resolved.map((item) => ({
        ...item,
        y: Math.round(item.y / SCALE),
        h: Math.round(item.h / SCALE)
      }))
      const patch = layoutToPatch(coarseLayout, widgets)
      if (patch.length > 0) {
        onLayoutChange(patch)
      }
    },
    [autoUnits, baseLayout, getStrategy, onLayoutChange, widgets, width]
  )

  const sortedWidgets = useMemo(() => {
    return widgets.toSorted((a, b) => a.sort_order - b.sort_order)
  }, [widgets])

  const useSingleColumn = isMobile || (mounted && width < SINGLE_COLUMN_CONTENT_WIDTH)
  const renderedLayout = isInteracting || liveBaseLayout === baseLayout ? liveLayout : baseLayout

  if (useSingleColumn) {
    return (
      <div className="space-y-4" ref={containerRef}>
        {sortedWidgets.map((widget) => {
          const isAuto = getStrategy(widget.id).kind === 'content-height'
          const mobileHeight = widget.grid_h * MOBILE_ROW_PX
          return (
            <div className="relative" data-widget-id={widget.id} key={widget.id}>
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
                style={isAuto ? { minHeight: mobileHeight } : { height: mobileHeight }}
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
          layout={renderedLayout}
          onDrag={(next) => updateLiveLayout(next)}
          onDragStart={() => startInteraction('dragging')}
          onDragStop={commitLayoutChange}
          onLayoutChange={handleLayoutChange}
          onResize={(next) => updateLiveLayout(next)}
          onResizeStart={() => startInteraction('resizing')}
          onResizeStop={commitLayoutChange}
          resizeConfig={{ enabled: isEditing, handleComponent: renderResizeHandle, handles: ['s', 'e', 'se'] }}
          width={width}
        >
          {widgets.map((widget) => {
            const isAuto = getStrategy(widget.id).kind === 'content-height'
            return (
              <div className="relative h-full" data-widget-id={widget.id} key={widget.id}>
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
  const { t } = useTranslation('dashboard')
  const toggleStaticLabel = isStatic ? t('unlock_widget_position') : t('lock_widget_position')

  return (
    <div
      className={cn(
        'absolute top-1 right-1 z-10 flex gap-1 transition-opacity [div:hover>&]:opacity-100',
        forceVisible ? 'opacity-100' : 'opacity-0'
      )}
    >
      {onToggleStatic && (
        <Button
          aria-label={toggleStaticLabel}
          className="size-7"
          onClick={(e) => {
            e.stopPropagation()
            onToggleStatic()
          }}
          size="icon-sm"
          title={toggleStaticLabel}
          variant="outline"
        >
          {isStatic ? <LockIcon className="size-3.5" /> : <UnlockIcon className="size-3.5" />}
        </Button>
      )}
      <Button
        aria-label={t('configure_widget')}
        className="size-7"
        onClick={(e) => {
          e.stopPropagation()
          onEdit()
        }}
        size="icon-sm"
        title={t('configure_widget')}
        variant="outline"
      >
        <PencilIcon className="size-3.5" />
      </Button>
      <Button
        aria-label={t('delete_widget')}
        className="size-7"
        onClick={(e) => {
          e.stopPropagation()
          onDelete()
        }}
        size="icon-sm"
        title={t('delete_widget')}
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
