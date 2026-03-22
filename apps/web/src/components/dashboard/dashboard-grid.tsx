import { PencilIcon, PlusIcon, TrashIcon } from 'lucide-react'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { GridLayout, type Layout, useContainerWidth } from 'react-grid-layout'
import { useTranslation } from 'react-i18next'
import 'react-grid-layout/css/styles.css'
import { Button } from '@/components/ui/button'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import type { DashboardWidget } from '@/lib/widget-types'
import { layoutToPatch, widgetsToLayout } from './dashboard-layout'
import { WidgetRenderer } from './widget-renderer'

interface DashboardGridProps {
  isEditing: boolean
  onAddWidget?: () => void
  onLayoutChange: (updates: { id: string; grid_x: number; grid_y: number; grid_w: number; grid_h: number }[]) => void
  onWidgetDelete: (widgetId: string) => void
  onWidgetEdit: (widgetId: string) => void
  servers: ServerMetrics[]
  widgets: DashboardWidget[]
}

const COLS = 12
const ROW_HEIGHT = 80
const MARGIN: [number, number] = [16, 16]
const MOBILE_BREAKPOINT = 768

type InteractionState = 'dragging' | 'idle' | 'resizing'

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
  onAddWidget,
  servers
}: DashboardGridProps) {
  const { t } = useTranslation('dashboard')
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

  const handleLayoutChange = useCallback((newLayout: Layout) => {
    updateLiveLayout(newLayout)
  }, [updateLiveLayout])

  const commitLayoutChange = useCallback((finalLayout: Layout) => {
    setInteractionState('idle')
    const patch = layoutToPatch(finalLayout, widgets)
    if (patch.length > 0) {
      onLayoutChange(patch)
    }
  }, [onLayoutChange, widgets])

  const sortedWidgets = useMemo(() => {
    return [...widgets].sort((a, b) => a.sort_order - b.sort_order)
  }, [widgets])

  if (isMobile) {
    return (
      <div className="space-y-4">
        {sortedWidgets.map((widget) => (
          <div className="relative" key={widget.id}>
            {isEditing && (
              <EditOverlay onDelete={() => onWidgetDelete(widget.id)} onEdit={() => onWidgetEdit(widget.id)} />
            )}
            <div style={{ minHeight: widget.grid_h * ROW_HEIGHT }}>
              <WidgetRenderer servers={servers} widget={widget} />
            </div>
          </div>
        ))}
        {isEditing && onAddWidget && <AddWidgetButton label={t('add_widget', 'Add Widget')} onClick={onAddWidget} />}
      </div>
    )
  }

  return (
    <div ref={containerRef}>
      {mounted && (
        <GridLayout
          autoSize
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
          resizeConfig={{ enabled: isEditing, handles: ['se'] }}
          width={width}
        >
          {widgets.map((widget) => (
            <div className="relative" key={widget.id}>
              {isEditing && (
                <EditOverlay onDelete={() => onWidgetDelete(widget.id)} onEdit={() => onWidgetEdit(widget.id)} />
              )}
              <div className="h-full">
                <WidgetRenderer servers={servers} widget={widget} />
              </div>
            </div>
          ))}
        </GridLayout>
      )}
      {isEditing && onAddWidget && (
        <div className="mt-4 flex justify-center">
          <AddWidgetButton label={t('add_widget', 'Add Widget')} onClick={onAddWidget} />
        </div>
      )}
    </div>
  )
}

function EditOverlay({ onEdit, onDelete }: { onDelete: () => void; onEdit: () => void }) {
  return (
    <div className="absolute top-1 right-1 z-10 flex gap-1 opacity-0 transition-opacity [div:hover>&]:opacity-100">
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

function AddWidgetButton({ onClick, label }: { label: string; onClick: () => void }) {
  return (
    <Button className="gap-1.5" onClick={onClick} variant="outline">
      <PlusIcon className="size-4" />
      {label}
    </Button>
  )
}
