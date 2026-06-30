import { useApiMutation, useApiQuery } from './escape-hatch'

export interface AlertEvent {
  created_at: string
  id: string
  message: string
  severity: string
}

export interface ServiceMonitor {
  id: string
  name: string
  status: string
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
  day: string
  incidents: number
  uptime_pct: number
}

export function useUptime(serverId: string | null, days = 30) {
  return useApiQuery<UptimeEntry[]>(serverId ? `/api/servers/${serverId}/uptime-daily` : '/api/uptime/overview', {
    params: { days }
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
