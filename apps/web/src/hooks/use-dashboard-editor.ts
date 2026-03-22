import { useMemo, useState } from 'react'
import {
  type LayoutPatch,
  mergeLayoutPatch,
  normalizeNewWidgetPlacement
} from '@/components/dashboard/dashboard-layout'
import { parseConfig } from '@/lib/widget-helpers'
import type { DashboardWidget } from '@/lib/widget-types'
import { WIDGET_TYPES, type WidgetTypeDefinition } from '@/lib/widget-types'
import type { WidgetInput } from './use-dashboard'

interface AddWidgetInput {
  configJson: string
  dashboardId: string
  title: string | null
  widgetType: string
}

interface UpdateWidgetChanges {
  config_json?: string
  title?: string | null
}

const WIDGET_TYPE_MAP = new Map<string, WidgetTypeDefinition>(WIDGET_TYPES.map((widget) => [widget.id, widget]))

function getWidgetTypeDefaults(widgetType: string) {
  const definition = WIDGET_TYPE_MAP.get(widgetType)
  return {
    grid_h: definition?.defaultH ?? 2,
    grid_w: definition?.defaultW ?? 2
  }
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

  function addWidget({ configJson, dashboardId, title, widgetType }: AddWidgetInput) {
    setDraftWidgets((current) => {
      const { grid_h, grid_w } = getWidgetTypeDefaults(widgetType)
      const newWidget: DashboardWidget = {
        config_json: configJson,
        created_at: new Date().toISOString(),
        dashboard_id: dashboardId,
        grid_h,
        grid_w,
        grid_x: 0,
        grid_y: Number.POSITIVE_INFINITY,
        id: `temp-${crypto.randomUUID()}`,
        sort_order: current.length,
        title,
        widget_type: widgetType
      }
      return normalizeNewWidgetPlacement(current, newWidget)
    })
  }

  function updateWidget(id: string, changes: UpdateWidgetChanges) {
    setDraftWidgets((current) => current.map((widget) => (widget.id === id ? { ...widget, ...changes } : widget)))
  }

  function deleteWidget(id: string) {
    setDraftWidgets((current) =>
      current.filter((widget) => widget.id !== id).map((widget, index) => ({ ...widget, sort_order: index }))
    )
  }

  function toggleWidgetStatic(id: string) {
    setDraftWidgets((current) =>
      current.map((widget) => {
        if (widget.id !== id) {
          return widget
        }
        const config = parseConfig<Record<string, unknown>>(widget.config_json)
        const isStatic = config.is_static === true
        const { is_static: _, ...rest } = config
        const nextConfig = isStatic ? rest : { ...rest, is_static: true }
        return { ...widget, config_json: JSON.stringify(nextConfig) }
      })
    )
  }

  function buildSaveInput(): WidgetInput[] {
    return draftWidgets.map((widget) => ({
      id: widget.id.startsWith('temp-') ? undefined : widget.id,
      widget_type: widget.widget_type,
      title: widget.title,
      config_json: parseConfig(widget.config_json),
      grid_x: widget.grid_x,
      grid_y: widget.grid_y,
      grid_w: widget.grid_w,
      grid_h: widget.grid_h,
      sort_order: widget.sort_order
    }))
  }

  const isDirty = useMemo(
    () => JSON.stringify(baseWidgets) !== JSON.stringify(draftWidgets),
    [baseWidgets, draftWidgets]
  )

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
    toggleWidgetStatic,
    updateWidget
  }
}
