import { useQuery } from '@tanstack/react-query'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { api } from '@/lib/api-client'
import { filterByIds, formatRelativeTime } from '@/lib/widget-helpers'
import type { AlertListConfig } from '@/lib/widget-types'

interface AlertListWidgetProps {
  config: AlertListConfig
  servers: ServerMetrics[]
}

interface AlertEvent {
  count: number
  event_at: string
  resolved_at: string | null
  rule_id: string
  rule_name: string
  server_id: string
  server_name: string
  status: string
}

export function AlertListWidget({ config, servers }: AlertListWidgetProps) {
  const { t } = useTranslation('dashboard')
  const limit = config.max_items ?? 10
  const serverIds = config.server_ids

  const { data: events } = useQuery<AlertEvent[]>({
    queryKey: ['alert-events', limit],
    queryFn: () => api.get<AlertEvent[]>(`/api/alert-events?limit=${limit}`),
    refetchInterval: 60_000
  })

  const filtered = useMemo(() => filterByIds(events ?? [], serverIds, (e) => e.server_id), [events, serverIds])

  const serverNameMap = useMemo(() => {
    const map = new Map<string, string>()
    for (const s of servers) {
      map.set(s.id, s.name)
    }
    return map
  }, [servers])

  if (!events) {
    return (
      <div className="flex h-full flex-col rounded-lg border bg-card p-4">
        <h3 className="mb-3 font-semibold text-sm">{t('widgets.alertList.title')}</h3>
        <div className="flex flex-1 items-center justify-center text-muted-foreground text-xs">
          {t('states.loading')}
        </div>
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col rounded-lg border bg-card p-4">
      <h3 className="mb-3 font-semibold text-sm">{t('widgets.alertList.title')}</h3>
      <div className="flex flex-1 flex-col gap-1.5 overflow-auto">
        {filtered.map((event) => {
          const isFiring = event.status === 'firing'
          const serverName = serverNameMap.get(event.server_id) ?? event.server_name
          return (
            <div
              className="flex items-center gap-2 rounded-md px-2 py-1.5 text-xs hover:bg-muted/50"
              key={`${event.rule_id}-${event.server_id}-${event.status}`}
            >
              <span
                className={`inline-block size-2 shrink-0 rounded-full ${isFiring ? 'bg-red-500' : 'bg-green-500'}`}
              />
              <span className="min-w-0 flex-1 truncate">
                <span className="font-medium">{event.rule_name}</span>
                <span className="text-muted-foreground"> - {serverName}</span>
              </span>
              <span className="shrink-0 text-muted-foreground tabular-nums">{formatRelativeTime(event.event_at)}</span>
            </div>
          )
        })}
        {filtered.length === 0 && (
          <div className="flex flex-1 items-center justify-center text-muted-foreground text-xs">
            {t('widgets.alertList.empty.noEvents')}
          </div>
        )}
      </div>
    </div>
  )
}
