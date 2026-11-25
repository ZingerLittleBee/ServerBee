import { useQueries } from '@tanstack/react-query'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { DashboardCard } from '@/components/dashboard/dashboard-card'
import { WidgetHeader, WidgetMessage } from '@/components/dashboard/widget-chrome'
import { ScrollArea } from '@/components/ui/scroll-area'
import { UptimeTimeline } from '@/components/uptime/uptime-timeline'
import { api } from '@/lib/api-client'
import type { UptimeDailyEntry } from '@/lib/api-schema'
import type { ServerMetrics } from '@/lib/server-catalog'
import { cn } from '@/lib/utils'
import { computeAggregateUptime } from '@/lib/widget-helpers'
import type { UptimeTimelineConfig } from '@/lib/widget-types'

/** Matches the timeline's red threshold so the percentage and the cells agree. */
const DOWN_THRESHOLD = 95

function formatPct(pct: number | null, digits: number): string {
  return pct === null ? '\u2014' : `${pct.toFixed(digits)}%`
}

function pctClassName(pct: number | null): string {
  return pct !== null && pct < DOWN_THRESHOLD ? 'text-status-danger-text' : 'text-foreground'
}

interface UptimeTimelineWidgetProps {
  config: UptimeTimelineConfig
  servers: ServerMetrics[]
}

export function UptimeTimelineWidget({ config, servers }: UptimeTimelineWidgetProps) {
  const { t } = useTranslation('dashboard')
  // Empty selection means "all servers", matching the filterByIds semantics
  // used by the other server_ids widgets (server-cards, server-map, alert-list).
  const serverIds = useMemo(
    () => (config.server_ids?.length ? config.server_ids : servers.map((s) => s.id)),
    [config.server_ids, servers]
  )
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

  const title = t('widgets.uptimeTimeline.title')
  const rangeLabel = t('widgets.common.range.days', { count: days })

  if (serverIds.length === 0) {
    return (
      <DashboardCard className="flex h-full bg-card">
        <WidgetMessage title={t('widgets.common.empty.noServers')} />
      </DashboardCard>
    )
  }

  if (serverIds.length === 1) {
    const uptimeData = queries[0]?.data ?? []
    const pct = computeAggregateUptime(uptimeData)
    const name = serverNameMap.get(serverIds[0]) ?? serverIds[0]
    return (
      <DashboardCard className="h-full bg-card">
        <div className="flex flex-col gap-3 p-5" data-measure>
          <WidgetHeader
            aside={
              <span className={cn('font-semibold text-sm tabular-nums', pctClassName(pct))}>{formatPct(pct, 2)}</span>
            }
            context={rangeLabel}
            title={name}
          />
          {isLoading ? (
            <WidgetMessage className="h-[72px]" title={t('states.loading')} />
          ) : (
            <UptimeTimeline days={uptimeData} rangeDays={days} rounded showLabels />
          )}
        </div>
      </DashboardCard>
    )
  }

  if (isLoading) {
    return (
      <DashboardCard className="flex h-full flex-col bg-card p-5">
        <WidgetHeader context={rangeLabel} title={title} />
        <WidgetMessage title={t('states.loading')} />
      </DashboardCard>
    )
  }

  return (
    <DashboardCard className="flex h-full flex-col bg-card p-5">
      <WidgetHeader context={rangeLabel} title={title} />
      <ScrollArea className="mt-3 min-h-0 flex-1" contentClassName="space-y-3.5">
        {serverIds.map((id, i) => {
          const uptimeData = queries[i]?.data ?? []
          const pct = computeAggregateUptime(uptimeData)
          const name = serverNameMap.get(id) ?? id
          return (
            <div key={id}>
              <div className="mb-1.5 flex items-baseline justify-between gap-3">
                <span className="truncate font-medium text-sm">{name}</span>
                <span className={cn('shrink-0 font-semibold text-sm tabular-nums', pctClassName(pct))}>
                  {formatPct(pct, 2)}
                </span>
              </div>
              <UptimeTimeline days={uptimeData} height={18} rangeDays={days} rounded />
            </div>
          )
        })}
      </ScrollArea>
    </DashboardCard>
  )
}
