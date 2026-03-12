import { useQueryClient } from '@tanstack/react-query'
import { createFileRoute, Link } from '@tanstack/react-router'
import { ArrowLeft, CreditCard, Pencil, Terminal as TerminalIcon } from 'lucide-react'
import { useMemo, useState } from 'react'
import { MetricsChart } from '@/components/server/metrics-chart'
import { ServerEditDialog } from '@/components/server/server-edit-dialog'
import { StatusBadge } from '@/components/server/status-badge'
import { Button } from '@/components/ui/button'
import { useServer, useServerRecords } from '@/hooks/use-api'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { cn } from '@/lib/utils'

export const Route = createFileRoute('/_authed/servers/$id')({
  component: ServerDetailPage
})

interface TimeRange {
  hours: number
  interval: string
  label: string
}

const TIME_RANGES: TimeRange[] = [
  { label: '1h', hours: 1, interval: 'raw' },
  { label: '6h', hours: 6, interval: 'raw' },
  { label: '24h', hours: 24, interval: 'raw' },
  { label: '7d', hours: 168, interval: 'hourly' },
  { label: '30d', hours: 720, interval: 'hourly' }
]

function formatBytes(bytes: number): string {
  if (bytes === 0) {
    return '0 B'
  }
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(1024))
  const value = bytes / 1024 ** i
  return `${value.toFixed(1)} ${units[i]}`
}

function formatCurrency(price: number, currency: string): string {
  try {
    return new Intl.NumberFormat(undefined, { style: 'currency', currency }).format(price)
  } catch {
    return `${currency} ${price.toFixed(2)}`
  }
}

function ServerDetailPage() {
  const { id } = Route.useParams()
  const [selectedRange, setSelectedRange] = useState(1)
  const [editOpen, setEditOpen] = useState(false)

  const range = TIME_RANGES[selectedRange]
  const now = useMemo(() => new Date(), [])
  const from = new Date(now.getTime() - range.hours * 3600 * 1000).toISOString()
  const to = now.toISOString()

  const { data: server, isLoading: serverLoading } = useServer(id)
  const { data: records } = useServerRecords(id, from, to, range.interval)

  const queryClient = useQueryClient()
  const liveServers = queryClient.getQueryData<ServerMetrics[]>(['servers'])
  const liveData = liveServers?.find((s) => s.id === id)

  const chartData = useMemo(() => {
    if (!records) {
      return []
    }
    return records.map((r) => ({
      timestamp: r.time,
      cpu: r.cpu,
      memory_pct: server?.mem_total ? (r.mem_used / server.mem_total) * 100 : 0,
      disk_pct: server?.disk_total ? (r.disk_used / server.disk_total) * 100 : 0,
      net_in_speed: r.net_in_speed,
      net_out_speed: r.net_out_speed,
      load1: r.load1,
      load5: r.load5,
      load15: r.load15
    }))
  }, [records, server])

  if (serverLoading) {
    return (
      <div className="flex min-h-[400px] items-center justify-center">
        <div className="mx-auto size-8 animate-spin rounded-full border-4 border-muted border-t-primary" />
      </div>
    )
  }

  if (!server) {
    return (
      <div className="flex min-h-[400px] items-center justify-center">
        <p className="text-muted-foreground">Server not found</p>
      </div>
    )
  }

  const isOnline = liveData?.online ?? false
  const hasBilling = server.price != null || server.expired_at != null || server.traffic_limit != null

  return (
    <div>
      <div className="mb-6">
        <Link
          className="mb-3 inline-flex items-center gap-1 text-muted-foreground text-sm hover:text-foreground"
          to="/"
        >
          <ArrowLeft className="size-4" />
          Back to Dashboard
        </Link>

        <div className="flex items-start justify-between">
          <div>
            <div className="flex items-center gap-3">
              <h1 className="font-bold text-2xl">{server.name}</h1>
              <StatusBadge online={isOnline} />
            </div>
            <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-muted-foreground text-sm">
              {server.os && <span>OS: {server.os}</span>}
              {server.cpu_name && (
                <span>
                  CPU: {server.cpu_name} ({server.cpu_cores} cores)
                </span>
              )}
              {server.mem_total != null && <span>RAM: {formatBytes(server.mem_total)}</span>}
              {server.ipv4 && <span>IP: {server.ipv4}</span>}
            </div>
          </div>
          <div className="flex gap-2">
            <Button onClick={() => setEditOpen(true)} size="sm" variant="outline">
              <Pencil className="mr-1 size-4" />
              Edit
            </Button>
            {isOnline && (
              <Link params={{ serverId: id }} to="/terminal/$serverId">
                <Button size="sm" variant="outline">
                  <TerminalIcon className="mr-1 size-4" />
                  Terminal
                </Button>
              </Link>
            )}
          </div>
        </div>
      </div>

      {hasBilling && <BillingInfoBar server={server} />}

      <div className="mb-4 flex gap-1">
        {TIME_RANGES.map((tr, i) => (
          <Button
            className={cn(selectedRange === i && 'bg-primary text-primary-foreground')}
            key={tr.label}
            onClick={() => setSelectedRange(i)}
            size="sm"
            variant={selectedRange === i ? 'default' : 'outline'}
          >
            {tr.label}
          </Button>
        ))}
      </div>

      <div className="grid gap-4 lg:grid-cols-2">
        <MetricsChart color="var(--color-chart-1)" data={chartData} dataKey="cpu" title="CPU Usage" unit="%" />
        <MetricsChart
          color="var(--color-chart-2)"
          data={chartData}
          dataKey="memory_pct"
          title="Memory Usage"
          unit="%"
        />
        <MetricsChart color="var(--color-chart-3)" data={chartData} dataKey="disk_pct" title="Disk Usage" unit="%" />
        <MetricsChart
          color="var(--color-chart-4)"
          data={chartData}
          dataKey="net_in_speed"
          formatValue={(v) => formatBytes(v)}
          title="Network In"
        />
        <MetricsChart
          color="var(--color-chart-5)"
          data={chartData}
          dataKey="net_out_speed"
          formatValue={(v) => formatBytes(v)}
          title="Network Out"
        />
        <MetricsChart color="var(--color-chart-1)" data={chartData} dataKey="load1" title="Load Average (1m)" />
      </div>

      <ServerEditDialog onClose={() => setEditOpen(false)} open={editOpen} server={server} />
    </div>
  )
}

