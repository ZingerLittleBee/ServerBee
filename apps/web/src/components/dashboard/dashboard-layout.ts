import type { Layout, LayoutItem } from 'react-grid-layout'
import type { DashboardWidget, WidgetTypeDefinition } from '@/lib/widget-types'
import { WIDGET_TYPES } from '@/lib/widget-types'

export interface LayoutPatch {
  grid_h: number
  grid_w: number
  grid_x: number
  grid_y: number
  id: string
}

const WIDGET_TYPE_MAP = new Map<string, WidgetTypeDefinition>(WIDGET_TYPES.map((widget) => [widget.id, widget]))

function getSizeConstraints(widgetType: string) {
  const definition = WIDGET_TYPE_MAP.get(widgetType)
  return {
    minW: definition?.minW ?? 2,
    minH: definition?.minH ?? 2,
    maxW: definition?.maxW,
    maxH: definition?.maxH
  }
}

function isWidgetStatic(configJson: string): boolean {
  try {
    const config = JSON.parse(configJson)
    return config?.is_static === true
  } catch {
    return false
  }
}

export function widgetsToLayout(widgets: DashboardWidget[]): Layout {
  return widgets.map((widget) => {
    const { minW, minH, maxW, maxH } = getSizeConstraints(widget.widget_type)
    const item: LayoutItem = {
      i: widget.id,
      x: widget.grid_x,
      y: widget.grid_y,
      w: widget.grid_w,
      h: widget.grid_h,
      minW,
      minH
    }
    if (maxW !== undefined) {
      item.maxW = maxW
    }
    if (maxH !== undefined) {
      item.maxH = maxH
    }
    if (isWidgetStatic(widget.config_json)) {
      item.static = true
    }
    return item
  })
}

export function layoutToPatch(
  layout: readonly Pick<LayoutItem, 'i' | 'x' | 'y' | 'w' | 'h'>[],
  widgets: DashboardWidget[]
): LayoutPatch[] {
  const widgetMap = new Map(widgets.map((widget) => [widget.id, widget]))
  return layout.flatMap((item) => {
    const widget = widgetMap.get(item.i)
    if (!widget) {
      return []
    }
    if (item.x === widget.grid_x && item.y === widget.grid_y && item.w === widget.grid_w && item.h === widget.grid_h) {
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
  const normalizedGridY = Number.isFinite(newWidget.grid_y)
    ? newWidget.grid_y
    : widgets.reduce((maxY, widget) => Math.max(maxY, widget.grid_y + widget.grid_h), 0)

  return [...widgets, { ...newWidget, grid_y: normalizedGridY, sort_order: widgets.length }]
}
