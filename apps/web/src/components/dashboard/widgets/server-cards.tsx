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

  return (
    // data-measure: natural content height (grows with the number of cards),
    // measured by the grid to size the cell so the widget is never height-capped
    // or scrolled — it always hugs exactly as many rows of cards as it renders.
    <div
      className="grid content-start gap-4"
      data-measure
      style={{
        gridTemplateColumns: 'repeat(auto-fill, minmax(320px, 1fr))'
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
