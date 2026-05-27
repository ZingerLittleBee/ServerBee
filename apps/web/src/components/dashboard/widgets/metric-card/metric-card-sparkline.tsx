import { useId } from 'react'
import { Area, AreaChart, ResponsiveContainer } from 'recharts'
import type { MetricSeriesPoint } from '@/hooks/use-metric-series'

interface MetricCardSparklineProps {
  accent: string
  points: MetricSeriesPoint[]
}

export function MetricCardSparkline({ points, accent }: MetricCardSparklineProps) {
  const gradientId = useId()
  const color = `var(${accent})`

  if (points.length < 2) {
    return <div className="h-full w-full" data-testid="metric-card-sparkline-empty" />
  }

  return (
    <ResponsiveContainer data-testid="metric-card-sparkline" height="100%" width="100%">
      <AreaChart data={points} margin={{ top: 2, right: 0, bottom: 0, left: 0 }}>
        <defs>
          <linearGradient id={gradientId} x1="0" x2="0" y1="0" y2="1">
            <stop offset="0%" stopColor={color} stopOpacity={0.35} />
            <stop offset="100%" stopColor={color} stopOpacity={0} />
          </linearGradient>
        </defs>
        <Area
          dataKey="v"
          fill={`url(#${gradientId})`}
          isAnimationActive={false}
          stroke={color}
          strokeWidth={1.5}
          type="monotone"
        />
      </AreaChart>
    </ResponsiveContainer>
  )
}
