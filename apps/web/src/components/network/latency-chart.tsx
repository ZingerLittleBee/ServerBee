import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
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
  hours?: number
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
  return new Date(timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false })
}

function formatDateMD(timestamp: string): string {
  const d = new Date(timestamp)
  return `${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
}

function formatDateTimeMDHM(timestamp: string): string {
  const d = new Date(timestamp)
  return `${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

export function LatencyChart({ records, targets, isRealtime = false, hours = 1 }: LatencyChartProps) {
  const { t } = useTranslation('network')
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

  const isExtendedRange = hours >= 168

  const tickFormatter = useMemo<(v: string) => string>(() => {
    if (isExtendedRange) {
      let lastDate = ''
      return (v: string) => {
        const dateStr = formatDateMD(v)
        if (dateStr === lastDate) {
          return ''
        }
        lastDate = dateStr
        return dateStr
      }
    }
    return (v: string) => (isRealtime ? formatTime24(v) : formatTimeHM(v))
  }, [isRealtime, isExtendedRange])

  const tooltipLabelFormatter = useMemo(() => {
    if (isExtendedRange) {
      return (label: string) => formatDateTimeMDHM(label)
    }
    return (label: string) =>
      new Date(label).toLocaleString([], {
        month: 'short',
        day: 'numeric',
        hour: '2-digit',
        minute: '2-digit',
        second: '2-digit',
        hour12: false
      })
  }, [isExtendedRange])

  if (chartData.length === 0) {
    return (
      <div className="flex h-[300px] items-center justify-center rounded-lg border bg-card">
        <p className="text-muted-foreground text-sm">{t('latency_chart_no_data')}</p>
      </div>
    )
  }

  return (
    <div className="rounded-lg border bg-card p-4">
      <h3 className="mb-3 font-semibold text-sm">{t('latency_title')}</h3>
      <ChartContainer className="h-[300px] w-full" config={chartConfig}>
        <AreaChart accessibilityLayer data={chartData}>
          <CartesianGrid vertical={false} />
          <XAxis
            axisLine={false}
            dataKey="timestamp"
            interval={tickInterval}
            tickFormatter={tickFormatter}
            tickLine={false}
          />
          <YAxis axisLine={false} tickLine={false} unit=" ms" width={60} />
          <ChartTooltip
            content={
              <ChartTooltipContent
                labelFormatter={tooltipLabelFormatter}
                valueFormatter={(v) => `${v.toFixed(1)} ms`}
              />
            }
          />
          {visibleWithIndex.map(({ id, originalIndex }) => (
            <Area
              connectNulls={false}
              dataKey={`target_${originalIndex}`}
              fill="transparent"
              fillOpacity={0}
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
