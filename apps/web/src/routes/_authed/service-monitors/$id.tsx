import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute, Link } from '@tanstack/react-router'
import { ArrowLeft, Play, RefreshCw } from 'lucide-react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
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

// API uses #[serde(flatten)] so monitor fields are at the top level
interface MonitorWithRecord extends ServiceMonitor {
  latest_record: ServiceMonitorRecord | null
}

function useTypeLabels(t: (key: string) => string): Record<string, string> {
  return {
    ssl: t('monitorTypes.ssl'),
    dns: t('monitorTypes.dns'),
    http_keyword: t('monitorTypes.http_keyword'),
    tcp: t('monitorTypes.tcp'),
    whois: t('monitorTypes.whois')
  }
}

// ---------------------------------------------------------------------------
// Stats Row
// ---------------------------------------------------------------------------

function StatsRow({ records, t }: { records: ServiceMonitorRecord[]; t: (key: string) => string }) {
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
          <CardTitle className="text-muted-foreground text-xs">{t('stats.uptime')}</CardTitle>
        </CardHeader>
        <CardContent>
          <p className="font-bold text-2xl">{stats.uptime != null ? `${stats.uptime.toFixed(1)}%` : '--'}</p>
        </CardContent>
      </Card>
      <Card size="sm">
        <CardHeader>
          <CardTitle className="text-muted-foreground text-xs">{t('stats.avgLatency')}</CardTitle>
        </CardHeader>
        <CardContent>
          <p className="font-bold text-2xl">{stats.avgLatency != null ? `${stats.avgLatency.toFixed(1)} ms` : '--'}</p>
        </CardContent>
      </Card>
      <Card size="sm">
        <CardHeader>
          <CardTitle className="text-muted-foreground text-xs">{t('stats.lastCheck')}</CardTitle>
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

function ResponseTimeChart({ records, t }: { records: ServiceMonitorRecord[]; t: (key: string) => string }) {
  const chartData = useMemo(() => {
    // Records come in desc order from API, reverse for chronological chart
    return [...records].reverse().map((r) => ({
      timestamp: r.time,
      latency: r.success ? r.latency : null
    }))
  }, [records])

  const latencyConfig = {
    latency: { label: t('chart.latency'), color: 'var(--chart-4)' }
  } satisfies ChartConfig

  if (chartData.length === 0) {
    return (
      <div className="rounded-lg border bg-card p-6 text-center text-muted-foreground text-sm">
        {t('chart.noRecords')}
      </div>
    )
  }

  return (
    <div className="rounded-lg border bg-card p-4">
      <h3 className="mb-3 font-semibold text-sm">{t('chart.responseTime')}</h3>
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

function SslDetail({ detail, t }: { detail: Record<string, unknown>; t: (key: string) => string }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('detail.ssl.title')}</CardTitle>
      </CardHeader>
      <CardContent>
        <dl className="grid gap-2 text-sm sm:grid-cols-2">
          <DetailItem label={t('detail.ssl.subject')} value={detail.subject as string} />
          <DetailItem label={t('detail.ssl.issuer')} value={detail.issuer as string} />
          <DetailItem label={t('detail.ssl.notBefore')} value={detail.not_before as string} />
          <DetailItem label={t('detail.ssl.notAfter')} value={detail.not_after as string} />
          <DetailItem label={t('detail.ssl.daysRemaining')} value={String(detail.days_remaining ?? '--')} />
          <DetailItem label={t('detail.ssl.sha256Fingerprint')} value={detail.sha256_fingerprint as string} />
          {Boolean(detail.warning) && <DetailItem label={t('detail.ssl.warning')} value={detail.warning as string} />}
        </dl>
      </CardContent>
    </Card>
  )
}

function DnsDetail({ detail, t }: { detail: Record<string, unknown>; t: (key: string) => string }) {
  const values = Array.isArray(detail.values) ? (detail.values as string[]) : []
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('detail.dns.title')}</CardTitle>
      </CardHeader>
      <CardContent>
        <dl className="grid gap-2 text-sm sm:grid-cols-2">
          <DetailItem label={t('detail.dns.recordType')} value={detail.record_type as string} />
          <DetailItem label={t('detail.dns.nameserver')} value={detail.nameserver as string} />
          <DetailItem
            label={t('detail.dns.changed')}
            value={detail.changed ? t('detail.dns.yes') : t('detail.dns.no')}
          />
        </dl>
        {values.length > 0 && (
          <div className="mt-3">
            <p className="mb-1 font-medium text-sm">{t('detail.dns.resolvedValues')}</p>
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

function HttpKeywordDetail({ detail, t }: { detail: Record<string, unknown>; t: (key: string) => string }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('detail.http.title')}</CardTitle>
      </CardHeader>
      <CardContent>
        <dl className="grid gap-2 text-sm sm:grid-cols-2">
          <DetailItem label={t('detail.http.statusCode')} value={String(detail.status_code ?? '--')} />
          <DetailItem label={t('detail.http.keywordFound')} value={formatKeywordFound(detail.keyword_found, t)} />
          <DetailItem
            label={t('detail.http.responseTime')}
            value={detail.response_time_ms ? `${Number(detail.response_time_ms).toFixed(1)} ms` : '--'}
          />
        </dl>
      </CardContent>
    </Card>
  )
}

function TcpDetail({ detail, t }: { detail: Record<string, unknown>; t: (key: string) => string }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('detail.tcp.title')}</CardTitle>
      </CardHeader>
      <CardContent>
        <dl className="grid gap-2 text-sm">
          <DetailItem
            label={t('detail.tcp.connected')}
            value={detail.connected ? t('detail.tcp.yes') : t('detail.tcp.no')}
          />
        </dl>
      </CardContent>
    </Card>
  )
}

