import { useQuery } from '@tanstack/react-query'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { MetricAreaPlot, type MetricAreaSeries } from '@/components/charts/metric-area-plot'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api-client'
import type { PingRecord } from '@/lib/api-schema'
import { formatDateTime } from '@/lib/format'

function formatLatency(value: number): string {
  return `${value.toFixed(1)}ms`
}

function formatClockTime(time: string): string {
  return new Date(time).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false })
}

function formatRecordTime(time: string): string {
  return formatDateTime(time, { hour12: false })
}

function createPingRecordWindow() {
  const now = new Date()
  return {
    from: new Date(now.getTime() - 24 * 3600 * 1000).toISOString(),
    to: now.toISOString()
  }
}

export function PingResultsChart({ taskId }: { taskId: string }) {
  const { t } = useTranslation('settings')
  const { from, to } = useMemo(createPingRecordWindow, [])
  const series = useMemo<MetricAreaSeries[]>(
    () => [{ dataKey: 'latency', label: t('ping.chart_latency'), color: 'var(--chart-4)' }],
    [t]
  )

  const { data: records, isLoading } = useQuery<PingRecord[]>({
    queryKey: ['ping-records', taskId, from, to],
    queryFn: () =>
      api.get<PingRecord[]>(
        `/api/ping-tasks/${taskId}/records?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`
      )
  })

  if (isLoading) {
    return <Skeleton className="h-48" />
  }

  if (!records || records.length === 0) {
    return <p className="py-4 text-center text-muted-foreground text-xs">{t('ping.no_records')}</p>
  }

  const chartData = records.map((record) => ({
    timestamp: record.time,
    latency: record.success ? record.latency : null
  }))

  const successfulRecords = records.filter((record) => record.success)
  const successRate = ((successfulRecords.length / records.length) * 100).toFixed(1)
  const avgLatency =
    successfulRecords.reduce((sum, record) => sum + record.latency, 0) / Math.max(1, successfulRecords.length)

  return (
    <div className="space-y-2">
      <div className="flex gap-4 text-muted-foreground text-xs">
        <span>{t('ping.success_rate', { rate: successRate })}</span>
        <span>{t('ping.avg_latency', { value: avgLatency.toFixed(1) })}</span>
        <span>{t('ping.record_count', { count: records.length })}</span>
      </div>
      <MetricAreaPlot
        ariaLabel={t('ping.chart_label')}
        className="h-[180px] w-full"
        data={chartData}
        formatTime={formatClockTime}
        formatTooltipLabel={formatRecordTime}
        formatValue={formatLatency}
        series={series}
        timeLabel={t('ping.chart_time')}
        yMarginLeft={48}
      />
    </div>
  )
}
