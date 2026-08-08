import { useQuery } from '@tanstack/react-query'
import { Activity, BarChart3, ShieldAlert, ShieldCheck } from 'lucide-react'
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
import { api } from '@/lib/api-client'
import type {
  PublicMetricsPoint,
  PublicServerDetail,
  ServerMetricRecord,
  ServerResponse,
  UptimeDailyEntry
} from '@/lib/api-schema'
import { buildMergedDiskIoSeries, buildPerDiskIoSeries } from '@/lib/disk-io'
import {
  buildGpuChartRows,
  deriveNetworkLabels,
  type GpuRecordAggregated,
  METRIC_CHART_SPECS,
  type MetricChartSpec,
  makeTickFormatter,
  makeTooltipFormatter,
  toMetricChartRow
} from '@/lib/metric-chart-model'
import { useLiveServers } from '@/lib/server-catalog'
import { type RangeKey, rangesForVariant, resolveRange, type TimeRange } from '@/lib/server-detail-nav'
import { cn, formatBytes, isoWindow } from '@/lib/utils'
import { computeAggregateUptime } from '@/lib/widget-helpers'

export interface ServerDetailContentProps {
  /** Currently selected detail tab. When provided (admin), the tabs become
   *  URL-controlled; the public surface omits it and stays uncontrolled. */
  activeTab?: string
  /** Content for the Network tab. When provided, a Network trigger is added
   *  after Metrics. Admin passes the full network-quality experience; the
   *  public surface passes the redacted summary view (gated on the status
   *  page `show_network` toggle). */
  networkTab?: React.ReactNode
  /** Called by range buttons when the viewer picks a new historical window. */
  onRangeChange?: (rangeKey: RangeKey) => void
  /** Called when the viewer switches detail tabs. */
  onTabChange?: (tab: string) => void
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

// Fetches the historical metric series, branching on variant. Admin uses the
// auth'd `useServerRecords` (includes disk-io + temperature blobs); public
// hits `/api/status/servers/{id}/metrics` which returns the normalised
// `PublicMetricsPoint` shape and is rate-limited at the API boundary.
function useMetricSeries(serverId: string, range: TimeRange, isAdminVariant: boolean, isRealtime: boolean) {
  const adminQuery = useServerRecords(serverId, range.hours, range.interval, {
    enabled: isAdminVariant && !isRealtime
  })
  const { data: publicMetrics } = useQuery<PublicMetricsPoint[]>({
    queryKey: ['public-status', 'server', serverId, 'metrics', range.hours, range.interval],
    queryFn: () => {
      const { from, to } = isoWindow(range.hours)
      return api.get<PublicMetricsPoint[]>(
        `/api/status/servers/${serverId}/metrics?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}&interval=${encodeURIComponent(range.interval)}`
      )
    },
    enabled: !isAdminVariant && serverId.length > 0,
    refetchInterval: 60_000
  })
  return { adminRecords: adminQuery.data, publicMetrics }
}

function useAdminGpuRecords(serverId: string, range: TimeRange, isAdminVariant: boolean, isRealtime: boolean) {
  return useQuery<GpuRecordAggregated[]>({
    queryKey: ['servers', serverId, 'gpu-records', range.hours],
    queryFn: () => {
      const { from, to } = isoWindow(range.hours)
      return api.get<GpuRecordAggregated[]>(
        `/api/servers/${serverId}/gpu-records?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`
      )
    },
    enabled: isAdminVariant && serverId.length > 0 && !isRealtime,
    refetchInterval: 60_000
  })
}

// Pulls the live-traffic strip data from the WS-driven server catalog.
// Public variant intentionally does not subscribe; the strip falls back to
// the snapshot in `PublicServerDetail.metrics`.
function useLiveServerMetrics(serverId: string, isAdminVariant: boolean) {
  const { data: liveServers } = useLiveServers({ enabled: isAdminVariant })
  return liveServers?.find((s) => s.id === serverId)
}

export function ServerDetailContent(props: ServerDetailContentProps) {
  const { activeTab, networkTab, rangeKey, server, serverId, onRangeChange, onTabChange, variant } = props
  const { t } = useTranslation('servers')
  const isPublic = variant === 'public'
  const isAdminVariant = !isPublic
  const ranges = rangesForVariant(variant)
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

  const chartFormatTime = useChartTickFormatter(isRealtime, range, chartData)
  const tooltipFormatTime = useTooltipFormatter(isRealtime, range)
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
  const availableMetrics = useMemo<AvailableMetrics>(
    () => ({ diskIo: hasDiskIo, gpu: hasGpu, temperature: hasTemperature }),
    [hasDiskIo, hasGpu, hasTemperature]
  )

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

  // The billing/network/uptime overview blocks live inside the metrics tab so
  // the tab bar can sit at the very top of the page; metrics is the default
  // tab, so they are still the first thing a viewer sees.
  const metricsOverview = (
    <div className="mt-4">
      {isAdminVariant && hasBilling && adminServer && <CostInsightBar server={adminServer} serverId={serverId} />}

      {/* Network bar — admin: WS-driven live data. Public: snapshot from
          PublicServerDetail.metrics. In both cases we render placeholders
          before data is available so the content below does not shift down. */}
      <div className="mb-6 flex flex-wrap gap-6 rounded-xl bg-card p-3 text-sm ring-1 ring-foreground/10">
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
    </div>
  )

  return (
    <DetailTabs
      activeTab={activeTab}
      adminServer={adminServer}
      billingCycle={billingCycle}
      isAdminVariant={isAdminVariant}
      metricsTab={
        <>
          {metricsOverview}
          <MetricsTabContent
            availableMetrics={availableMetrics}
            chartData={chartData}
            diskIoMergedData={diskIoMergedData}
            diskIoPerDiskData={diskIoPerDiskData}
            formatTime={chartFormatTime}
            formatTooltipLabel={tooltipFormatTime}
            gpuChartData={gpuChartData}
            onRangeChange={onRangeChange}
            rangeIndex={rangeIndex}
            ranges={ranges}
            serverId={serverId}
            variant={isPublic ? 'public' : 'admin'}
          />
        </>
      }
      networkTab={networkTab}
      onTabChange={onTabChange}
      serverId={serverId}
    />
  )
}

function DetailTabs({
  activeTab,
  adminServer,
  billingCycle,
  isAdminVariant,
  metricsTab,
  networkTab,
  onTabChange,
  serverId
}: {
  activeTab?: string
  adminServer: ServerResponse | null
  billingCycle: string | null
  isAdminVariant: boolean
  metricsTab: React.ReactNode
  networkTab?: React.ReactNode
  onTabChange?: (tab: string) => void
  serverId: string
}) {
  const { t } = useTranslation('servers')
  return (
    <Tabs
      {...(activeTab === undefined ? { defaultValue: 'metrics' } : { onValueChange: onTabChange, value: activeTab })}
    >
      <TabsList>
        <TabsTrigger value="metrics">{t('metrics_tab')}</TabsTrigger>
        {networkTab != null && (
          <TabsTrigger value="network">
            <Activity aria-hidden="true" className="mr-1 size-3.5" />
            {t('network:tab_title')}
          </TabsTrigger>
        )}
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

      {networkTab != null && <TabsContent value="network">{networkTab}</TabsContent>}

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
      return (adminRecords ?? []).map((r) => toMetricChartRow(r, memTotal, diskTotal))
    }
    return (publicMetrics ?? []).map((p) => toMetricChartRow(p, memTotal, diskTotal))
  }, [adminRecords, diskTotal, isAdminVariant, isRealtime, memTotal, publicMetrics, realtimeData])
}

