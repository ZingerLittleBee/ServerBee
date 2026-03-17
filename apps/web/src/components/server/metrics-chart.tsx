import { Area, AreaChart, CartesianGrid, XAxis, YAxis } from 'recharts'
import { type ChartConfig, ChartContainer, ChartTooltip, ChartTooltipContent } from '@/components/ui/chart'

interface MetricsChartProps {
  color?: string
  data: Record<string, unknown>[]
  dataKey: string
  formatTick?: (value: number) => string
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
  formatTick,
  formatTime = defaultFormatTime
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
          <YAxis axisLine={false} tickFormatter={formatTick} tickLine={false} width={formatTick ? 60 : 45} />
          <ChartTooltip
            content={
              <ChartTooltipContent
                labelFormatter={(label) => formatTime(String(label))}
                valueFormatter={(v) => `${formatValue(v)}${unit}`}
              />
            }
          />
          <Area
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
