import { useQuery } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'
import { AlertTriangle, ArrowDownToLine, ArrowUpFromLine, Crown, Server } from 'lucide-react'
import { useMemo, useState } from 'react'
import { Area, AreaChart, CartesianGrid, XAxis, YAxis } from 'recharts'
import { Badge } from '@/components/ui/badge'
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
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { api } from '@/lib/api-client'
import { cn, formatBytes } from '@/lib/utils'

export const Route = createFileRoute('/_authed/traffic/')({
  component: TrafficPage
})

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface TrafficOverviewItem {
  billing_cycle: string | null
  cycle_in: number
  cycle_out: number
  days_remaining: number | null
  name: string
  percent_used: number | null
  server_id: string
  traffic_limit: number | null
}

interface DailyTrafficItem {
  bytes_in: number
  bytes_out: number
  date: string
}

// ---------------------------------------------------------------------------
// Sort helpers
// ---------------------------------------------------------------------------

type SortField = 'name' | 'total' | 'percent'
type SortDir = 'asc' | 'desc'

function getTotal(s: TrafficOverviewItem): number {
  return s.cycle_in + s.cycle_out
}

function compareServers(a: TrafficOverviewItem, b: TrafficOverviewItem, field: SortField, dir: SortDir): number {
  let cmp = 0
  switch (field) {
    case 'name':
      cmp = a.name.localeCompare(b.name)
      break
    case 'total':
      cmp = getTotal(a) - getTotal(b)
      break
    case 'percent':
      cmp = (a.percent_used ?? -1) - (b.percent_used ?? -1)
      break
    default:
      break
  }
  return dir === 'asc' ? cmp : -cmp
}

// ---------------------------------------------------------------------------
// Chart config
// ---------------------------------------------------------------------------

const trendConfig = {
  bytes_in: { label: 'Inbound', color: 'var(--chart-1)' },
  bytes_out: { label: 'Outbound', color: 'var(--chart-2)' }
} satisfies ChartConfig

// ---------------------------------------------------------------------------
// Stat Card
// ---------------------------------------------------------------------------

function StatCard({
  icon: Icon,
  label,
  value
}: {
  icon: React.ComponentType<{ className?: string }>
  label: string
  value: string
}) {
  return (
    <Card>
      <CardContent className="flex items-center gap-4 pt-4">
        <div className="flex size-10 items-center justify-center rounded-lg bg-muted">
          <Icon className="size-5 text-muted-foreground" />
        </div>
        <div>
          <p className="text-muted-foreground text-sm">{label}</p>
          <p className="font-semibold text-lg">{value}</p>
        </div>
      </CardContent>
    </Card>
  )
}

// ---------------------------------------------------------------------------
// Progress bar (inline)
// ---------------------------------------------------------------------------

