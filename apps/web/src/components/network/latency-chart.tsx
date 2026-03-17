import { useMemo } from 'react'
import { Area, AreaChart, CartesianGrid, XAxis, YAxis } from 'recharts'
import { type ChartConfig, ChartContainer, ChartTooltip, ChartTooltipContent } from '@/components/ui/chart'
import { CHART_COLORS } from '@/lib/chart-colors'
import type { NetworkProbeRecord } from '@/lib/network-types'

interface TargetInfo {
  color: string
  id: string
  name: string
  visible: boolean
}

interface LatencyChartProps {
  isRealtime?: boolean
  records: NetworkProbeRecord[]
  targets: TargetInfo[]
}

function formatTime24(timestamp: string): string {
  return new Date(timestamp).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false
  })
}

function formatTimeHM(timestamp: string): string {
  return new Date(timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}

export function LatencyChart({ records, targets, isRealtime = false }: LatencyChartProps) {
  // Build chartConfig for ALL targets (ChartContainer needs all color vars injected)
  const chartConfig = useMemo(() => {
    const config: ChartConfig = {}
    targets.forEach((target, i) => {
      config[`target_${i}`] = {
        label: target.name,
        color: CHART_COLORS[i % CHART_COLORS.length]
      }
    })
    return config
  }, [targets])

  // Only render Area series for visible targets
  const visibleWithIndex = useMemo(
    () => targets.map((t, i) => ({ ...t, originalIndex: i })).filter((t) => t.visible),
    [targets]
  )

  const chartData = useMemo(() => {
    const bucketMs = 60_000
    const now = Date.now()
    const bucketMap = new Map<number, Record<string, unknown>>()

    for (const record of records) {
      const ts = new Date(record.timestamp).getTime()
      if (ts > now + 30_000) {
        continue
      }
      const bucketKey = Math.floor(ts / bucketMs) * bucketMs

      if (!bucketMap.has(bucketKey)) {
        bucketMap.set(bucketKey, { timestamp: new Date(bucketKey).toISOString() })
      }
      const entry = bucketMap.get(bucketKey)
      if (entry) {
        // Use target_${index} as dataKey instead of record.target_id
        const targetIndex = targets.findIndex((t) => t.id === record.target_id)
        if (targetIndex >= 0) {
          entry[`target_${targetIndex}`] = record.avg_latency
        }
      }
    }

    return Array.from(bucketMap.entries())
      .sort((a, b) => a[0] - b[0])
      .map(([, v]) => v)
  }, [records, targets])

  const tickInterval = useMemo(() => {
    if (chartData.length <= 12) {
      return 0
    }
    return Math.ceil(chartData.length / 10) - 1
  }, [chartData.length])

  if (chartData.length === 0) {
    return (
      <div className="flex h-[300px] items-center justify-center rounded-lg border bg-card">
        <p className="text-muted-foreground text-sm">No data available</p>
      </div>
    )
  }

  return (
    <div className="rounded-lg border bg-card p-4">
      <h3 className="mb-3 font-semibold text-sm">Latency (ms)</h3>
      <ChartContainer className="h-[300px] w-full" config={chartConfig}>
        <AreaChart accessibilityLayer data={chartData}>
          <CartesianGrid vertical={false} />
          <XAxis
            axisLine={false}
            dataKey="timestamp"
            interval={tickInterval}
            tickFormatter={(v) => (isRealtime ? formatTime24(v) : formatTimeHM(v))}
            tickLine={false}
          />
          <YAxis axisLine={false} tickLine={false} unit=" ms" width={60} />
          <ChartTooltip
            content={
              <ChartTooltipContent
                formatter={(value) => `${Number(value).toFixed(1)} ms`}
                labelFormatter={(label) =>
                  new Date(label).toLocaleString([], {
                    month: 'short',
                    day: 'numeric',
                    hour: '2-digit',
                    minute: '2-digit',
                    second: '2-digit'
                  })
                }
              />
            }
          />
          {visibleWithIndex.map(({ id, originalIndex }) => (
            <Area
              connectNulls={false}
              dataKey={`target_${originalIndex}`}
              fill={`var(--color-target_${originalIndex})`}
              fillOpacity={0.05}
              key={id}
              stroke={`var(--color-target_${originalIndex})`}
              strokeWidth={2}
              type="monotone"
            />
          ))}
        </AreaChart>
      </ChartContainer>
    </div>
  )
}
