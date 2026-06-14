import { useQuery } from '@tanstack/react-query'
import { BarChart3, ShieldAlert, ShieldCheck } from 'lucide-react'
import type * as React from 'react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { IpQualityTab } from '@/components/ip-quality/ip-quality-tab'
import { ServerSecurityTab } from '@/components/security/server-security-tab'
import { CostInsightBar } from '@/components/server/cost-insight-bar'
import { DiskIoChart } from '@/components/server/disk-io-chart'
import { MetricsChart } from '@/components/server/metrics-chart'
import { TrafficCard } from '@/components/server/traffic-card'
import { TrafficTab } from '@/components/server/traffic-tab'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { UptimeTimeline } from '@/components/uptime/uptime-timeline'
import { useServerRecords, useUptimeDaily } from '@/hooks/use-api'
import { useRealtimeMetrics } from '@/hooks/use-realtime-metrics'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { api } from '@/lib/api-client'
import type {
  PublicMetricsPoint,
  PublicServerDetail,
  ServerMetricRecord,
  ServerResponse,
  UptimeDailyEntry
} from '@/lib/api-schema'
import { buildMergedDiskIoSeries, buildPerDiskIoSeries } from '@/lib/disk-io'
import { cn, formatBytes } from '@/lib/utils'
import { computeAggregateUptime } from '@/lib/widget-helpers'

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

const ADMIN_TIME_RANGES: TimeRange[] = [
  { key: 'realtime', label: 'range_realtime', hours: 0, interval: 'realtime' },
  { key: '1h', label: 'range_1h', hours: 1, interval: 'raw' },
  { key: '6h', label: 'range_6h', hours: 6, interval: 'raw' },
  { key: '24h', label: 'range_24h', hours: 24, interval: 'raw' },
  { key: '7d', label: 'range_7d', hours: 168, interval: 'hourly' },
  { key: '30d', label: 'range_30d', hours: 720, interval: 'hourly' }
]

// Public variant cannot rely on WS-driven realtime metrics, so realtime is
// dropped; everything else mirrors the admin range options because the
// public metrics endpoint accepts the same `interval` query parameter.
const PUBLIC_TIME_RANGES: TimeRange[] = ADMIN_TIME_RANGES.filter((r) => r.key !== 'realtime')

export interface ServerDetailContentProps {
  /** Called by range buttons when the viewer picks a new historical window. */
  onRangeChange?: (rangeKey: string) => void
  /** Currently selected range key from the URL or local state. */
  rangeKey?: string
  /** Server detail payload — full admin shape, or redacted public shape. */
  server: ServerResponse | PublicServerDetail
  serverId: string
  variant: 'admin' | 'public'
}

function isAdminServer(server: ServerResponse | PublicServerDetail): server is ServerResponse {
  // ServerResponse always carries the `ipv4` key (even if null) because it's
  // the unredacted entity row; PublicServerDetail omits the field entirely.
  return 'ipv4' in server
}

function resolveRange(rangeKey: string | undefined, ranges: TimeRange[]) {
  const idx = ranges.findIndex((tr) => tr.key === rangeKey)
  const rangeIndex = idx >= 0 ? idx : 0
  return { range: ranges[rangeIndex], rangeIndex }
}

function buildIsoWindow(hours: number) {
  const now = new Date()
  return {
    from: new Date(now.getTime() - hours * 3600 * 1000).toISOString(),
    to: now.toISOString()
  }
}

function adminRecordToChartRow(r: ServerMetricRecord, memTotal: number, diskTotal: number) {
  return {
    timestamp: r.time,
    cpu: r.cpu,
    memory_pct: memTotal ? (r.mem_used / memTotal) * 100 : 0,
    disk_pct: diskTotal ? (r.disk_used / diskTotal) * 100 : 0,
    net_in_speed: r.net_in_speed,
    net_out_speed: r.net_out_speed,
    net_in_transfer: r.net_in_transfer,
    net_out_transfer: r.net_out_transfer,
    load1: r.load1,
    load5: r.load5,
    load15: r.load15,
    temperature: r.temperature
  }
}

