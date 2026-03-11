import { Area, AreaChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'

interface MetricsChartProps {
  color?: string
  data: Record<string, unknown>[]
  dataKey: string
  formatTime?: (time: string) => string
  formatValue?: (value: number) => string
  title: string
  unit?: string
}

function defaultFormatTime(time: string): string {
  const date = new Date(time)
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}

function defaultFormatValue(value: number): string {
  return value.toFixed(1)
}

export function MetricsChart({
  title,
  data,
  dataKey,
  color = 'var(--color-chart-1)',
  unit = '',
  formatValue = defaultFormatValue,
  formatTime = defaultFormatTime
}: MetricsChartProps) {
  return (
    <div className="rounded-lg border bg-card p-4">
      <h3 className="mb-3 font-semibold text-sm">{title}</h3>
      <ResponsiveContainer height={200} width="100%">
        <AreaChart data={data}>
          <defs>
            <linearGradient id={`gradient-${dataKey}`} x1="0" x2="0" y1="0" y2="1">
              <stop offset="5%" stopColor={color} stopOpacity={0.3} />
              <stop offset="95%" stopColor={color} stopOpacity={0} />
            </linearGradient>
          </defs>
          <CartesianGrid stroke="var(--color-border)" strokeDasharray="3 3" vertical={false} />
          <XAxis
            axisLine={false}
            dataKey="timestamp"
            stroke="var(--color-muted-foreground)"
            tick={{ fontSize: 11 }}
            tickFormatter={formatTime}
            tickLine={false}
          />
          <YAxis
            axisLine={false}
            stroke="var(--color-muted-foreground)"
            tick={{ fontSize: 11 }}
            tickLine={false}
            width={45}
          />
          <Tooltip
            contentStyle={{
              backgroundColor: 'var(--color-card)',
              border: '1px solid var(--color-border)',
              borderRadius: '8px',
              fontSize: '12px'
            }}
            formatter={(value) => [`${formatValue(Number(value))}${unit}`, title]}
            labelFormatter={(label) => formatTime(String(label))}
          />
          <Area dataKey={dataKey} fill={`url(#gradient-${dataKey})`} stroke={color} strokeWidth={2} type="monotone" />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  )
}
