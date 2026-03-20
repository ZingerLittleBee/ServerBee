import { useMemo } from 'react'
import { ServerCard } from '@/components/server/server-card'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import type { ServerCardsConfig } from '@/lib/widget-types'

interface ServerCardsWidgetProps {
  config: ServerCardsConfig
  servers: ServerMetrics[]
}

function filterServers(servers: ServerMetrics[], serverIds?: string[]): ServerMetrics[] {
  if (!serverIds || serverIds.length === 0) {
    return servers
  }
  const idSet = new Set(serverIds)
  return servers.filter((s) => idSet.has(s.id))
}

export function ServerCardsWidget({ config, servers }: ServerCardsWidgetProps) {
  const filtered = useMemo(() => filterServers(servers, config.server_ids), [servers, config.server_ids])
  const columns = config.columns ?? 3

  return (
    <div
      className="grid h-full gap-4 overflow-auto"
      style={{
        gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`
      }}
    >
      {filtered.map((server) => (
        <ServerCard key={server.id} server={server} />
      ))}
      {filtered.length === 0 && (
        <div className="col-span-full flex items-center justify-center py-8 text-muted-foreground text-sm">
          No servers to display
        </div>
      )}
    </div>
  )
}
