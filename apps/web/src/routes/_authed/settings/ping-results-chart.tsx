import { useQuery } from '@tanstack/react-query'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { type ChartConfig, ChartContainer, ChartTooltip, ChartTooltipContent } from '@/components/ui/chart'
import { Area, AreaChart, CartesianGrid, XAxis, YAxis } from '@/components/ui/recharts-lazy'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api-client'
import type { PingRecord } from '@/lib/api-schema'

const PING_CHART_CONFIG = {
  latency: { label: 'Latency', color: 'var(--chart-4)' }
} satisfies ChartConfig

function createPingRecordWindow() {
  const now = new Date()
  return {
    from: new Date(now.getTime() - 24 * 3600 * 1000).toISOString(),
    to: now.toISOString()
  }
}

export function PingResultsChart({ taskId }: { taskId: string }) {
  const { t } = useTranslation('settings')
  const { from, to } = useMemo(createPingRecordWindow, [])

  const { data: records, isLoading } = useQuery<PingRecord[]>({
    queryKey: ['ping-records', taskId, from, to],
    queryFn: () =>
      api.get<PingRecord[]>(
        `/api/ping-tasks/${taskId}/records?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`
      )
  })

  if (isLoading) {
    return <Skeleton className="h-48" />
  }

  if (!records || records.length === 0) {
    return <p className="py-4 text-center text-muted-foreground text-xs">{t('ping.no_records')}</p>
  }

  const chartData = records.map((record) => ({
    timestamp: record.time,
    latency: record.success ? record.latency : null
  }))

  const successfulRecords = records.filter((record) => record.success)
  const successRate = ((successfulRecords.length / records.length) * 100).toFixed(1)
  const avgLatency =
    successfulRecords.reduce((sum, record) => sum + record.latency, 0) / Math.max(1, successfulRecords.length)

  return (
    <div className="space-y-2">
      <div className="flex gap-4 text-muted-foreground text-xs">
        <span>{t('ping.success_rate', { rate: successRate })}</span>
        <span>{t('ping.avg_latency', { value: avgLatency.toFixed(1) })}</span>
        <span>{t('ping.record_count', { count: records.length })}</span>
      </div>
      <ChartContainer className="h-[180px] w-full" config={PING_CHART_CONFIG}>
        <AreaChart accessibilityLayer data={chartData}>
          <CartesianGrid vertical={false} />
          <XAxis
            axisLine={false}
            dataKey="timestamp"
            tickFormatter={(value: string) =>
              new Date(value).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false })
            }
            tickLine={false}
          />
          <YAxis axisLine={false} tickLine={false} width={40} />
          <ChartTooltip
            content={
              <ChartTooltipContent
                labelFormatter={(label) => new Date(String(label)).toLocaleString([], { hour12: false })}
                valueFormatter={(value) => `${value.toFixed(1)}ms`}
              />
            }
          />
          <Area
            connectNulls={false}
            dataKey="latency"
            fill="var(--color-latency)"
            fillOpacity={0.1}
            stroke="var(--color-latency)"
            strokeWidth={2}
            type="monotone"
          />
        </AreaChart>
      </ChartContainer>
    </div>
  )
}
