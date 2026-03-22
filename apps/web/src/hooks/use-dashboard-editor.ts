import { useMemo, useState } from 'react'
import { mergeLayoutPatch, type LayoutPatch } from '@/components/dashboard/dashboard-layout'
import type { DashboardWidget } from '@/lib/widget-types'
import type { WidgetInput } from './use-dashboard'

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
    updateWidget
  }
}
