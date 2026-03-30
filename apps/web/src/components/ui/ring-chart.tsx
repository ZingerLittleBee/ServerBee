interface RingChartProps {
  color: string
  label: string
  size?: number
  strokeWidth?: number
  value: number
}

const VIEWBOX = 36
const DEFAULT_SIZE = 56
const DEFAULT_STROKE = 3.5

export function RingChart({ value, size = DEFAULT_SIZE, strokeWidth = DEFAULT_STROKE, color, label }: RingChartProps) {
  const clamped = Math.min(100, Math.max(0, value))
  const radius = (VIEWBOX - strokeWidth) / 2
  const circumference = 2 * Math.PI * radius
  const dashArray = `${(clamped / 100) * circumference} ${circumference}`

  return (
    <div style={{ width: size }}>
      <div style={{ position: 'relative', width: size, height: size }}>
        <svg
          aria-label={`${label} ${clamped.toFixed(1)}%`}
          role="img"
          style={{ transform: 'rotate(-90deg)' }}
          viewBox={`0 0 ${VIEWBOX} ${VIEWBOX}`}
        >
          <circle
            cx={VIEWBOX / 2}
            cy={VIEWBOX / 2}
            fill="none"
            r={radius}
            stroke="rgba(128,128,128,0.15)"
            strokeWidth={strokeWidth}
          />
          <circle
            cx={VIEWBOX / 2}
            cy={VIEWBOX / 2}
            fill="none"
            r={radius}
            stroke={color}
            strokeDasharray={dashArray}
            strokeLinecap="round"
            strokeWidth={strokeWidth}
          />
        </svg>
        <div
          style={{
            position: 'absolute',
            inset: 0,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            fontSize: '11px',
            fontWeight: 700
          }}
        >
          {clamped.toFixed(1)}%
        </div>
      </div>
      <div className="mt-0.5 text-center text-[10px] text-muted-foreground">{label}</div>
    </div>
  )
}