function BillingInfoBar({
  server
}: {
  server: {
    billing_cycle: string | null
    currency: string | null
    expired_at: string | null
    price: number | null
    traffic_limit: number | null
    traffic_limit_type: string | null
  }
}) {
  const isExpired = server.expired_at ? new Date(server.expired_at) < new Date() : false
  const daysUntilExpiry = server.expired_at
    ? Math.ceil((new Date(server.expired_at).getTime() - Date.now()) / 86_400_000)
    : null

  return (
    <div className="mb-6 flex flex-wrap items-center gap-4 rounded-lg border bg-card p-3 text-sm">
      <CreditCard className="size-4 text-muted-foreground" />
      {server.price != null && (
        <span>
          {formatCurrency(server.price, server.currency ?? 'USD')}
          {server.billing_cycle && <span className="text-muted-foreground"> / {server.billing_cycle}</span>}
        </span>
      )}
      {server.expired_at && (
        <span
          className={cn(
            isExpired
              ? 'text-destructive'
              : daysUntilExpiry != null && daysUntilExpiry <= 7
                ? 'text-yellow-600 dark:text-yellow-400'
                : 'text-muted-foreground'
          )}
        >
          {isExpired
            ? `Expired ${new Date(server.expired_at).toLocaleDateString()}`
            : `Expires ${new Date(server.expired_at).toLocaleDateString()}`}
          {daysUntilExpiry != null && !isExpired && ` (${daysUntilExpiry}d)`}
        </span>
      )}
      {server.traffic_limit != null && (
        <span className="text-muted-foreground">
          Traffic: {formatBytes(server.traffic_limit)}
          {server.traffic_limit_type && server.traffic_limit_type !== 'sum' && ` (${server.traffic_limit_type})`}
        </span>
      )}
    </div>
  )
}
