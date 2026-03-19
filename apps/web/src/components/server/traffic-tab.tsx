import { useQuery } from '@tanstack/react-query'
import { useMemo, useState } from 'react'
import { Bar, BarChart, CartesianGrid, XAxis, YAxis } from 'recharts'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  type ChartConfig,
  ChartContainer,
  ChartLegend,
  ChartLegendContent,
  ChartTooltip,
  ChartTooltipContent
} from '@/components/ui/chart'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api-client'
import { cn, formatBytes } from '@/lib/utils'

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface CycleData {
  current: {
    bytes_in: number
    bytes_out: number
    end: string
    limit: number | null
    percent: number | null
    start: string
  }
  history: Array<{
    bytes_in: number
    bytes_out: number
    end: string
    period: string
    start: string
  }>
}

interface DailyItem {
  bytes_in: number
  bytes_out: number
  date: string
}

// ---------------------------------------------------------------------------
// Chart configs
// ---------------------------------------------------------------------------

const dailyConfig = {
  bytes_in: { label: 'Inbound', color: 'var(--chart-1)' },
  bytes_out: { label: 'Outbound', color: 'var(--chart-2)' }
} satisfies ChartConfig

const historyConfig = {
  bytes_in: { label: 'Inbound', color: 'var(--chart-1)' },
  bytes_out: { label: 'Outbound', color: 'var(--chart-2)' }
} satisfies ChartConfig

// ---------------------------------------------------------------------------
// Time range options
// ---------------------------------------------------------------------------

type DayRange = 7 | 30 | 90

const DAY_RANGES: { days: DayRange; label: string }[] = [
  { days: 7, label: '7d' },
  { days: 30, label: '30d' },
  { days: 90, label: '90d' }
]

// ---------------------------------------------------------------------------
// Cycle overview card
// ---------------------------------------------------------------------------

function CycleOverviewCard({ cycle }: { cycle: CycleData['current'] }) {
  const percent = cycle.percent ?? 0
  const total = cycle.bytes_in + cycle.bytes_out

  const barColor = (() => {
    if (percent >= 90) {
      return 'bg-red-500'
    }
    if (percent >= 70) {
      return 'bg-yellow-500'
    }
    return 'bg-green-500'
  })()

  const percentColor = (() => {
    if (percent >= 90) {
      return 'text-red-500'
    }
    if (percent >= 70) {
      return 'text-yellow-500'
    }
    return 'text-foreground'
  })()

  return (
    <Card>
      <CardHeader>
        <CardTitle>Current Billing Cycle</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex flex-wrap gap-x-6 gap-y-2 text-sm">
          <div>
            <span className="text-muted-foreground">Start:</span>{' '}
            <span className="font-medium">{new Date(cycle.start).toLocaleDateString()}</span>
          </div>
          <div>
            <span className="text-muted-foreground">End:</span>{' '}
            <span className="font-medium">{new Date(cycle.end).toLocaleDateString()}</span>
          </div>
        </div>

        {cycle.limit != null && (
          <div className="space-y-2">
            <div className="flex items-center justify-between text-sm">
              <span className="text-muted-foreground">
                {formatBytes(total)} / {formatBytes(cycle.limit)}
              </span>
              <span className={cn('font-semibold', percentColor)}>{percent.toFixed(1)}%</span>
            </div>
            <div className="h-3 w-full overflow-hidden rounded-full bg-muted">
              <div
                className={cn('h-full rounded-full transition-all', barColor)}
                style={{ width: `${Math.min(percent, 100)}%` }}
              />
            </div>
          </div>
        )}

        <div className="flex flex-wrap gap-6 text-sm">
          <div>
            <span className="text-muted-foreground">Inbound:</span>{' '}
            <span className="font-medium">{formatBytes(cycle.bytes_in)}</span>
          </div>
          <div>
            <span className="text-muted-foreground">Outbound:</span>{' '}
            <span className="font-medium">{formatBytes(cycle.bytes_out)}</span>
          </div>
          <div>
            <span className="text-muted-foreground">Total:</span>{' '}
            <span className="font-medium">{formatBytes(total)}</span>
          </div>
        </div>
      </CardContent>
    </Card>
  )
}

// ---------------------------------------------------------------------------
// Daily trend chart
// ---------------------------------------------------------------------------

