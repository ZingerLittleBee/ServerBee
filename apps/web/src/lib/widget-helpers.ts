import i18next from 'i18next'
import type { ServerMetricRecord, UptimeDailyEntry } from '@/lib/api-schema'
import type { ServerMetrics } from '@/lib/server-catalog'
import { parseDiskIoJson } from './disk-io'
import { activeLocale, formatDateTime } from './format'

// --- Shared metric labels ---

// Maps a metric id to its localized label key in the `dashboard` namespace
// (see `common.metrics.*` in the dashboard locale files).
const METRIC_LABEL_KEYS: Record<string, string> = {
  cpu: 'common.metrics.cpu',
  memory: 'common.metrics.memory',
  disk: 'common.metrics.disk',
  swap: 'common.metrics.swap',
  load1: 'common.metrics.load1m',
  load5: 'common.metrics.load5m',
  load15: 'common.metrics.load15m',
  net_in: 'common.metrics.networkIn',
  net_out: 'common.metrics.networkOut',
  bandwidth: 'common.metrics.bandwidth',
  network: 'common.metrics.network',
  disk_io: 'common.metrics.diskIo'
}

// Resolves a metric id to its localized display label. Pass a `t` bound to the
// `dashboard` namespace (e.g. from `useTranslation('dashboard')`).
export function metricLabel(metric: string, t: (key: string) => string): string {
  const key = METRIC_LABEL_KEYS[metric]
  return key ? t(key) : metric
}

export const METRIC_UNITS: Record<string, string> = {
  cpu: '%',
  memory: '%',
  disk: '%'
}

// --- Metric extraction ---

export function isNetworkMetric(metric: string): boolean {
  return metric === 'net_in' || metric === 'net_out'
}

export function extractLiveMetric(server: ServerMetrics, metric: string): number {
  switch (metric) {
    case 'cpu':
      return server.cpu
    case 'memory':
      return server.mem_total > 0 ? (server.mem_used / server.mem_total) * 100 : 0
    case 'disk':
      return server.disk_total > 0 ? (server.disk_used / server.disk_total) * 100 : 0
    case 'swap':
      return server.swap_total > 0 ? (server.swap_used / server.swap_total) * 100 : 0
    case 'bandwidth':
    case 'network':
      return server.net_in_speed + server.net_out_speed
    case 'disk_io':
      return server.disk_read_bytes_per_sec + server.disk_write_bytes_per_sec
    default:
      return 0
  }
}

function sumDiskIoJson(raw: string | null | undefined): number {
  const samples = parseDiskIoJson(raw)
  let total = 0
  for (const sample of samples) {
    total += sample.read_bytes_per_sec + sample.write_bytes_per_sec
  }
  return total
}

export function extractRecordMetric(record: ServerMetricRecord, metric: string, server?: ServerMetrics): number {
  switch (metric) {
    case 'cpu':
      return record.cpu
    case 'memory':
      return server?.mem_total ? (record.mem_used / server.mem_total) * 100 : 0
    case 'disk':
      return server?.disk_total ? (record.disk_used / server.disk_total) * 100 : 0
    case 'load1':
      return record.load1
    case 'load5':
      return record.load5
    case 'load15':
      return record.load15
    case 'net_in':
      return record.net_in_speed
    case 'net_out':
      return record.net_out_speed
    case 'network':
      return record.net_in_speed + record.net_out_speed
    case 'disk_io':
      return sumDiskIoJson(record.disk_io_json)
    default:
      return 0
  }
}

// --- Time formatting ---

export function formatChartTime(time: string): string {
  const date = new Date(time)
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false })
}

export function formatChartDateTime(time: string): string {
  return formatDateTime(time, {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    hour12: false
  })
}

export function formatRelativeTime(input: string | number | null): string {
  if (input === null) {
    return i18next.t('status.never')
  }
  const ms = typeof input === 'number' ? input * 1000 : new Date(input).getTime()
  const diff = Math.max(0, Math.floor((Date.now() - ms) / 1000))
  const rtf = new Intl.RelativeTimeFormat(activeLocale(), { numeric: 'always', style: 'narrow' })

  if (diff < 60) {
    return rtf.format(-diff, 'second')
  }
  if (diff < 3600) {
    return rtf.format(-Math.floor(diff / 60), 'minute')
  }
  if (diff < 86_400) {
    return rtf.format(-Math.floor(diff / 3600), 'hour')
  }
  return rtf.format(-Math.floor(diff / 86_400), 'day')
}

// --- JSON config parsing ---

export function parseConfig<T>(configJson: string): T {
  try {
    return JSON.parse(configJson) as T
  } catch {
    return {} as T
  }
}

// --- Server ID filtering ---

export function filterByIds<T>(items: T[], ids: string[] | undefined, key: (t: T) => string): T[] {
  if (!ids || ids.length === 0) {
    return items
  }
  const idSet = new Set(ids)
  return items.filter((item) => idSet.has(key(item)))
}

// --- Uptime helpers ---

export type UptimeColor = 'green' | 'yellow' | 'red' | 'gray'

export function computeUptimeColor(
  onlineMinutes: number,
  totalMinutes: number,
  yellowThreshold = 100,
  redThreshold = 95
): UptimeColor {
  if (totalMinutes === 0) {
    return 'gray'
  }
  const pct = (onlineMinutes / totalMinutes) * 100
  if (pct >= yellowThreshold) {
    return 'green'
  }
  if (pct >= redThreshold) {
    return 'yellow'
  }
  return 'red'
}

export function computeAggregateUptime(days: UptimeDailyEntry[]): number | null {
  let totalOnline = 0
  let totalMinutes = 0
  for (const d of days) {
    totalOnline += d.online_minutes
    totalMinutes += d.total_minutes
  }
  if (totalMinutes === 0) {
    return null
  }
  return (totalOnline / totalMinutes) * 100
}

export function formatUptimeTooltip(entry: UptimeDailyEntry): {
  date: string
  duration: string
  incidents: string
  percentage: string
} {
  if (entry.total_minutes === 0) {
    const noData = i18next.t('status:uptime_no_data')
    return {
      date: entry.date,
      percentage: noData,
      duration: noData,
      incidents: noData
    }
  }
  const pct = (entry.online_minutes / entry.total_minutes) * 100
  const downMinutes = entry.total_minutes - entry.online_minutes
  const hours = Math.floor(downMinutes / 60)
  const mins = Math.round(downMinutes % 60)
  const duration =
    hours > 0
      ? i18next.t('status:uptime_downtime_hours', { hours, mins })
      : i18next.t('status:uptime_downtime_minutes', { mins })
  return {
    date: entry.date,
    percentage: `${pct.toFixed(2)}%`,
    duration,
    incidents: i18next.t('status:uptime_incidents', { count: entry.downtime_incidents })
  }
}
