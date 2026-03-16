import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { createFileRoute, Link } from '@tanstack/react-router'
import { ArrowLeft, CreditCard, FileText, Pencil, Terminal as TerminalIcon } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { MetricsChart } from '@/components/server/metrics-chart'
import { ServerEditDialog } from '@/components/server/server-edit-dialog'
import { StatusBadge } from '@/components/server/status-badge'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { Switch } from '@/components/ui/switch'
import { useServer, useServerRecords } from '@/hooks/use-api'
import { useAuth } from '@/hooks/use-auth'
import { useRealtimeMetrics } from '@/hooks/use-realtime-metrics'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { api } from '@/lib/api-client'
import type { ServerResponse } from '@/lib/api-schema'
import { CAP_FILE, CAPABILITIES, hasCap } from '@/lib/capabilities'
import { cn, countryCodeToFlag, formatBytes } from '@/lib/utils'

export const Route = createFileRoute('/_authed/servers/$id')({
  component: ServerDetailPage,
  validateSearch: (search: Record<string, unknown>) => ({
    range: (search.range as string) || 'realtime'
  })
})

interface TimeRange {
  hours: number
  interval: string
  label: string
}

interface GpuRecordAggregated {
  gpu_usage_avg: number
  mem_total_avg: number
  mem_used_avg: number
  temperature_avg: number
  time: string
}

interface ServerWithCaps {
  capabilities?: number | null
  id: string
  protocol_version?: number | null
}

const TIME_RANGES: TimeRange[] = [
  { label: 'range_realtime', hours: 0, interval: 'realtime' },
  { label: 'range_1h', hours: 1, interval: 'raw' },
  { label: 'range_6h', hours: 6, interval: 'raw' },
  { label: 'range_24h', hours: 24, interval: 'raw' },
  { label: 'range_7d', hours: 168, interval: 'hourly' },
  { label: 'range_30d', hours: 720, interval: 'hourly' }
]

function formatCurrency(price: number, currency: string): string {
  try {
    return new Intl.NumberFormat(undefined, { style: 'currency', currency }).format(price)
  } catch {
    return `${currency} ${price.toFixed(2)}`
  }
}

function ServerInfoMeta({ server }: { server: ServerResponse }) {
  const { t } = useTranslation('servers')
  return (
    <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-muted-foreground text-sm">
      {server.os && (
        <span>
          {t('detail_os')} {server.os}
        </span>
      )}
      {server.cpu_name && (
        <span>
          {t('detail_cpu')} {server.cpu_name}
          {server.cpu_cores && ` (${t('detail_cores', { count: server.cpu_cores })})`}
          {server.cpu_arch && ` ${server.cpu_arch}`}
        </span>
      )}
      {server.mem_total != null && (
        <span>
          {t('detail_ram')} {formatBytes(server.mem_total)}
        </span>
      )}
      {server.ipv4 && <span>IPv4: {server.ipv4}</span>}
      {server.ipv6 && <span>IPv6: {server.ipv6}</span>}
      {server.kernel_version && <span>Kernel: {server.kernel_version}</span>}
      {server.region && <span>Region: {server.region}</span>}
      {server.agent_version && <span>Agent: v{server.agent_version}</span>}
    </div>
  )
}

