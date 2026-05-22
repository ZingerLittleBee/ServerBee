import type { InfiniteData } from '@tanstack/react-query'
import { useQueryClient } from '@tanstack/react-query'
import i18next from 'i18next'
import { useEffect, useRef } from 'react'
import { toast } from 'sonner'
import type { RecoveryJobResponse, SecurityEventDto, SecurityEventList } from '@/lib/api-schema'
import type { IpQualitySnapshotData, ServerIpQualityData, UnlockResultDto, UnlockStatus } from '@/lib/ip-quality-types'
import type { NetworkProbeResultData } from '@/lib/network-types'
import { WsClient } from '@/lib/ws-client'
import type {
  DockerContainer,
  DockerContainerStats,
  DockerEventInfo
} from '@/routes/_authed/servers/$serverId/docker/types'
import { useRecoveryJobsStore } from '@/stores/recovery-jobs-store'
import { type UpgradeJob, useUpgradeJobsStore } from '@/stores/upgrade-jobs-store'

const MAX_DOCKER_EVENTS = 100
const MAX_SECURITY_EVENTS_IN_CACHE = 200

interface ServerMetrics {
  agent_local_capabilities?: number | null
  capabilities?: number
  country_code: string | null
  cpu: number
  cpu_cores?: number | null
  cpu_name: string | null
  disk_read_bytes_per_sec: number
  disk_total: number
  disk_used: number
  disk_write_bytes_per_sec: number
  effective_capabilities?: number | null
  features?: string[]
  group_id: string | null
  id: string
  last_active: number
  load1: number
  load5: number
  load15: number
  mem_total: number
  mem_used: number
  name: string
  net_in_speed: number
  net_in_transfer: number
  net_out_speed: number
  net_out_transfer: number
  online: boolean
  os: string | null
  process_count: number
  protocol_version?: number
  region: string | null
  swap_total: number
  swap_used: number
  tags?: string[]
  tcp_conn: number
  udp_conn: number
  uptime: number
}

type WsMessage =
  | { type: 'full_sync'; servers: ServerMetrics[]; upgrades?: UpgradeJob[]; recoveries?: RecoveryJobResponse[] }
  | { type: 'update'; servers: ServerMetrics[]; recoveries?: RecoveryJobResponse[] | null }
  | { type: 'server_online'; server_id: string }
  | { type: 'server_offline'; server_id: string }
  | {
      type: 'capabilities_changed'
      server_id: string
      capabilities: number
      agent_local_capabilities?: number | null
      effective_capabilities?: number | null
    }
  | { type: 'agent_info_updated'; server_id: string; protocol_version: number; agent_version?: string | null }
  | { type: 'network_probe_update'; server_id: string; results: NetworkProbeResultData[] }
  | {
      type: 'docker_update'
      server_id: string
      containers: DockerContainer[]
      stats: DockerContainerStats[] | null
    }
  | { type: 'docker_event'; server_id: string; event: DockerEventInfo }
  | { type: 'docker_availability_changed'; server_id: string; available: boolean }
  | { type: 'upgrade_progress'; server_id: string; job_id: string; target_version: string; stage: string }
  | {
      type: 'upgrade_result'
      server_id: string
      job_id: string
      target_version: string
      status: string
      stage?: string
      error?: string | null
      backup_path?: string | null
    }
  | {
      type: 'security_event'
      server_id: string
      event_id: string
      event: SecurityEventDto
    }
  | {
      type: 'blocklist_changed'
      kind: 'created' | 'deleted'
      block_id: string
      target: string
    }
  | {
      type: 'firewall_apply_state_changed'
      block_id: string
      server_id: string
      state: 'present' | 'absent' | 'failed'
      reason?: string | null
    }
  | {
      type: 'ip_quality_update'
      server_id: string
      unlock_results: WsUnlockResult[]
      ip_quality: IpQualitySnapshotData | null
    }

