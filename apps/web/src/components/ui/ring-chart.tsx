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

export function RingChart({ value, size, strokeWidth, color, label, compact = false }: RingChartProps) {
  const resolvedSize = size ?? (compact ? COMPACT_SIZE : DEFAULT_SIZE)
  const resolvedStroke = strokeWidth ?? (compact ? COMPACT_STROKE : DEFAULT_STROKE)
  const clamped = Math.min(100, Math.max(0, value))
  const radius = (VIEWBOX - resolvedStroke) / 2
  const circumference = 2 * Math.PI * radius
  const dashArray = `${(clamped / 100) * circumference} ${circumference}`
  const labelFontSize = compact ? '10px' : '12px'

  return (
    <div style={{ width: resolvedSize }}>
      <div style={{ position: 'relative', width: resolvedSize, height: resolvedSize }}>
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
            stroke="rgba(128,128,128,0.15)"
            strokeWidth={resolvedStroke}
          />
          <circle
            cx={VIEWBOX / 2}
            cy={VIEWBOX / 2}
            fill="none"
            r={radius}
            stroke={color}
            strokeDasharray={dashArray}
            strokeLinecap="round"
            strokeWidth={resolvedStroke}
          />
        </svg>
        <div
          style={{
            position: 'absolute',
            inset: 0,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            fontSize: labelFontSize,
            fontWeight: 700
          }}
        >
          {clamped.toFixed(0)}
        </div>
      </div>
      {!compact && <div className="mt-0.5 text-center text-[10px] text-muted-foreground">{label}</div>}
    </div>
  )
}