function publicPointToChartRow(p: PublicMetricsPoint, memTotal: number, diskTotal: number) {
  return {
    timestamp: p.time,
    cpu: p.cpu,
    memory_pct: memTotal ? (p.mem_used / memTotal) * 100 : 0,
    disk_pct: diskTotal ? (p.disk_used / diskTotal) * 100 : 0,
    net_in_speed: p.net_in_speed,
    net_out_speed: p.net_out_speed,
    net_in_transfer: p.net_in_transfer,
    net_out_transfer: p.net_out_transfer,
    load1: p.load1,
    load5: p.load5,
    load15: p.load15,
    temperature: p.temperature
  }
}

// Fetches the historical metric series, branching on variant. Admin uses the
// auth'd `useServerRecords` (includes disk-io + temperature blobs); public
// hits `/api/status/servers/{id}/metrics` which returns the normalised
// `PublicMetricsPoint` shape and is rate-limited at the API boundary.
function useMetricSeries(serverId: string, range: TimeRange, isAdminVariant: boolean, isRealtime: boolean) {
  const adminQuery = useServerRecords(serverId, range.hours, range.interval, {
    enabled: isAdminVariant && !isRealtime
  })
  const publicQuery = useQuery<PublicMetricsPoint[]>({
    queryKey: ['public-status', 'server', serverId, 'metrics', range.hours, range.interval],
    queryFn: () => {
      const { from, to } = buildIsoWindow(range.hours)
      return api.get<PublicMetricsPoint[]>(
        `/api/status/servers/${serverId}/metrics?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}&interval=${encodeURIComponent(range.interval)}`
      )
    },
    enabled: !isAdminVariant && serverId.length > 0,
    refetchInterval: 60_000
  })
  return { adminRecords: adminQuery.data, publicMetrics: publicQuery.data }
}

function useAdminGpuRecords(serverId: string, range: TimeRange, isAdminVariant: boolean, isRealtime: boolean) {
  return useQuery<GpuRecordAggregated[]>({
    queryKey: ['servers', serverId, 'gpu-records', range.hours],
    queryFn: () => {
      const { from, to } = buildIsoWindow(range.hours)
      return api.get<GpuRecordAggregated[]>(
        `/api/servers/${serverId}/gpu-records?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`
      )
    },
    enabled: isAdminVariant && serverId.length > 0 && !isRealtime,
    refetchInterval: 60_000
  })
}

// Pulls the live-traffic strip data from the WS-driven `['servers']` cache.
// Public variant intentionally does not subscribe; the strip falls back to
// the snapshot in `PublicServerDetail.metrics`.
function useLiveServerMetrics(serverId: string, isAdminVariant: boolean) {
  const { data: liveServers } = useQuery<ServerMetrics[]>({
    queryKey: ['servers'],
    queryFn: () => [],
    staleTime: Number.POSITIVE_INFINITY,
    refetchOnMount: false,
    refetchOnWindowFocus: false,
    enabled: isAdminVariant
  })
  return liveServers?.find((s) => s.id === serverId)
}

interface NetworkLabels {
  netInLabel: string
  netOutLabel: string
  netTotalLabel: string | null
}

function deriveNetworkLabels(
  isAdminVariant: boolean,
  liveData: ServerMetrics | undefined,
  publicMetricsSnapshot: PublicServerDetail['metrics'] | null
): NetworkLabels {
  if (isAdminVariant) {
    if (!liveData) {
      return { netInLabel: '—', netOutLabel: '—', netTotalLabel: '—' }
    }
    const inBytes = liveData.net_in_transfer ?? 0
    const outBytes = liveData.net_out_transfer ?? 0
    return {
      netInLabel: formatBytes(inBytes),
      netOutLabel: formatBytes(outBytes),
      netTotalLabel: formatBytes(inBytes + outBytes)
    }
  }
  if (!publicMetricsSnapshot) {
    return { netInLabel: '—', netOutLabel: '—', netTotalLabel: null }
  }
  // Use cumulative transfer (not the instantaneous *_speed rate) so the public
  // bar matches the admin bar: the `detail_network_in/out/total` labels describe a
  // total amount transferred, and formatBytes renders bytes — feeding a rate here
  // mislabelled "1.2 MB/s" as a cumulative "1.2 MB".
  const inBytes = publicMetricsSnapshot.net_in_transfer
  const outBytes = publicMetricsSnapshot.net_out_transfer
  return {
    netInLabel: formatBytes(inBytes),
    netOutLabel: formatBytes(outBytes),
    netTotalLabel: formatBytes(inBytes + outBytes)
  }
}

