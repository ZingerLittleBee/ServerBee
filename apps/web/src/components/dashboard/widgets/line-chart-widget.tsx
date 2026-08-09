import { lazy, Suspense, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { Skeleton } from '@/components/ui/skeleton'
import { useServerRecords } from '@/hooks/use-api'
import type { ServerMetrics } from '@/lib/server-catalog'
import { formatBytes } from '@/lib/utils'
import { DEFAULT_WIDGET_CHART_COLOR, resolveWidgetColor } from '@/lib/widget-color'
import { extractRecordMetric, formatChartTime, isNetworkMetric, METRIC_UNITS, metricLabel } from '@/lib/widget-helpers'
import type { LineChartConfig } from '@/lib/widget-types'

interface LineChartWidgetProps {
  config: LineChartConfig
  servers: ServerMetrics[]
  title?: string | null
}

const DEFAULT_HOURS = 24
const DEFAULT_INTERVAL = 'raw'

const LazyMetricAreaPlot = lazy(() =>
  import('@/components/charts/metric-area-plot').then((module) => ({
    default: module.MetricAreaPlot
  }))
)

export function LineChartWidget({ config, servers, title }: LineChartWidgetProps) {
  const { t } = useTranslation('dashboard')
  const server_id = config.server_id ?? ''
  const { metric } = config
  const hours = config.hours ?? DEFAULT_HOURS
  const interval = config.interval ?? DEFAULT_INTERVAL

  const { data: records, isLoading } = useServerRecords(server_id, hours, interval)

  const server = useMemo(() => servers.find((s) => s.id === server_id), [servers, server_id])

  const chartData = useMemo(() => {
    if (!records) {
      return []
    }
    return records.map((r) => ({
      timestamp: r.time,
      value: extractRecordMetric(r, metric, server)
    }))
  }, [records, metric, server])

  const label = metricLabel(metric, t)
  const unit = METRIC_UNITS[metric] ?? ''
  const serverName = server?.name ?? t('metricCard.unknownServer')
  const isNetwork = isNetworkMetric(metric)
  const seriesColor = resolveWidgetColor(config.color, DEFAULT_WIDGET_CHART_COLOR)

  if (isLoading) {
    return (
      <div className="flex h-full flex-col rounded-lg border bg-card p-4">
        <Skeleton className="mb-2 h-4 w-32" />
        <Skeleton className="flex-1" />
      </div>
    )
  }

  return (
    <div className="flex h-full min-w-0 flex-col rounded-lg border bg-card p-4">
      <div className="mb-3">
        <h3 className="font-semibold text-sm">{title ?? label}</h3>
        <p className="text-muted-foreground text-xs">{serverName}</p>
      </div>
      <div className="min-h-0 min-w-0 flex-1">
        <Suspense fallback={<Skeleton className="h-full min-h-0 w-full" />}>
          <LazyMetricAreaPlot
            ariaLabel={title ?? label}
            className="h-full min-h-0 w-full"
            data={chartData}
            formatTime={formatChartTime}
            formatTooltipLabel={formatChartTime}
            formatValue={(value) => (isNetwork ? `${formatBytes(value)}/s` : `${value.toFixed(1)}${unit}`)}
            formatYAxisValue={isNetwork ? formatBytes : (value) => String(value)}
            series={[{ dataKey: 'value', label, color: seriesColor }]}
            timeLabel={t('chart_time')}
            yMarginLeft={isNetwork ? 68 : 52}
          />
        </Suspense>
      </div>
    </div>
  )
}
