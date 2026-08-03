import { useCallback, useMemo } from 'react'
import { cn } from '@/lib/utils'
import { formatFiniteChartValue, sampleChartRows } from './chart-accessibility'
import { Grid } from './grid'
import { Line } from './line'
import { LineChart } from './line-chart'
import { ChartTooltip } from './tooltip/chart-tooltip'
import { TooltipContent, type TooltipRow } from './tooltip/tooltip-content'
import { XAxis } from './x-axis'
import { YAxis } from './y-axis'
import type { YDomain } from './y-domain-utils'

export interface MetricLineSeries {
  color: string
  dataKey: string
  label: string
}

export interface MetricLinePlotProps {
  ariaLabel: string
  className: string
  data: Record<string, unknown>[]
  formatTime?: (time: string) => string
  formatTooltipLabel?: (time: string) => string
  formatValue: (value: number, series: MetricLineSeries) => string
  formatYAxisValue?: (value: number) => string
  series: MetricLineSeries[]
  timeLabel: string
  yDomain?: YDomain
  yMarginLeft?: number
}

function defaultFormatTime(time: string): string {
  return new Date(time).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false })
}

export function MetricLinePlot({
  ariaLabel,
  className,
  data,
  formatTime = defaultFormatTime,
  formatTooltipLabel = formatTime,
  formatValue,
  formatYAxisValue,
  series,
  timeLabel,
  yDomain,
  yMarginLeft = 68
}: MetricLinePlotProps) {
  const accessibleRows = useMemo(() => sampleChartRows(data), [data])
  const formatXAxisValue = useCallback((date: Date) => formatTime(date.toISOString()), [formatTime])
  const tooltipRows = useCallback(
    (point: Record<string, unknown>): TooltipRow[] =>
      series.flatMap((item) => {
        const value = point[item.dataKey]
        if (typeof value !== 'number' || !Number.isFinite(value)) {
          return []
        }
        return [{ color: item.color, label: item.label, value: formatValue(value, item) }]
      }),
    [formatValue, series]
  )

  return (
    <figure aria-label={ariaLabel} className={cn('flex min-w-0 flex-col', className)}>
      <div aria-hidden="true" className="min-h-0 w-full min-w-0 flex-1" data-testid="bklit-metric-line-chart">
        <LineChart
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
          {series.map((item) => (
            <Line
              animate={false}
              connectNulls
              dataKey={item.dataKey}
              fadeEdges={false}
              key={item.dataKey}
              stroke={item.color}
              strokeWidth={2}
            />
          ))}
        </LineChart>
      </div>

      <ul className="mt-1 flex flex-wrap justify-center gap-x-4 gap-y-1 text-muted-foreground text-xs">
        {series.map((item) => (
          <li className="flex items-center gap-1.5" key={item.dataKey}>
            <span aria-hidden="true" className="size-2 rounded-full" style={{ backgroundColor: item.color }} />
            <span>{item.label}</span>
          </li>
        ))}
      </ul>

      <table className="sr-only">
        <caption>{ariaLabel}</caption>
        <thead>
          <tr>
            <th scope="col">{timeLabel}</th>
            {series.map((item) => (
              <th key={item.dataKey} scope="col">
                {item.label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {accessibleRows.map((point) => (
            <tr key={String(point.timestamp)}>
              <td>{formatTooltipLabel(String(point.timestamp))}</td>
              {series.map((item) => (
                <td key={item.dataKey}>
                  {formatFiniteChartValue(point[item.dataKey], (value) => formatValue(value, item))}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </figure>
  )
}
