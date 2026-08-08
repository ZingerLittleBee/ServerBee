'use client'

import { memo, useEffect, useMemo, useState } from 'react'
import { createPortal } from 'react-dom'
import { useChartStable } from './chart-context'
import { resolveYAxisTickCount, Y_AXIS_DEFAULT_TICK_COUNT } from './y-axis-ticks'

/**
 * Numeric axis along the bottom of a horizontal `<BarChart>`. Bklit ships
 * categorical bar axes (`BarXAxis` / `BarYAxis`) plus the time-series `XAxis`,
 * so horizontal bars have no value axis upstream — this fills that gap the
 * same way `YAxis` labels the vertical value scale.
 */
export interface BarValueAxisProps {
  /** Custom formatter for tick labels (e.g. bytes). */
  formatValue?: (value: number) => string
  /** Approximate tick count hint for `scale.ticks()` (d3). Default: fits the plot width. */
  numTicks?: number
}

/** Horizontal room a value label needs before neighbours collide. */
const TICK_SLOT_PX = 96

export function BarValueAxis(props: BarValueAxisProps) {
  const { containerRef } = useChartStable()
  const [mounted, setMounted] = useState(false)

  useEffect(() => {
    setMounted(true)
  }, [])

  const container = containerRef.current
  if (!(mounted && container)) {
    return null
  }

  return <BarValueAxisInner {...props} container={container} />
}

const BarValueAxisInner = memo(function BarValueAxisInner({
  formatValue,
  numTicks: numTicksProp,
  container
}: BarValueAxisProps & { container: HTMLDivElement }) {
  const { innerWidth, margin, yScale } = useChartStable()
  const numTicks =
    numTicksProp ?? Math.max(2, Math.min(Y_AXIS_DEFAULT_TICK_COUNT, Math.floor(innerWidth / TICK_SLOT_PX)))

  const ticks = useMemo(
    () =>
      yScale.ticks(resolveYAxisTickCount(numTicks)).map((value) => ({
        label: formatValue ? formatValue(value) : String(value),
        value,
        x: (yScale(value) ?? 0) + margin.left
      })),
    [formatValue, margin.left, numTicks, yScale]
  )

  return createPortal(
    <div className="pointer-events-none absolute inset-0">
      {ticks.map((tick) => (
        <div className="absolute flex justify-center" key={tick.value} style={{ left: tick.x, bottom: 12, width: 0 }}>
          <span className="whitespace-nowrap text-chart-label text-xs">{tick.label}</span>
        </div>
      ))}
    </div>,
    container
  )
})

BarValueAxis.displayName = 'BarValueAxis'

export default BarValueAxis
