import { useQuery } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { useState } from 'react'
import { DashboardEditorView } from '@/components/dashboard/dashboard-editor-view'
import { useAuth } from '@/hooks/use-auth'
import { useDashboard, useDashboards, useDefaultDashboard, useUpdateDashboard } from '@/hooks/use-dashboard'
import type { ServerMetrics } from '@/hooks/use-servers-ws'

export const Route = createFileRoute('/_authed/')({
  component: DashboardPage
})

function DashboardPage() {
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
  const updateDashboard = useUpdateDashboard()

  async function handleSave(widgets: Parameters<typeof updateDashboard.mutateAsync>[0]['widgets']) {
    if (!dashboard) {
      return
    }

    await updateDashboard.mutateAsync({ id: dashboard.id, widgets })
  }

  function handleDashboardSelect(id: string) {
    setSelectedId(id)
  }

  return (
    <DashboardEditorView
      dashboard={dashboard}
      dashboards={dashboards}
      isAdmin={isAdmin}
      isSaving={updateDashboard.isPending}
      key={dashboard?.id ?? 'no-dashboard'}
      onSave={handleSave}
      onSelectDashboard={handleDashboardSelect}
      servers={servers}
    />
  )
}
