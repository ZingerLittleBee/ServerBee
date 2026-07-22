import { createFileRoute } from '@tanstack/react-router'
import { useMemo, useState } from 'react'
import { DashboardEditorView } from '@/components/dashboard/dashboard-editor-view'
import { useAuth } from '@/hooks/use-auth'
import { useDashboard, useDashboards, useDefaultDashboard, useUpdateDashboard } from '@/hooks/use-dashboard'
import { withMockServers } from '@/lib/dev-mock-servers'
import { useLiveServers } from '@/lib/server-catalog'

export const Route = createFileRoute('/_authed/')({
  component: DashboardPage
})

const SELECTED_DASHBOARD_KEY = 'serverbee.dashboard.selected'

function readStoredDashboardId(): string | null {
  try {
    return localStorage.getItem(SELECTED_DASHBOARD_KEY)
  } catch {
    return null
  }
}

function storeDashboardId(id: string) {
  try {
    localStorage.setItem(SELECTED_DASHBOARD_KEY, id)
  } catch {
    // Storage unavailable (private mode, quota); selection just won't persist.
  }
}

export function DashboardPage() {
  const { user } = useAuth()
  const isAdmin = user?.role === 'admin'

  const { data: rawServers = [] } = useLiveServers()
  const servers = useMemo(() => withMockServers(rawServers), [rawServers])

  const { data: dashboards = [], isSuccess: dashboardsLoaded } = useDashboards()
  const { data: defaultDashboard } = useDefaultDashboard()

  const [rawSelectedId, setRawSelectedId] = useState<string | null>(readStoredDashboardId)
  // A stored id may reference a dashboard deleted since (possibly by another
  // session); once the list is loaded, unknown ids fall back to the default.
  const selectedId =
    rawSelectedId && (!dashboardsLoaded || dashboards.some((d) => d.id === rawSelectedId)) ? rawSelectedId : null
  const activeId = selectedId ?? defaultDashboard?.id ?? ''
  const { data: activeDashboard } = useDashboard(activeId)

  const dashboard = selectedId ? activeDashboard : (activeDashboard ?? defaultDashboard)
  const updateDashboard = useUpdateDashboard()

  async function handleSave(widgets: Parameters<typeof updateDashboard.mutateAsync>[0]['widgets']) {
    if (!dashboard) {
      return
    }

    await updateDashboard.mutateAsync({ id: dashboard.id, widgets })
  }

  function handleDashboardSelect(id: string) {
    setRawSelectedId(id)
    storeDashboardId(id)
  }

  return (
    <DashboardEditorView
      activeDashboardId={activeId}
      dashboard={dashboard}
      dashboards={dashboards}
      isAdmin={isAdmin}
      isSaving={updateDashboard.isPending}
      key={activeId || 'no-dashboard'}
      onSave={handleSave}
      onSelectDashboard={handleDashboardSelect}
      servers={servers}
    />
  )
}
