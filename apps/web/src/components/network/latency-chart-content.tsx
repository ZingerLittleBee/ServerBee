import { useCallback, useId, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { Area } from '@/components/charts/area'
import { AreaChart } from '@/components/charts/area-chart'
import { formatFiniteChartValue, sampleChartRows } from '@/components/charts/chart-accessibility'
import { Grid } from '@/components/charts/grid'
import { ChartTooltip } from '@/components/charts/tooltip/chart-tooltip'
import { TooltipContent, type TooltipRow } from '@/components/charts/tooltip/tooltip-content'
import { XAxis } from '@/components/charts/x-axis'
import { YAxis } from '@/components/charts/y-axis'
import type { LatencyChartProps, TargetInfo } from '@/components/network/latency-chart'
import { formatDateTime } from '@/lib/format'
import type { NetworkProbeRecord } from '@/lib/network-types'

const BUCKET_MS = 60_000
const FUTURE_TOLERANCE_MS = 30_000

interface VisibleSeries {
  color: string
  dataKey: string
  id: string
  name: string
}

export function buildLatencyChartData(
  records: NetworkProbeRecord[],
  targets: TargetInfo[],
  nowMs = Date.now()
): Record<string, unknown>[] {
  const bucketMap = new Map<number, Record<string, unknown>>()
  const targetIndexById = new Map(targets.map((target, index) => [target.id, index]))

  for (const record of records) {
    const targetIndex = targetIndexById.get(record.target_id)
    if (targetIndex === undefined) {
      continue
    }

    const timestampMs = new Date(record.timestamp).getTime()
    if (!Number.isFinite(timestampMs) || timestampMs > nowMs + FUTURE_TOLERANCE_MS) {
      continue
    }

    const bucketKey = Math.floor(timestampMs / BUCKET_MS) * BUCKET_MS
    let bucket = bucketMap.get(bucketKey)
    if (!bucket) {
      bucket = { timestamp: new Date(bucketKey).toISOString() }
      bucketMap.set(bucketKey, bucket)
    }
    bucket[`target_${targetIndex}`] = record.avg_latency
  }

  return Array.from(bucketMap.entries())
    .sort(([left], [right]) => left - right)
    .map(([, bucket]) => bucket)
}

function formatTime24(date: Date): string {
  return date.toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false
  })
}

function formatTimeHM(date: Date): string {
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false })
}

function formatDateMD(date: Date): string {
  return `${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`
}

function formatDateTimeMDHM(timestamp: string): string {
  const date = new Date(timestamp)
  return `${formatDateMD(date)} ${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`
}

function formatLatency(value: unknown): string {
  return formatFiniteChartValue(value, (latency) => `${latency.toFixed(1)} ms`)
}

export function LatencyChartContent({
  records,
  targets,
  isRealtime = false,
  hours = 1,
  embedded = false
}: LatencyChartProps) {
  const { t } = useTranslation('network')
  const titleId = useId()
  const chartData = useMemo(() => buildLatencyChartData(records, targets), [records, targets])
  const accessibleRows = useMemo(() => sampleChartRows(chartData), [chartData])
  const visibleSeries = useMemo<VisibleSeries[]>(
    () =>
      targets.flatMap((target, index) =>
        target.visible
          ? [
              {
                color: target.color,
                dataKey: `target_${index}`,
                id: target.id,
                name: target.name
              }
            ]
          : []
      ),
    [targets]
  )
  const isExtendedRange = hours >= 168
  const axisFormatter = useCallback(
    (date: Date) => {
      if (isExtendedRange) {
        return formatDateMD(date)
      }
      return isRealtime ? formatTime24(date) : formatTimeHM(date)
    },
    [isExtendedRange, isRealtime]
  )
  const tooltipLabelFormatter = useCallback(
    (timestamp: string) =>
      isExtendedRange
        ? formatDateTimeMDHM(timestamp)
        : formatDateTime(timestamp, {
            month: 'short',
            day: 'numeric',
            hour: '2-digit',
            minute: '2-digit',
            second: '2-digit',
            hour12: false
          }),
    [isExtendedRange]
  )
  const tooltipRows = useCallback(
    (point: Record<string, unknown>): TooltipRow[] =>
      visibleSeries.flatMap((series) => {
        const value = point[series.dataKey]
        return typeof value === 'number' && Number.isFinite(value)
          ? [{ color: series.color, label: series.name, value: formatLatency(value) }]
          : []
      }),
    [visibleSeries]
  )

  if (chartData.length === 0) {
    if (embedded) {
      return (
        <div className="flex h-full items-center justify-center">
          <p className="text-muted-foreground text-sm">{t('latency_chart_no_data')}</p>
        </div>
      )
    }
    return (
      <div className="flex h-[300px] items-center justify-center rounded-lg border bg-card">
        <p className="text-muted-foreground text-sm">{t('latency_chart_no_data')}</p>
      </div>
    )
  }

  const title = t('latency_title')

  return (
    <figure
      aria-label={embedded ? title : undefined}
      aria-labelledby={embedded ? undefined : titleId}
      className={embedded ? 'h-full min-h-0 w-full' : 'rounded-lg border bg-card p-4'}
    >
      {embedded ? null : (
        <h3 className="mb-3 font-semibold text-sm" id={titleId}>
          {title}
        </h3>
      )}
      <div
        aria-hidden="true"
        className={embedded ? 'h-full min-h-0 w-full' : undefined}
        data-testid="bklit-latency-chart"
      >
        <AreaChart
          animationDuration={500}
          aspectRatio=""
          className={embedded ? 'h-full min-h-0 w-full' : 'h-[300px] w-full'}
          data={chartData}
          margin={{ left: 60, right: 16, top: 8, bottom: 36 }}
          xDataKey="timestamp"
          yDomainTweenDuration={200}
        >
          <Grid vertical={false} />
          <XAxis fadeOnHover={false} formatValue={axisFormatter} numTicks={5} />
          <YAxis formatValue={(value) => `${value.toFixed(0)} ms`} />
          <ChartTooltip
            content={({ point }) => (
              <TooltipContent rows={tooltipRows(point)} title={tooltipLabelFormatter(String(point.timestamp))} />
            )}
            showDatePill={false}
          />
          {visibleSeries.map((series) => (
            <Area
              dataKey={series.dataKey}
              fill="transparent"
              fillOpacity={0}
              key={series.id}
              stroke={series.color}
              strokeWidth={2}
            />
          ))}
        </AreaChart>
      </div>

      <table className="sr-only">
        <caption>{title}</caption>
        <thead>
          <tr>
            <th scope="col">{t('latency_time')}</th>
            {visibleSeries.map((series) => (
              <th key={series.id} scope="col">
                {series.name}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {accessibleRows.map((point) => (
            <tr key={String(point.timestamp)}>
              <td>{tooltipLabelFormatter(String(point.timestamp))}</td>
              {visibleSeries.map((series) => (
                <td key={series.id}>{formatLatency(point[series.dataKey])}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </figure>
  )
}
