export type TailscaleConnectionMode = 'direct' | 'derp' | 'peer_relay' | 'idle'

export interface TailscaleNodeSummary {
  backend_state: string
  dns_name?: string | null
  exit_node: boolean
  exit_node_option: boolean
  hostname?: string | null
  key_expiry?: string | null
  online: boolean
  tailnet_name?: string | null
  tailscale_ips: string[]
  version?: string | null
}

export interface TailscalePeerInfo {
  active: boolean
  connection_mode: TailscaleConnectionMode | string
  cur_addr?: string | null
  dns_name?: string | null
  exit_node: boolean
  exit_node_option: boolean
  host_name: string
  id: string
  last_handshake?: string | null
  last_seen?: string | null
  online: boolean
  os?: string | null
  relay?: string | null
  rx_bytes: number
  tags: string[]
  tailscale_ips: string[]
  tx_bytes: number
}

export interface TailscaleStatusReport {
  advertised_routes: string[]
  collected_at: string
  derp_region?: string | null
  health_warnings: string[]
  peers: TailscalePeerInfo[]
  summary: TailscaleNodeSummary
  tags: string[]
}

export interface ServerTailscaleStatusResponse {
  available: boolean
  cached: boolean
  online: boolean
  status?: TailscaleStatusReport | null
  tailscale_dns_name?: string | null
  tailscale_ipv4?: string | null
  tailscale_ipv6?: string | null
  tailscale_tailnet?: string | null
}

export interface TailnetDeviceItem {
  addresses: string[]
  authorized: boolean
  client_version?: string | null
  expires?: string | null
  hostname?: string | null
  id: string
  key_expiry_disabled: boolean
  last_seen?: string | null
  matched_server_id?: string | null
  matched_server_name?: string | null
  name: string
  online?: boolean | null
  os?: string | null
  tags: string[]
}

export interface TailnetOverviewResponse {
  configured: boolean
  devices: TailnetDeviceItem[]
  error?: string | null
  expiring_keys_count: number
  fetched_at?: string | null
  linked_servers: number
  online_devices: number
  provider: string
  server_tailnet_url?: string | null
  tailnet: string
  total_devices: number
}

export interface TailscaleAdminConfigResponse {
  api_base_url: string
  auth_enabled: boolean
  auto_provision_role: string
  enabled: boolean
  has_api_key: boolean
  preferred_server_url: string
  provider: string
  tailnet: string
}

export interface UpdateTailscaleConfigRequest {
  api_base_url: string
  api_key?: string | null
  auth_enabled?: boolean
  auto_provision_role?: string
  enabled: boolean
  preferred_server_url?: string | null
  provider: string
  tailnet: string
}

export interface TestTailscaleConfigRequest {
  api_base_url?: string
  api_key?: string
  provider?: string
  tailnet?: string
}

export interface TailscaleConnectionTestResponse {
  device_count: number
  message: string
  success: boolean
}

const ZERO_TIME_PREFIX = '0001-01-01'

export function isZeroTimestamp(value?: string | null): boolean {
  if (!value) {
    return true
  }
  const trimmed = value.trim()
  if (!trimmed || trimmed.startsWith(ZERO_TIME_PREFIX)) {
    return true
  }
  const ms = Date.parse(trimmed)
  return Number.isNaN(ms) || ms <= 0
}

export interface KeyExpiryInfo {
  daysRemaining: number | null
  formattedDate: string | null
  state: 'disabled' | 'expired' | 'expiring_soon' | 'valid'
}

export function evaluateKeyExpiry(
  keyExpiry?: string | null,
  explicitlyDisabled = false,
  nowMs = Date.now()
): KeyExpiryInfo {
  if (explicitlyDisabled || isZeroTimestamp(keyExpiry)) {
    return { state: 'disabled', daysRemaining: null, formattedDate: null }
  }
  const expiryMs = Date.parse(keyExpiry as string)
  const diffMs = expiryMs - nowMs
  const formattedDate = new Date(expiryMs).toISOString().slice(0, 10)
  if (diffMs <= 0) {
    return { state: 'expired', daysRemaining: 0, formattedDate }
  }
  const daysRemaining = Math.ceil(diffMs / 86_400_000)
  if (daysRemaining <= 7) {
    return { state: 'expiring_soon', daysRemaining, formattedDate }
  }
  return { state: 'valid', daysRemaining, formattedDate }
}

export function splitTailscaleIps(ips: string[]): { ipv4: string | null; ipv6: string | null } {
  let ipv4: string | null = null
  let ipv6: string | null = null
  for (const ip of ips) {
    if (!ipv4 && ip.includes('.')) {
      ipv4 = ip
    } else if (!ipv6 && ip.includes(':')) {
      ipv6 = ip
    }
  }
  return { ipv4, ipv6 }
}