function useChartTickFormatter(isRealtime: boolean, range: TimeRange, chartData: Record<string, unknown>[]) {
  return useMemo(() => makeTickFormatter(isRealtime, range.hours, chartData), [isRealtime, chartData, range])
}

function useTooltipFormatter(isRealtime: boolean, range: TimeRange) {
  return useMemo(() => makeTooltipFormatter(isRealtime, range.hours), [isRealtime, range])
}

function useGpuChartData(
  isAdminVariant: boolean,
  gpuRecords: GpuRecordAggregated[] | undefined,
  publicMetrics: PublicMetricsPoint[] | undefined
) {
  return useMemo(
    () => buildGpuChartRows(isAdminVariant, gpuRecords, publicMetrics),
    [isAdminVariant, gpuRecords, publicMetrics]
  )
}

interface AvailableMetrics {
  diskIo: boolean
  gpu: boolean
  temperature: boolean
}

function MetricsTabContent({
  availableMetrics,
  chartData,
  diskIoMergedData,
  diskIoPerDiskData,
  gpuChartData,
  onRangeChange,
  rangeIndex,
  ranges,
  formatTime,
  formatTooltipLabel,
  serverId,
  variant
}: {
  availableMetrics: AvailableMetrics
  chartData: Record<string, unknown>[]
  diskIoMergedData: { read_bytes_per_sec: number; timestamp: string; write_bytes_per_sec: number }[]
  diskIoPerDiskData: {
    data: { read_bytes_per_sec: number; timestamp: string; write_bytes_per_sec: number }[]
    name: string
  }[]
  gpuChartData: Record<string, unknown>[]
  onRangeChange?: (rangeKey: RangeKey) => void
  rangeIndex: number
  ranges: TimeRange[]
  formatTime: ((time: string) => string) | undefined
  formatTooltipLabel: ((time: string) => string) | undefined
  serverId: string
  variant: 'admin' | 'public'
}) {
  const { t } = useTranslation('servers')
  const hasGpuTemp = gpuChartData.some((d) => 'gpu_temp' in d && d.gpu_temp != null)
  const isPublic = variant === 'public'
  const gates: Record<NonNullable<MetricChartSpec['gate']>, boolean> = {
    gpu: availableMetrics.gpu,
    // GPU temp series is admin-only; the public surface does not expose it,
    // so the gate also requires a non-empty data key.
    gpuTemp: availableMetrics.gpu && hasGpuTemp,
    temperature: availableMetrics.temperature
  }

  // One container owns the rhythm between the range picker, the chart grid and
  // the trailing cards; the blocks themselves carry no outer margins.
  return (
    <div className="mt-4 space-y-4">
      <div className="flex flex-wrap gap-1">
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
        {METRIC_CHART_SPECS.filter((spec) => !spec.gate || gates[spec.gate]).map((spec) => (
          <MetricsChart
            color={spec.color}
            data={spec.source === 'gpu' ? gpuChartData : chartData}
            dataKey={spec.dataKey}
            domain={spec.domain}
            formatTick={spec.bytes ? formatBytes : undefined}
            formatTime={formatTime}
            formatTooltipLabel={formatTooltipLabel}
            formatValue={spec.bytes ? formatBytes : undefined}
            key={spec.dataKey}
            title={t(spec.labelKey)}
            unit={spec.unit}
          />
        ))}
      </div>

      {availableMetrics.diskIo && (
        <DiskIoChart formatTime={formatTime} mergedData={diskIoMergedData} perDiskData={diskIoPerDiskData} />
      )}

      {/* TrafficCard hits an admin-only endpoint; omit it on the public
          surface where there is no equivalent traffic API exposed. */}
      {!isPublic && <TrafficCard serverId={serverId} />}
    </div>
  )
}