/** Unlock result as carried by the `ip_quality_update` WS message — the
 *  protocol's `UnlockResultData`, which is leaner than the REST `UnlockResultDto`. */
interface WsUnlockResult {
  detail: string | null
  latency_ms: number | null
  region: string | null
  service_id: string
  status: UnlockStatus
}

export type { ServerMetrics }

const STATIC_FIELDS = new Set([
  'mem_total',
  'swap_total',
  'disk_total',
  'cpu_name',
  'cpu_cores',
  'os',
  'region',
  'country_code',
  'group_id',
  'tags',
  'features'
])

export function mergeServerUpdate(prev: ServerMetrics[], incoming: ServerMetrics[]): ServerMetrics[] {
  const updated = [...prev]
  for (const server of incoming) {
    const idx = updated.findIndex((s) => s.id === server.id)
    if (idx >= 0) {
      const merged = { ...updated[idx] }
      for (const [key, value] of Object.entries(server)) {
        const isStaticDefault =
          STATIC_FIELDS.has(key) && (value === null || value === 0 || (Array.isArray(value) && value.length === 0))
        if (!isStaticDefault) {
          ;(merged as Record<string, unknown>)[key] = value
        }
      }
      updated[idx] = merged as ServerMetrics
    }
  }
  return updated
}

export function setServerOnlineStatus(prev: ServerMetrics[], serverId: string, online: boolean): ServerMetrics[] {
  return prev.map((s) => (s.id === serverId ? { ...s, online } : s))
}

export function setServerDockerAvailability(
  prev: ServerMetrics[],
  serverId: string,
  available: boolean
): ServerMetrics[] {
  return prev.map((s) => {
    if (s.id !== serverId) {
      return s
    }
    const features = s.features ?? []
    if (available && !features.includes('docker')) {
      return { ...s, features: [...features, 'docker'] }
    }
    if (!available && features.includes('docker')) {
      return { ...s, features: features.filter((f) => f !== 'docker') }
    }
    return s
  })
}

export function applyServerEdit(
  prev: ServerMetrics[],
  serverId: string,
  edited: { name: string; group_id: string | null }
): ServerMetrics[] {
  return prev.map((server) =>
    server.id === serverId ? { ...server, name: edited.name, group_id: edited.group_id } : server
  )
}

export function setServerCapabilities(
  prev: ServerMetrics[],
  serverId: string,
  capabilities: number,
  agentLocalCapabilities: number | null | undefined,
  effectiveCapabilities: number | null | undefined
): ServerMetrics[] {
  return prev.map((server) =>
    server.id === serverId
      ? {
          ...server,
          capabilities,
          agent_local_capabilities: agentLocalCapabilities ?? null,
          effective_capabilities: effectiveCapabilities ?? null
        }
      : server
  )
}

function setServerDetailDockerAvailability(
  prev: Record<string, unknown> | undefined,
  available: boolean
): Record<string, unknown> | undefined {
  if (!prev) {
    return prev
  }

  const features = Array.isArray(prev.features)
    ? prev.features.filter((feature): feature is string => typeof feature === 'string')
    : []

  if (available && !features.includes('docker')) {
    return { ...prev, features: [...features, 'docker'] }
  }

  if (!available && features.includes('docker')) {
    return { ...prev, features: features.filter((feature) => feature !== 'docker') }
  }

  return prev
}

type QueryClient = ReturnType<typeof useQueryClient>
type FullSyncMessage = Extract<WsMessage, { type: 'full_sync' }>
type UpdateMessage = Extract<WsMessage, { type: 'update' }>

function isWsMessageLike(raw: unknown): raw is { type: string } & Record<string, unknown> {
  return typeof raw === 'object' && raw !== null && 'type' in raw && typeof (raw as { type: unknown }).type === 'string'
}