export function ServerDetailContent(props: ServerDetailContentProps) {
  const { rangeKey, server, serverId, onRangeChange, variant } = props
  const { t } = useTranslation('servers')
  const isPublic = variant === 'public'
  const isAdminVariant = !isPublic
  const ranges = isPublic ? PUBLIC_TIME_RANGES : ADMIN_TIME_RANGES
  const { range, rangeIndex } = resolveRange(rangeKey, ranges)
  const isRealtime = range.key === 'realtime'

  const { adminRecords, publicMetrics } = useMetricSeries(serverId, range, isAdminVariant, isRealtime)
  const realtimeData = useRealtimeMetrics(serverId)
  const { data: gpuRecords } = useAdminGpuRecords(serverId, range, isAdminVariant, isRealtime)
  const liveData = useLiveServerMetrics(serverId, isAdminVariant)

  const memTotal = server.mem_total ?? 0
  const diskTotal = server.disk_total ?? 0

  const chartData = useAggregatedChartData({
    isAdminVariant,
    isRealtime,
    realtimeData,
    adminRecords,
    publicMetrics,
    memTotal,
    diskTotal
  })

  const chartFormatTime = useChartTickFormatter(isRealtime, range, realtimeData)
  const tooltipFormatTime = useTooltipFormatter(isRealtime, range)
  const xAxisInterval = useXAxisInterval(isRealtime, range, chartData.length)
  const gpuChartData = useGpuChartData(isAdminVariant, gpuRecords, publicMetrics)

  const diskIoMergedData = useMemo(
    () => (isAdminVariant && !isRealtime && adminRecords ? buildMergedDiskIoSeries(adminRecords) : []),
    [isAdminVariant, isRealtime, adminRecords]
  )
  const diskIoPerDiskData = useMemo(
    () => (isAdminVariant && !isRealtime && adminRecords ? buildPerDiskIoSeries(adminRecords) : []),
    [isAdminVariant, isRealtime, adminRecords]
  )

  const hasTemperature =
    !isRealtime && chartData.some((d) => 'temperature' in d && d.temperature != null && (d.temperature as number) > 0)
  const hasDiskIo = isAdminVariant && !isRealtime && diskIoPerDiskData.length > 0
  const hasGpu = !isRealtime && gpuChartData.length > 0

  const publicMetricsSnapshot = isAdminVariant || isAdminServer(server) ? null : server.metrics
  const { netInLabel, netOutLabel, netTotalLabel } = deriveNetworkLabels(
    isAdminVariant,
    liveData,
    publicMetricsSnapshot
  )

  const adminServer = isAdminServer(server) ? server : null
  const hasBilling =
    adminServer != null &&
    (adminServer.price != null || adminServer.expired_at != null || adminServer.traffic_limit != null)
  const billingCycle = adminServer?.billing_cycle ?? null

  return (
    <>
      {isAdminVariant && hasBilling && adminServer && <CostInsightBar server={adminServer} serverId={serverId} />}

      {/* Network bar — admin: WS-driven live data. Public: snapshot from
          PublicServerDetail.metrics. In both cases we render placeholders
          before data is available so the tabs below do not shift down. */}
      <div className="mb-6 flex flex-wrap gap-6 rounded-lg border bg-card p-3 text-sm">
        <span className="text-muted-foreground">
          {t('detail_network_in')} <span className="font-medium text-foreground">{netInLabel}</span>
        </span>
        <span className="text-muted-foreground">
          {t('detail_network_out')} <span className="font-medium text-foreground">{netOutLabel}</span>
        </span>
        {netTotalLabel !== null && (
          <span className="text-muted-foreground">
            {t('detail_network_total')} <span className="font-medium text-foreground">{netTotalLabel}</span>
          </span>
        )}
      </div>

      <UptimeCard isPublic={isPublic} serverId={serverId} />

      <DetailTabs
        adminServer={adminServer}
        billingCycle={billingCycle}
        isAdminVariant={isAdminVariant}
        metricsTab={
          <MetricsTabContent
            chartData={chartData}
            diskIoMergedData={diskIoMergedData}
            diskIoPerDiskData={diskIoPerDiskData}
            formatTime={chartFormatTime}
            formatTooltipLabel={tooltipFormatTime}
            gpuChartData={gpuChartData}
            hasDiskIo={hasDiskIo}
            hasGpu={hasGpu}
            hasTemperature={hasTemperature}
            isPublic={isPublic}
            onRangeChange={onRangeChange}
            rangeIndex={rangeIndex}
            ranges={ranges}
            serverId={serverId}
            xAxisInterval={xAxisInterval}
          />
        }
        serverId={serverId}
      />
    </>
  )
}

