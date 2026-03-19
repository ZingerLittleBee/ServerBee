import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute, Link } from '@tanstack/react-router'
import { ArrowLeft, Play, RefreshCw } from 'lucide-react'
import { useMemo } from 'react'
import { Area, AreaChart, CartesianGrid, XAxis, YAxis } from 'recharts'
import { toast } from 'sonner'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { type ChartConfig, ChartContainer, ChartTooltip, ChartTooltipContent } from '@/components/ui/chart'
import { Skeleton } from '@/components/ui/skeleton'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { api } from '@/lib/api-client'

export const Route = createFileRoute('/_authed/service-monitors/$id')({
  component: ServiceMonitorDetailPage
})

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface ServiceMonitor {
  config_json: string
  consecutive_failures: number
  created_at: string
  enabled: boolean
  id: string
  interval: number
  last_checked_at: string | null
  last_status: boolean | null
  monitor_type: string
  name: string
  notification_group_id: string | null
  retry_count: number
  server_ids_json: string | null
  target: string
  updated_at: string
}

interface ServiceMonitorRecord {
  detail_json: string
  error: string | null
  id: number
  latency: number | null
  monitor_id: string
  success: boolean
  time: string
}

interface MonitorWithRecord {
  latest_record: ServiceMonitorRecord | null
  monitor: ServiceMonitor
}

const TYPE_LABELS: Record<string, string> = {
  ssl: 'SSL',
  dns: 'DNS',
  http_keyword: 'HTTP Keyword',
  tcp: 'TCP',
  whois: 'WHOIS'
}

// ---------------------------------------------------------------------------
// Stats Row
// ---------------------------------------------------------------------------