function CapabilitiesSection({ server }: { server: ServerWithCaps }) {
  const { t } = useTranslation('servers')
  const { user } = useAuth()
  const queryClient = useQueryClient()

  const mutation = useMutation({
    mutationFn: (newCaps: number) => api.put(`/api/servers/${server.id}`, { capabilities: newCaps }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['servers', server.id] })
    }
  })

  if (user?.role !== 'admin') {
    return null
  }

  const caps = server.capabilities ?? 56

  const toggle = (bit: number) => {
    // biome-ignore lint/suspicious/noBitwiseOperators: intentional capability bitmask toggle
    const newCaps = caps & bit ? caps & ~bit : caps | bit
    mutation.mutate(newCaps, {
      onSuccess: () => {
        toast.success('Capabilities updated')
      },
      onError: (err) => {
        toast.error(err instanceof Error ? err.message : 'Operation failed')
      }
    })
  }

  return (
    <div className="mt-6 rounded-lg border bg-card p-6">
      <div className="mb-4 flex items-center justify-between">
        <h3 className="font-semibold">{t('cap_toggles')}</h3>
        {server.protocol_version != null && server.protocol_version < 2 && (
          <span className="rounded bg-amber-100 px-2 py-1 text-amber-600 text-xs dark:bg-amber-900/30 dark:text-amber-400">
            {t('cap_upgrade_warning')}
          </span>
        )}
      </div>
      <div className="space-y-3">
        {CAPABILITIES.map(({ bit, labelKey, risk }) => (
          <div className="flex items-center justify-between" key={bit}>
            <div className="flex items-center gap-2">
              <span>{t(labelKey)}</span>
              <span
                className={`rounded px-1.5 py-0.5 text-xs ${
                  risk === 'high'
                    ? 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400'
                    : 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400'
                }`}
              >
                {risk === 'high' ? t('cap_high_risk') : t('cap_low_risk')}
              </span>
            </div>
            <Switch
              checked={
                // biome-ignore lint/suspicious/noBitwiseOperators: intentional capability bitmask check
                !!(caps & bit)
              }
              disabled={mutation.isPending}
              onCheckedChange={() => toggle(bit)}
            />
          </div>
        ))}
      </div>
    </div>
  )
}