function DailyTrendChart({ serverId }: { serverId: string }) {
  const [dayRange, setDayRange] = useState<DayRange>(30)

  const fromDate = useMemo(() => {
    const d = new Date()
    d.setDate(d.getDate() - dayRange)
    return d.toISOString().slice(0, 10)
  }, [dayRange])

  const toDate = useMemo(() => new Date().toISOString().slice(0, 10), [])

  const { data, isLoading } = useQuery<DailyItem[]>({
    queryKey: ['traffic', serverId, 'daily', dayRange],
    queryFn: () =>
      api.get<DailyItem[]>(
        `/api/traffic/${serverId}/daily?from=${encodeURIComponent(fromDate)}&to=${encodeURIComponent(toDate)}`
      ),
    staleTime: 60_000
  })

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between">
          <CardTitle>Daily Traffic Trend</CardTitle>
          <div className="flex gap-1">
            {DAY_RANGES.map((r) => (
              <button
                className={cn(
                  'rounded-md px-2.5 py-1 font-medium text-xs transition-colors',
                  dayRange === r.days
                    ? 'bg-primary text-primary-foreground'
                    : 'bg-muted text-muted-foreground hover:text-foreground'
                )}
                key={r.days}
                onClick={() => setDayRange(r.days)}
                type="button"
              >
                {r.label}
              </button>
            ))}
          </div>
        </div>
      </CardHeader>
      <CardContent>
        {isLoading && <Skeleton className="h-[260px] w-full" />}
        {!isLoading && data && data.length > 0 && (
          <ChartContainer className="h-[260px] w-full" config={dailyConfig}>
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
        )}
        {!isLoading && (!data || data.length === 0) && (
          <div className="flex h-[200px] items-center justify-center text-muted-foreground text-sm">
            No daily traffic data available.
          </div>
        )}
      </CardContent>
    </Card>
  )
}

// ---------------------------------------------------------------------------
// Historical cycle comparison chart
// ---------------------------------------------------------------------------

function HistoryCycleChart({ history }: { history: CycleData['history'] }) {
  if (history.length === 0) {
    return null
  }

  const chartData = [...history].reverse().map((h) => ({
    period: h.period,
    bytes_in: h.bytes_in,
    bytes_out: h.bytes_out
  }))

  return (
    <Card>
      <CardHeader>
        <CardTitle>Historical Cycle Comparison</CardTitle>
      </CardHeader>
      <CardContent>
        <ChartContainer className="h-[260px] w-full" config={historyConfig}>
          <BarChart accessibilityLayer data={chartData} layout="vertical" maxBarSize={24}>
            <CartesianGrid horizontal={false} />
            <XAxis axisLine={false} tickFormatter={formatBytes} tickLine={false} type="number" />
            <YAxis axisLine={false} dataKey="period" tickLine={false} type="category" width={80} />
            <ChartTooltip
              content={<ChartTooltipContent hideLabel valueFormatter={(v) => formatBytes(v)} />}
              cursor={false}
            />
            <ChartLegend content={<ChartLegendContent />} />
            <Bar dataKey="bytes_in" fill="var(--color-bytes_in)" radius={[0, 0, 4, 4]} stackId="cycle" />
            <Bar dataKey="bytes_out" fill="var(--color-bytes_out)" radius={[4, 4, 0, 0]} stackId="cycle" />
          </BarChart>
        </ChartContainer>
      </CardContent>
    </Card>
  )
}

// ---------------------------------------------------------------------------
// Main Traffic Tab component
// ---------------------------------------------------------------------------

export function TrafficTab({ billingCycle, serverId }: { billingCycle: string | null | undefined; serverId: string }) {
  const hasBillingCycle = billingCycle != null && billingCycle.length > 0

  const { data: cycleData, isLoading } = useQuery<CycleData>({
    queryKey: ['traffic', serverId, 'cycle'],
    queryFn: () => api.get<CycleData>(`/api/traffic/${serverId}/cycle?history=6`),
    staleTime: 60_000,
    enabled: hasBillingCycle
  })

  if (!hasBillingCycle) {
    return (
      <div className="mt-4 rounded-lg border bg-card p-12 text-center">
        <p className="text-muted-foreground">Configure billing cycle in server settings to view traffic statistics.</p>
      </div>
    )
  }

  if (isLoading) {
    return (
      <div className="mt-4 space-y-4">
        <Skeleton className="h-48" />
        <Skeleton className="h-72" />
        <Skeleton className="h-72" />
      </div>
    )
  }

  return (
    <div className="mt-4 space-y-4">
      {cycleData?.current && <CycleOverviewCard cycle={cycleData.current} />}
      <DailyTrendChart serverId={serverId} />
      {cycleData?.history && <HistoryCycleChart history={cycleData.history} />}
    </div>
  )
}
