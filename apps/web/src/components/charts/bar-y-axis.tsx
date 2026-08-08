'use client'

import { motion } from 'motion/react'
import { memo, useEffect, useMemo, useState } from 'react'
import { createPortal } from 'react-dom'
import { cn } from '@/lib/utils'
import { useChart, useChartStable } from './chart-context'

export interface BarYAxisProps {
  /** Local extension: render category keys as display labels (Recharts `tickFormatter`). */
  formatLabel?: (category: string) => string
  /** Maximum number of labels to show. Default: 20 */
  maxLabels?: number
  /** Whether to show all labels or skip some for dense data. Default: true */
  showAllLabels?: boolean
}

interface BarYAxisLabelProps {
  bandHeight: number
  isHovered: boolean
  label: string
  y: number
}

function BarYAxisLabel({ label, y, bandHeight, isHovered }: BarYAxisLabelProps) {
  return (
    <div
      className="absolute right-0 flex items-center justify-end pr-2"
      style={{
        top: y,
        height: bandHeight
      }}
    >
      <motion.span
        animate={{
          opacity: isHovered ? 1 : 0.7,
          color: isHovered ? 'var(--foreground)' : 'var(--chart-label, var(--color-zinc-500))'
        }}
        className={cn('truncate whitespace-nowrap text-right text-xs')}
        initial={{
          opacity: 0.7,
          color: 'var(--chart-label, var(--color-zinc-500))'
        }}
        style={{ maxWidth: 70 }}
        transition={{ duration: 0.15 }}
      >
        {label}
      </motion.span>
    </div>
  )
}

export function BarYAxis(props: BarYAxisProps) {
  const { containerRef, barScale } = useChartStable()
  const [mounted, setMounted] = useState(false)

  useEffect(() => {
    setMounted(true)
  }, [])

  const container = containerRef.current
  if (!(mounted && container)) {
    return null
  }

  if (!barScale) {
    return null
  }

  return <BarYAxisInner {...props} container={container} />
}

const BarYAxisInner = memo(function BarYAxisInner({
  showAllLabels = true,
  maxLabels = 20,
  formatLabel,
  container
}: BarYAxisProps & { container: HTMLDivElement }) {
  const { margin, barScale, bandWidth, barXAccessor, data, hoveredBarIndex } = useChart()

  // Generate labels for each bar
  const labelsToShow = useMemo(() => {
    if (!(barScale && bandWidth && barXAccessor)) {
      return []
    }

    const allLabels = data.map((d, i) => {
      const category = barXAccessor(d)
      const bandY = barScale(category) ?? 0
      // Center the label vertically within the band
      const y = bandY + margin.top
      return { label: formatLabel ? formatLabel(category) : category, y, bandHeight: bandWidth, index: i }
    })

    // If showAllLabels is true or we have fewer than maxLabels, show all
    if (showAllLabels || allLabels.length <= maxLabels) {
      return allLabels
    }

    // Otherwise, skip some labels to avoid crowding
    const step = Math.ceil(allLabels.length / maxLabels)
    return allLabels.filter((_, i) => i % step === 0)
  }, [barScale, bandWidth, barXAccessor, data, margin.top, showAllLabels, maxLabels, formatLabel])

  return createPortal(
    <div
      className="pointer-events-none absolute top-0 bottom-0"
      style={{
        left: 0,
        width: margin.left
      }}
    >
      {labelsToShow.map((item) => (
        <BarYAxisLabel
          bandHeight={item.bandHeight}
          isHovered={hoveredBarIndex === item.index}
          key={`${item.label}-${item.y}`}
          label={item.label}
          y={item.y}
        />
      ))}
    </div>,
    container
  )
})

BarYAxis.displayName = 'BarYAxis'

export default BarYAxis
