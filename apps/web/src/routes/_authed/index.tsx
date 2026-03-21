import { useQuery } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { PencilIcon, SaveIcon, XIcon } from 'lucide-react'
import { useCallback, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { DashboardGrid } from '@/components/dashboard/dashboard-grid'
import { DashboardSwitcher } from '@/components/dashboard/dashboard-switcher'
import { WidgetConfigDialog } from '@/components/dashboard/widget-config-dialog'
import { WidgetPicker } from '@/components/dashboard/widget-picker'
import { Button } from '@/components/ui/button'
import { useAuth } from '@/hooks/use-auth'
import { useDashboard, useDashboards, useDefaultDashboard, useUpdateDashboard } from '@/hooks/use-dashboard'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import type { DashboardWidget } from '@/lib/widget-types'
import { WIDGET_TYPES } from '@/lib/widget-types'

export const Route = createFileRoute('/_authed/')({
  component: DashboardPage
})

function DashboardPage() {
  const { t } = useTranslation('dashboard')
  const { user } = useAuth()
  const isAdmin = user?.role === 'admin'

  const { data: servers = [] } = useQuery<ServerMetrics[]>({
    queryKey: ['servers'],
    queryFn: () => [],
    staleTime: Number.POSITIVE_INFINITY,
    refetchOnMount: false,
    refetchOnWindowFocus: false
  })

  const { data: dashboards = [] } = useDashboards()
  const { data: defaultDashboard } = useDefaultDashboard()

  const [selectedId, setSelectedId] = useState<string | null>(null)
  const activeId = selectedId ?? defaultDashboard?.id ?? ''
  const { data: activeDashboard } = useDashboard(activeId)

  const dashboard = selectedId ? activeDashboard : (activeDashboard ?? defaultDashboard)
  const widgets = dashboard?.widgets ?? []

  const [isEditing, setIsEditing] = useState(false)
  const [draftWidgets, setDraftWidgets] = useState<DashboardWidget[]>([])
  const [pickerOpen, setPickerOpen] = useState(false)
  const [configOpen, setConfigOpen] = useState(false)
  const [configWidgetType, setConfigWidgetType] = useState('')
  const [editingWidget, setEditingWidget] = useState<DashboardWidget | undefined>(undefined)

  const updateDashboard = useUpdateDashboard()

  const displayWidgets = isEditing ? draftWidgets : widgets

  const handleEdit = () => {
    setDraftWidgets([...widgets])
    setIsEditing(true)
  }

  const handleCancel = () => {
    setDraftWidgets([])
    setIsEditing(false)
  }

  const handleSave = () => {
    if (!dashboard) {
      return
    }
    const widgetInputs = draftWidgets.map((w, idx) => ({
      id: w.id.startsWith('temp-') ? undefined : w.id,
      widget_type: w.widget_type,
      title: w.title,
      config_json: JSON.parse(w.config_json),
      grid_x: w.grid_x,
      grid_y: w.grid_y,
      grid_w: w.grid_w,
      grid_h: w.grid_h,
      sort_order: idx
    }))
    updateDashboard.mutate(
      { id: dashboard.id, widgets: widgetInputs },
      {
        onSuccess: () => {
          setIsEditing(false)
          setDraftWidgets([])
        }
      }
    )
  }

  const handleLayoutChange = useCallback(
    (updates: { id: string; grid_x: number; grid_y: number; grid_w: number; grid_h: number }[]) => {
      setDraftWidgets((prev) =>
        prev.map((w) => {
          const update = updates.find((u) => u.id === w.id)
          if (!update) {
            return w
          }
          return { ...w, grid_x: update.grid_x, grid_y: update.grid_y, grid_w: update.grid_w, grid_h: update.grid_h }
        })
      )
    },
    []
  )

  const handlePickerSelect = (widgetType: string) => {
    setConfigWidgetType(widgetType)
    setEditingWidget(undefined)
    setConfigOpen(true)
  }

  const handleWidgetEdit = (widgetId: string) => {
    const widget = draftWidgets.find((w) => w.id === widgetId)
    if (!widget) {
      return
    }
    setConfigWidgetType(widget.widget_type)
    setEditingWidget(widget)
    setConfigOpen(true)
  }

  const handleWidgetDelete = (widgetId: string) => {
    setDraftWidgets((prev) => prev.filter((w) => w.id !== widgetId))
  }

  const handleConfigSubmit = (title: string, configJson: string) => {
    if (editingWidget) {
      setDraftWidgets((prev) =>
        prev.map((w) => (w.id === editingWidget.id ? { ...w, title: title || null, config_json: configJson } : w))
      )
    } else {
      const def = WIDGET_TYPES.find((wt) => wt.id === configWidgetType)
      const newWidget: DashboardWidget = {
        id: `temp-${crypto.randomUUID()}`,
        dashboard_id: dashboard?.id ?? '',
        widget_type: configWidgetType,
        title: title || null,
        config_json: configJson,
        grid_x: 0,
        grid_y: Number.POSITIVE_INFINITY,
        grid_w: def?.defaultW ?? 4,
        grid_h: def?.defaultH ?? 3,
        sort_order: draftWidgets.length,
        created_at: new Date().toISOString()
      }
      setDraftWidgets((prev) => [...prev, newWidget])
    }
  }

  const handleDashboardSelect = (id: string) => {
    if (isEditing) {
      setIsEditing(false)
      setDraftWidgets([])
    }
    setSelectedId(id)
  }

  return (
    <div>
      <div className="mb-6 flex flex-wrap items-center justify-between gap-3">
        <DashboardSwitcher
          currentId={activeId}
          dashboards={dashboards}
          isAdmin={isAdmin}
          onSelect={handleDashboardSelect}
        />
        <div className="flex items-center gap-2">
          {isAdmin && !isEditing && (
            <Button onClick={handleEdit} size="sm" variant="outline">
              <PencilIcon className="mr-1 size-4" />
              {t('edit')}
            </Button>
          )}
          {isEditing && (
            <>
              <Button disabled={updateDashboard.isPending} onClick={handleSave} size="sm">
                <SaveIcon className="mr-1 size-4" />
                {t('save')}
              </Button>
              <Button onClick={handleCancel} size="sm" variant="ghost">
                <XIcon className="mr-1 size-4" />
                {t('cancel')}
              </Button>
            </>
          )}
        </div>
      </div>

      {displayWidgets.length === 0 && !isEditing && (
        <div className="flex min-h-[300px] items-center justify-center rounded-lg border border-dashed">
          <div className="text-center">
            <p className="text-muted-foreground text-sm">{t('no_widgets_title')}</p>
            <p className="mt-1 text-muted-foreground text-xs">{t('no_widgets_description')}</p>
          </div>
        </div>
      )}

      {(displayWidgets.length > 0 || isEditing) && (
        <DashboardGrid
          isEditing={isEditing}
          onAddWidget={() => setPickerOpen(true)}
          onLayoutChange={handleLayoutChange}
          onWidgetDelete={handleWidgetDelete}
          onWidgetEdit={handleWidgetEdit}
          servers={servers}
          widgets={displayWidgets}
        />
      )}

      <WidgetPicker onOpenChange={setPickerOpen} onSelect={handlePickerSelect} open={pickerOpen} />

      <WidgetConfigDialog
        onOpenChange={setConfigOpen}
        onSubmit={handleConfigSubmit}
        open={configOpen}
        servers={servers}
        widget={editingWidget}
        widgetType={configWidgetType}
      />
    </div>
  )
}
