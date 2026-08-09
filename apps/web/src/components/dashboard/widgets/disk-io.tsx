import { lazy, Suspense, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { Skeleton } from '@/components/ui/skeleton'
import { useServerRecords } from '@/hooks/use-api'
import { buildMergedDiskIoSeries } from '@/lib/disk-io'
import type { ServerMetrics } from '@/lib/server-catalog'
import { formatSpeed } from '@/lib/utils'
import {
  DEFAULT_WIDGET_CHART_COLOR,
  DEFAULT_WIDGET_CHART_COLOR_SECONDARY,
  resolveWidgetColor
} from '@/lib/widget-color'
import { formatChartTime } from '@/lib/widget-helpers'
import type { DiskIoConfig } from '@/lib/widget-types'

interface DiskIoWidgetProps {
  config: DiskIoConfig
  servers: ServerMetrics[]
}

const DEFAULT_HOURS = 24
const DEFAULT_INTERVAL = 'raw'

const LazyMetricLinePlot = lazy(() =>
  import('@/components/charts/metric-line-plot').then((module) => ({
    default: module.MetricLinePlot
  }))
)

export function DiskIoWidget({ config, servers }: DiskIoWidgetProps) {
  const { t } = useTranslation('dashboard')
  const server_id = config.server_id ?? ''
  const hours = config.hours ?? DEFAULT_HOURS
  const interval = config.interval ?? DEFAULT_INTERVAL

  const { data: records, isLoading } = useServerRecords(server_id, hours, interval)

  const server = useMemo(() => servers.find((s) => s.id === server_id), [servers, server_id])

  const chartData = useMemo(() => {
    if (!records) {
      return []
    }
    return buildMergedDiskIoSeries(records)
  }, [records])

  const serverName = server?.name ?? t('metricCard.unknownServer')
  const chartSeries = useMemo(
    () => [
      {
        dataKey: 'read_bytes_per_sec',
        label: t('widgets.diskIo.legend.read'),
        color: resolveWidgetColor(config.color, DEFAULT_WIDGET_CHART_COLOR)
      },
      {
        dataKey: 'write_bytes_per_sec',
        label: t('widgets.diskIo.legend.write'),
        color: resolveWidgetColor(config.color_secondary, DEFAULT_WIDGET_CHART_COLOR_SECONDARY)
      }
    ],
    [config.color, config.color_secondary, t]
  )

  if (isLoading) {
    return (
      <div className="flex h-full flex-col rounded-lg border bg-card p-4">
        <Skeleton className="mb-2 h-4 w-32" />
        <Skeleton className="flex-1" />
      </div>
    )
  }

  if (chartData.length === 0) {
    return (
      <div className="flex h-full flex-col rounded-lg border bg-card p-4">
        <h3 className="mb-3 font-semibold text-sm">{t('widgets.diskIo.title')}</h3>
        <p className="text-muted-foreground text-xs">{serverName}</p>
        <div className="flex flex-1 items-center justify-center text-muted-foreground text-sm">
          {t('widgets.diskIo.empty.noData')}
        </div>
      </div>
    )
  }

  return (
    <div className="flex h-full min-w-0 flex-col rounded-lg border bg-card p-4">
      <div className="mb-3">
        <h3 className="font-semibold text-sm">{t('widgets.diskIo.title')}</h3>
        <p className="text-muted-foreground text-xs">{serverName}</p>
      </div>
      <div className="min-h-0 min-w-0 flex-1">
        <Suspense fallback={<Skeleton className="h-full min-h-0 w-full" />}>
          <LazyMetricLinePlot
            ariaLabel={t('widgets.diskIo.title')}
            className="h-full min-h-0 w-full"
            data={chartData}
            formatTime={formatChartTime}
            formatTooltipLabel={formatChartTime}
            formatValue={formatSpeed}
            formatYAxisValue={formatSpeed}
            series={chartSeries}
            timeLabel={t('chart_time')}
          />
        </Suspense>
      </div>
    </div>
  )
}
