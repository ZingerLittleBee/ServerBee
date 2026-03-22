import { useMemo } from 'react'
import { ServerCard } from '@/components/server/server-card'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { filterByIds } from '@/lib/widget-helpers'
import type { ServerCardsConfig } from '@/lib/widget-types'

interface ServerCardsWidgetProps {
  config: ServerCardsConfig
  servers: ServerMetrics[]
}

export function ServerCardsWidget({ config, servers }: ServerCardsWidgetProps) {
  const filtered = useMemo(() => filterByIds(servers, config.server_ids, (s) => s.id), [servers, config.server_ids])
  const maxColumns = config.columns ?? 3
  const columns = Math.min(maxColumns, filtered.length || 1)

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
