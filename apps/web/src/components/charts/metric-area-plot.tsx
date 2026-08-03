import { useCallback, useMemo } from 'react'
import { cn } from '@/lib/utils'
import { Area } from './area'
import { AreaChart } from './area-chart'
import { Grid } from './grid'
import { ChartTooltip } from './tooltip/chart-tooltip'
import { TooltipContent, type TooltipRow } from './tooltip/tooltip-content'
import { XAxis } from './x-axis'
import { YAxis } from './y-axis'
import type { YDomain } from './y-domain-utils'

const MAX_ACCESSIBLE_ROWS = 50

export interface MetricAreaPlotProps {
  ariaLabel: string
  className: string
  color: string
  data: Record<string, unknown>[]
  dataKey: string
  formatTime?: (time: string) => string
  formatTooltipLabel?: (time: string) => string
  formatValue: (value: number) => string
  formatYAxisValue?: (value: number) => string
  timeLabel: string
  valueLabel: string
  yDomain?: YDomain
  yMarginLeft?: number
}

function defaultFormatTime(time: string): string {
  return new Date(time).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false })
}

function sampleRows(data: Record<string, unknown>[]): Record<string, unknown>[] {
  if (data.length <= MAX_ACCESSIBLE_ROWS) {
    return data
  }

  const lastIndex = data.length - 1
  const indices = Array.from({ length: MAX_ACCESSIBLE_ROWS }, (_, index) =>
    Math.round((index / (MAX_ACCESSIBLE_ROWS - 1)) * lastIndex)
  )
  return [...new Set(indices)].flatMap((index) => (data[index] ? [data[index]] : []))
}

function formatFiniteValue(value: unknown, formatter: (value: number) => string): string {
  return typeof value === 'number' && Number.isFinite(value) ? formatter(value) : '--'
}

export function MetricAreaPlot({
  ariaLabel,
  className,
  color,
  data,
  dataKey,
  formatTime = defaultFormatTime,
  formatTooltipLabel = formatTime,
  formatValue,
  formatYAxisValue,
  timeLabel,
  valueLabel,
  yDomain,
  yMarginLeft = 52
}: MetricAreaPlotProps) {
  const accessibleRows = useMemo(() => sampleRows(data), [data])
  const formatXAxisValue = useCallback((date: Date) => formatTime(date.toISOString()), [formatTime])
  const tooltipRows = useCallback(
    (point: Record<string, unknown>): TooltipRow[] => [
      {
        color,
        label: valueLabel,
        value: formatFiniteValue(point[dataKey], formatValue)
      }
    ],
    [color, dataKey, formatValue, valueLabel]
  )

  return (
    <figure aria-label={ariaLabel} className={cn('min-w-0', className)}>
      <div aria-hidden="true" className="h-full min-h-0 w-full min-w-0" data-testid="bklit-metric-area-chart">
        <AreaChart
          animationDuration={500}
          aspectRatio=""
          className="h-full min-h-0 w-full min-w-0"
          data={data}
          margin={{ left: yMarginLeft, right: 16, top: 8, bottom: 36 }}
          xDataKey="timestamp"
          yDomain={yDomain}
          yDomainTweenDuration={200}
        >
          <Grid vertical={false} />
          <XAxis formatValue={formatXAxisValue} numTicks={5} tickMode="domain" />
          <YAxis formatValue={formatYAxisValue} />
          <ChartTooltip
            content={({ point }) => (
              <TooltipContent rows={tooltipRows(point)} title={formatTooltipLabel(String(point.timestamp))} />
            )}
            showDatePill={false}
          />
          <Area animate={false} dataKey={dataKey} fill={color} fillOpacity={0.1} stroke={color} strokeWidth={2} />
        </AreaChart>
      </div>

      <table className="sr-only">
        <caption>{ariaLabel}</caption>
        <thead>
          <tr>
            <th scope="col">{timeLabel}</th>
            <th scope="col">{valueLabel}</th>
          </tr>
        </thead>
        <tbody>
          {accessibleRows.map((point) => (
            <tr key={String(point.timestamp)}>
              <td>{formatTooltipLabel(String(point.timestamp))}</td>
              <td>{formatFiniteValue(point[dataKey], formatValue)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </figure>
  )
}
