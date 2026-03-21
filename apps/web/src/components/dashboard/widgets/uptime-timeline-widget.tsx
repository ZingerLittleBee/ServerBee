import { useQueries } from '@tanstack/react-query'
import { useMemo } from 'react'
import { UptimeTimeline } from '@/components/uptime/uptime-timeline'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { api } from '@/lib/api-client'
import type { UptimeDailyEntry } from '@/lib/api-schema'
import { computeAggregateUptime } from '@/lib/widget-helpers'
import type { UptimeTimelineConfig } from '@/lib/widget-types'

interface UptimeTimelineWidgetProps {
  config: UptimeTimelineConfig
  servers: ServerMetrics[]
}

export function UptimeTimelineWidget({ config, servers }: UptimeTimelineWidgetProps) {
  const serverIds = config.server_ids ?? []
  const days = config.days ?? 90

  const queries = useQueries({
    queries: serverIds.map((id) => ({
      queryKey: ['servers', id, 'uptime-daily', days],
      queryFn: () => api.get<UptimeDailyEntry[]>(`/api/servers/${id}/uptime-daily?days=${days}`),
      staleTime: 300_000
    }))
  })

  const serverNameMap = useMemo(() => {
    const map = new Map<string, string>()
    for (const s of servers) {
      map.set(s.id, s.name)
    }
    return map
  }, [servers])

  const isLoading = queries.some((q) => q.isLoading)

  if (serverIds.length === 0) {
    return (
      <div className="flex h-full items-center justify-center rounded-lg border bg-card text-muted-foreground text-sm">
        No servers configured
      </div>
    )
  }

  if (isLoading) {
    return (
      <div className="flex h-full flex-col rounded-lg border bg-card p-4">
        <h3 className="mb-3 font-semibold text-sm">Uptime Timeline</h3>
        <div className="flex flex-1 items-center justify-center text-muted-foreground text-xs">Loading...</div>
      </div>
    )
  }

  if (serverIds.length === 1) {
    const uptimeData = queries[0]?.data ?? []
    const pct = computeAggregateUptime(uptimeData)
    const name = serverNameMap.get(serverIds[0]) ?? serverIds[0]
    return (
      <div className="flex h-full flex-col rounded-lg border bg-card p-4">
        <div className="mb-3 flex items-center justify-between">
          <h3 className="font-semibold text-sm">{name}</h3>
          <span className="font-medium text-sm">{pct !== null ? `${pct.toFixed(2)}%` : '\u2014'}</span>
        </div>
        <div className="flex flex-1 items-end">
          <UptimeTimeline days={uptimeData} rangeDays={days} showLabels showLegend />
        </div>
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col rounded-lg border bg-card p-4">
      <h3 className="mb-3 font-semibold text-sm">Uptime Timeline</h3>
      <div className="flex-1 space-y-3 overflow-auto">
        {serverIds.map((id, i) => {
          const uptimeData = queries[i]?.data ?? []
          const pct = computeAggregateUptime(uptimeData)
          const name = serverNameMap.get(id) ?? id
          return (
            <div key={id}>
              <div className="mb-1 flex items-center justify-between">
                <span className="truncate text-xs">{name}</span>
                <span className="text-muted-foreground text-xs">{pct !== null ? `${pct.toFixed(1)}%` : '\u2014'}</span>
              </div>
              <UptimeTimeline days={uptimeData} height={20} rangeDays={days} />
            </div>
          )
        })}
      </div>
    </div>
  )
}
