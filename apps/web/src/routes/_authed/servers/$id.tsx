import { useQuery } from '@tanstack/react-query'
import { createFileRoute, Link } from '@tanstack/react-router'
import { ArrowLeft, BarChart3, Container, CreditCard, FileText, Pencil, Terminal as TerminalIcon } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { CapabilitiesDialog } from '@/components/server/capabilities-dialog'
import { DiskIoChart } from '@/components/server/disk-io-chart'
import { MetricsChart } from '@/components/server/metrics-chart'
import { ServerEditDialog } from '@/components/server/server-edit-dialog'
import { StatusBadge } from '@/components/server/status-badge'
import { TrafficCard } from '@/components/server/traffic-card'
import { TrafficProgress } from '@/components/server/traffic-progress'
import { TrafficTab } from '@/components/server/traffic-tab'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { useServer, useServerRecords } from '@/hooks/use-api'
import { useRealtimeMetrics } from '@/hooks/use-realtime-metrics'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { api } from '@/lib/api-client'
import type { ServerResponse } from '@/lib/api-schema'
import { CAP_DOCKER, CAP_FILE, hasCap } from '@/lib/capabilities'
import { buildMergedDiskIoSeries, buildPerDiskIoSeries } from '@/lib/disk-io'
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
  key: string
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
  { key: 'realtime', label: 'range_realtime', hours: 0, interval: 'realtime' },
  { key: '1h', label: 'range_1h', hours: 1, interval: 'raw' },
  { key: '6h', label: 'range_6h', hours: 6, interval: 'raw' },
  { key: '24h', label: 'range_24h', hours: 24, interval: 'raw' },
  { key: '7d', label: 'range_7d', hours: 168, interval: 'hourly' },
  { key: '30d', label: 'range_30d', hours: 720, interval: 'hourly' }
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

function ServerActionButtons({
  dockerEnabled,
  fileEnabled,
  id,
  isOnline,
  onEditOpen,
  serverWithCaps,
  terminalEnabled
}: {
  dockerEnabled: boolean
  fileEnabled: boolean
  id: string
  isOnline: boolean
  onEditOpen: () => void
  serverWithCaps: ServerResponse & ServerWithCaps
  terminalEnabled: boolean
}) {
  const { t } = useTranslation('servers')
  return (
    <div className="flex flex-wrap gap-2">
      <Button onClick={onEditOpen} size="sm" variant="outline">
        <Pencil aria-hidden="true" className="mr-1 size-4" />
        {t('detail_edit')}
      </Button>
      <CapabilitiesDialog server={serverWithCaps} />
      {isOnline && terminalEnabled && (
        <Link params={{ serverId: id }} to="/terminal/$serverId">
          <Button size="sm" variant="outline">
            <TerminalIcon aria-hidden="true" className="mr-1 size-4" />
            {t('detail_terminal')}
          </Button>
        </Link>
      )}
      {isOnline && fileEnabled && (
        <Link params={{ serverId: id }} search={{ path: '/' }} to="/files/$serverId">
          <Button size="sm" variant="outline">
            <FileText aria-hidden="true" className="mr-1 size-4" />
            {t('detail_files')}
          </Button>
        </Link>
      )}
      {isOnline && dockerEnabled && (
        <Link params={{ serverId: id }} to="/servers/$serverId/docker">
          <Button size="sm" variant="outline">
            <Container aria-hidden="true" className="mr-1 size-4" />
            {t('detail_docker')}
          </Button>
        </Link>
      )}
    </div>
  )
}

