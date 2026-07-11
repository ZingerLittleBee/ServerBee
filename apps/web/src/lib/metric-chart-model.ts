import type { PublicMetricsPoint, PublicServerDetail } from '@/lib/api-schema'
import type { ServerMetrics } from '@/lib/server-catalog'
import { formatBytes } from '@/lib/utils'

/**
 * Chart model for the server-detail Metrics tab: every transform between
 * API/WS payloads and Recharts rows lives here, testable through this
 * interface. Rendering (and the trivial admin/public branch glue) stays in
 * `server-detail-content.tsx`.
 */

/** Fields shared by the admin `ServerMetricRecord` and public `PublicMetricsPoint` series. */
export interface MetricSeriesPoint {
  cpu: number
  disk_used: number
  load1: number
  load5: number
  load15: number
  mem_used: number
  net_in_speed: number
  net_in_transfer: number
  net_out_speed: number
  net_out_transfer: number
  temperature?: number | null
  time: string
}

// biome-ignore lint/style/useConsistentTypeDefinitions: interfaces lack an implicit index signature, and chart rows must be assignable to the Record<string, unknown>[] the chart components accept
export type MetricChartRow = {
  cpu: number
  disk_pct: number
  load1: number
  load5: number
  load15: number
  memory_pct: number
  net_in_speed: number
  net_in_transfer: number
  net_out_speed: number
  net_out_transfer: number
  temperature?: number | null
  timestamp: string
}

/**
 * Display spec for one metric chart. Everything the Metrics tab needs to
 * render a series — data key, label, color, unit, domain, byte formatting,
 * data source and availability gate — lives in this one table, so adding a
 * chart means adding a row here instead of a hand-written JSX block.
 */
export interface MetricChartSpec {
  /** Format axis ticks and tooltip values with formatBytes. */
  bytes?: boolean
  /** CSS color value for the area series. */
  color: string
  /** Row key in the chart data. */
  dataKey: string
  domain?: [number, number]
  /** Availability gate; ungated charts always render. */
  gate?: 'gpu' | 'gpuTemp' | 'temperature'
  /** i18n key in the `servers` namespace. */
  labelKey: string
  /** Which row array feeds the chart. */
  source: 'gpu' | 'metrics'
  unit?: string
}

export const METRIC_CHART_SPECS: MetricChartSpec[] = [
  {
    color: 'var(--color-chart-1)',
    dataKey: 'cpu',
    domain: [0, 100],
    labelKey: 'chart_cpu',
    source: 'metrics',
    unit: '%'
  },
  {
    color: 'var(--color-chart-2)',
    dataKey: 'memory_pct',
    domain: [0, 100],
    labelKey: 'chart_memory',
    source: 'metrics',
    unit: '%'
  },
  {
    color: 'var(--color-chart-3)',
    dataKey: 'disk_pct',
    domain: [0, 100],
    labelKey: 'chart_disk',
    source: 'metrics',
    unit: '%'
  },
  { bytes: true, color: 'var(--color-chart-4)', dataKey: 'net_in_speed', labelKey: 'chart_net_in', source: 'metrics' },
  {
    bytes: true,
    color: 'var(--color-chart-5)',
    dataKey: 'net_out_speed',
    labelKey: 'chart_net_out',
    source: 'metrics'
  },
  { color: 'var(--color-chart-1)', dataKey: 'load1', labelKey: 'chart_load', source: 'metrics' },
  {
    color: 'var(--color-chart-4)',
    dataKey: 'temperature',
    gate: 'temperature',
    labelKey: 'chart_temperature',
    source: 'metrics',
    unit: '°C'
  },
  {
    color: 'var(--color-chart-5)',
    dataKey: 'gpu_usage',
    domain: [0, 100],
    gate: 'gpu',
    labelKey: 'chart_gpu_usage',
    source: 'gpu',
    unit: '%'
  },
  // GPU temp is admin-only: the public surface never populates gpu_temp, so
  // the gpuTemp gate (non-empty data key) keeps the chart off that surface.
  {
    color: 'var(--color-chart-2)',
    dataKey: 'gpu_temp',
    gate: 'gpuTemp',
    labelKey: 'chart_gpu_temp',
    source: 'gpu',
    unit: '°C'
  }
]

