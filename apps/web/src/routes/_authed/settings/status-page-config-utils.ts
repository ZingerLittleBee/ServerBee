import type { UpdateStatusPageRequest } from '@/lib/api-schema'

export interface ConfigFormState {
  defaultLayout: 'grid' | 'list'
  description: string
  enabled: boolean
  redThreshold: number
  selectedServers: string[]
  showIncidents: boolean
  showIpQuality: boolean
  showMaintenance: boolean
  showNetwork: boolean
  showServerDetail: boolean
  title: string
  yellowThreshold: number
}

export function parseServerIds(raw: string | null | undefined): string[] {
  if (!raw) {
    return []
  }
  try {
    const parsed = JSON.parse(raw) as unknown
    if (Array.isArray(parsed)) {
      return parsed.filter((v): v is string => typeof v === 'string')
    }
  } catch {
    // ignore malformed JSON; fall through to []
  }
  return []
}

export function buildStatusPageUpdatePayload(state: ConfigFormState): UpdateStatusPageRequest {
  return {
    default_layout: state.defaultLayout,
    description: state.description.trim() ? state.description.trim() : null,
    enabled: state.enabled,
    server_ids: state.selectedServers,
    show_incidents: state.showIncidents,
    show_ip_quality: state.showIpQuality,
    show_maintenance: state.showMaintenance,
    show_network: state.showNetwork,
    show_server_detail: state.showServerDetail,
    title: state.title.trim(),
    uptime_red_threshold: state.redThreshold,
    uptime_yellow_threshold: state.yellowThreshold
  }
}
