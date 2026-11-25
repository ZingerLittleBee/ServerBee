import { useQuery } from '@tanstack/react-query'
import { CircleCheck } from 'lucide-react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { DashboardCard } from '@/components/dashboard/dashboard-card'
import { WidgetHeader, WidgetMessage } from '@/components/dashboard/widget-chrome'
import { ScrollArea } from '@/components/ui/scroll-area'
import { api } from '@/lib/api-client'
import type { ServerMetrics } from '@/lib/server-catalog'
import { cn } from '@/lib/utils'
import { filterByIds, formatRelativeTime } from '@/lib/widget-helpers'
import type { AlertListConfig } from '@/lib/widget-types'

interface AlertListWidgetProps {
  config: AlertListConfig
  servers: ServerMetrics[]
}

interface AlertEvent {
  alert_key?: string
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
      <DashboardCard className="flex h-full flex-col bg-card p-5">
        <WidgetHeader title={t('widgets.alertList.title')} />
        <WidgetMessage title={t('states.loading')} />
      </DashboardCard>
    )
  }

  const firingCount = filtered.filter((event) => event.status === 'firing').length

  return (
    <DashboardCard className="flex h-full flex-col bg-card p-5">
      <WidgetHeader
        aside={
          firingCount > 0 ? (
            <span className="font-medium text-status-danger-text">
              {t('widgets.alertList.firingCount', { count: firingCount })}
            </span>
          ) : null
        }
        title={t('widgets.alertList.title')}
      />
      {filtered.length === 0 ? (
        <WidgetMessage
          description={t('widgets.alertList.empty.noEvents')}
          icon={<CircleCheck aria-hidden="true" className="size-6 text-status-healthy" />}
          title={t('widgets.alertList.empty.allClear')}
        />
      ) : (
        <ScrollArea className="mt-2 min-h-0 flex-1">
          <ul className="flex flex-col">
            {filtered.map((event) => {
              const isFiring = event.status === 'firing'
              const isResolved = event.status === 'resolved'
              const statusLabel = t(`widgets.alertList.status.${event.status}`, { defaultValue: event.status })
              let statusColor = 'bg-muted-foreground/50'
              if (isFiring) {
                statusColor = 'bg-status-danger'
              } else if (isResolved) {
                statusColor = 'bg-status-healthy'
              }
              const serverName = serverNameMap.get(event.server_id) ?? event.server_name
              return (
                <li
                  className="flex items-center gap-3 border-border/60 border-t py-2"
                  key={event.alert_key ?? `${event.rule_id}-${event.server_id}-${event.event_at}-${event.status}`}
                >
                  <span
                    aria-label={statusLabel}
                    className={cn('inline-block size-2 shrink-0 rounded-full', statusColor)}
                    role="img"
                    title={statusLabel}
                  />
                  <span className="flex min-w-0 flex-1 flex-col">
                    <span className={cn('truncate font-medium text-sm', !isFiring && 'text-foreground/80')}>
                      {event.rule_name}
                    </span>
                    <span className="truncate text-muted-foreground text-xs">{serverName}</span>
                  </span>
                  <span className="shrink-0 text-muted-foreground text-xs tabular-nums">
                    {formatRelativeTime(event.event_at)}
                  </span>
                </li>
              )
            })}
          </ul>
        </ScrollArea>
      )}
    </DashboardCard>
  )
}