function hydrateRecoveryJobs(raw: FullSyncMessage | UpdateMessage, replaceMissing: boolean): void {
  if (Array.isArray(raw.recoveries)) {
    useRecoveryJobsStore.getState().setJobs(raw.recoveries)
    return
  }

  if (replaceMissing) {
    useRecoveryJobsStore.getState().setJobs([])
  }
}

function handleFullSyncMessage(msg: FullSyncMessage, queryClient: QueryClient): void {
  queryClient.setQueryData<ServerMetrics[]>(['servers'], msg.servers)
  if (Array.isArray(msg.upgrades)) {
    useUpgradeJobsStore.getState().setJobs(msg.upgrades as UpgradeJob[])
  }
  hydrateRecoveryJobs(msg, true)
}

function handleUpdateMessage(msg: UpdateMessage, queryClient: QueryClient): void {
  queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) =>
    prev ? mergeServerUpdate(prev, msg.servers) : msg.servers
  )
  hydrateRecoveryJobs(msg, false)
}

function handleServerMetricsMessage(raw: { type: string } & Record<string, unknown>, queryClient: QueryClient): void {
  if (raw.type === 'full_sync' || raw.type === 'update') {
    if (!Array.isArray(raw.servers) || raw.servers.some((s: unknown) => s == null || typeof s !== 'object')) {
      return
    }
    const msg = raw as FullSyncMessage | UpdateMessage
    if (raw.type === 'full_sync') {
      handleFullSyncMessage(msg as FullSyncMessage, queryClient)
    } else {
      handleUpdateMessage(msg as UpdateMessage, queryClient)
    }
    return
  }
  if (raw.type === 'server_online' || raw.type === 'server_offline') {
    if (typeof raw.server_id !== 'string') {
      return
    }
    const online = raw.type === 'server_online'
    const server_id = raw.server_id as string
    queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) =>
      prev ? setServerOnlineStatus(prev, server_id, online) : prev
    )
  }
}

function handleCapabilityMessage(raw: { type: string } & Record<string, unknown>, queryClient: QueryClient): void {
  if (raw.type === 'capabilities_changed') {
    if (typeof raw.server_id !== 'string' || typeof raw.capabilities !== 'number') {
      return
    }
    const msg = raw as WsMessage & { type: 'capabilities_changed' }
    const { server_id, capabilities, agent_local_capabilities, effective_capabilities } = msg
    queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) =>
      prev
        ? setServerCapabilities(prev, server_id, capabilities, agent_local_capabilities, effective_capabilities)
        : prev
    )
    queryClient.setQueryData(['servers', server_id], (prev: Record<string, unknown> | undefined) =>
      prev
        ? {
            ...prev,
            capabilities,
            agent_local_capabilities: agent_local_capabilities ?? null,
            effective_capabilities: effective_capabilities ?? null
          }
        : prev
    )
    queryClient.setQueryData<Record<string, unknown>[]>(['servers-list'], (prev) =>
      prev?.map((s) =>
        s.id === server_id
          ? {
              ...s,
              capabilities,
              agent_local_capabilities: agent_local_capabilities ?? null,
              effective_capabilities: effective_capabilities ?? null
            }
          : s
      )
    )
    return
  }
  if (raw.type === 'agent_info_updated') {
    if (typeof raw.server_id !== 'string' || typeof raw.protocol_version !== 'number') {
      return
    }
    const msg = raw as WsMessage & { type: 'agent_info_updated' }
    const { server_id, protocol_version, agent_version } = msg
    queryClient.setQueryData(['servers', server_id], (prev: Record<string, unknown> | undefined) =>
      prev ? { ...prev, protocol_version, agent_version: agent_version ?? null } : prev
    )
    queryClient.setQueryData<Record<string, unknown>[]>(['servers-list'], (prev) =>
      prev?.map((s) => (s.id === server_id ? { ...s, protocol_version, agent_version: agent_version ?? null } : s))
    )
  }
}

