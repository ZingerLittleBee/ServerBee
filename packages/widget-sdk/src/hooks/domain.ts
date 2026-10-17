import { useApiMutation, useApiQuery } from './escape-hatch'

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

export interface HistoryPoint {
  ts: number
  value: number
}

export function useHistory(serverId: string | null, path: string, range: string) {
  return useApiQuery<HistoryPoint[]>('/api/metrics/history', {
    params: { server_id: serverId ?? undefined, path, range },
    enabled: serverId !== null
  })
}

export function useGeoIp() {
  const status = useApiQuery<{ installed: boolean; source?: string }>('/api/geoip/status')
  const download = useApiMutation<{ success: boolean }>('POST', '/api/geoip/download', {
    invalidateQueryKeys: [['widget-api', '/api/geoip/status']]
  })
  return { status, download }
}
