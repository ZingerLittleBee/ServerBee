import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { DashboardCard } from '@/components/dashboard/dashboard-card'
import {
  WIDGET_PADDING_CLASS_NAME,
  WidgetHeader,
  WidgetMessage,
  WidgetSkeleton
} from '@/components/dashboard/widget-chrome'
import { LatencyChart } from '@/components/network/latency-chart'
import { useNetworkServerSummary } from '@/hooks/use-network-api'
import { useNetworkChartRecords } from '@/hooks/use-network-chart-records'
import { CHART_COLORS } from '@/lib/chart-colors'
import type { ServerMetrics } from '@/lib/server-catalog'
import { cn } from '@/lib/utils'
import { widgetRangeLabel } from '@/lib/widget-helpers'
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
    return <WidgetSkeleton />
  }

  const header = (
    <WidgetHeader
      aside={widgetRangeLabel(hours, t)}
      context={summary?.server_name}
      title={t('widgets.networkLatency.title', 'Network Latency')}
    />
  )

  if (records.length === 0) {
    return (
      <DashboardCard className={cn('flex h-full flex-col gap-3 bg-card', WIDGET_PADDING_CLASS_NAME)}>
        {header}
        <WidgetMessage title={t('widgets.networkLatency.empty.noData', 'No network probe data available')} />
      </DashboardCard>
    )
  }

  return (
    <DashboardCard className={cn('flex h-full flex-col gap-3 bg-card', WIDGET_PADDING_CLASS_NAME)}>
      {header}
      <div className="min-h-0 flex-1">
        <LatencyChart
          embedded
          hours={isRealtime ? 1 : hours}
          isRealtime={isRealtime}
          records={records}
          targets={chartTargets}
        />
      </div>
    </DashboardCard>
  )
}