function handleDockerMessage(raw: { type: string } & Record<string, unknown>, queryClient: QueryClient): void {
  if (raw.type === 'docker_update') {
    if (
      typeof raw.server_id !== 'string' ||
      !Array.isArray(raw.containers) ||
      raw.containers.some((c: unknown) => c == null || typeof c !== 'object')
    ) {
      return
    }
    const msg = raw as WsMessage & { type: 'docker_update' }
    const { server_id, containers, stats } = msg
    queryClient.setQueryData<DockerContainer[]>(['docker', 'containers', server_id], containers)
    if (stats) {
      queryClient.setQueryData<DockerContainerStats[]>(['docker', 'stats', server_id], stats)
    }
    return
  }
  if (raw.type === 'docker_event') {
    if (typeof raw.server_id !== 'string' || typeof raw.event !== 'object' || raw.event === null) {
      return
    }
    const msg = raw as WsMessage & { type: 'docker_event' }
    const { server_id, event } = msg
    queryClient.setQueryData<DockerEventInfo[]>(['docker', 'events', server_id], (prev) => {
      const events = prev ?? []
      const updated = [event, ...events]
      return updated.length > MAX_DOCKER_EVENTS ? updated.slice(0, MAX_DOCKER_EVENTS) : updated
    })
    return
  }
  if (raw.type === 'docker_availability_changed') {
    if (typeof raw.server_id !== 'string' || typeof raw.available !== 'boolean') {
      return
    }
    const msg = raw as WsMessage & { type: 'docker_availability_changed' }
    const { server_id, available } = msg
    queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) =>
      prev ? setServerDockerAvailability(prev, server_id, available) : prev
    )
    queryClient.setQueryData(['servers', server_id], (prev: Record<string, unknown> | undefined) =>
      setServerDetailDockerAvailability(prev, available)
    )
  }
}

function prependSecurityEventToInfinite(
  prev: InfiniteData<SecurityEventList> | undefined,
  event: SecurityEventDto
): InfiniteData<SecurityEventList> | undefined {
  if (!prev || prev.pages.length === 0) {
    return prev
  }
  const [firstPage, ...rest] = prev.pages
  if (firstPage.items.some((existing) => existing.id === event.id)) {
    return prev
  }
  const combined = [event, ...firstPage.items]
  const capped =
    combined.length > MAX_SECURITY_EVENTS_IN_CACHE ? combined.slice(0, MAX_SECURITY_EVENTS_IN_CACHE) : combined
  const updatedFirst: SecurityEventList = { ...firstPage, items: capped }
  return { ...prev, pages: [updatedFirst, ...rest] }
}

const FIREWALL_DEBOUNCE_TIMERS = new Map<string, ReturnType<typeof setTimeout>>()

function debounceInvalidate(queryClient: QueryClient, queryKey: readonly unknown[], delayMs: number): void {
  const cacheKey = JSON.stringify(queryKey)
  const existing = FIREWALL_DEBOUNCE_TIMERS.get(cacheKey)
  if (existing) {
    clearTimeout(existing)
  }
  const handle = setTimeout(() => {
    FIREWALL_DEBOUNCE_TIMERS.delete(cacheKey)
    queryClient.invalidateQueries({ queryKey: queryKey as unknown as unknown[] }).catch(() => undefined)
  }, delayMs)
  FIREWALL_DEBOUNCE_TIMERS.set(cacheKey, handle)
}

function handleFirewallMessage(raw: { type: string } & Record<string, unknown>, queryClient: QueryClient): void {
  if (raw.type === 'blocklist_changed') {
    if (typeof raw.block_id !== 'string' || typeof raw.target !== 'string') {
      return
    }
    debounceInvalidate(queryClient, ['firewall', 'blocks'], 1000)
    queryClient.invalidateQueries({ queryKey: ['firewall', 'stats'] }).catch(() => undefined)
    return
  }
  if (raw.type === 'firewall_apply_state_changed') {
    if (typeof raw.block_id !== 'string' || typeof raw.server_id !== 'string' || typeof raw.state !== 'string') {
      return
    }
    queryClient.invalidateQueries({ queryKey: ['firewall', 'block', raw.block_id] }).catch(() => undefined)
    debounceInvalidate(queryClient, ['firewall', 'activity'], 500)
  }
}