function ServerDetailPage() {
  const { t } = useTranslation('servers')
  const { id } = Route.useParams()
  const navigate = Route.useNavigate()
  const { range: rangeParam } = Route.useSearch()
  const [editOpen, setEditOpen] = useState(false)

  const selectedRange = TIME_RANGES.findIndex((tr) => tr.interval === rangeParam)
  const rangeIndex = selectedRange >= 0 ? selectedRange : 0
  const range = TIME_RANGES[rangeIndex]
  const isRealtime = range.interval === 'realtime'

  const { data: server, isLoading: serverLoading } = useServer(id)
  const realtimeData = useRealtimeMetrics(id)
  const { data: records } = useServerRecords(id, range.hours, range.interval, { enabled: !isRealtime })

  const { data: gpuRecords } = useQuery<GpuRecordAggregated[]>({
    queryKey: ['servers', id, 'gpu-records', range.hours],
    queryFn: () => {
      const now = new Date()
      const gpuFrom = new Date(now.getTime() - range.hours * 3600 * 1000).toISOString()
      const gpuTo = now.toISOString()
      return api.get<GpuRecordAggregated[]>(
        `/api/servers/${id}/gpu-records?from=${encodeURIComponent(gpuFrom)}&to=${encodeURIComponent(gpuTo)}`
      )
    },
    enabled: id.length > 0 && !isRealtime,
    refetchInterval: 60_000
  })

  const { data: liveServers } = useQuery<ServerMetrics[]>({
    queryKey: ['servers'],
    queryFn: () => [],
    staleTime: Number.POSITIVE_INFINITY,
    refetchOnMount: false,
    refetchOnWindowFocus: false
  })
  const liveData = liveServers?.find((s) => s.id === id)

  const chartData: Record<string, unknown>[] = useMemo(() => {
    if (isRealtime) {
      return realtimeData as unknown as Record<string, unknown>[]
    }
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
      net_in_transfer: r.net_in_transfer,
      net_out_transfer: r.net_out_transfer,
      load1: r.load1,
      load5: r.load5,
      load15: r.load15,
      temperature: r.temperature
    }))
  }, [isRealtime, realtimeData, records, server])

  const realtimeFormatTime = useMemo(() => {
    if (!isRealtime) {
      return undefined
    }
    const firstTimestamp = realtimeData.length > 0 ? realtimeData[0].timestamp : ''
    return (time: string) => {
      if (time === firstTimestamp) {
        return new Date(time).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })
      }
      const d = new Date(time)
      return `${String(d.getMinutes()).padStart(2, '0')}:${String(d.getSeconds()).padStart(2, '0')}`
    }
  }, [isRealtime, realtimeData])

  const gpuChartData = useMemo(() => {
    if (!gpuRecords || gpuRecords.length === 0) {
      return []
    }
    return gpuRecords.map((r) => ({
      timestamp: r.time,
      gpu_usage: r.gpu_usage_avg,
      gpu_temp: r.temperature_avg,
      gpu_mem_pct: r.mem_total_avg > 0 ? (r.mem_used_avg / r.mem_total_avg) * 100 : 0
    }))
  }, [gpuRecords])

  const hasTemperature =
    !isRealtime && chartData.some((d) => 'temperature' in d && d.temperature != null && (d.temperature as number) > 0)
  const hasGpu = !isRealtime && gpuChartData.length > 0

  if (serverLoading) {
    return (
      <div className="space-y-6">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-4 w-96" />
        <div className="grid gap-4 lg:grid-cols-2">
          <Skeleton className="h-64" />
          <Skeleton className="h-64" />
        </div>
      </div>
    )
  }

  if (!server) {
    return (
      <div className="flex min-h-[400px] items-center justify-center">
        <p className="text-muted-foreground">{t('detail_not_found')}</p>
      </div>
    )
  }

  const serverWithCaps = server as ServerResponse & ServerWithCaps
  const isOnline = liveData?.online ?? false
  const hasBilling = server.price != null || server.expired_at != null || server.traffic_limit != null
  const flag = countryCodeToFlag(server.country_code)
  // biome-ignore lint/suspicious/noBitwiseOperators: intentional capability bitmask check
  const terminalEnabled = serverWithCaps.capabilities == null || (serverWithCaps.capabilities & 1) !== 0
  const fileEnabled = hasCap(serverWithCaps.capabilities ?? 0, CAP_FILE)

  // Network cumulative traffic from live data
  const liveNetIn = liveData?.net_in_transfer ?? 0
  const liveNetOut = liveData?.net_out_transfer ?? 0

  return (
    <div>
      <div className="mb-6">
        <Link
          className="mb-3 inline-flex items-center gap-1 text-muted-foreground text-sm hover:text-foreground"
          to="/"
        >
          <ArrowLeft aria-hidden="true" className="size-4" />
          {t('detail_back')}
        </Link>

        <div className="flex items-start justify-between">
          <div>
            <div className="flex items-center gap-3">
              {flag && <span className="text-xl">{flag}</span>}
              <h1 className="font-bold text-2xl">{server.name}</h1>
              <StatusBadge online={isOnline} />
            </div>
            <ServerInfoMeta server={server} />
          </div>
          <div className="flex gap-2">
            <Button onClick={() => setEditOpen(true)} size="sm" variant="outline">
              <Pencil aria-hidden="true" className="mr-1 size-4" />
              {t('detail_edit')}
            </Button>
            {isOnline && terminalEnabled && (
              <Link params={{ serverId: id }} to="/terminal/$serverId">
                <Button size="sm" variant="outline">
                  <TerminalIcon aria-hidden="true" className="mr-1 size-4" />
                  {t('detail_terminal')}
                </Button>
              </Link>
            )}
            {isOnline && fileEnabled && (
              <Link params={{ serverId: id }} search={{}} to="/files/$serverId">
                <Button size="sm" variant="outline">
                  <FileText aria-hidden="true" className="mr-1 size-4" />
                  {t('detail_files')}
                </Button>
              </Link>
            )}
          </div>
        </div>
      </div>

      {hasBilling && <BillingInfoBar server={server} />}

      {isOnline && (liveNetIn > 0 || liveNetOut > 0) && (
        <div className="mb-6 flex flex-wrap gap-6 rounded-lg border bg-card p-3 text-sm">
          <span className="text-muted-foreground">
            {t('detail_network_in')} <span className="font-medium text-foreground">{formatBytes(liveNetIn)}</span>
          </span>
          <span className="text-muted-foreground">
            {t('detail_network_out')} <span className="font-medium text-foreground">{formatBytes(liveNetOut)}</span>
          </span>
          <span className="text-muted-foreground">
            {t('detail_network_total')}{' '}
            <span className="font-medium text-foreground">{formatBytes(liveNetIn + liveNetOut)}</span>
          </span>
        </div>
      )}

      <div className="mb-4 flex gap-1">
        {TIME_RANGES.map((tr, i) => (
          <Button
            className={cn(rangeIndex === i && 'bg-primary text-primary-foreground')}
            key={tr.label}
            onClick={() => navigate({ search: (prev) => ({ ...prev, range: tr.interval }) })}
            size="sm"
            variant={rangeIndex === i ? 'default' : 'outline'}
          >
            {t(tr.label)}
          </Button>
        ))}
      </div>

      <div className="grid gap-4 lg:grid-cols-2">
        <MetricsChart
          color="var(--color-chart-1)"
          data={chartData}
          dataKey="cpu"
          formatTime={realtimeFormatTime}
          title={t('chart_cpu')}
          unit="%"
        />
        <MetricsChart
          color="var(--color-chart-2)"
          data={chartData}
          dataKey="memory_pct"
          formatTime={realtimeFormatTime}
          title={t('chart_memory')}
          unit="%"
        />
        <MetricsChart
          color="var(--color-chart-3)"
          data={chartData}
          dataKey="disk_pct"
          formatTime={realtimeFormatTime}
          title={t('chart_disk')}
          unit="%"
        />
        <MetricsChart
          color="var(--color-chart-4)"
          data={chartData}
          dataKey="net_in_speed"
          formatTime={realtimeFormatTime}
          formatValue={(v) => formatBytes(v)}
          title={t('chart_net_in')}
        />
        <MetricsChart
          color="var(--color-chart-5)"
          data={chartData}
          dataKey="net_out_speed"
          formatTime={realtimeFormatTime}
          formatValue={(v) => formatBytes(v)}
          title={t('chart_net_out')}
        />
        <MetricsChart
          color="var(--color-chart-1)"
          data={chartData}
          dataKey="load1"
          formatTime={realtimeFormatTime}
          title={t('chart_load')}
        />

        {hasTemperature && (
          <MetricsChart
            color="var(--color-chart-4)"
            data={chartData}
            dataKey="temperature"
            formatTime={realtimeFormatTime}
            title={t('chart_temperature')}
            unit="°C"
          />
        )}

        {hasGpu && (
          <>
            <MetricsChart
              color="var(--color-chart-5)"
              data={gpuChartData}
              dataKey="gpu_usage"
              formatTime={realtimeFormatTime}
              title={t('chart_gpu')}
              unit="%"
            />
            <MetricsChart
              color="var(--color-chart-2)"
              data={gpuChartData}
              dataKey="gpu_temp"
              formatTime={realtimeFormatTime}
              title={t('chart_gpu_temp')}
              unit="°C"
            />
          </>
        )}
      </div>

      <CapabilitiesSection server={serverWithCaps} />

      <ServerEditDialog onClose={() => setEditOpen(false)} open={editOpen} server={server} />
    </div>
  )
}