function DetailTabs({
  adminServer,
  billingCycle,
  isAdminVariant,
  metricsTab,
  serverId
}: {
  adminServer: ServerResponse | null
  billingCycle: string | null
  isAdminVariant: boolean
  metricsTab: React.ReactNode
  serverId: string
}) {
  const { t } = useTranslation('servers')
  return (
    <Tabs className="mt-6" defaultValue="metrics">
      <TabsList>
        <TabsTrigger value="metrics">{t('metrics_tab')}</TabsTrigger>
        {isAdminVariant && billingCycle && (
          <TabsTrigger value="traffic">
            <BarChart3 aria-hidden="true" className="mr-1 size-3.5" />
            {t('traffic_tab')}
          </TabsTrigger>
        )}
        {isAdminVariant && (
          <TabsTrigger value="security">
            <ShieldAlert aria-hidden="true" className="mr-1 size-3.5" />
            {t('security_tab')}
          </TabsTrigger>
        )}
        {isAdminVariant && (
          <TabsTrigger value="ip-quality">
            <ShieldCheck aria-hidden="true" className="mr-1 size-3.5" />
            {t('ip-quality:tab_title')}
          </TabsTrigger>
        )}
      </TabsList>

      <TabsContent value="metrics">{metricsTab}</TabsContent>

      {isAdminVariant && billingCycle && (
        <TabsContent value="traffic">
          <TrafficTab billingCycle={billingCycle} serverId={serverId} />
        </TabsContent>
      )}

      {isAdminVariant && adminServer && (
        <TabsContent value="security">
          <ServerSecurityTab serverId={serverId} />
        </TabsContent>
      )}

      {isAdminVariant && adminServer && (
        <TabsContent value="ip-quality">
          <IpQualityTab
            agentLocalCapabilities={adminServer.agent_local_capabilities}
            capabilities={adminServer.capabilities}
            serverId={serverId}
            serverName={adminServer.name}
          />
        </TabsContent>
      )}
    </Tabs>
  )
}

function useAggregatedChartData(args: {
  adminRecords: ServerMetricRecord[] | undefined
  diskTotal: number
  isAdminVariant: boolean
  isRealtime: boolean
  memTotal: number
  publicMetrics: PublicMetricsPoint[] | undefined
  realtimeData: unknown
}) {
  const { adminRecords, diskTotal, isAdminVariant, isRealtime, memTotal, publicMetrics, realtimeData } = args
  return useMemo<Record<string, unknown>[]>(() => {
    if (isAdminVariant) {
      if (isRealtime) {
        return realtimeData as Record<string, unknown>[]
      }
      return (adminRecords ?? []).map((r) => adminRecordToChartRow(r, memTotal, diskTotal))
    }
    return (publicMetrics ?? []).map((p) => publicPointToChartRow(p, memTotal, diskTotal))
  }, [adminRecords, diskTotal, isAdminVariant, isRealtime, memTotal, publicMetrics, realtimeData])
}

