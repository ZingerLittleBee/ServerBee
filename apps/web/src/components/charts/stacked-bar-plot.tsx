import { useCallback, useMemo } from 'react'
import { cn } from '@/lib/utils'
import { Bar } from './bar'
import { BarChart } from './bar-chart'
import { BarValueAxis } from './bar-value-axis'
import { BarXAxis } from './bar-x-axis'
import { BarYAxis } from './bar-y-axis'
import { formatFiniteChartValue, sampleChartRows } from './chart-accessibility'
import { Grid } from './grid'
import { ChartTooltip } from './tooltip/chart-tooltip'
import { TooltipContent, type TooltipRow } from './tooltip/tooltip-content'
import { YAxis } from './y-axis'

/** Corner radius of the topmost stack segment (Recharts `radius`). */
const BAR_CORNER_RADIUS = 4

/** Upper bound for auto-sized bars (Recharts `maxBarSize`). */
const DEFAULT_MAX_BAR_WIDTH = 40

export interface StackedBarSeries {
  color: string
  dataKey: string
  label: string
}

export interface StackedBarPlotProps {
  ariaLabel: string
  /** Key in data holding the category (date, hour, billing period, …). */
  categoryKey: string
  /** Header for the category column of the screen-reader table. */
  categoryLabel: string
  className: string
  data: Record<string, unknown>[]
  /** Format the value axis ticks. Default: `formatValue`. */
  formatAxisValue?: (value: number) => string
  /** Format the category for axis ticks. Default: raw value. */
  formatCategory?: (category: string) => string
  /** Tooltip title. Omit to render the tooltip without a title. */
  formatTooltipLabel?: (category: string) => string
  formatValue: (value: number) => string
  /** Left margin reserved for the value (vertical) or category (horizontal) axis. */
  marginLeft?: number
  maxBarWidth?: number
  /** `"horizontal"` puts categories on the y-axis (Recharts `layout="vertical"`). */
  orientation?: 'horizontal' | 'vertical'
  /** Stack order, bottom segment first. */
  series: StackedBarSeries[]
}

export function StackedBarPlot({
  ariaLabel,
  categoryKey,
  categoryLabel,
  className,
  data,
  formatCategory,
  formatAxisValue,
  formatTooltipLabel,
  formatValue,
  marginLeft,
  maxBarWidth = DEFAULT_MAX_BAR_WIDTH,
  orientation = 'vertical',
  series
}: StackedBarPlotProps) {
  const isHorizontal = orientation === 'horizontal'
  const accessibleRows = useMemo(() => sampleChartRows(data), [data])
  const axisValueFormatter = formatAxisValue ?? formatValue

  const tooltipRows = useCallback(
    (point: Record<string, unknown>): TooltipRow[] =>
      series.flatMap((item) => {
        const value = point[item.dataKey]
        if (typeof value !== 'number' || !Number.isFinite(value)) {
          return []
        }
        return [{ color: item.color, label: item.label, value: formatValue(value) }]
      }),
    [formatValue, series]
  )

  const tooltipTitle = useCallback(
    (point: Record<string, unknown>) =>
      formatTooltipLabel ? formatTooltipLabel(String(point[categoryKey] ?? '')) : undefined,
    [categoryKey, formatTooltipLabel]
  )

  return (
    <figure aria-label={ariaLabel} className={cn('flex min-w-0 flex-col', className)}>
      <div aria-hidden="true" className="min-h-0 w-full min-w-0 flex-1" data-testid="bklit-stacked-bar-chart">
        <BarChart
          animationDuration={500}
          aspectRatio=""
          className="h-full min-h-0 w-full min-w-0"
          data={data}
          margin={{ left: marginLeft ?? (isHorizontal ? 88 : 64), right: 16, top: 8, bottom: 32 }}
          maxBarWidth={maxBarWidth}
          orientation={orientation}
          stacked
          xDataKey={categoryKey}
        >
          <Grid horizontal={!isHorizontal} vertical={isHorizontal} />
          {isHorizontal ? (
            <>
              <BarYAxis formatLabel={formatCategory} />
              <BarValueAxis formatValue={axisValueFormatter} />
            </>
          ) : (
            <>
              <BarXAxis formatLabel={formatCategory} />
              <YAxis formatValue={axisValueFormatter} />
            </>
          )}
          <ChartTooltip
            content={({ point }) => <TooltipContent rows={tooltipRows(point)} title={tooltipTitle(point)} />}
            showDatePill={false}
          />
          {series.map((item) => (
            <Bar
              animate={false}
              dataKey={item.dataKey}
              fill={item.color}
              key={item.dataKey}
              lineCap={BAR_CORNER_RADIUS}
            />
          ))}
        </BarChart>
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
            <th scope="col">{categoryLabel}</th>
            {series.map((item) => (
              <th key={item.dataKey} scope="col">
                {item.label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {accessibleRows.map((point) => {
            const category = String(point[categoryKey] ?? '')
            return (
              <tr key={category}>
                <td>{formatTooltipLabel?.(category) ?? formatCategory?.(category) ?? category}</td>
                {series.map((item) => (
                  <td key={item.dataKey}>{formatFiniteChartValue(point[item.dataKey], formatValue)}</td>
                ))}
              </tr>
            )
          })}
        </tbody>
      </table>
    </figure>
  )
}
