import { PencilIcon, PlusIcon, SaveIcon, XIcon } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { SiteHeaderActions, SiteHeaderLeading } from '@/components/site-header'
import { Button } from '@/components/ui/button'
import type { WidgetInput } from '@/hooks/use-dashboard'
import { useDashboardEditor } from '@/hooks/use-dashboard-editor'
import type { ServerMetrics } from '@/lib/server-catalog'
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
  const [pendingScrollId, setPendingScrollId] = useState<string | null>(null)

  // New widgets are appended below every existing row, which can land outside
  // the viewport — bring the widget into view once it has rendered.
  // biome-ignore lint/correctness/useExhaustiveDependencies: editor.draftWidgets retriggers the lookup after the grid renders the new widget
  useEffect(() => {
    if (!pendingScrollId) {
      return
    }
    const element = document.querySelector(`[data-widget-id="${CSS.escape(pendingScrollId)}"]`)
    if (element) {
      element.scrollIntoView({ behavior: 'smooth', block: 'center' })
      setPendingScrollId(null)
    }
  }, [pendingScrollId, editor.draftWidgets])

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
        const addedId = editor.addWidget({
          dashboardId: dashboard.id,
          widgetType: 'module',
          moduleId: selection.moduleId,
          title: selection.manifest.name,
          configJson: '{}',
          gridW: sizing.defaultW ?? 4,
          gridH: sizing.defaultH ?? 3
        })
        setPendingScrollId(addedId)
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
      const addedId = editor.addWidget({
        dashboardId: dashboard.id,
        widgetType: configWidgetType,
        title: title || null,
        configJson
      })
      setPendingScrollId(addedId)
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
    <div className="w-full min-w-0 max-w-[calc(100vw-1.5rem)] sm:max-w-full">
      <SiteHeaderLeading>
        <DashboardSwitcher
          currentId={activeDashboardId}
          dashboards={dashboards}
          isAdmin={isAdmin}
          onSelect={handleDashboardSelect}
        />
      </SiteHeaderLeading>
      <SiteHeaderActions>
        <div className="flex min-w-0 flex-wrap items-center justify-end gap-2">
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
      </SiteHeaderActions>

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