function useChartTickFormatter(isRealtime: boolean, range: TimeRange, realtimeData: unknown) {
  // biome-ignore lint/correctness/useExhaustiveDependencies: realtimeData in deps forces closure rebuild on buffer updates so `lastLabel` resets before Recharts re-iterates ticks
  return useMemo<((time: string) => string) | undefined>(() => {
    if (isRealtime) {
      let lastLabel = ''
      return (time: string) => {
        const d = new Date(time)
        const label = `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
        if (label === lastLabel) {
          return ''
        }
        lastLabel = label
        return label
      }
    }
    if (range.hours >= 168) {
      return (time: string) => {
        const d = new Date(time)
        const mm = String(d.getMonth() + 1).padStart(2, '0')
        const dd = String(d.getDate()).padStart(2, '0')
        return `${mm}-${dd}`
      }
    }
    return undefined
  }, [isRealtime, realtimeData, range])
}

/** Recharts X-axis `interval` (stride = N+1 ticks). Returns a numeric stride for
 * long ranges so the auto-generated tick labels do not overlap on narrow viewports.
 * 7d/30d hourly buckets produce 168 / 720 samples — labelling every one collapses
 * into illegible overlap, so we target roughly 8 evenly spaced labels. */
function useXAxisInterval(isRealtime: boolean, range: TimeRange, dataLength: number) {
  return useMemo<number | undefined>(() => {
    if (isRealtime) {
      return 0
    }
    if (range.hours >= 168 && dataLength > 0) {
      const targetLabels = 8
      return Math.max(0, Math.floor(dataLength / targetLabels) - 1)
    }
    return undefined
  }, [isRealtime, range, dataLength])
}

function useTooltipFormatter(isRealtime: boolean, range: TimeRange) {
  return useMemo<((time: string) => string) | undefined>(() => {
    if (isRealtime) {
      return (time: string) => {
        const d = new Date(time)
        return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}:${String(d.getSeconds()).padStart(2, '0')}`
      }
    }
    if (range.hours >= 168) {
      return (time: string) => {
        const d = new Date(time)
        return `${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
      }
    }
    return undefined
  }, [isRealtime, range])
}

function useGpuChartData(
  isAdminVariant: boolean,
  gpuRecords: GpuRecordAggregated[] | undefined,
  publicMetrics: PublicMetricsPoint[] | undefined
) {
  return useMemo<Record<string, unknown>[]>(() => {
    if (isAdminVariant) {
      if (!gpuRecords || gpuRecords.length === 0) {
        return []
      }
      return gpuRecords.map((r) => ({
        timestamp: r.time,
        gpu_usage: r.gpu_usage_avg,
        gpu_temp: r.temperature_avg,
        gpu_mem_pct: r.mem_total_avg > 0 ? (r.mem_used_avg / r.mem_total_avg) * 100 : 0
      }))
    }
    if (!publicMetrics) {
      return []
    }
    return publicMetrics.filter((p) => p.gpu_usage != null).map((p) => ({ timestamp: p.time, gpu_usage: p.gpu_usage }))
  }, [isAdminVariant, gpuRecords, publicMetrics])
}

function MetricsTabContent({
  chartData,
  diskIoMergedData,
  diskIoPerDiskData,
  gpuChartData,
  hasDiskIo,
  hasGpu,
  hasTemperature,
  isPublic,
  onRangeChange,
  rangeIndex,
  ranges,
  formatTime,
  formatTooltipLabel,
  serverId,
  xAxisInterval
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
  isPublic: boolean
  onRangeChange?: (rangeKey: string) => void
  rangeIndex: number
  ranges: TimeRange[]
  formatTime: ((time: string) => string) | undefined
  formatTooltipLabel: ((time: string) => string) | undefined
  serverId: string
  xAxisInterval?: number | 'preserveStart' | 'preserveEnd' | 'preserveStartEnd' | 'equidistantPreserveStart'
}) {
  const { t } = useTranslation('servers')
  const hasGpuTemp = gpuChartData.some((d) => 'gpu_temp' in d && d.gpu_temp != null)

  return (
    <>
      <div className="mt-4 mb-4 flex flex-wrap gap-1">
        {ranges.map((tr, i) => (
          <Button
            className={cn(rangeIndex === i && 'bg-primary text-primary-foreground')}
            key={tr.label}
            onClick={() => onRangeChange?.(tr.key)}
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
          domain={[0, 100]}
          formatTime={formatTime}
          formatTooltipLabel={formatTooltipLabel}
          title={t('chart_cpu')}
          unit="%"
          xAxisInterval={xAxisInterval}
        />
        <MetricsChart
          color="var(--color-chart-2)"
          data={chartData}
          dataKey="memory_pct"
          domain={[0, 100]}
          formatTime={formatTime}
          formatTooltipLabel={formatTooltipLabel}
          title={t('chart_memory')}
          unit="%"
          xAxisInterval={xAxisInterval}
        />
        <MetricsChart
          color="var(--color-chart-3)"
          data={chartData}
          dataKey="disk_pct"
          domain={[0, 100]}
          formatTime={formatTime}
          formatTooltipLabel={formatTooltipLabel}
          title={t('chart_disk')}
          unit="%"
          xAxisInterval={xAxisInterval}
        />
        <MetricsChart
          color="var(--color-chart-4)"
          data={chartData}
          dataKey="net_in_speed"
          formatTick={(v) => formatBytes(v)}
          formatTime={formatTime}
          formatTooltipLabel={formatTooltipLabel}
          formatValue={(v) => formatBytes(v)}
          title={t('chart_net_in')}
          xAxisInterval={xAxisInterval}
        />
        <MetricsChart
          color="var(--color-chart-5)"
          data={chartData}
          dataKey="net_out_speed"
          formatTick={(v) => formatBytes(v)}
          formatTime={formatTime}
          formatTooltipLabel={formatTooltipLabel}
          formatValue={(v) => formatBytes(v)}
          title={t('chart_net_out')}
          xAxisInterval={xAxisInterval}
        />
        <MetricsChart
          color="var(--color-chart-1)"
          data={chartData}
          dataKey="load1"
          formatTime={formatTime}
          formatTooltipLabel={formatTooltipLabel}
          title={t('chart_load')}
          xAxisInterval={xAxisInterval}
        />

        {hasTemperature && (
          <MetricsChart
            color="var(--color-chart-4)"
            data={chartData}
            dataKey="temperature"
            formatTime={formatTime}
            formatTooltipLabel={formatTooltipLabel}
            title={t('chart_temperature')}
            unit="°C"
            xAxisInterval={xAxisInterval}
          />
        )}

        {hasGpu && (
          <MetricsChart
            color="var(--color-chart-5)"
            data={gpuChartData}
            dataKey="gpu_usage"
            domain={[0, 100]}
            formatTime={formatTime}
            formatTooltipLabel={formatTooltipLabel}
            title={t('chart_gpu')}
            unit="%"
            xAxisInterval={xAxisInterval}
          />
        )}
        {/* GPU temp series is admin-only; the public surface does not
            expose it, so we gate the chart on a non-empty data key. */}
        {hasGpu && hasGpuTemp && (
          <MetricsChart
            color="var(--color-chart-2)"
            data={gpuChartData}
            dataKey="gpu_temp"
            formatTime={formatTime}
            formatTooltipLabel={formatTooltipLabel}
            title={t('chart_gpu_temp')}
            unit="°C"
            xAxisInterval={xAxisInterval}
          />
        )}
      </div>

      {hasDiskIo && (
        <DiskIoChart formatTime={formatTime} mergedData={diskIoMergedData} perDiskData={diskIoPerDiskData} />
      )}

      {/* TrafficCard hits an admin-only endpoint; omit it on the public
          surface where there is no equivalent traffic API exposed. */}
      {!isPublic && <TrafficCard serverId={serverId} />}
    </>
  )
}

function UptimeCard({ isPublic, serverId }: { isPublic: boolean; serverId: string }) {
  const { t } = useTranslation('servers')

  // Admin viewers use the auth'd hook; public viewers fetch the redacted
  // public uptime endpoint that is gated by `show_server_detail`.
  const adminQuery = useUptimeDaily(serverId)
  const publicQuery = useQuery<UptimeDailyEntry[]>({
    queryKey: ['public-status', 'server', serverId, 'uptime-daily'],
    queryFn: () => api.get<UptimeDailyEntry[]>(`/api/status/servers/${serverId}/uptime-daily`),
    enabled: isPublic && serverId.length > 0,
    staleTime: 300_000
  })

  const isPending = isPublic ? publicQuery.isPending : adminQuery.isPending
  const uptimeDays = isPublic ? publicQuery.data : adminQuery.data

  if (isPending) {
    return (
      <div className="mb-6 rounded-lg border bg-card p-4">
        <div className="mb-3 flex items-center justify-between">
          <Skeleton className="h-5 w-24" />
          <Skeleton className="h-4 w-14" />
        </div>
        <Skeleton className="h-12 w-full" />
      </div>
    )
  }
  if (!uptimeDays || uptimeDays.length === 0) {
    return null
  }
  const uptimePct = computeAggregateUptime(uptimeDays)
  return (
    <div className="mb-6 rounded-lg border bg-card p-4">
      <div className="mb-3 flex items-center justify-between">
        <h3 className="font-semibold text-sm">{t('uptime_title')}</h3>
        <span className="font-medium text-sm">{uptimePct !== null ? `${uptimePct.toFixed(2)}%` : '—'}</span>
      </div>
      <UptimeTimeline days={uptimeDays} rangeDays={90} showLabels showLegend />
    </div>
  )
}