function UptimeCard({ isPublic, serverId }: { isPublic: boolean; serverId: string }) {
  const { t } = useTranslation('servers')

  // Admin viewers use the auth'd hook; public viewers fetch the redacted
  // public uptime endpoint that is gated by `show_server_detail`.
  const { data: adminUptimeDays, isPending: isAdminPending } = useUptimeDaily(serverId)
  const { data: publicUptimeDays, isPending: isPublicPending } = useQuery<UptimeDailyEntry[]>({
    queryKey: ['public-status', 'server', serverId, 'uptime-daily'],
    queryFn: () => api.get<UptimeDailyEntry[]>(`/api/status/servers/${serverId}/uptime-daily`),
    enabled: isPublic && serverId.length > 0,
    staleTime: 300_000
  })

  const isPending = isPublic ? isPublicPending : isAdminPending
  const uptimeDays = isPublic ? publicUptimeDays : adminUptimeDays

  if (isPending) {
    return (
      <div className="mb-6 rounded-xl bg-card p-4 ring-1 ring-foreground/10">
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
    <div className="mb-6 rounded-xl bg-card p-4 ring-1 ring-foreground/10">
      <div className="mb-3 flex items-center justify-between">
        <h3 className="font-semibold text-sm">{t('uptime_title')}</h3>
        <span className="font-medium text-sm">{uptimePct !== null ? `${uptimePct.toFixed(2)}%` : '—'}</span>
      </div>
      <UptimeTimeline appearance="status-history" days={uptimeDays} height={34} rangeDays={90} showLabels showLegend />
    </div>
  )
}
