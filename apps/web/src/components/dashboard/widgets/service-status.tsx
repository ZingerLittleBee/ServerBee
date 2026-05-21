import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { ScrollArea } from '@/components/ui/scroll-area'
import { api } from '@/lib/api-client'
import { cn } from '@/lib/utils'
import { formatRelativeTime } from '@/lib/widget-helpers'
import type { ServiceStatusConfig } from '@/lib/widget-types'

interface ServiceMonitor {
  config_json: string
  consecutive_failures: number
  created_at: string
  enabled: boolean
  id: string
  interval: number
  last_checked_at: string | null
  last_status: boolean | null
  monitor_type: string
  name: string
  notification_group_id: string | null
  retry_count: number
  server_ids_json: string | null
  target: string
  updated_at: string
}

type StatusKey = 'healthy' | 'degraded' | 'down' | 'pending'

interface ServiceStatusWidgetProps {
  config: ServiceStatusConfig
}

const STATUS_STYLES: Record<StatusKey, { dot: string; pill: string }> = {
  healthy: { dot: 'bg-green-500', pill: 'bg-green-500/15 text-green-600 dark:text-green-400' },
  degraded: { dot: 'bg-yellow-500', pill: 'bg-yellow-500/15 text-yellow-600 dark:text-yellow-400' },
  down: { dot: 'bg-red-500', pill: 'bg-red-500/15 text-red-600 dark:text-red-400' },
  pending: { dot: 'bg-gray-400', pill: 'bg-muted text-muted-foreground' }
}

function getStatus(monitor: ServiceMonitor): StatusKey {
  if (monitor.last_status === null) {
    return 'pending'
  }
  if (monitor.last_status === true) {
    return monitor.consecutive_failures > 0 ? 'degraded' : 'healthy'
  }
  return 'down'
}

export function ServiceStatusWidget({ config }: ServiceStatusWidgetProps) {
  const { t } = useTranslation(['dashboard', 'service-monitors'])
  const monitorIds = config.monitor_ids

  const { data: monitors } = useQuery<ServiceMonitor[]>({
    queryKey: ['service-monitors'],
    queryFn: () => api.get<ServiceMonitor[]>('/api/service-monitors'),
    refetchInterval: 60_000
  })

  const filtered = useMemo(() => {
    if (!monitors) {
      return []
    }
    if (!monitorIds || monitorIds.length === 0) {
      return monitors.filter((m) => m.enabled)
    }
    const idSet = new Set(monitorIds)
    return monitors.filter((m) => idSet.has(m.id))
  }, [monitors, monitorIds])

  const counts = useMemo(() => {
    const acc: Record<StatusKey, number> = { healthy: 0, degraded: 0, down: 0, pending: 0 }
    for (const m of filtered) {
      acc[getStatus(m)] += 1
    }
    return acc
  }, [filtered])

  const orderedStatuses: StatusKey[] = ['down', 'degraded', 'healthy', 'pending']

  if (!monitors) {
    return (
      <div className="flex h-full flex-col rounded-lg border bg-card p-4">
        <h3 className="mb-3 font-semibold text-sm">{t('widgets.serviceStatus.title')}</h3>
        <div className="flex flex-1 items-center justify-center text-muted-foreground text-xs">
          {t('states.loading')}
        </div>
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col rounded-lg border bg-card p-4">
      <div className="mb-3 flex items-center justify-between gap-2">
        <h3 className="font-semibold text-sm">{t('widgets.serviceStatus.title')}</h3>
        {filtered.length > 0 && (
          <div className="flex flex-wrap items-center gap-1.5">
            {orderedStatuses.map((status) =>
              counts[status] > 0 ? (
                <span
                  className={cn(
                    'inline-flex items-center gap-1 rounded-full px-2 py-0.5 font-medium text-xs tabular-nums',
                    STATUS_STYLES[status].pill
                  )}
                  key={status}
                >
                  <span className={cn('size-1.5 rounded-full', STATUS_STYLES[status].dot)} />
                  {t(`widgets.serviceStatus.summary.${status}`, { count: counts[status] })}
                </span>
              ) : null
            )}
          </div>
        )}
      </div>
      {filtered.length === 0 ? (
        <div className="flex flex-1 items-center justify-center text-muted-foreground text-xs">
          {t('widgets.serviceStatus.empty.noMonitors')}
        </div>
      ) : (
        <ScrollArea className="flex-1">
          <ul className="flex flex-col gap-1">
            {filtered.map((monitor) => {
              const status = getStatus(monitor)
              const typeLabel = t(`service-monitors:monitorTypes.${monitor.monitor_type}`, {
                defaultValue: monitor.monitor_type.toUpperCase()
              })
              const lastChecked = monitor.last_checked_at
                ? formatRelativeTime(monitor.last_checked_at)
                : t('widgets.serviceStatus.neverChecked')
              return (
                <li key={monitor.id}>
                  <Link
                    className="flex items-center gap-2 rounded-md px-2 py-1.5 text-xs hover:bg-muted/50"
                    params={{ id: monitor.id }}
                    to="/service-monitors/$id"
                  >
                    <span className={cn('inline-block size-2 shrink-0 rounded-full', STATUS_STYLES[status].dot)} />
                    <span className="min-w-0 flex-1 truncate font-medium">{monitor.name}</span>
                    <span className="shrink-0 rounded bg-muted px-1.5 py-0.5 font-medium text-[10px] text-muted-foreground uppercase">
                      {typeLabel}
                    </span>
                    <span className="hidden min-w-0 max-w-[40%] shrink truncate text-muted-foreground sm:inline">
                      {monitor.target}
                    </span>
                    <span className="shrink-0 text-muted-foreground tabular-nums">{lastChecked}</span>
                  </Link>
                </li>
              )
            })}
          </ul>
        </ScrollArea>
      )}
    </div>
  )
}
