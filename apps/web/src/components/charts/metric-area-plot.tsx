import { useCallback, useMemo } from 'react'
import { cn } from '@/lib/utils'
import { Area } from './area'
import { AreaChart } from './area-chart'
import { formatFiniteChartValue, sampleChartRows } from './chart-accessibility'
import { Grid } from './grid'
import { ChartTooltip } from './tooltip/chart-tooltip'
import { TooltipContent, type TooltipRow } from './tooltip/tooltip-content'
import { XAxis } from './x-axis'
import { YAxis } from './y-axis'
import type { YDomain } from './y-domain-utils'

export interface MetricAreaSeries {
  color: string
  dataKey: string
  label: string
}

export interface MetricAreaPlotProps {
  ariaLabel: string
  className: string
  data: Record<string, unknown>[]
  formatTime?: (time: string) => string
  formatTooltipLabel?: (time: string) => string
  formatValue: (value: number) => string
  formatYAxisValue?: (value: number) => string
  /** Stacking order is irrelevant — areas overlay each other. */
  series: MetricAreaSeries[]
  /** Key in data holding the x value. Default: `"timestamp"`. */
  timeKey?: string
  /** Header for the x column of the screen-reader table. */
  timeLabel: string
  yDomain?: YDomain
  yMarginLeft?: number
}

function defaultFormatTime(time: string): string {
  return new Date(time).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false })
}

export function MetricAreaPlot({
  ariaLabel,
  className,
  data,
  formatTime = defaultFormatTime,
  formatTooltipLabel = formatTime,
  formatValue,
  formatYAxisValue,
  series,
  timeKey = 'timestamp',
  timeLabel,
  yDomain,
  yMarginLeft = 52
}: MetricAreaPlotProps) {
  const accessibleRows = useMemo(() => sampleChartRows(data), [data])
  const formatXAxisValue = useCallback((date: Date) => formatTime(date.toISOString()), [formatTime])
  const tooltipRows = useCallback(
    (point: Record<string, unknown>): TooltipRow[] =>
      series.map((item) => ({
        color: item.color,
        label: item.label,
        value: formatFiniteChartValue(point[item.dataKey], formatValue)
      })),
    [formatValue, series]
  )

  return (
    <figure aria-label={ariaLabel} className={cn('flex min-w-0 flex-col', className)}>
      <div aria-hidden="true" className="min-h-0 w-full min-w-0 flex-1" data-testid="bklit-metric-area-chart">
        <AreaChart
          animationDuration={500}
          aspectRatio=""
          className="h-full min-h-0 w-full min-w-0"
          data={data}
          margin={{ left: yMarginLeft, right: 16, top: 8, bottom: 36 }}
          xDataKey={timeKey}
          yDomain={yDomain}
          yDomainTweenDuration={200}
        >
          <Grid vertical={false} />
          <XAxis formatValue={formatXAxisValue} numTicks={5} tickMode="domain" />
          <YAxis formatValue={formatYAxisValue} />
          <ChartTooltip
            content={({ point }) => (
              <TooltipContent rows={tooltipRows(point)} title={formatTooltipLabel(String(point[timeKey]))} />
            )}
            showDatePill={false}
          />
          {series.map((item) => (
            <Area
              animate={false}
              dataKey={item.dataKey}
              fill={item.color}
              fillOpacity={0.1}
              key={item.dataKey}
              stroke={item.color}
              strokeWidth={2}
            />
          ))}
        </AreaChart>
      </div>

      {series.length > 1 && (
        <ul className="mt-1 flex flex-wrap justify-center gap-x-4 gap-y-1 text-muted-foreground text-xs">
          {series.map((item) => (
            <li className="flex items-center gap-1.5" key={item.dataKey}>
              <span aria-hidden="true" className="size-2 rounded-full" style={{ backgroundColor: item.color }} />
              <span>{item.label}</span>
            </li>
          ))}
        </ul>
      )}

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
            <tr key={String(point[timeKey])}>
              <td>{formatTooltipLabel(String(point[timeKey]))}</td>
              {series.map((item) => (
                <td key={item.dataKey}>{formatFiniteChartValue(point[item.dataKey], formatValue)}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </figure>
  )
}
