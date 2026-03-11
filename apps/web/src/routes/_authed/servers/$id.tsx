import { createFileRoute, Link } from '@tanstack/react-router'
import { ArrowLeft } from 'lucide-react'
import { useMemo, useState } from 'react'
import { MetricsChart } from '@/components/server/metrics-chart'
import { StatusBadge } from '@/components/server/status-badge'
import { Button } from '@/components/ui/button'
import { useServer, useServerRecords } from '@/hooks/use-api'
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
  { label: '1h', hours: 1, interval: '1m' },
  { label: '6h', hours: 6, interval: '5m' },
  { label: '24h', hours: 24, interval: '15m' },
  { label: '7d', hours: 168, interval: '1h' },
  { label: '30d', hours: 720, interval: '6h' }
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

function ServerDetailPage() {
  const { id } = Route.useParams()
  const [selectedRange, setSelectedRange] = useState(1)

  const range = TIME_RANGES[selectedRange]
  const now = useMemo(() => new Date(), [])
  const from = new Date(now.getTime() - range.hours * 3600 * 1000).toISOString()
  const to = now.toISOString()

  const { data: server, isLoading: serverLoading } = useServer(id)
  const { data: records } = useServerRecords(id, from, to, range.interval)

  const chartData = useMemo(() => {
    if (!records) {
      return []
    }
    return records.map((r) => ({
      timestamp: r.timestamp,
      cpu_usage: r.cpu_usage,
      memory_pct: r.memory_total > 0 ? (r.memory_used / r.memory_total) * 100 : 0,
      disk_pct: r.disk_total > 0 ? (r.disk_used / r.disk_total) * 100 : 0,
      network_in: r.network_in,
      network_out: r.network_out,
      load_1: r.load_avg[0],
      load_5: r.load_avg[1],
      load_15: r.load_avg[2]
    }))
  }, [records])

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
              <StatusBadge online={server.online} />
            </div>
            <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-muted-foreground text-sm">
              <span>OS: {server.os}</span>
              <span>CPU: {server.cpu_name}</span>
              <span>RAM: {formatBytes(server.memory_total)}</span>
              <span>IP: {server.ip}</span>
            </div>
          </div>
        </div>
      </div>

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
        <MetricsChart color="var(--color-chart-1)" data={chartData} dataKey="cpu_usage" title="CPU Usage" unit="%" />
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
          dataKey="network_in"
          formatValue={(v) => formatBytes(v)}
          title="Network In"
        />
        <MetricsChart
          color="var(--color-chart-5)"
          data={chartData}
          dataKey="network_out"
          formatValue={(v) => formatBytes(v)}
          title="Network Out"
        />
        <MetricsChart color="var(--color-chart-1)" data={chartData} dataKey="load_1" title="Load Average (1m)" />
      </div>
    </div>
  )
}