function handleSecurityEventMessage(raw: { type: string } & Record<string, unknown>, queryClient: QueryClient): void {
  if (raw.type !== 'security_event') {
    return
  }
  if (typeof raw.server_id !== 'string' || typeof raw.event_id !== 'string') {
    return
  }
  if (typeof raw.event !== 'object' || raw.event === null) {
    return
  }
  const event = raw.event as SecurityEventDto
  if (typeof event.id !== 'string' || typeof event.severity !== 'string') {
    return
  }

  queryClient.setQueriesData<InfiniteData<SecurityEventList>>({ queryKey: ['security', 'events'] }, (prev) =>
    prependSecurityEventToInfinite(prev, event)
  )
  queryClient.invalidateQueries({ queryKey: ['security', 'stats'] })

  const severity = event.severity
  if (severity === 'high' || severity === 'critical') {
    const message = i18next.t('security:toast.attack_detected', {
      defaultValue: 'Security event detected from {{ip}}',
      ip: event.source_ip
    })
    toast.warning(message)
  }
}

/** Merge the leaner WS unlock results into a server's cached `UnlockResultDto`
 *  list, replacing entries with the same `service_id` and keeping the rest. */
function mergeUnlockResults(prev: UnlockResultDto[], serverId: string, incoming: WsUnlockResult[]): UnlockResultDto[] {
  const checkedAt = new Date().toISOString()
  const byServiceId = new Map(prev.map((r) => [r.service_id, r]))
  for (const result of incoming) {
    const existing = byServiceId.get(result.service_id)
    byServiceId.set(result.service_id, {
      id: existing?.id ?? `${serverId}:${result.service_id}`,
      server_id: serverId,
      service_id: result.service_id,
      status: result.status,
      region: result.region,
      latency_ms: result.latency_ms,
      detail: result.detail,
      checked_at: checkedAt
    })
  }
  return [...byServiceId.values()]
}

function patchServerIpQuality(
  prev: ServerIpQualityData | undefined,
  serverId: string,
  incoming: WsUnlockResult[],
  ipQuality: IpQualitySnapshotData | null
): ServerIpQualityData {
  const base: ServerIpQualityData = prev ?? { server_id: serverId, unlock_results: [], ip_quality: null }
  return {
    server_id: serverId,
    unlock_results: mergeUnlockResults(base.unlock_results, serverId, incoming),
    // A partial update (ip_quality: null) keeps the previously scored snapshot;
    // a full update replaces it.
    ip_quality: ipQuality ?? base.ip_quality
  }
}

function handleIpQualityMessage(raw: { type: string } & Record<string, unknown>, queryClient: QueryClient): void {
  if (raw.type !== 'ip_quality_update') {
    return
  }
  if (typeof raw.server_id !== 'string' || !Array.isArray(raw.unlock_results)) {
    return
  }
  if (raw.unlock_results.some((r: unknown) => r == null || typeof r !== 'object')) {
    return
  }
  const msg = raw as WsMessage & { type: 'ip_quality_update' }
  const { server_id, unlock_results, ip_quality } = msg

  // Patch the per-server detail cache.
  queryClient.setQueryData<ServerIpQualityData>(['ip-quality', 'servers', server_id], (prev) =>
    patchServerIpQuality(prev, server_id, unlock_results, ip_quality)
  )

  // Patch the all-servers overview cache.
  queryClient.setQueryData<ServerIpQualityData[]>(['ip-quality', 'overview'], (prev) => {
    if (!prev) {
      return prev
    }
    const idx = prev.findIndex((entry) => entry.server_id === server_id)
    const patched = patchServerIpQuality(idx >= 0 ? prev[idx] : undefined, server_id, unlock_results, ip_quality)
    if (idx >= 0) {
      const next = [...prev]
      next[idx] = patched
      return next
    }
    return [...prev, patched]
  })
}

