import { cn } from '@/lib/utils'

interface RingChartProps {
  color: string
  compact?: boolean
  label: string
  size?: number
  strokeWidth?: number
  value: number
}

const VIEWBOX = 36
const DEFAULT_SIZE = 56
const DEFAULT_STROKE = 3.5
const COMPACT_SIZE = 32
const COMPACT_STROKE = 4
/** Unitless path length so dash offset is a simple 0–100 scale (independent of radius). */
const PATH_LENGTH = 100
/**
 * Arc + color transitions for live metric updates. Pure CSS so dozens of rings
 * on a dashboard only pay for browser style interpolation — no rAF or JS
 * tweening per instance. Durations stay short: metric ticks are high-frequency.
 */
const PROGRESS_TRANSITION = 'stroke-dashoffset 280ms cubic-bezier(0.2, 0, 0, 1), stroke 200ms ease-out'

export function RingChart({ value, size, strokeWidth, color, label, compact = false }: RingChartProps) {
  const resolvedSize = size ?? (compact ? COMPACT_SIZE : DEFAULT_SIZE)
  const resolvedStroke = strokeWidth ?? (compact ? COMPACT_STROKE : DEFAULT_STROKE)
  const clamped = Math.min(100, Math.max(0, value))
  const radius = (VIEWBOX - resolvedStroke) / 2
  // Empty at PATH_LENGTH, full at 0 — same convention as classic SVG meters.
  const dashOffset = PATH_LENGTH * (1 - clamped / 100)

  return (
    <div style={{ width: resolvedSize }}>
      <div className="relative" style={{ width: resolvedSize, height: resolvedSize }}>
        <svg
          aria-label={`${label} ${clamped.toFixed(1)}%`}
          height={resolvedSize}
          role="img"
          style={{ transform: 'rotate(-90deg)' }}
          viewBox={`0 0 ${VIEWBOX} ${VIEWBOX}`}
          width={resolvedSize}
        >
          <circle
            cx={VIEWBOX / 2}
            cy={VIEWBOX / 2}
            fill="none"
            r={radius}
            stroke="var(--metric-ring-track)"
            strokeWidth={resolvedStroke}
          />
          <circle
            className="motion-reduce:[transition:none]"
            cx={VIEWBOX / 2}
            cy={VIEWBOX / 2}
            fill="none"
            pathLength={PATH_LENGTH}
            r={radius}
            strokeLinecap="round"
            strokeWidth={resolvedStroke}
            style={{
              stroke: color,
              strokeDasharray: PATH_LENGTH,
              strokeDashoffset: dashOffset,
              transition: PROGRESS_TRANSITION
            }}
          />
        </svg>
        <div
          className={cn(
            'absolute inset-0 flex items-center justify-center font-bold tabular-nums',
            compact ? 'text-[10px]' : 'text-xs'
          )}
        >
          {clamped.toFixed(0)}
        </div>
      </div>
      {!compact && <div className="mt-0.5 text-center text-[10px] text-muted-foreground">{label}</div>}
    </div>
  )
}