function WhoisDetail({ detail, t }: { detail: Record<string, unknown>; t: (key: string) => string }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('detail.whois.title')}</CardTitle>
      </CardHeader>
      <CardContent>
        <dl className="grid gap-2 text-sm sm:grid-cols-2">
          <DetailItem label={t('detail.whois.registrar')} value={(detail.registrar as string) || '--'} />
          <DetailItem label={t('detail.whois.expiryDate')} value={(detail.expiry_date as string) || '--'} />
          <DetailItem label={t('detail.whois.daysRemaining')} value={String(detail.days_remaining ?? '--')} />
          {Boolean(detail.warning) && <DetailItem label={t('detail.whois.warning')} value={detail.warning as string} />}
        </dl>
      </CardContent>
    </Card>
  )
}

function formatKeywordFound(value: unknown, t: (key: string) => string): string {
  if (value == null) {
    return t('detail.http.na')
  }
  return value ? t('detail.http.yes') : t('detail.http.no')
}

function DetailItem({ label, value }: { label: string; value: string | undefined }) {
  return (
    <div>
      <dt className="text-muted-foreground text-xs">{label}</dt>
      <dd className="break-all font-mono text-sm">{value ?? '--'}</dd>
    </div>
  )
}

function TypeSpecificDetail({
  detail,
  type,
  t
}: {
  detail: Record<string, unknown>
  type: string
  t: (key: string) => string
}) {
  switch (type) {
    case 'ssl':
      return <SslDetail detail={detail} t={t} />
    case 'dns':
      return <DnsDetail detail={detail} t={t} />
    case 'http_keyword':
      return <HttpKeywordDetail detail={detail} t={t} />
    case 'tcp':
      return <TcpDetail detail={detail} t={t} />
    case 'whois':
      return <WhoisDetail detail={detail} t={t} />
    default:
      return null
  }
}

// ---------------------------------------------------------------------------
// History Table
// ---------------------------------------------------------------------------

function HistoryTable({ records, t }: { records: ServiceMonitorRecord[]; t: (key: string) => string }) {
  if (records.length === 0) {
    return <p className="py-4 text-center text-muted-foreground text-sm">{t('history.empty')}</p>
  }

  return (
    <div className="rounded-lg border">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>{t('history.table.time')}</TableHead>
            <TableHead>{t('history.table.status')}</TableHead>
            <TableHead>{t('history.table.latency')}</TableHead>
            <TableHead>{t('history.table.error')}</TableHead>
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
                    {t('history.status.ok')}
                  </span>
                ) : (
                  <span className="inline-flex items-center gap-1 text-red-600 text-xs dark:text-red-400">
                    <span className="inline-block size-2 rounded-full bg-red-500" />
                    {t('history.status.fail')}
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
  const { t } = useTranslation('service-monitors')
  const { t: tCommon } = useTranslation('common')
  const TYPE_LABELS = useTypeLabels(t)

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
      toast.success(t('toast.checkTriggered'))
    },
    onError: (err) => {
      toast.error(err instanceof Error ? err.message : t('toast.triggerFailed'))
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
        <p className="text-muted-foreground">{t('notFound')}</p>
      </div>
    )
  }

  // data is flat (serde flatten) — monitor fields are at top level
  const monitor = data
  const latestRecord = data.latest_record

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
          {t('navigation.backToList')}
        </Link>

        <div className="flex items-start justify-between">
          <div>
            <div className="flex items-center gap-3">
              <h1 className="font-bold text-2xl">{monitor.name}</h1>
              <Badge variant="secondary">{TYPE_LABELS[monitor.monitor_type] ?? monitor.monitor_type}</Badge>
              {monitor.last_status === true && (
                <Badge className="bg-emerald-500/10 text-emerald-600 dark:text-emerald-400" variant="outline">
                  {tCommon('status.online')}
                </Badge>
              )}
              {monitor.last_status === false && (
                <Badge className="bg-red-500/10 text-red-600 dark:text-red-400" variant="outline">
                  {tCommon('status.offline')}
                </Badge>
              )}
              {monitor.last_status == null && <Badge variant="outline">{tCommon('status.pending')}</Badge>}
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
              {t('actions.checkNow')}
            </Button>
          </div>
        </div>
      </div>

      <div className="space-y-6">
        {/* Stats */}
        <StatsRow records={records} t={t} />

        {/* Response Time Chart */}
        <ResponseTimeChart records={records} t={t} />

        {/* Type-Specific Detail */}
        {latestRecord && Object.keys(latestDetail).length > 0 && (
          <TypeSpecificDetail detail={latestDetail} t={t} type={monitor.monitor_type} />
        )}

        {/* History Table */}
        <div>
          <h3 className="mb-3 font-semibold text-lg">{t('history.title')}</h3>
          <HistoryTable records={records.slice(0, 50)} t={t} />
        </div>
      </div>
    </div>
  )
}
