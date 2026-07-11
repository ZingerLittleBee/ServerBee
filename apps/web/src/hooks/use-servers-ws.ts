import type { InfiniteData } from '@tanstack/react-query'
import { useQueryClient } from '@tanstack/react-query'
import i18next from 'i18next'
import { useEffect, useRef } from 'react'
import { toast } from 'sonner'
import type { SecurityEventDto, SecurityEventList } from '@/lib/api-schema'
import type { IpQualitySnapshotData, ServerIpQualityData, UnlockResultDto, UnlockStatus } from '@/lib/ip-quality-types'
import type { NetworkProbeResultData } from '@/lib/network-types'
import { type LiveMetrics, projectServerCatalog, type ServerMetrics } from '@/lib/server-catalog'
import { WsClient } from '@/lib/ws-client'
import type {
  DockerContainer,
  DockerContainerStats,
  DockerEventInfo
} from '@/routes/_authed/servers/$serverId/docker/types'
import { type UpgradeJob, useUpgradeJobsStore } from '@/stores/upgrade-jobs-store'

const MAX_DOCKER_EVENTS = 100
const MAX_SECURITY_EVENTS_IN_CACHE = 200

type WsMessage =
  | { type: 'full_sync'; servers: ServerMetrics[]; upgrades?: UpgradeJob[] }
  | { type: 'update'; servers: LiveMetrics[] }
  | { type: 'server_online'; server_id: string }
  | { type: 'server_offline'; server_id: string }
  | {
      type: 'capabilities_changed'
      server_id: string
      capabilities: number
      agent_local_capabilities?: number | null
      effective_capabilities?: number | null
      temporary?: Array<{ cap: string; granted_at: number; expires_at: number }>
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

type QueryClient = ReturnType<typeof useQueryClient>
type FullSyncMessage = Extract<WsMessage, { type: 'full_sync' }>
type UpdateMessage = Extract<WsMessage, { type: 'update' }>

function isWsMessageLike(raw: unknown): raw is { type: string } & Record<string, unknown> {
  return typeof raw === 'object' && raw !== null && 'type' in raw && typeof (raw as { type: unknown }).type === 'string'
}

function handleFullSyncMessage(msg: FullSyncMessage, queryClient: QueryClient): void {
  projectServerCatalog(queryClient, { kind: 'ws_full_sync', servers: msg.servers })
  if (Array.isArray(msg.upgrades)) {
    useUpgradeJobsStore.getState().setJobs(msg.upgrades)
  }
}

function handleUpdateMessage(msg: UpdateMessage, queryClient: QueryClient): void {
  projectServerCatalog(queryClient, { kind: 'ws_update', servers: msg.servers })
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
    projectServerCatalog(queryClient, { kind: 'online_changed', serverId: server_id, online })
  }
}

function handleCapabilityMessage(raw: { type: string } & Record<string, unknown>, queryClient: QueryClient): void {
  if (raw.type === 'capabilities_changed') {
    if (typeof raw.server_id !== 'string' || typeof raw.capabilities !== 'number') {
      return
    }
    const msg = raw as WsMessage & { type: 'capabilities_changed' }
    const { server_id, capabilities, agent_local_capabilities, effective_capabilities, temporary } = msg
    projectServerCatalog(queryClient, {
      kind: 'capabilities_changed',
      serverId: server_id,
      capabilities,
      agentLocalCapabilities: agent_local_capabilities,
      effectiveCapabilities: effective_capabilities,
      temporary
    })
    return
  }
  if (raw.type === 'agent_info_updated') {
    if (typeof raw.server_id !== 'string' || typeof raw.protocol_version !== 'number') {
      return
    }
    const msg = raw as WsMessage & { type: 'agent_info_updated' }
    const { server_id, protocol_version, agent_version } = msg
    projectServerCatalog(queryClient, {
      kind: 'agent_info_changed',
      serverId: server_id,
      protocolVersion: protocol_version,
      agentVersion: agent_version
    })
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
    projectServerCatalog(queryClient, { kind: 'docker_availability_changed', serverId: server_id, available })
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

type BrowserMessageHandler = (msg: Record<string, unknown>) => void
const subscribers = new Map<string, Set<BrowserMessageHandler>>()

export function subscribeBrowserMessage(type: string, handler: BrowserMessageHandler): () => void {
  let set = subscribers.get(type)
  if (!set) {
    set = new Set()
    subscribers.set(type, set)
  }
  set.add(handler)
  return () => {
    set?.delete(handler)
  }
}

function dispatchToSubscribers(type: string, msg: Record<string, unknown>): void {
  const set = subscribers.get(type)
  if (!set) {
    return
  }
  for (const handler of set) {
    handler(msg)
  }
}

export function handleWsMessage(raw: unknown, queryClient: QueryClient): void {
  if (!isWsMessageLike(raw)) {
    console.warn('WS: unexpected message shape', raw)
    return
  }
  switch (raw.type) {
    case 'traceroute_update':
      dispatchToSubscribers('traceroute_update', raw)
      break
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
      dispatchToSubscribers('network_probe_update', raw)
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