function StatsRow({ records }: { records: ServiceMonitorRecord[] }) {
  const stats = useMemo(() => {
    if (records.length === 0) {
      return { uptime: null, avgLatency: null, lastCheck: null }
    }
    const successCount = records.filter((r) => r.success).length
    const uptime = (successCount / records.length) * 100
    const latencies = records.filter((r) => r.latency != null).map((r) => r.latency as number)
    const avgLatency = latencies.length > 0 ? latencies.reduce((a, b) => a + b, 0) / latencies.length : null
    const lastCheck = records[0]?.time ?? null
    return { uptime, avgLatency, lastCheck }
  }, [records])

  return (
    <div className="grid gap-4 sm:grid-cols-3">
      <Card size="sm">
        <CardHeader>
          <CardTitle className="text-muted-foreground text-xs">Uptime</CardTitle>
        </CardHeader>
        <CardContent>
          <p className="font-bold text-2xl">{stats.uptime != null ? `${stats.uptime.toFixed(1)}%` : '--'}</p>
        </CardContent>
      </Card>
      <Card size="sm">
        <CardHeader>
          <CardTitle className="text-muted-foreground text-xs">Avg Latency</CardTitle>
        </CardHeader>
        <CardContent>
          <p className="font-bold text-2xl">{stats.avgLatency != null ? `${stats.avgLatency.toFixed(1)} ms` : '--'}</p>
        </CardContent>
      </Card>
      <Card size="sm">
        <CardHeader>
          <CardTitle className="text-muted-foreground text-xs">Last Check</CardTitle>
        </CardHeader>
        <CardContent>
          <p className="font-bold text-sm">{stats.lastCheck ? new Date(stats.lastCheck).toLocaleString() : '--'}</p>
        </CardContent>
      </Card>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Response Time Chart
// ---------------------------------------------------------------------------

function ResponseTimeChart({ records }: { records: ServiceMonitorRecord[] }) {
  const chartData = useMemo(() => {
    // Records come in desc order from API, reverse for chronological chart
    return [...records].reverse().map((r) => ({
      timestamp: r.time,
      latency: r.success ? r.latency : null
    }))
  }, [records])

  const latencyConfig = {
    latency: { label: 'Latency', color: 'var(--chart-4)' }
  } satisfies ChartConfig

  if (chartData.length === 0) {
    return (
      <div className="rounded-lg border bg-card p-6 text-center text-muted-foreground text-sm">
        No records available for charting.
      </div>
    )
  }

  return (
    <div className="rounded-lg border bg-card p-4">
      <h3 className="mb-3 font-semibold text-sm">Response Time</h3>
      <ChartContainer className="h-[260px] w-full" config={latencyConfig}>
        <AreaChart accessibilityLayer data={chartData}>
          <CartesianGrid vertical={false} />
          <XAxis
            axisLine={false}
            dataKey="timestamp"
            tickFormatter={(v: string) => new Date(v).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
            tickLine={false}
          />
          <YAxis axisLine={false} tickLine={false} width={50} />
          <ChartTooltip
            content={
              <ChartTooltipContent
                labelFormatter={(label) => new Date(String(label)).toLocaleString()}
                valueFormatter={(v) => `${Number(v).toFixed(1)} ms`}
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

// ---------------------------------------------------------------------------
// Type-Specific Detail Cards
// ---------------------------------------------------------------------------

function SslDetail({ detail }: { detail: Record<string, unknown> }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>SSL Certificate</CardTitle>
      </CardHeader>
      <CardContent>
        <dl className="grid gap-2 text-sm sm:grid-cols-2">
          <DetailItem label="Subject" value={detail.subject as string} />
          <DetailItem label="Issuer" value={detail.issuer as string} />
          <DetailItem label="Not Before" value={detail.not_before as string} />
          <DetailItem label="Not After" value={detail.not_after as string} />
          <DetailItem label="Days Remaining" value={String(detail.days_remaining ?? '--')} />
          <DetailItem label="SHA-256 Fingerprint" value={detail.sha256_fingerprint as string} />
          {Boolean(detail.warning) && <DetailItem label="Warning" value={detail.warning as string} />}
        </dl>
      </CardContent>
    </Card>
  )
}

function DnsDetail({ detail }: { detail: Record<string, unknown> }) {
  const values = Array.isArray(detail.values) ? (detail.values as string[]) : []
  return (
    <Card>
      <CardHeader>
        <CardTitle>DNS Resolution</CardTitle>
      </CardHeader>
      <CardContent>
        <dl className="grid gap-2 text-sm sm:grid-cols-2">
          <DetailItem label="Record Type" value={detail.record_type as string} />
          <DetailItem label="Nameserver" value={detail.nameserver as string} />
          <DetailItem label="Changed" value={detail.changed ? 'Yes' : 'No'} />
        </dl>
        {values.length > 0 && (
          <div className="mt-3">
            <p className="mb-1 font-medium text-sm">Resolved Values</p>
            <ul className="list-inside list-disc text-muted-foreground text-sm">
              {values.map((v) => (
                <li key={v}>{v}</li>
              ))}
            </ul>
          </div>
        )}
      </CardContent>
    </Card>
  )
}

function HttpKeywordDetail({ detail }: { detail: Record<string, unknown> }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>HTTP Check</CardTitle>
      </CardHeader>
      <CardContent>
        <dl className="grid gap-2 text-sm sm:grid-cols-2">
          <DetailItem label="Status Code" value={String(detail.status_code ?? '--')} />
          <DetailItem label="Keyword Found" value={formatKeywordFound(detail.keyword_found)} />
          <DetailItem
            label="Response Time"
            value={detail.response_time_ms ? `${Number(detail.response_time_ms).toFixed(1)} ms` : '--'}
          />
        </dl>
      </CardContent>
    </Card>
  )
}

function TcpDetail({ detail }: { detail: Record<string, unknown> }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>TCP Connection</CardTitle>
      </CardHeader>
      <CardContent>
        <dl className="grid gap-2 text-sm">
          <DetailItem label="Connected" value={detail.connected ? 'Yes' : 'No'} />
        </dl>
      </CardContent>
    </Card>
  )
}

function WhoisDetail({ detail }: { detail: Record<string, unknown> }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>WHOIS Information</CardTitle>
      </CardHeader>
      <CardContent>
        <dl className="grid gap-2 text-sm sm:grid-cols-2">
          <DetailItem label="Registrar" value={(detail.registrar as string) || '--'} />
          <DetailItem label="Expiry Date" value={(detail.expiry_date as string) || '--'} />
          <DetailItem label="Days Remaining" value={String(detail.days_remaining ?? '--')} />
          {Boolean(detail.warning) && <DetailItem label="Warning" value={detail.warning as string} />}
        </dl>
      </CardContent>
    </Card>
  )
}

function formatKeywordFound(value: unknown): string {
  if (value == null) {
    return 'N/A'
  }
  return value ? 'Yes' : 'No'
}

function DetailItem({ label, value }: { label: string; value: string | undefined }) {
  return (
    <div>
      <dt className="text-muted-foreground text-xs">{label}</dt>
      <dd className="break-all font-mono text-sm">{value ?? '--'}</dd>
    </div>
  )
}

function TypeSpecificDetail({ detail, type }: { detail: Record<string, unknown>; type: string }) {
  switch (type) {
    case 'ssl':
      return <SslDetail detail={detail} />
    case 'dns':
      return <DnsDetail detail={detail} />
    case 'http_keyword':
      return <HttpKeywordDetail detail={detail} />
    case 'tcp':
      return <TcpDetail detail={detail} />
    case 'whois':
      return <WhoisDetail detail={detail} />
    default:
      return null
  }
}

// ---------------------------------------------------------------------------
// History Table
// ---------------------------------------------------------------------------

function HistoryTable({ records }: { records: ServiceMonitorRecord[] }) {
  if (records.length === 0) {
    return <p className="py-4 text-center text-muted-foreground text-sm">No check history yet.</p>
  }

  return (
    <div className="rounded-lg border">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Time</TableHead>
            <TableHead>Status</TableHead>
            <TableHead>Latency</TableHead>
            <TableHead>Error</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {records.map((record) => (
            <TableRow key={record.id}>
              <TableCell className="text-xs">{new Date(record.time).toLocaleString()}</TableCell>
              <TableCell>
                {record.success ? (
                  <span className="inline-flex items-center gap-1 text-emerald-600 text-xs dark:text-emerald-400">
                    <span className="inline-block size-2 rounded-full bg-emerald-500" />
                    OK
                  </span>
                ) : (
                  <span className="inline-flex items-center gap-1 text-red-600 text-xs dark:text-red-400">
                    <span className="inline-block size-2 rounded-full bg-red-500" />
                    Fail
                  </span>
                )}
              </TableCell>
              <TableCell className="font-mono text-xs">
                {record.latency != null ? `${record.latency.toFixed(1)} ms` : '--'}
              </TableCell>
              <TableCell className="max-w-[300px] truncate text-muted-foreground text-xs">
                {record.error ?? '--'}
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Main Detail Page
// ---------------------------------------------------------------------------

function ServiceMonitorDetailPage() {
  const { id } = Route.useParams()
  const queryClient = useQueryClient()

  const { data, isLoading } = useQuery<MonitorWithRecord>({
    queryKey: ['service-monitor', id],
    queryFn: () => api.get<MonitorWithRecord>(`/api/service-monitors/${id}`)
  })

  const { data: records = [] } = useQuery<ServiceMonitorRecord[]>({
    queryKey: ['service-monitor-records', id],
    queryFn: () => api.get<ServiceMonitorRecord[]>(`/api/service-monitors/${id}/records?limit=100`)
  })

  const triggerMutation = useMutation({
    mutationFn: () => api.post(`/api/service-monitors/${id}/check`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['service-monitor', id] }).catch(() => undefined)
      queryClient.invalidateQueries({ queryKey: ['service-monitor-records', id] }).catch(() => undefined)
      toast.success('Check triggered')
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : 'Failed to trigger check')
    }
  })

  if (isLoading) {
    return (
      <div className="space-y-6">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-4 w-96" />
        <div className="grid gap-4 sm:grid-cols-3">
          <Skeleton className="h-24" />
          <Skeleton className="h-24" />
          <Skeleton className="h-24" />
        </div>
        <Skeleton className="h-64" />
      </div>
    )
  }

  if (!data) {
    return (
      <div className="flex min-h-[400px] items-center justify-center">
        <p className="text-muted-foreground">Service monitor not found.</p>
      </div>
    )
  }

  const { monitor, latest_record: latestRecord } = data

  let latestDetail: Record<string, unknown> = {}
  if (latestRecord?.detail_json) {
    try {
      latestDetail = JSON.parse(latestRecord.detail_json) as Record<string, unknown>
    } catch {
      // ignore malformed JSON
    }
  }

  return (
    <div>
      {/* Header */}
      <div className="mb-6">
        <Link
          className="mb-3 inline-flex items-center gap-1 text-muted-foreground text-sm hover:text-foreground"
          to="/settings/service-monitors"
        >
          <ArrowLeft aria-hidden="true" className="size-4" />
          Back to Service Monitors
        </Link>

        <div className="flex items-start justify-between">
          <div>
            <div className="flex items-center gap-3">
              <h1 className="font-bold text-2xl">{monitor.name}</h1>
              <Badge variant="secondary">{TYPE_LABELS[monitor.monitor_type] ?? monitor.monitor_type}</Badge>
              {monitor.last_status === true && (
                <Badge className="bg-emerald-500/10 text-emerald-600 dark:text-emerald-400" variant="outline">
                  Online
                </Badge>
              )}
              {monitor.last_status === false && (
                <Badge className="bg-red-500/10 text-red-600 dark:text-red-400" variant="outline">
                  Offline
                </Badge>
              )}
              {monitor.last_status == null && <Badge variant="outline">Pending</Badge>}
            </div>
            <p className="mt-1 font-mono text-muted-foreground text-sm">{monitor.target}</p>
          </div>
          <div className="flex gap-2">
            <Button
              disabled={triggerMutation.isPending}
              onClick={() => triggerMutation.mutate()}
              size="sm"
              variant="outline"
            >
              {triggerMutation.isPending ? <RefreshCw className="size-4 animate-spin" /> : <Play className="size-4" />}
              Check Now
            </Button>
          </div>
        </div>
      </div>

      <div className="space-y-6">
        {/* Stats */}
        <StatsRow records={records} />

        {/* Response Time Chart */}
        <ResponseTimeChart records={records} />

        {/* Type-Specific Detail */}
        {latestRecord && Object.keys(latestDetail).length > 0 && (
          <TypeSpecificDetail detail={latestDetail} type={monitor.monitor_type} />
        )}

        {/* History Table */}
        <div>
          <h3 className="mb-3 font-semibold text-lg">Check History</h3>
          <HistoryTable records={records.slice(0, 50)} />
        </div>
      </div>
    </div>
  )
}
