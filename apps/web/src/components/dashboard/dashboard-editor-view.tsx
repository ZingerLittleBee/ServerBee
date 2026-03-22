import { PencilIcon, SaveIcon, XIcon } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import type { WidgetInput } from '@/hooks/use-dashboard'
import { useDashboardEditor } from '@/hooks/use-dashboard-editor'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import type { Dashboard, DashboardWithWidgets } from '@/lib/widget-types'
import { DashboardGrid } from './dashboard-grid'
import { DashboardSwitcher } from './dashboard-switcher'
import { WidgetConfigDialog } from './widget-config-dialog'
import { WidgetPicker } from './widget-picker'

interface DashboardEditorViewProps {
  dashboard?: DashboardWithWidgets
  dashboards: Dashboard[]
  isAdmin: boolean
  isSaving: boolean
  onSave: (widgets: WidgetInput[]) => Promise<void>
  onSelectDashboard: (id: string) => void
  servers: ServerMetrics[]
}

export function DashboardEditorView({
  dashboard,
  dashboards,
  isAdmin,
  isSaving,
  onSave,
  onSelectDashboard,
  servers
}: DashboardEditorViewProps) {
  const { t } = useTranslation('dashboard')
  const editor = useDashboardEditor()
  const [pickerOpen, setPickerOpen] = useState(false)
  const [configOpen, setConfigOpen] = useState(false)
  const [configWidgetType, setConfigWidgetType] = useState('')
  const [editingWidgetId, setEditingWidgetId] = useState<string | null>(null)

  const dashboardId = dashboard?.id ?? null
  const activeId = dashboardId ?? ''
  const widgets = dashboard?.widgets ?? []
  const displayWidgets = editor.isEditing ? editor.draftWidgets : widgets
  const editingWidget =
    editor.isEditing && editingWidgetId
      ? editor.draftWidgets.find((widget) => widget.id === editingWidgetId)
      : undefined
  const cancelEditingRef = useRef(editor.cancelEditing)
  cancelEditingRef.current = editor.cancelEditing

  useEffect(() => {
    if (dashboardId === null) {
      cancelEditingRef.current()
      setPickerOpen(false)
      setConfigOpen(false)
      setConfigWidgetType('')
      setEditingWidgetId(null)
      return
    }

    cancelEditingRef.current()
    setPickerOpen(false)
    setConfigOpen(false)
    setConfigWidgetType('')
    setEditingWidgetId(null)
  }, [dashboardId])

  function resetViewState() {
    setPickerOpen(false)
    setConfigOpen(false)
    setConfigWidgetType('')
    setEditingWidgetId(null)
  }

  function handleEdit() {
    editor.startEditing(widgets)
  }

  function handleCancel() {
    editor.cancelEditing()
    resetViewState()
  }

  async function handleSave() {
    await onSave(editor.buildSaveInput())
    handleCancel()
  }

  function handlePickerSelect(widgetType: string) {
    setPickerOpen(false)
    setEditingWidgetId(null)
    setConfigWidgetType(widgetType)
    setConfigOpen(true)
  }

  function handleWidgetEdit(widgetId: string) {
    const widget = editor.draftWidgets.find((draftWidget) => draftWidget.id === widgetId)
    if (!widget) {
      return
    }
    setEditingWidgetId(widgetId)
    setConfigWidgetType(widget.widget_type)
    setConfigOpen(true)
  }

  function handleWidgetDelete(widgetId: string) {
    if (editingWidgetId === widgetId) {
      setEditingWidgetId(null)
      setConfigOpen(false)
    }
    editor.deleteWidget(widgetId)
  }

  function handleConfigSubmit(title: string, configJson: string) {
    if (editingWidget) {
      editor.updateWidget(editingWidget.id, {
        title: title || null,
        config_json: configJson
      })
    } else if (dashboard) {
      editor.addWidget({
        dashboardId: dashboard.id,
        widgetType: configWidgetType,
        title: title || null,
        configJson
      })
    }

    resetViewState()
  }

  function handleDashboardSelect(id: string) {
    if (editor.isEditing) {
      handleCancel()
    }
    onSelectDashboard(id)
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
          {isAdmin && !editor.isEditing && (
            <Button onClick={handleEdit} size="sm" variant="outline">
              <PencilIcon className="mr-1 size-4" />
              {t('edit')}
            </Button>
          )}
          {editor.isEditing && (
            <>
              <Button disabled={isSaving} onClick={handleSave} size="sm">
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

      {displayWidgets.length === 0 && !editor.isEditing && (
        <div className="flex min-h-[300px] items-center justify-center rounded-lg border border-dashed">
          <div className="text-center">
            <p className="text-muted-foreground text-sm">{t('no_widgets_title')}</p>
            <p className="mt-1 text-muted-foreground text-xs">{t('no_widgets_description')}</p>
          </div>
        </div>
      )}

      {(displayWidgets.length > 0 || editor.isEditing) && (
        <DashboardGrid
          isEditing={editor.isEditing}
          onAddWidget={() => setPickerOpen(true)}
          onLayoutChange={editor.commitLayoutPatch}
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
