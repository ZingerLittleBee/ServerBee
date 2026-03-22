import {
  LockIcon,
  MoveDiagonal2Icon,
  MoveHorizontalIcon,
  MoveVerticalIcon,
  PencilIcon,
  TrashIcon,
  UnlockIcon
} from 'lucide-react'
import { type Ref, useCallback, useEffect, useMemo, useState } from 'react'
import { GridLayout, type Layout, type ResizeHandleAxis, useContainerWidth } from 'react-grid-layout'
import 'react-grid-layout/css/styles.css'
import { Button } from '@/components/ui/button'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { cn } from '@/lib/utils'
import type { DashboardWidget } from '@/lib/widget-types'
import { layoutToPatch, widgetsToLayout } from './dashboard-layout'
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
const ROW_HEIGHT = 80
const MARGIN: [number, number] = [16, 16]
const MOBILE_BREAKPOINT = 768

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

  const baseLayout = useMemo(() => widgetsToLayout(widgets), [widgets])
  const [liveLayout, setLiveLayout] = useState<Layout>(baseLayout)
  const [interactionState, setInteractionState] = useState<InteractionState>('idle')

  const updateLiveLayout = useCallback((nextLayout: Layout) => {
    setLiveLayout(nextLayout)
  }, [])

  useEffect(() => {
    if (interactionState === 'idle') {
      updateLiveLayout(baseLayout)
    }
  }, [baseLayout, interactionState, updateLiveLayout])

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
      const patch = layoutToPatch(finalLayout, widgets)
      if (patch.length > 0) {
        onLayoutChange(patch)
      }
    },
    [onLayoutChange, widgets]
  )

  const sortedWidgets = useMemo(() => {
    return [...widgets].sort((a, b) => a.sort_order - b.sort_order)
  }, [widgets])

  if (isMobile) {
    return (
      <div className="space-y-4">
        {sortedWidgets.map((widget) => (
          <div className="relative" key={widget.id}>
            {isEditing && (
              <EditOverlay
                isStatic={isWidgetStatic(widget.config_json)}
                onDelete={() => onWidgetDelete(widget.id)}
                onEdit={() => onWidgetEdit(widget.id)}
                onToggleStatic={onWidgetToggleStatic ? () => onWidgetToggleStatic(widget.id) : undefined}
              />
            )}
            <div
              className={isEditing ? 'pointer-events-none' : undefined}
              style={{ minHeight: widget.grid_h * ROW_HEIGHT }}
            >
              <WidgetRenderer servers={servers} widget={widget} />
            </div>
          </div>
        ))}
      </div>
    )
  }

  return (
    <div ref={containerRef}>
      {mounted && (
        <GridLayout
          autoSize
          className={cn('dashboard-grid', isEditing && 'dashboard-grid--editing')}
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
          {widgets.map((widget) => (
            <div className="relative h-full" key={widget.id}>
              {isEditing && (
                <EditOverlay
                  isStatic={isWidgetStatic(widget.config_json)}
                  onDelete={() => onWidgetDelete(widget.id)}
                  onEdit={() => onWidgetEdit(widget.id)}
                  onToggleStatic={onWidgetToggleStatic ? () => onWidgetToggleStatic(widget.id) : undefined}
                />
              )}
              <div className={isEditing ? 'pointer-events-none h-full' : 'h-full'}>
                <WidgetRenderer servers={servers} widget={widget} />
              </div>
            </div>
          ))}
        </GridLayout>
      )}
    </div>
  )
}

function EditOverlay({
  isStatic,
  onEdit,
  onDelete,
  onToggleStatic
}: {
  isStatic?: boolean
  onDelete: () => void
  onEdit: () => void
  onToggleStatic?: () => void
}) {
  return (
    <div className="absolute top-1 right-1 z-10 flex gap-1 opacity-0 transition-opacity [div:hover>&]:opacity-100">
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