function UsageBar({ percent }: { percent: number | null }) {
  if (percent == null) {
    return <span className="text-muted-foreground text-xs">N/A</span>
  }

  const barColor = (() => {
    if (percent >= 90) {
      return 'bg-red-500'
    }
    if (percent >= 70) {
      return 'bg-yellow-500'
    }
    return 'bg-green-500'
  })()

  const textColor = (() => {
    if (percent >= 90) {
      return 'text-red-500'
    }
    if (percent >= 70) {
      return 'text-yellow-500'
    }
    return ''
  })()

  return (
    <div className="flex items-center gap-2">
      <div className="h-2 w-24 overflow-hidden rounded-full bg-muted">
        <div
          className={cn('h-full rounded-full transition-all', barColor)}
          style={{ width: `${Math.min(percent, 100)}%` }}
        />
      </div>
      <span className={cn('text-xs tabular-nums', textColor)}>{percent.toFixed(1)}%</span>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Sortable header
// ---------------------------------------------------------------------------

function SortableHead({
  children,
  field,
  sortDir,
  sortField,
  onSort
}: {
  children: React.ReactNode
  field: SortField
  sortDir: SortDir
  sortField: SortField
  onSort: (f: SortField) => void
}) {
  const active = sortField === field
  return (
    <TableHead className="cursor-pointer select-none" onClick={() => onSort(field)}>
      <span className={cn(active && 'font-bold')}>
        {children}
        {active && (sortDir === 'asc' ? ' \u2191' : ' \u2193')}
      </span>
    </TableHead>
  )
}

// ---------------------------------------------------------------------------
// Main Page
// ---------------------------------------------------------------------------

function TrafficPage() {
  const [sortField, setSortField] = useState<SortField>('total')
  const [sortDir, setSortDir] = useState<SortDir>('desc')

  const { data: overview, isLoading: overviewLoading } = useQuery<TrafficOverviewItem[]>({
    queryKey: ['traffic', 'overview'],
    queryFn: () => api.get<TrafficOverviewItem[]>('/api/traffic/overview'),
    staleTime: 60_000
  })

  const { data: dailyData, isLoading: dailyLoading } = useQuery<DailyTrafficItem[]>({
    queryKey: ['traffic', 'overview', 'daily'],
    queryFn: () => api.get<DailyTrafficItem[]>('/api/traffic/overview/daily?days=30'),
    staleTime: 60_000
  })

  const handleSort = (field: SortField) => {
    if (sortField === field) {
      setSortDir((d) => (d === 'asc' ? 'desc' : 'asc'))
    } else {
      setSortField(field)
      setSortDir('desc')
    }
  }

  const sorted = useMemo(() => {
    if (!overview) {
      return []
    }
    return [...overview].sort((a, b) => compareServers(a, b, sortField, sortDir))
  }, [overview, sortField, sortDir])

  // Stat card aggregations
  const totalIn = useMemo(() => (overview ?? []).reduce((sum, s) => sum + s.cycle_in, 0), [overview])
  const totalOut = useMemo(() => (overview ?? []).reduce((sum, s) => sum + s.cycle_out, 0), [overview])

  const highestServer = useMemo(() => {
    if (!overview || overview.length === 0) {
      return '-'
    }
    const top = overview.reduce((max, s) => (getTotal(s) > getTotal(max) ? s : max), overview[0])
    return top.name
  }, [overview])

  const warnCount = useMemo(
    () => (overview ?? []).filter((s) => s.percent_used != null && s.percent_used > 80).length,
    [overview]
  )

  const isLoading = overviewLoading || dailyLoading

  if (isLoading) {
    return (
      <div className="space-y-6">
        <Skeleton className="h-8 w-48" />
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
          {Array.from({ length: 4 }, (_, i) => (
            <Skeleton className="h-24" key={`stat-${i.toString()}`} />
          ))}
        </div>
        <Skeleton className="h-64" />
        <Skeleton className="h-96" />
      </div>
    )
  }

  return (
    <div>
      <h1 className="mb-6 font-bold text-2xl">Traffic Overview</h1>

      {/* Stat cards */}
      <div className="mb-6 grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <StatCard icon={ArrowDownToLine} label="Cycle Inbound" value={formatBytes(totalIn)} />
        <StatCard icon={ArrowUpFromLine} label="Cycle Outbound" value={formatBytes(totalOut)} />
        <StatCard icon={Crown} label="Highest Usage" value={highestServer} />
        <StatCard icon={warnCount > 0 ? AlertTriangle : Server} label="Servers > 80%" value={String(warnCount)} />
      </div>

      {/* Server traffic ranking table */}
      {sorted.length > 0 && (
        <div className="mb-6 rounded-lg border">
          <Table>
            <TableHeader>
              <TableRow>
                <SortableHead field="name" onSort={handleSort} sortDir={sortDir} sortField={sortField}>
                  Server
                </SortableHead>
                <TableHead>Inbound</TableHead>
                <TableHead>Outbound</TableHead>
                <SortableHead field="total" onSort={handleSort} sortDir={sortDir} sortField={sortField}>
                  Total
                </SortableHead>
                <TableHead>Limit</TableHead>
                <SortableHead field="percent" onSort={handleSort} sortDir={sortDir} sortField={sortField}>
                  Usage
                </SortableHead>
                <TableHead>Days Left</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {sorted.map((s) => (
                <TableRow key={s.server_id}>
                  <TableCell className="font-medium">{s.name}</TableCell>
                  <TableCell className="tabular-nums">{formatBytes(s.cycle_in)}</TableCell>
                  <TableCell className="tabular-nums">{formatBytes(s.cycle_out)}</TableCell>
                  <TableCell className="tabular-nums">{formatBytes(getTotal(s))}</TableCell>
                  <TableCell className="tabular-nums">
                    {s.traffic_limit != null ? (
                      formatBytes(s.traffic_limit)
                    ) : (
                      <Badge variant="secondary">Unlimited</Badge>
                    )}
                  </TableCell>
                  <TableCell>
                    <UsageBar percent={s.percent_used} />
                  </TableCell>
                  <TableCell className="tabular-nums">
                    {s.days_remaining != null ? (
                      <span>{s.days_remaining}d</span>
                    ) : (
                      <span className="text-muted-foreground">-</span>
                    )}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}

      {sorted.length === 0 && (
        <div className="mb-6 rounded-lg border bg-card p-12 text-center">
          <p className="text-muted-foreground">No servers with traffic data yet.</p>
        </div>
      )}

      {/* Global 30-day trend chart */}
      {dailyData && dailyData.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle>Global Traffic Trend (Last 30 Days)</CardTitle>
          </CardHeader>
          <CardContent>
            <ChartContainer className="h-[300px] w-full" config={trendConfig}>
              <AreaChart accessibilityLayer data={dailyData}>
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
                <Area
                  dataKey="bytes_in"
                  fill="var(--color-bytes_in)"
                  fillOpacity={0.15}
                  stroke="var(--color-bytes_in)"
                  strokeWidth={2}
                  type="monotone"
                />
                <Area
                  dataKey="bytes_out"
                  fill="var(--color-bytes_out)"
                  fillOpacity={0.15}
                  stroke="var(--color-bytes_out)"
                  strokeWidth={2}
                  type="monotone"
                />
              </AreaChart>
            </ChartContainer>
          </CardContent>
        </Card>
      )}
    </div>
  )
}
