import { useQuery } from '@tanstack/react-query'
import { useMemo } from 'react'
import { Bar, BarChart, CartesianGrid, XAxis, YAxis } from 'recharts'
import {
  type ChartConfig,
  ChartContainer,
  ChartLegend,
  ChartLegendContent,
  ChartTooltip,
  ChartTooltipContent
} from '@/components/ui/chart'
import { Skeleton } from '@/components/ui/skeleton'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { api } from '@/lib/api-client'
import { formatBytes } from '@/lib/utils'
import type { TrafficBarConfig } from '@/lib/widget-types'

interface TrafficBarWidgetProps {
  config: TrafficBarConfig
  servers: ServerMetrics[]
}

interface DailyTrafficItem {
  bytes_in: number
  bytes_out: number
  date: string
}

const DEFAULT_DAYS = 30

const trafficConfig = {
  bytes_in: { label: 'Inbound', color: 'var(--chart-1)' },
  bytes_out: { label: 'Outbound', color: 'var(--chart-2)' }
} satisfies ChartConfig

function hoursToDays(hours?: number): number {
  if (!hours || hours <= 0) {
    return DEFAULT_DAYS
  }
  return Math.max(1, Math.round(hours / 24))
}

export function TrafficBarWidget({ config, servers }: TrafficBarWidgetProps) {
  const { server_id } = config
  const days = hoursToDays(config.hours)
  const hasServerId = server_id != null && server_id.length > 0

  const fromDate = useMemo(() => {
    const d = new Date()
    d.setDate(d.getDate() - days)
    return d.toISOString().slice(0, 10)
  }, [days])

  const toDate = useMemo(() => new Date().toISOString().slice(0, 10), [])

  // Per-server daily traffic
  const { data: serverDaily, isLoading: serverLoading } = useQuery<DailyTrafficItem[]>({
    queryKey: ['traffic', server_id, 'daily', days],
    queryFn: () =>
      api.get<DailyTrafficItem[]>(
        `/api/traffic/${server_id}/daily?from=${encodeURIComponent(fromDate)}&to=${encodeURIComponent(toDate)}`
      ),
    staleTime: 60_000,
    enabled: hasServerId
  })

  // Global overview daily traffic
  const { data: globalDaily, isLoading: globalLoading } = useQuery<DailyTrafficItem[]>({
    queryKey: ['traffic', 'overview', 'daily', days],
    queryFn: () => api.get<DailyTrafficItem[]>(`/api/traffic/overview/daily?days=${days}`),
    staleTime: 60_000,
    enabled: !hasServerId
  })

  const isLoading = hasServerId ? serverLoading : globalLoading
  const data = hasServerId ? serverDaily : globalDaily

  const serverName = useMemo(() => {
    if (!hasServerId) {
      return 'All Servers'
    }
    return servers.find((s) => s.id === server_id)?.name ?? 'Unknown'
  }, [hasServerId, server_id, servers])

  if (isLoading) {
    return (
      <div className="flex h-full flex-col rounded-lg border bg-card p-4">
        <Skeleton className="mb-2 h-4 w-32" />
        <Skeleton className="flex-1" />
      </div>
    )
  }

  if (!data || data.length === 0) {
    return (
      <div className="flex h-full flex-col rounded-lg border bg-card p-4">
        <h3 className="mb-3 font-semibold text-sm">Traffic</h3>
        <p className="text-muted-foreground text-xs">{serverName}</p>
        <div className="flex flex-1 items-center justify-center text-muted-foreground text-sm">
          No traffic data available
        </div>
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col rounded-lg border bg-card p-4">
      <div className="mb-3">
        <h3 className="font-semibold text-sm">Traffic</h3>
        <p className="text-muted-foreground text-xs">{serverName}</p>
      </div>
      <div className="min-h-0 flex-1">
        <ChartContainer className="h-full w-full" config={trafficConfig}>
          <BarChart accessibilityLayer data={data} maxBarSize={40}>
            <CartesianGrid vertical={false} />
            <XAxis
              axisLine={false}
              dataKey="date"
              tickFormatter={(v: string) => v.slice(5)}
              tickLine={false}
              tickMargin={10}
            />
            <YAxis axisLine={false} tickFormatter={formatBytes} tickLine={false} width={60} />
            <ChartTooltip
              content={<ChartTooltipContent hideLabel valueFormatter={(v) => formatBytes(v)} />}
              cursor={false}
            />
            <ChartLegend content={<ChartLegendContent />} />
            <Bar dataKey="bytes_in" fill="var(--color-bytes_in)" radius={[0, 0, 4, 4]} stackId="traffic" />
            <Bar dataKey="bytes_out" fill="var(--color-bytes_out)" radius={[4, 4, 0, 0]} stackId="traffic" />
          </BarChart>
        </ChartContainer>
      </div>
    </div>
  )
}
