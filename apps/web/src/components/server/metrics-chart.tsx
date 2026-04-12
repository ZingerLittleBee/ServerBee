import { Area, AreaChart, CartesianGrid, XAxis, YAxis } from 'recharts'
import { type ChartConfig, ChartContainer, ChartTooltip, ChartTooltipContent } from '@/components/ui/chart'

interface MetricsChartProps {
  color?: string
  data: Record<string, unknown>[]
  dataKey: string
  domain?: [number, number]
  formatTick?: (value: number) => string
  formatTime?: (time: string) => string
  formatTooltipLabel?: (time: string) => string
  formatValue?: (value: number) => string
  title: string
  unit?: string
}

function defaultFormatTime(time: string): string {
  const d = new Date(time)
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

function defaultFormatValue(value: number): string {
  return value.toFixed(1)
}

export function MetricsChart({
  title,
  data,
  dataKey,
  color = 'var(--color-chart-1)',
  domain,
  unit = '',
  formatValue = defaultFormatValue,
  formatTick,
  formatTime = defaultFormatTime,
  formatTooltipLabel
}: MetricsChartProps) {
  const chartConfig = {
    [dataKey]: { label: title, color }
  } satisfies ChartConfig

  return (
    <div className="rounded-lg border bg-card p-4">
      <h3 className="mb-3 font-semibold text-sm">{title}</h3>
      <ChartContainer className="h-[260px] w-full" config={chartConfig}>
        <AreaChart accessibilityLayer data={data}>
          <CartesianGrid vertical={false} />
          <XAxis axisLine={false} dataKey="timestamp" tickFormatter={formatTime} tickLine={false} />
          <YAxis
            axisLine={false}
            domain={domain}
            tickFormatter={formatTick}
            tickLine={false}
            width={formatTick ? 60 : 45}
          />
          <ChartTooltip
            content={
              <ChartTooltipContent
                labelFormatter={(label) => (formatTooltipLabel ?? formatTime)(String(label))}
                valueFormatter={(v) => `${formatValue(v)}${unit}`}
              />
            }
          />
          <Area
            animationDuration={800}
            dataKey={dataKey}
            fill={`var(--color-${dataKey})`}
            fillOpacity={0.1}
            stroke={`var(--color-${dataKey})`}
            strokeWidth={2}
            type="monotone"
          />
        </AreaChart>
      </ChartContainer>
    </div>
  )
}