export function handleWsMessage(raw: unknown, queryClient: QueryClient): void {
  if (!isWsMessageLike(raw)) {
    console.warn('WS: unexpected message shape', raw)
    return
  }
  switch (raw.type) {
    case 'full_sync':
    case 'update':
    case 'server_online':
    case 'server_offline':
      handleServerMetricsMessage(raw, queryClient)
      break
    case 'capabilities_changed':
    case 'agent_info_updated':
      handleCapabilityMessage(raw, queryClient)
      break
    case 'network_probe_update': {
      if (
        typeof raw.server_id !== 'string' ||
        !Array.isArray(raw.results) ||
        raw.results.some((r: unknown) => r == null || typeof r !== 'object')
      ) {
        break
      }
      const msg = raw as WsMessage & { type: 'network_probe_update' }
      window.dispatchEvent(
        new CustomEvent('network-probe-update', {
          detail: { server_id: msg.server_id, results: msg.results }
        })
      )
      break
    }
    case 'docker_update':
    case 'docker_event':
    case 'docker_availability_changed':
      handleDockerMessage(raw, queryClient)
      break
    case 'security_event':
      handleSecurityEventMessage(raw, queryClient)
      break
    case 'ip_quality_update':
      handleIpQualityMessage(raw, queryClient)
      break
    case 'blocklist_changed':
    case 'firewall_apply_state_changed':
      handleFirewallMessage(raw, queryClient)
      break
    case 'upgrade_progress': {
      if (
        typeof raw.server_id !== 'string' ||
        typeof raw.job_id !== 'string' ||
        typeof raw.target_version !== 'string' ||
        typeof raw.stage !== 'string'
      ) {
        break
      }
      const { server_id, target_version, stage } = raw as unknown as {
        server_id: string
        job_id: string
        target_version: string
        stage: string
      }
      const existingJob = useUpgradeJobsStore.getState().getJob(server_id)
      if (existingJob) {
        useUpgradeJobsStore.getState().setJob(server_id, {
          ...existingJob,
          stage: stage as UpgradeJob['stage'],
          target_version
        })
      }
      break
    }
    case 'upgrade_result': {
      if (
        typeof raw.server_id !== 'string' ||
        typeof raw.job_id !== 'string' ||
        typeof raw.target_version !== 'string' ||
        typeof raw.status !== 'string'
      ) {
        break
      }
      const { server_id, job_id, target_version, status, stage, error, backup_path } = raw as unknown as {
        server_id: string
        job_id: string
        target_version: string
        status: string
        stage?: string
        error?: string | null
        backup_path?: string | null
      }
      const existingJob = useUpgradeJobsStore.getState().getJob(server_id)
      const now = new Date().toISOString()
      useUpgradeJobsStore.getState().setJob(server_id, {
        server_id,
        job_id,
        target_version,
        stage: (stage as UpgradeJob['stage']) ?? existingJob?.stage ?? 'downloading',
        status: status as UpgradeJob['status'],
        error: error ?? null,
        backup_path: backup_path ?? null,
        started_at: existingJob?.started_at ?? now,
        finished_at: now
      })
      break
    }
    default:
      break
  }
}

export function useServersWs(enabled = true): React.RefObject<WsClient | null> {
  const queryClient = useQueryClient()
  const wsRef = useRef<WsClient | null>(null)

  useEffect(() => {
    if (!enabled) {
      wsRef.current = null
      return
    }

    const ws = new WsClient('/api/ws/servers')
    wsRef.current = ws

    ws.onMessage((raw) => handleWsMessage(raw, queryClient))

    return () => {
      ws.close()
      wsRef.current = null
    }
  }, [enabled, queryClient])

  return wsRef
}
