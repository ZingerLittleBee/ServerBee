import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { DashboardCard } from '@/components/dashboard/dashboard-card'
import { WidgetHeader, WidgetMessage, WidgetValue } from '@/components/dashboard/widget-chrome'
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

const STATUS_STYLES: Record<StatusKey, { dot: string; text: string }> = {
  healthy: { dot: 'bg-status-healthy', text: 'text-muted-foreground' },
  degraded: { dot: 'bg-status-warning', text: 'text-status-warning-text' },
  down: { dot: 'bg-status-danger', text: 'text-status-danger-text' },
  pending: { dot: 'bg-muted-foreground/50', text: 'text-muted-foreground' }
}
/** Problems first in the summary line; healthy is implied by the headline count. */
const ATTENTION_STATUSES: StatusKey[] = ['down', 'degraded', 'pending']

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

  if (!monitors) {
    return (
      <DashboardCard className="flex h-full flex-col bg-card p-5">
        <WidgetHeader title={t('widgets.serviceStatus.title')} />
        <WidgetMessage title={t('states.loading')} />
      </DashboardCard>
    )
  }

  const attention = ATTENTION_STATUSES.filter((status) => counts[status] > 0)

  return (
    <DashboardCard className="flex h-full flex-col bg-card p-5">
      <WidgetHeader title={t('widgets.serviceStatus.title')} />
      {filtered.length === 0 ? (
        <WidgetMessage title={t('widgets.serviceStatus.empty.noMonitors')} />
      ) : (
        <>
          <div className="mt-2 mb-2 flex min-w-0 items-baseline gap-2.5">
            <WidgetValue
              className="text-[1.375rem]"
              data-testid="service-status-healthy-count"
              formatted={`${counts.healthy} / ${filtered.length}`}
            />
            {attention.length === 0 ? (
              <span className="truncate text-sm text-status-healthy-text">{t('widgets.serviceStatus.allHealthy')}</span>
            ) : (
              <span className="truncate font-medium text-sm">
                {attention.map((status, index) => (
                  <span className={STATUS_STYLES[status].text} key={status}>
                    {index > 0 ? ' · ' : ''}
                    {t(`widgets.serviceStatus.summary.${status}`, { count: counts[status] })}
                  </span>
                ))}
              </span>
            )}
          </div>
          <ScrollArea className="min-h-0 flex-1">
            <ul className="flex flex-col">
              {filtered.map((monitor) => {
                const status = getStatus(monitor)
                const typeLabel = t(`service-monitors:monitorTypes.${monitor.monitor_type}`, {
                  defaultValue: monitor.monitor_type.toUpperCase()
                })
                const lastChecked = monitor.last_checked_at
                  ? formatRelativeTime(monitor.last_checked_at)
                  : t('widgets.serviceStatus.neverChecked')
                return (
                  <li className="border-border/60 border-t" key={monitor.id}>
                    <Link
                      className="-mx-2 flex items-center gap-3 rounded-lg px-2 py-2 transition-colors hover:bg-muted/50"
                      params={{ id: monitor.id }}
                      to="/service-monitors/$id"
                    >
                      <span
                        aria-hidden="true"
                        className={cn('size-2 shrink-0 rounded-full', STATUS_STYLES[status].dot)}
                      />
                      <span className="flex min-w-0 flex-1 flex-col">
                        <span className="truncate font-medium text-sm">{monitor.name}</span>
                        <span className="truncate text-muted-foreground text-xs">
                          {typeLabel} · {monitor.target}
                        </span>
                      </span>
                      <span className="flex shrink-0 flex-col items-end">
                        <span className={cn('font-medium text-xs', STATUS_STYLES[status].text)}>
                          {t(`widgets.serviceStatus.status.${status}`)}
                        </span>
                        <span className="text-muted-foreground text-xs tabular-nums">{lastChecked}</span>
                      </span>
                    </Link>
                  </li>
                )
              })}
            </ul>
          </ScrollArea>
        </>
      )}
    </DashboardCard>
  )
}
