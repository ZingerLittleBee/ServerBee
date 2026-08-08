import { Area } from './area'
import { AreaChart } from './area-chart'

const SPARKLINE_MARGIN = { top: 2, right: 0, bottom: 0, left: 0 }

export interface SparklinePlotProps {
  className?: string
  /** Line and gradient color. */
  color: string
  data: Record<string, unknown>[]
  dataKey: string
  /** Key in data holding the x value. Default: `"t"`. */
  timeKey?: string
}

/**
 * Decorative trend line for stat tiles — no axes, grid, or tooltip. The value
 * it illustrates is always rendered as text next to it, so the chart itself
 * stays hidden from assistive tech.
 */
export function SparklinePlot({ className, color, data, dataKey, timeKey = 't' }: SparklinePlotProps) {
  return (
    <div aria-hidden="true" className={className} data-testid="bklit-sparkline">
      <AreaChart
        animationDuration={0}
        aspectRatio=""
        className="h-full min-h-0 w-full min-w-0"
        data={data}
        margin={SPARKLINE_MARGIN}
        xDataKey={timeKey}
      >
        <Area
          animate={false}
          dataKey={dataKey}
          fill={color}
          fillOpacity={0.35}
          showHighlight={false}
          stroke={color}
          strokeWidth={1.5}
        />
      </AreaChart>
    </div>
  )
}
