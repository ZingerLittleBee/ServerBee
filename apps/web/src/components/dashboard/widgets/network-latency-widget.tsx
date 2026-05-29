import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { LatencyChart } from '@/components/network/latency-chart'
import { Skeleton } from '@/components/ui/skeleton'
import { useNetworkServerSummary } from '@/hooks/use-network-api'
import { useNetworkChartRecords } from '@/hooks/use-network-chart-records'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { CHART_COLORS } from '@/lib/chart-colors'
import type { NetworkLatencyConfig } from '@/lib/widget-types'

interface NetworkLatencyWidgetProps {
  config: NetworkLatencyConfig
  servers: ServerMetrics[]
}

export function NetworkLatencyWidget({ config }: NetworkLatencyWidgetProps) {
  const { t } = useTranslation('dashboard')
  const serverId = config.server_id ?? ''
  const hours = config.hours ?? 24
  const isRealtime = hours === 0

  const { records, isLoading: recordsLoading } = useNetworkChartRecords(serverId, hours)
  const { data: summary, isLoading: summaryLoading } = useNetworkServerSummary(serverId)

  const chartTargets = useMemo(
    () =>
      (summary?.targets ?? []).map((target, i) => ({
        id: target.target_id,
        name: target.target_name,
        color: CHART_COLORS[i % CHART_COLORS.length],
        visible: true
      })),
    [summary]
  )

  // Wait for both the records and the summary (which supplies chart targets) so
  // we render a skeleton instead of flashing the empty state or an axis-only chart.
  if (recordsLoading || summaryLoading) {
    return (
      <div className="flex h-full flex-col gap-2 rounded-lg border bg-card p-4">
        <Skeleton className="h-4 w-32" />
        <Skeleton className="flex-1" />
      </div>
    )
  }

  if (records.length === 0) {
    return (
      <div className="flex h-full flex-col rounded-lg border bg-card p-4">
        <h3 className="mb-1 font-semibold text-sm">{t('widgets.networkLatency.title', 'Network Latency')}</h3>
        <div className="flex flex-1 items-center justify-center text-muted-foreground text-sm">
          {t('widgets.networkLatency.empty.noData', 'No network probe data available')}
        </div>
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col rounded-lg border bg-card p-4">
      <div className="mb-2">
        <h3 className="font-semibold text-sm">{t('widgets.networkLatency.title', 'Network Latency')}</h3>
        <p className="text-muted-foreground text-xs">{summary?.server_name}</p>
      </div>
      <div className="min-h-0 flex-1">
        <LatencyChart
          embedded
          hours={isRealtime ? 1 : hours}
          isRealtime={isRealtime}
          records={records}
          targets={chartTargets}
        />
      </div>
    </div>
  )
}
