import { type UseQueryResult, useQuery } from '@tanstack/react-query'
import { request, useApiMutation, useApiQuery } from './escape-hatch'

export interface AlertEvent {
  alert_key: string
  count: number
  event_at: string
  resolved_at: string | null
  rule_id: string
  rule_name: string
  server_id: string
  server_name: string
  status: string
}

export interface ServiceMonitor {
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

export function useAlerts(opts?: { limit?: number }) {
  return useApiQuery<AlertEvent[]>('/api/alert-events', {
    params: { limit: opts?.limit ?? 20 }
  })
}

export function useServiceMonitors() {
  return useApiQuery<ServiceMonitor[]>('/api/service-monitors')
}

export interface TrafficPoint {
  rx: number
  ts: number
  tx: number
}

export function useTraffic(serverId: string | null, range?: string) {
  return useApiQuery<TrafficPoint[]>(serverId ? `/api/servers/${serverId}/traffic` : '/api/traffic/overview/daily', {
    params: { range },
    enabled: true
  })
}

export interface UptimeEntry {
  date: string
  downtime_incidents: number
  online_minutes: number
  total_minutes: number
}

export function useUptime(serverId: string | null, days = 30) {
  return useApiQuery<UptimeEntry[]>(`/api/servers/${serverId ?? ''}/uptime-daily`, {
    params: { days },
    enabled: Boolean(serverId)
  })
}

export type HistoryInterval = 'auto' | 'raw' | 'hourly'

export interface ServerRecord {
  cpu: number
  disk_io_json: string | null
  disk_used: number
  gpu_usage: number | null
  id: number
  load1: number
  load5: number
  load15: number
  mem_used: number
  net_in_speed: number
  net_in_transfer: number
  net_out_speed: number
  net_out_transfer: number
  process_count: number
  server_id: string
  swap_used: number
  tcp_conn: number
  temperature: number | null
  time: string
  udp_conn: number
}

export interface HistoryPoint {
  ts: number
  value: number
}

const DURATION_RE = /^(\d+)(s|m|h|d)$/

const DURATION_UNIT_MS: Record<string, number> = {
  s: 1000,
  m: 60_000,
  h: 3_600_000,
  d: 86_400_000
}

function parseDurationMs(range: string): number | null {
  const match = DURATION_RE.exec(range.trim())
  if (!match) {
    return null
  }
  const amount = Number(match[1])
  const unitMs = DURATION_UNIT_MS[match[2]]
  if (!Number.isFinite(amount) || amount <= 0 || !unitMs) {
    return null
  }
  return amount * unitMs
}

function readRecordValue(record: ServerRecord, field: string): number | null {
  switch (field) {
    case 'cpu':
      return record.cpu
    case 'mem_used':
      return record.mem_used
    case 'swap_used':
      return record.swap_used
    case 'disk_used':
      return record.disk_used
    case 'net_in_speed':
    case 'net_in':
      return record.net_in_speed
    case 'net_out_speed':
    case 'net_out':
      return record.net_out_speed
    case 'network':
      return record.net_in_speed + record.net_out_speed
    case 'net_in_transfer':
      return record.net_in_transfer
    case 'net_out_transfer':
      return record.net_out_transfer
    case 'load1':
      return record.load1
    case 'load5':
      return record.load5
    case 'load15':
      return record.load15
    case 'tcp_conn':
      return record.tcp_conn
    case 'udp_conn':
      return record.udp_conn
    case 'process_count':
      return record.process_count
    case 'temperature':
      return record.temperature
    case 'gpu_usage':
      return record.gpu_usage
    default:
      return null
  }
}

function extractHistoryPoints(records: ServerRecord[], field: string): HistoryPoint[] {
  const points: HistoryPoint[] = []
  for (const record of records) {
    const ts = Date.parse(record.time)
    const value = readRecordValue(record, field)
    if (Number.isFinite(ts) && value !== null && Number.isFinite(value)) {
      points.push({ ts, value })
    }
  }
  return points
}

export function useHistory(
  serverId: string | null,
  field: string,
  range = '1h',
  opts?: { interval?: HistoryInterval }
): UseQueryResult<HistoryPoint[]> {
  const durationMs = parseDurationMs(range)
  const interval = opts?.interval ?? 'auto'
  return useQuery<ServerRecord[], Error, HistoryPoint[]>({
    queryKey: ['widget-api', 'history', serverId, range, interval],
    queryFn: () => {
      const to = new Date()
      const from = new Date(to.getTime() - (durationMs ?? 3_600_000))
      const params = new URLSearchParams({
        from: from.toISOString(),
        to: to.toISOString(),
        interval
      })
      return request<ServerRecord[]>(
        'GET',
        `/api/servers/${encodeURIComponent(serverId ?? '')}/records?${params.toString()}`
      )
    },
    select: (records) => extractHistoryPoints(records, field),
    enabled: Boolean(serverId) && durationMs !== null
  })
}

export function useGeoIp() {
  const status = useApiQuery<{ installed: boolean; source?: string }>('/api/geoip/status')
  const download = useApiMutation<{ success: boolean }>('POST', '/api/geoip/download', {
    invalidateQueryKeys: [['widget-api', '/api/geoip/status']]
  })
  return { status, download }
}
