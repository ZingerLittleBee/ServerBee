import { useQuery } from '@tanstack/react-query'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { StackedBarPlot, type StackedBarSeries } from '@/components/charts/stacked-bar-plot'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api-client'
import type { ServerMetrics } from '@/lib/server-catalog'
import { formatBytes } from '@/lib/utils'
import type { TrafficBarConfig } from '@/lib/widget-types'

interface TrafficBarWidgetProps {
  config: TrafficBarConfig
  servers: ServerMetrics[]
}

interface DailyTrafficItem extends Record<string, unknown> {
  bytes_in: number
  bytes_out: number
  date: string
}

interface ServerTrafficResponse {
  daily: DailyTrafficItem[]
}

const DEFAULT_DAYS = 30

function useTrafficSeries(t: (key: string) => string): StackedBarSeries[] {
  return [
    { dataKey: 'bytes_in', label: t('widgets.trafficBar.legend.inbound'), color: 'var(--chart-1)' },
    { dataKey: 'bytes_out', label: t('widgets.trafficBar.legend.outbound'), color: 'var(--chart-2)' }
  ]
}

function formatDayTick(date: string): string {
  return date.slice(5)
}

function hoursToDays(hours?: number): number {
  if (!hours || hours <= 0) {
    return DEFAULT_DAYS
  }
  return Math.max(1, Math.round(hours / 24))
}

export function TrafficBarWidget({ config, servers }: TrafficBarWidgetProps) {
  const { t } = useTranslation('dashboard')
  const trafficSeries = useTrafficSeries(t)
  const { server_id } = config
  const days = hoursToDays(config.hours)
  const hasServerId = server_id != null && server_id.length > 0

  // Per-server daily traffic (billing-cycle breakdown), client-sliced to `days`
  const { data: serverResponse, isLoading: serverLoading } = useQuery<ServerTrafficResponse>({
    queryKey: ['servers', server_id, 'traffic'],
    queryFn: () => api.get<ServerTrafficResponse>(`/api/servers/${server_id}/traffic`),
    staleTime: 60_000,
    enabled: hasServerId
  })

  const serverDaily = useMemo(() => (serverResponse?.daily ?? []).slice(-days), [serverResponse, days])

  // Global overview daily traffic
  const { data: globalDaily, isLoading: globalLoading } = useQuery<DailyTrafficItem[]>({
    queryKey: ['traffic', 'overview', 'daily', days],
    queryFn: () => api.get<DailyTrafficItem[]>(`/api/traffic/overview/daily?days=${days}`),
    staleTime: 60_000,
    enabled: !hasServerId
  })

  const isLoading = hasServerId ? serverLoading : globalLoading
  const data = hasServerId ? serverDaily : globalDaily

  const serverName = useMemo(() => {
    if (!hasServerId) {
      return t('widgets.common.placeholders.allServers')
    }
    return servers.find((s) => s.id === server_id)?.name ?? t('unknown')
  }, [hasServerId, server_id, servers, t])

  if (isLoading) {
    return (
      <div className="flex h-full flex-col rounded-lg border bg-card p-4">
        <Skeleton className="mb-2 h-4 w-32" />
        <Skeleton className="flex-1" />
      </div>
    )
  }

  if (!data || data.length === 0) {
    return (
      <div className="flex h-full flex-col rounded-lg border bg-card p-4">
        <h3 className="mb-3 font-semibold text-sm">{t('widgets.trafficBar.title')}</h3>
        <p className="text-muted-foreground text-xs">{serverName}</p>
        <div className="flex flex-1 items-center justify-center text-muted-foreground text-sm">
          {t('widgets.trafficBar.empty.noData')}
        </div>
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col rounded-lg border bg-card p-4">
      <div className="mb-3">
        <h3 className="font-semibold text-sm">{t('widgets.trafficBar.title')}</h3>
        <p className="text-muted-foreground text-xs">{serverName}</p>
      </div>
      <div className="min-h-0 flex-1">
        <StackedBarPlot
          ariaLabel={t('widgets.trafficBar.title')}
          categoryKey="date"
          categoryLabel={t('chart_date')}
          className="h-full"
          data={data}
          formatCategory={formatDayTick}
          formatTooltipLabel={(date) => date}
          formatValue={formatBytes}
          series={trafficSeries}
        />
      </div>
    </div>
  )
}
