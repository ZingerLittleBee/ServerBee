import { useQueryClient } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { ServerCard } from '@/components/server/server-card'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { useServersWs } from '@/hooks/use-servers-ws'

export const Route = createFileRoute('/_authed/')({
  component: DashboardPage
})

function DashboardPage() {
  useServersWs()

  const queryClient = useQueryClient()
  const servers = queryClient.getQueryData<ServerMetrics[]>(['servers']) ?? []
  const onlineCount = servers.filter((s) => s.online).length

  return (
    <div>
      <div className="mb-6 flex items-center justify-between">
        <div>
          <h1 className="font-bold text-2xl">Dashboard</h1>
          <p className="text-muted-foreground text-sm">
            {onlineCount} of {servers.length} servers online
          </p>
        </div>
      </div>

      {servers.length === 0 ? (
        <div className="flex min-h-[300px] items-center justify-center rounded-lg border border-dashed">
          <div className="text-center">
            <p className="text-muted-foreground text-sm">No servers connected yet</p>
            <p className="mt-1 text-muted-foreground text-xs">
              Servers will appear here once they connect via the agent
            </p>
          </div>
        </div>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {servers.map((server) => (
            <ServerCard key={server.id} server={server} />
          ))}
        </div>
      )}
    </div>
  )
}
