import type { ServerMetrics } from '@/hooks/use-servers-ws'
import type { ServerMetricRecord, UptimeDailyEntry } from '@/lib/api-schema'

// --- Shared metric labels ---

export const METRIC_LABELS: Record<string, string> = {
  cpu: 'CPU',
  memory: 'Memory',
  disk: 'Disk',
  swap: 'Swap',
  load1: 'Load (1m)',
  load5: 'Load (5m)',
  load15: 'Load (15m)',
  net_in: 'Network In',
  net_out: 'Network Out',
  bandwidth: 'Bandwidth'
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
      return server.net_in_speed + server.net_out_speed
    default:
      return 0
  }
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
    default:
      return 0
  }
}

// --- Time formatting ---

export function formatChartTime(time: string): string {
  const date = new Date(time)
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}

export function formatRelativeTime(input: string | number | null): string {
  if (input === null) {
    return 'Never'
  }
  const ms = typeof input === 'number' ? input * 1000 : new Date(input).getTime()
  const diff = Math.max(0, Math.floor((Date.now() - ms) / 1000))

  if (diff < 60) {
    return `${diff}s ago`
  }
  if (diff < 3600) {
    return `${Math.floor(diff / 60)}m ago`
  }
  if (diff < 86_400) {
    return `${Math.floor(diff / 3600)}h ago`
  }
  return `${Math.floor(diff / 86_400)}d ago`
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
    return {
      date: entry.date,
      percentage: 'No data',
      duration: 'No data',
      incidents: 'No data'
    }
  }
  const pct = (entry.online_minutes / entry.total_minutes) * 100
  const downMinutes = entry.total_minutes - entry.online_minutes
  const hours = Math.floor(downMinutes / 60)
  const mins = Math.round(downMinutes % 60)
  const duration = hours > 0 ? `${hours}h ${mins}m downtime` : `${mins}m downtime`
  return {
    date: entry.date,
    percentage: `${pct.toFixed(2)}%`,
    duration,
    incidents: `${entry.downtime_incidents} incident${entry.downtime_incidents !== 1 ? 's' : ''}`
  }
}