function BillingInfoBar({
  server
}: {
  server: Pick<
    ServerResponse,
    'billing_cycle' | 'currency' | 'expired_at' | 'price' | 'traffic_limit' | 'traffic_limit_type'
  >
}) {
  const { t } = useTranslation('servers')
  const isExpired = server.expired_at ? new Date(server.expired_at) < new Date() : false
  const daysUntilExpiry = server.expired_at
    ? Math.ceil((new Date(server.expired_at).getTime() - Date.now()) / 86_400_000)
    : null

  const expiryColor = (() => {
    if (isExpired) {
      return 'text-destructive'
    }
    if (daysUntilExpiry != null && daysUntilExpiry <= 7) {
      return 'text-yellow-600 dark:text-yellow-400'
    }
    return 'text-muted-foreground'
  })()

  return (
    <div className="mb-6 flex flex-wrap items-center gap-4 rounded-lg border bg-card p-3 text-sm">
      <CreditCard aria-hidden="true" className="size-4 text-muted-foreground" />
      {server.price != null && (
        <span>
          {formatCurrency(server.price, server.currency ?? 'USD')}
          {server.billing_cycle && <span className="text-muted-foreground"> / {server.billing_cycle}</span>}
        </span>
      )}
      {server.expired_at && (
        <span className={cn(expiryColor)}>
          {isExpired
            ? `${t('detail_expired')} ${new Date(server.expired_at).toLocaleDateString()}`
            : `${t('detail_expires')} ${new Date(server.expired_at).toLocaleDateString()}`}
          {daysUntilExpiry != null && !isExpired && ` (${t('detail_expires_days', { count: daysUntilExpiry })})`}
        </span>
      )}
      {server.traffic_limit != null && (
        <span className="text-muted-foreground">
          {t('detail_traffic')} {formatBytes(server.traffic_limit)}
          {server.traffic_limit_type && server.traffic_limit_type !== 'sum' && ` (${server.traffic_limit_type})`}
        </span>
      )}
    </div>
  )
}
