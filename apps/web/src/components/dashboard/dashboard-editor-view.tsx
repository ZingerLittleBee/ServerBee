import { PencilIcon, PlusIcon, SaveIcon, XIcon } from 'lucide-react'
import { useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import type { WidgetInput } from '@/hooks/use-dashboard'
import { useDashboardEditor } from '@/hooks/use-dashboard-editor'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import type { Dashboard, DashboardWithWidgets } from '@/lib/widget-types'
import { DashboardGrid } from './dashboard-grid'
import { DashboardSwitcher } from './dashboard-switcher'
import { WidgetConfigDialog } from './widget-config-dialog'
import { WidgetPicker, type WidgetPickerSelection } from './widget-picker'

interface DashboardEditorViewProps {
  activeDashboardId: string
  dashboard?: DashboardWithWidgets
  dashboards: Dashboard[]
  isAdmin: boolean
  isSaving: boolean
  onSave: (widgets: WidgetInput[]) => Promise<void>
  onSelectDashboard: (id: string) => void
  servers: ServerMetrics[]
}

export function DashboardEditorView({ activeDashboardId, ...props }: DashboardEditorViewProps) {
  return <DashboardEditorViewContent activeDashboardId={activeDashboardId} key={activeDashboardId} {...props} />
}

function DashboardEditorViewContent({
  activeDashboardId,
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

  const isDashboardReady = dashboard?.id === activeDashboardId
  const isDashboardLoading = activeDashboardId !== '' && !isDashboardReady
  const widgets = isDashboardReady ? dashboard.widgets : []
  const displayWidgets = editor.isEditing ? editor.draftWidgets : widgets
  const editingWidget =
    editor.isEditing && editingWidgetId
      ? editor.draftWidgets.find((widget) => widget.id === editingWidgetId)
      : undefined
  // While a dialog is open over the live dashboard, freeze the servers snapshot.
  // The dialog backdrop applies a full-viewport backdrop-filter blur; if the grid
  // behind keeps repainting on every websocket tick, the browser re-rasterizes the
  // blurred backdrop every frame, causing severe jank.
  const dialogOpen = pickerOpen || configOpen
  const frozenServersRef = useRef(servers)
  if (!dialogOpen) {
    frozenServersRef.current = servers
  }
  const gridServers = dialogOpen ? frozenServersRef.current : servers

  function resetViewState() {
    setPickerOpen(false)
    setConfigOpen(false)
    setConfigWidgetType('')
    setEditingWidgetId(null)
  }

  function handleEdit() {
    if (!isDashboardReady) {
      return
    }
    editor.startEditing(widgets)
  }

  function handleCancel() {
    editor.cancelEditing()
    resetViewState()
  }

  async function handleSave() {
    if (!isDashboardReady) {
      return
    }
    await onSave(editor.buildSaveInput())
    handleCancel()
  }

  function handlePickerSelect(selection: WidgetPickerSelection) {
    setPickerOpen(false)
    setEditingWidgetId(null)
    if (selection.type === 'module') {
      // Modules currently use their own configSchema; we add directly with an empty
      // config object instead of opening the legacy config dialog (which is built
      // around hard-coded form variants per builtin widget type).
      if (isDashboardReady && dashboard) {
        const sizing = selection.manifest.sizing
        editor.addWidget({
          dashboardId: dashboard.id,
          widgetType: 'module',
          moduleId: selection.moduleId,
          title: selection.manifest.name,
          configJson: '{}',
          gridW: sizing.defaultW ?? 4,
          gridH: sizing.defaultH ?? 3
        })
      }
      return
    }
    setConfigWidgetType(selection.widgetType)
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
    } else if (isDashboardReady && dashboard) {
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
      queueMicrotask(() => onSelectDashboard(id))
      return
    }
    onSelectDashboard(id)
  }

  return (
    <div className="w-full min-w-0 max-w-[calc(100vw-1.5rem)] overflow-hidden sm:max-w-full">
      <div className="mb-6 flex w-full min-w-0 max-w-full flex-col gap-3 sm:flex-row sm:flex-wrap sm:items-center sm:justify-between">
        <DashboardSwitcher
          currentId={activeDashboardId}
          dashboards={dashboards}
          isAdmin={isAdmin}
          onSelect={handleDashboardSelect}
        />
        <div className="flex flex-wrap items-center gap-2">
          {isAdmin && !editor.isEditing && isDashboardReady && (
            <Button onClick={handleEdit} size="sm" variant="outline">
              <PencilIcon className="mr-1 size-4" />
              {t('edit')}
            </Button>
          )}
          {editor.isEditing && (
            <>
              <Button onClick={() => setPickerOpen(true)} size="sm" variant="outline">
                <PlusIcon className="mr-1 size-4" />
                {t('add_widget', 'Add Widget')}
              </Button>
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

      {isDashboardReady && displayWidgets.length === 0 && !editor.isEditing && (
        <div className="flex min-h-[300px] items-center justify-center rounded-lg border border-dashed">
          <div className="text-center">
            <p className="text-muted-foreground text-sm">{t('no_widgets_title')}</p>
            <p className="mt-1 text-muted-foreground text-xs">{t('no_widgets_description')}</p>
          </div>
        </div>
      )}

      {isDashboardLoading && (
        <div aria-hidden="true" className="min-h-[300px] rounded-lg border border-dashed bg-muted/10" />
      )}

      {(displayWidgets.length > 0 || editor.isEditing) && (
        <DashboardGrid
          isEditing={editor.isEditing}
          onLayoutChange={editor.commitLayoutPatch}
          onWidgetDelete={handleWidgetDelete}
          onWidgetEdit={handleWidgetEdit}
          onWidgetToggleStatic={editor.toggleWidgetStatic}
          servers={gridServers}
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