function MetricsTabContent({
  chartData,
  diskIoMergedData,
  diskIoPerDiskData,
  gpuChartData,
  hasDiskIo,
  hasGpu,
  hasTemperature,
  rangeIndex,
  realtimeFormatTime,
  serverId
}: {
  chartData: Record<string, unknown>[]
  diskIoMergedData: { read_bytes_per_sec: number; timestamp: string; write_bytes_per_sec: number }[]
  diskIoPerDiskData: {
    data: { read_bytes_per_sec: number; timestamp: string; write_bytes_per_sec: number }[]
    name: string
  }[]
  gpuChartData: Record<string, unknown>[]
  hasDiskIo: boolean
  hasGpu: boolean
  hasTemperature: boolean
  rangeIndex: number
  realtimeFormatTime: ((time: string) => string) | undefined
  serverId: string
}) {
  const { t } = useTranslation('servers')
  const navigate = Route.useNavigate()

  return (
    <>
      <div className="mt-4 mb-4 flex flex-wrap gap-1">
        {TIME_RANGES.map((tr, i) => (
          <Button
            className={cn(rangeIndex === i && 'bg-primary text-primary-foreground')}
            key={tr.label}
            onClick={() => navigate({ search: (prev) => ({ ...prev, range: tr.key }) })}
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
          formatTick={(v) => formatBytes(v)}
          formatTime={realtimeFormatTime}
          formatValue={(v) => formatBytes(v)}
          title={t('chart_net_in')}
        />
        <MetricsChart
          color="var(--color-chart-5)"
          data={chartData}
          dataKey="net_out_speed"
          formatTick={(v) => formatBytes(v)}
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

      {hasDiskIo && <DiskIoChart mergedData={diskIoMergedData} perDiskData={diskIoPerDiskData} />}

      <TrafficCard serverId={serverId} />
    </>
  )
}

function ServerDetailPage() {
  const { t } = useTranslation('servers')
  const { id } = Route.useParams()
  const { range: rangeParam } = Route.useSearch()
  const [editOpen, setEditOpen] = useState(false)

  const selectedRange = TIME_RANGES.findIndex((tr) => tr.key === rangeParam)
  const rangeIndex = selectedRange >= 0 ? selectedRange : 0
  const range = TIME_RANGES[rangeIndex]
  const isRealtime = range.key === 'realtime'

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

  const diskIoMergedData = useMemo(() => {
    if (isRealtime || !records) {
      return []
    }

    return buildMergedDiskIoSeries(records)
  }, [isRealtime, records])

  const diskIoPerDiskData = useMemo(() => {
    if (isRealtime || !records) {
      return []
    }

    return buildPerDiskIoSeries(records)
  }, [isRealtime, records])

  const hasTemperature =
    !isRealtime && chartData.some((d) => 'temperature' in d && d.temperature != null && (d.temperature as number) > 0)
  const hasDiskIo = !isRealtime && diskIoPerDiskData.length > 0
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
  const dockerEnabled = hasCap(serverWithCaps.capabilities ?? 0, CAP_DOCKER)

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

        <div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
          <div>
            <div className="flex items-center gap-3">
              {flag && <span className="text-xl">{flag}</span>}
              <h1 className="font-bold text-2xl">{server.name}</h1>
              <StatusBadge online={isOnline} />
            </div>
            <ServerInfoMeta server={server} />
          </div>
          <ServerActionButtons
            dockerEnabled={dockerEnabled}
            fileEnabled={fileEnabled}
            id={id}
            isOnline={isOnline}
            onEditOpen={() => setEditOpen(true)}
            serverWithCaps={serverWithCaps}
            terminalEnabled={terminalEnabled}
          />
        </div>
      </div>

      {hasBilling && <BillingInfoBar server={server} serverId={id} />}

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

      <Tabs defaultValue="metrics">
        <TabsList>
          <TabsTrigger value="metrics">Metrics</TabsTrigger>
          {server.billing_cycle && (
            <TabsTrigger value="traffic">
              <BarChart3 aria-hidden="true" className="mr-1 size-3.5" />
              Traffic
            </TabsTrigger>
          )}
        </TabsList>

        <TabsContent value="metrics">
          <MetricsTabContent
            chartData={chartData}
            diskIoMergedData={diskIoMergedData}
            diskIoPerDiskData={diskIoPerDiskData}
            gpuChartData={gpuChartData}
            hasDiskIo={hasDiskIo}
            hasGpu={hasGpu}
            hasTemperature={hasTemperature}
            rangeIndex={rangeIndex}
            realtimeFormatTime={realtimeFormatTime}
            serverId={id}
          />
        </TabsContent>

        {server.billing_cycle && (
          <TabsContent value="traffic">
            <TrafficTab billingCycle={server.billing_cycle} serverId={id} />
          </TabsContent>
        )}
      </Tabs>

      <ServerEditDialog onClose={() => setEditOpen(false)} open={editOpen} server={server} />
    </div>
  )
}

function BillingInfoBar({
  server,
  serverId
}: {
  server: Pick<
    ServerResponse,
    'billing_cycle' | 'currency' | 'expired_at' | 'price' | 'traffic_limit' | 'traffic_limit_type'
  >
  serverId: string
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
      {server.traffic_limit != null && <TrafficProgress serverId={serverId} />}
    </div>
  )
}
