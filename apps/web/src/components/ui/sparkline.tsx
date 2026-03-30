import { Area, AreaChart, ResponsiveContainer } from 'recharts'

interface SparklineChartProps {
  color?: string
  data: number[]
  fillColor?: string
  height?: number
  showArea?: boolean
  strokeWidth?: number
  width?: number
}

export function SparklineChart({
  data,
  width = 60,
  height = 24,
  color = 'var(--color-chart-1)',
  fillColor,
  strokeWidth = 1.5,
  showArea = true
}: SparklineChartProps) {
  const chartData = data.map((value, index) => ({ index, value }))
  const defaultFillColor = fillColor || color.replace(')', ' / 0.2)').replace(')', ', 0.2)')

  if (data.length === 0) {
    return <div className="rounded bg-muted/50" style={{ width, height }} />
  }

  return (
    <div style={{ width, height }}>
      <ResponsiveContainer height="100%" width="100%">
        <AreaChart data={chartData} margin={{ top: 2, right: 2, bottom: 2, left: 2 }}>
          <Area
            animationDuration={300}
            dataKey="value"
            fill={showArea ? defaultFillColor : 'transparent'}
            stroke={color}
            strokeWidth={strokeWidth}
            type="monotone"
          />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  )
}