/** The one percentage guard: a missing/zero total renders as 0%, never NaN/Infinity. */
export function pct(used: number, total: number): number {
  return total > 0 ? (used / total) * 100 : 0
}

export function toMetricChartRow(p: MetricSeriesPoint, memTotal: number, diskTotal: number): MetricChartRow {
  return {
    cpu: p.cpu,
    disk_pct: pct(p.disk_used, diskTotal),
    load1: p.load1,
    load5: p.load5,
    load15: p.load15,
    memory_pct: pct(p.mem_used, memTotal),
    net_in_speed: p.net_in_speed,
    net_in_transfer: p.net_in_transfer,
    net_out_speed: p.net_out_speed,
    net_out_transfer: p.net_out_transfer,
    temperature: p.temperature,
    timestamp: p.time
  }
}

export interface GpuRecordAggregated {
  gpu_usage_avg: number
  mem_total_avg: number
  mem_used_avg: number
  temperature_avg: number
  time: string
}

export function buildGpuChartRows(
  isAdminVariant: boolean,
  gpuRecords: GpuRecordAggregated[] | undefined,
  publicMetrics: PublicMetricsPoint[] | undefined
): Record<string, unknown>[] {
  if (isAdminVariant) {
    if (!gpuRecords || gpuRecords.length === 0) {
      return []
    }
    return gpuRecords.map((r) => ({
      gpu_mem_pct: pct(r.mem_used_avg, r.mem_total_avg),
      gpu_temp: r.temperature_avg,
      gpu_usage: r.gpu_usage_avg,
      timestamp: r.time
    }))
  }
  if (!publicMetrics) {
    return []
  }
  return publicMetrics.filter((p) => p.gpu_usage != null).map((p) => ({ gpu_usage: p.gpu_usage, timestamp: p.time }))
}

function formatHourMinute(time: string) {
  const d = new Date(time)
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

function formatMonthDay(time: string) {
  const d = new Date(time)
  return `${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
}

/** Range length (hours) at which tick labels switch from HH:MM to MM-DD. */
const LONG_RANGE_HOURS = 168

/**
 * Realtime points arrive seconds apart, so consecutive ticks often share an
 * HH:MM label — repeats collapse to '' and only the first of each minute is
 * labelled.
 */
export function buildRealtimeTickLabels(rows: { timestamp?: unknown }[]): Map<string, string> {
  const labels = new Map<string, string>()
  let previousLabel = ''
  for (const point of rows) {
    if (typeof point.timestamp !== 'string') {
      continue
    }
    const label = formatHourMinute(point.timestamp)
    labels.set(point.timestamp, label === previousLabel ? '' : label)
    previousLabel = label
  }
  return labels
}

export function makeTickFormatter(
  isRealtime: boolean,
  rangeHours: number,
  rows: { timestamp?: unknown }[]
): ((time: string) => string) | undefined {
  if (isRealtime) {
    const realtimeLabels = buildRealtimeTickLabels(rows)
    return (time: string) => realtimeLabels.get(time) ?? formatHourMinute(time)
  }
  if (rangeHours >= LONG_RANGE_HOURS) {
    return formatMonthDay
  }
  return undefined
}

export function makeTooltipFormatter(isRealtime: boolean, rangeHours: number): ((time: string) => string) | undefined {
  if (isRealtime) {
    return (time: string) => {
      const d = new Date(time)
      return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}:${String(d.getSeconds()).padStart(2, '0')}`
    }
  }
  if (rangeHours >= LONG_RANGE_HOURS) {
    return (time: string) => `${formatMonthDay(time)} ${formatHourMinute(time)}`
  }
  return undefined
}

/**
 * Recharts X-axis `interval` (stride = N+1 ticks). Long ranges produce 168/720
 * hourly samples — labelling every one collapses into illegible overlap, so we
 * target roughly 8 evenly spaced labels.
 */
export function xAxisStride(isRealtime: boolean, rangeHours: number, dataLength: number): number | undefined {
  if (isRealtime) {
    return 0
  }
  if (rangeHours >= LONG_RANGE_HOURS && dataLength > 0) {
    const targetLabels = 8
    return Math.max(0, Math.floor(dataLength / targetLabels) - 1)
  }
  return undefined
}

export interface NetworkLabels {
  netInLabel: string
  netOutLabel: string
  netTotalLabel: string | null
}

export function deriveNetworkLabels(
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
