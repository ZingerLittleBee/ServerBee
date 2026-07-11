import { type QueryClient, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '@/lib/api-client'
import type { OutstandingEnrollmentSummary, ServerResponse } from '@/lib/api-schema'

const LIVE_SERVERS_KEY = ['server-catalog', 'live'] as const
const SERVER_LIST_KEY = ['server-catalog', 'list'] as const
const SERVER_DETAIL_PREFIX = ['server-catalog', 'detail'] as const
const listRequestGenerations = new WeakMap<QueryClient, number>()

type TemporaryGrant = NonNullable<ServerResponse['temporary']>[number]

interface CatalogQueryOptions {
  enabled?: boolean
}

/**
 * Partial projection carried by WS `update` frames. Mirrors the Rust
 * `LiveMetrics` wire type: only fields an agent report can populate. Static
 * facts (`mem_total`, `os`, `tags`, ...) never appear here — they arrive via
 * `full_sync`/REST and the cached values must survive an update merge.
 */
export interface LiveMetrics {
  cpu: number
  disk_read_bytes_per_sec: number
  disk_used: number
  disk_write_bytes_per_sec: number
  id: string
  last_active: number
  load1: number
  load5: number
  load15: number
  mem_used: number
  net_in_speed: number
  net_in_transfer: number
  net_out_speed: number
  net_out_transfer: number
  online: boolean
  process_count: number
  swap_used: number
  tcp_conn: number
  udp_conn: number
  uptime: number
}

export interface ServerMetrics extends LiveMetrics {
  agent_local_capabilities?: number | null
  agent_version?: string | null
  capabilities?: number
  country_code: string | null
  cpu_cores?: number | null
  cpu_name: string | null
  disk_total: number
  effective_capabilities?: number | null
  features?: string[]
  group_id: string | null
  has_token?: boolean
  mem_total: number
  name: string
  os: string | null
  outstanding_enrollment?: OutstandingEnrollmentSummary | null
  protocol_version?: number
  region: string | null
  swap_total: number
  tags?: string[]
  temporary?: TemporaryGrant[]
}

export type ServerCatalogEvent =
  | { kind: 'rest_snapshot'; servers: ServerResponse[] }
  | { kind: 'servers_removed'; serverIds: readonly string[] }
  | { kind: 'server_saved'; server: ServerResponse }
  | {
      kind: 'enrollment_changed'
      outstandingEnrollment: OutstandingEnrollmentSummary | null
      serverId: string
      tokenRevoked: boolean
    }
  | { kind: 'tags_changed'; serverId: string; tags: string[] }
  | { kind: 'ws_full_sync'; servers: ServerMetrics[] }
  | { kind: 'ws_update'; servers: LiveMetrics[] }
  | { kind: 'online_changed'; online: boolean; serverId: string }
  | {
      agentLocalCapabilities: number | null | undefined
      capabilities: number
      effectiveCapabilities: number | null | undefined
      kind: 'capabilities_changed'
      serverId: string
      temporary: TemporaryGrant[] | undefined
    }
  | {
      agentVersion: string | null | undefined
      kind: 'agent_info_changed'
      protocolVersion: number
      serverId: string
    }
  | { available: boolean; kind: 'docker_availability_changed'; serverId: string }

function serverDetailKey(serverId: string): readonly ['server-catalog', 'detail', string] {
  return [...SERVER_DETAIL_PREFIX, serverId]
}

function blankServerMetrics(id: string): ServerMetrics {
  return {
    agent_local_capabilities: null,
    agent_version: null,
    country_code: null,
    cpu: 0,
    cpu_cores: null,
    cpu_name: null,
    disk_read_bytes_per_sec: 0,
    disk_total: 0,
    disk_used: 0,
    disk_write_bytes_per_sec: 0,
    effective_capabilities: null,
    features: [],
    group_id: null,
    has_token: false,
    id,
    last_active: 0,
    load1: 0,
    load5: 0,
    load15: 0,
    mem_total: 0,
    mem_used: 0,
    name: '',
    net_in_speed: 0,
    net_in_transfer: 0,
    net_out_speed: 0,
    net_out_transfer: 0,
    online: false,
    os: null,
    outstanding_enrollment: null,
    process_count: 0,
    region: null,
    swap_total: 0,
    swap_used: 0,
    tags: [],
    tcp_conn: 0,
    temporary: [],
    udp_conn: 0,
    uptime: 0
  }
}

function projectRestServer(current: ServerMetrics, server: ServerResponse): ServerMetrics {
  return {
    ...current,
    agent_local_capabilities:
      server.agent_local_capabilities === undefined
        ? current.agent_local_capabilities
        : server.agent_local_capabilities,
    agent_version: server.agent_version === undefined ? current.agent_version : server.agent_version,
    capabilities: server.capabilities,
    country_code: server.country_code === undefined ? current.country_code : server.country_code,
    cpu_cores: server.cpu_cores === undefined ? current.cpu_cores : server.cpu_cores,
    cpu_name: server.cpu_name === undefined ? current.cpu_name : server.cpu_name,
    disk_total: server.disk_total === undefined ? current.disk_total : (server.disk_total ?? 0),
    effective_capabilities:
      server.effective_capabilities === undefined ? current.effective_capabilities : server.effective_capabilities,
    features: [...server.features],
    group_id: server.group_id === undefined ? current.group_id : server.group_id,
    has_token: server.has_token,
    mem_total: server.mem_total === undefined ? current.mem_total : (server.mem_total ?? 0),
    name: server.name,
    os: server.os === undefined ? current.os : server.os,
    outstanding_enrollment:
      server.outstanding_enrollment === undefined ? current.outstanding_enrollment : server.outstanding_enrollment,
    protocol_version: server.protocol_version,
    region: server.region === undefined ? current.region : server.region,
    swap_total: server.swap_total === undefined ? current.swap_total : (server.swap_total ?? 0),
    temporary: server.temporary === undefined ? current.temporary : [...server.temporary]
  }
}

function projectRestSnapshot(current: ServerMetrics[] | undefined, servers: ServerResponse[]): ServerMetrics[] {
  const currentById = new Map((current ?? []).map((server) => [server.id, server]))
  return servers.map((server) => projectRestServer(currentById.get(server.id) ?? blankServerMetrics(server.id), server))
}

function projectFullSyncServerToRest(current: ServerResponse, server: ServerMetrics): ServerResponse {
  return {
    ...current,
    country_code: server.country_code,
    cpu_cores: server.cpu_cores,
    cpu_name: server.cpu_name,
    features: server.features === undefined ? current.features : [...server.features],
    group_id: server.group_id,
    has_token: server.has_token === undefined ? current.has_token : server.has_token,
    name: server.name,
    os: server.os,
    outstanding_enrollment:
      server.outstanding_enrollment === undefined ? current.outstanding_enrollment : server.outstanding_enrollment,
    region: server.region
  }
}

function mergeWsServerUpdate(current: ServerMetrics, incoming: LiveMetrics): ServerMetrics {
  return { ...current, ...incoming }
}

function mergeWsUpdate(current: ServerMetrics[], incoming: LiveMetrics[]): ServerMetrics[] {
  const incomingById = new Map(incoming.map((server) => [server.id, server]))
  return current.map((server) => {
    const update = incomingById.get(server.id)
    return update ? mergeWsServerUpdate(server, update) : server
  })
}

function mergeWsFullSync(current: ServerMetrics[] | undefined, incoming: ServerMetrics[]): ServerMetrics[] {
  const currentById = new Map((current ?? []).map((server) => [server.id, server]))
  return incoming.map((server) => {
    const existing = currentById.get(server.id)
    if (!existing) {
      return server
    }
    return {
      ...server,
      agent_local_capabilities:
        server.agent_local_capabilities === undefined
          ? existing.agent_local_capabilities
          : server.agent_local_capabilities,
      agent_version: server.agent_version === undefined ? existing.agent_version : server.agent_version,
      capabilities: server.capabilities === undefined ? existing.capabilities : server.capabilities,
      effective_capabilities:
        server.effective_capabilities === undefined ? existing.effective_capabilities : server.effective_capabilities,
      protocol_version: server.protocol_version === undefined ? existing.protocol_version : server.protocol_version,
      temporary: server.temporary === undefined ? existing.temporary : server.temporary
    }
  })
}

function updateById<T extends { id: string }>(
  rows: T[] | undefined,
  serverId: string,
  update: (row: T) => T
): T[] | undefined {
  if (!rows) {
    return rows
  }
  const index = rows.findIndex((row) => row.id === serverId)
  if (index < 0) {
    return rows
  }
  const updated = update(rows[index])
  if (updated === rows[index]) {
    return rows
  }
  const next = [...rows]
  next[index] = updated
  return next
}

function upsertRestServerInLive(
  rows: ServerMetrics[] | undefined,
  server: ServerResponse
): ServerMetrics[] | undefined {
  if (!rows) {
    return rows
  }
  const index = rows.findIndex((row) => row.id === server.id)
  if (index < 0) {
    return [...rows, projectRestServer(blankServerMetrics(server.id), server)]
  }
  const next = [...rows]
  next[index] = projectRestServer(rows[index], server)
  return next
}

function upsertRestServerInList(
  rows: ServerResponse[] | undefined,
  server: ServerResponse
): ServerResponse[] | undefined {
  if (!rows) {
    return rows
  }
  const index = rows.findIndex((row) => row.id === server.id)
  if (index < 0) {
    return [...rows, server]
  }
  const next = [...rows]
  next[index] = server
  return next
}

function updateExistingDetail(
  queryClient: QueryClient,
  serverId: string,
  update: (server: ServerResponse) => ServerResponse
): void {
  const key = serverDetailKey(serverId)
  const current = queryClient.getQueryData<ServerResponse>(key)
  if (current !== undefined) {
    queryClient.setQueryData(key, update(current))
  }
}

function removeMissingDetails(queryClient: QueryClient, serverIds: ReadonlySet<string>): void {
  const detailQueries = queryClient.getQueryCache().findAll({ queryKey: SERVER_DETAIL_PREFIX })
  for (const query of detailQueries) {
    const serverId = query.queryKey[2]
    if (typeof serverId === 'string' && !serverIds.has(serverId)) {
      queryClient.removeQueries({ exact: true, queryKey: serverDetailKey(serverId) })
    }
  }
}

function setDockerAvailability(features: string[], available: boolean): string[] {
  const hasDocker = features.includes('docker')
  if (available === hasDocker) {
    return features
  }
  return available ? [...features, 'docker'] : features.filter((feature) => feature !== 'docker')
}

function assertNever(value: never): never {
  throw new Error(`Unhandled server catalog event: ${JSON.stringify(value)}`)
}

function ensureLiveProjectionLifetime(queryClient: QueryClient): void {
  queryClient.setQueryDefaults(LIVE_SERVERS_KEY, {
    gcTime: Number.POSITIVE_INFINITY,
    staleTime: Number.POSITIVE_INFINITY
  })
}

export function projectServerCatalog(queryClient: QueryClient, event: ServerCatalogEvent): void {
  ensureLiveProjectionLifetime(queryClient)
  switch (event.kind) {
    case 'rest_snapshot': {
      const serverIds = new Set(event.servers.map((server) => server.id))
      queryClient.setQueryData<ServerMetrics[]>(LIVE_SERVERS_KEY, (current) =>
        projectRestSnapshot(current, event.servers)
      )
      queryClient.setQueryData<ServerResponse[]>(SERVER_LIST_KEY, [...event.servers])
      removeMissingDetails(queryClient, serverIds)
      for (const server of event.servers) {
        updateExistingDetail(queryClient, server.id, () => server)
      }
      return
    }
    case 'server_saved': {
      queryClient.setQueryData<ServerMetrics[]>(LIVE_SERVERS_KEY, (current) =>
        upsertRestServerInLive(current, event.server)
      )
      queryClient.setQueryData<ServerResponse[]>(SERVER_LIST_KEY, (current) =>
        upsertRestServerInList(current, event.server)
      )
      queryClient.setQueryData<ServerResponse>(serverDetailKey(event.server.id), event.server)
      return
    }
    case 'servers_removed': {
      const removed = new Set(event.serverIds)
      queryClient.setQueryData<ServerMetrics[]>(LIVE_SERVERS_KEY, (current) =>
        current?.filter((server) => !removed.has(server.id))
      )
      queryClient.setQueryData<ServerResponse[]>(SERVER_LIST_KEY, (current) =>
        current?.filter((server) => !removed.has(server.id))
      )
      for (const serverId of event.serverIds) {
        queryClient.removeQueries({ exact: true, queryKey: serverDetailKey(serverId) })
      }
      return
    }
    case 'enrollment_changed': {
      queryClient.setQueryData<ServerMetrics[]>(LIVE_SERVERS_KEY, (current) =>
        updateById(current, event.serverId, (server) => ({
          ...server,
          has_token: event.tokenRevoked ? false : server.has_token,
          online: event.tokenRevoked ? false : server.online,
          outstanding_enrollment: event.outstandingEnrollment
        }))
      )
      queryClient.setQueryData<ServerResponse[]>(SERVER_LIST_KEY, (current) =>
        updateById(current, event.serverId, (server) => ({
          ...server,
          has_token: event.tokenRevoked ? false : server.has_token,
          outstanding_enrollment: event.outstandingEnrollment
        }))
      )
      updateExistingDetail(queryClient, event.serverId, (server) => ({
        ...server,
        has_token: event.tokenRevoked ? false : server.has_token,
        outstanding_enrollment: event.outstandingEnrollment
      }))
      return
    }
    case 'tags_changed': {
      queryClient.setQueryData<ServerMetrics[]>(LIVE_SERVERS_KEY, (current) =>
        updateById(current, event.serverId, (server) => ({ ...server, tags: [...event.tags] }))
      )
      return
    }
    case 'ws_full_sync': {
      const serverIds = new Set(event.servers.map((server) => server.id))
      const fullSyncById = new Map(event.servers.map((server) => [server.id, server]))
      const currentList = queryClient.getQueryData<ServerResponse[]>(SERVER_LIST_KEY)
      const hasUnlistedServer =
        currentList !== undefined && event.servers.some((server) => !currentList.some((row) => row.id === server.id))
      queryClient.setQueryData<ServerMetrics[]>(LIVE_SERVERS_KEY, (current) => mergeWsFullSync(current, event.servers))
      queryClient.setQueryData<ServerResponse[]>(SERVER_LIST_KEY, (current) =>
        current
          ?.filter((server) => serverIds.has(server.id))
          .map((server) => {
            const fullSync = fullSyncById.get(server.id)
            return fullSync ? projectFullSyncServerToRest(server, fullSync) : server
          })
      )
      removeMissingDetails(queryClient, serverIds)
      for (const server of event.servers) {
        updateExistingDetail(queryClient, server.id, (current) => projectFullSyncServerToRest(current, server))
      }
      if (hasUnlistedServer) {
        queryClient.invalidateQueries({ exact: true, queryKey: SERVER_LIST_KEY }).catch(() => undefined)
      }
      return
    }
    case 'ws_update': {
      // An update is a partial projection and can never seed the catalog:
      // until full_sync/REST provides the static fields, drop it.
      queryClient.setQueryData<ServerMetrics[]>(LIVE_SERVERS_KEY, (current) =>
        current ? mergeWsUpdate(current, event.servers) : current
      )
      return
    }
    case 'online_changed': {
      queryClient.setQueryData<ServerMetrics[]>(LIVE_SERVERS_KEY, (current) =>
        updateById(current, event.serverId, (server) => ({ ...server, online: event.online }))
      )
      return
    }
    case 'capabilities_changed': {
      const agentLocalCapabilities = event.agentLocalCapabilities ?? null
      const effectiveCapabilities = event.effectiveCapabilities ?? null
      const temporary = event.temporary ? [...event.temporary] : []
      queryClient.setQueryData<ServerMetrics[]>(LIVE_SERVERS_KEY, (current) =>
        updateById(current, event.serverId, (server) => ({
          ...server,
          agent_local_capabilities: agentLocalCapabilities,
          capabilities: event.capabilities,
          effective_capabilities: effectiveCapabilities,
          temporary
        }))
      )
      queryClient.setQueryData<ServerResponse[]>(SERVER_LIST_KEY, (current) =>
        updateById(current, event.serverId, (server) => ({
          ...server,
          agent_local_capabilities: agentLocalCapabilities,
          capabilities: event.capabilities,
          effective_capabilities: effectiveCapabilities,
          temporary
        }))
      )
      updateExistingDetail(queryClient, event.serverId, (server) => ({
        ...server,
        agent_local_capabilities: agentLocalCapabilities,
        capabilities: event.capabilities,
        effective_capabilities: effectiveCapabilities,
        temporary
      }))
      return
    }
    case 'agent_info_changed': {
      const agentVersion = event.agentVersion ?? null
      queryClient.setQueryData<ServerMetrics[]>(LIVE_SERVERS_KEY, (current) =>
        updateById(current, event.serverId, (server) => ({
          ...server,
          agent_version: agentVersion,
          protocol_version: event.protocolVersion
        }))
      )
      queryClient.setQueryData<ServerResponse[]>(SERVER_LIST_KEY, (current) =>
        updateById(current, event.serverId, (server) => ({
          ...server,
          agent_version: agentVersion,
          protocol_version: event.protocolVersion
        }))
      )
      updateExistingDetail(queryClient, event.serverId, (server) => ({
        ...server,
        agent_version: agentVersion,
        protocol_version: event.protocolVersion
      }))
      return
    }
    case 'docker_availability_changed': {
      queryClient.setQueryData<ServerMetrics[]>(LIVE_SERVERS_KEY, (current) =>
        updateById(current, event.serverId, (server) => ({
          ...server,
          features: setDockerAvailability(server.features ?? [], event.available)
        }))
      )
      queryClient.setQueryData<ServerResponse[]>(SERVER_LIST_KEY, (current) =>
        updateById(current, event.serverId, (server) => ({
          ...server,
          features: setDockerAvailability(server.features, event.available)
        }))
      )
      updateExistingDetail(queryClient, event.serverId, (server) => ({
        ...server,
        features: setDockerAvailability(server.features, event.available)
      }))
      return
    }
    default:
      assertNever(event)
  }
}

export function useLiveServers(options: CatalogQueryOptions = {}) {
  return useQuery<ServerMetrics[]>({
    enabled: options.enabled ?? true,
    gcTime: Number.POSITIVE_INFINITY,
    queryFn: () => [],
    queryKey: LIVE_SERVERS_KEY,
    refetchOnMount: false,
    refetchOnWindowFocus: false,
    staleTime: Number.POSITIVE_INFINITY
  })
}

export function useServerList(options: CatalogQueryOptions = {}) {
  const queryClient = useQueryClient()
  return useQuery<ServerResponse[]>({
    enabled: options.enabled ?? true,
    queryFn: () => fetchAndProjectRestSnapshot(queryClient),
    queryKey: SERVER_LIST_KEY
  })
}

export function useServerDetail(serverId: string, options: CatalogQueryOptions = {}) {
  const queryClient = useQueryClient()
  return useQuery<ServerResponse>({
    enabled: (options.enabled ?? true) && serverId.length > 0,
    queryFn: () => fetchAndProjectServerDetail(queryClient, serverId),
    queryKey: serverDetailKey(serverId)
  })
}

export async function refreshServerCatalog(queryClient: QueryClient): Promise<void> {
  const generation = nextListRequestGeneration(queryClient)
  await queryClient.cancelQueries({ exact: true, queryKey: SERVER_LIST_KEY })
  if (!isCurrentListRequest(queryClient, generation)) {
    return
  }
  try {
    await queryClient.fetchQuery({
      queryFn: () => fetchAndProjectRestSnapshot(queryClient, generation),
      queryKey: SERVER_LIST_KEY,
      staleTime: 0
    })
  } catch (error) {
    if (isCurrentListRequest(queryClient, generation)) {
      throw error
    }
  }
}

function nextListRequestGeneration(queryClient: QueryClient): number {
  const generation = (listRequestGenerations.get(queryClient) ?? 0) + 1
  listRequestGenerations.set(queryClient, generation)
  return generation
}

function isCurrentListRequest(queryClient: QueryClient, generation: number): boolean {
  return listRequestGenerations.get(queryClient) === generation
}

async function fetchAndProjectRestSnapshot(
  queryClient: QueryClient,
  generation = nextListRequestGeneration(queryClient)
): Promise<ServerResponse[]> {
  const servers = await api.get<ServerResponse[]>('/api/servers')
  if (isCurrentListRequest(queryClient, generation)) {
    projectServerCatalog(queryClient, { kind: 'rest_snapshot', servers })
  }
  return servers
}

async function fetchAndProjectServerDetail(queryClient: QueryClient, serverId: string): Promise<ServerResponse> {
  const server = await api.get<ServerResponse>(`/api/servers/${serverId}`)
  projectServerCatalog(queryClient, { kind: 'server_saved', server })
  return server
}

export function invalidateServerDetail(queryClient: QueryClient, serverId: string): Promise<void> {
  return queryClient.invalidateQueries({ exact: true, queryKey: serverDetailKey(serverId) })
}

export function readLiveServers(queryClient: QueryClient): ServerMetrics[] | undefined {
  return queryClient.getQueryData<ServerMetrics[]>(LIVE_SERVERS_KEY)
}

export function subscribeLiveServers(
  queryClient: QueryClient,
  listener: (servers: ServerMetrics[] | undefined) => void
): () => void {
  return queryClient.getQueryCache().subscribe((event) => {
    if (event.type !== 'added' && event.type !== 'removed' && event.type !== 'updated') {
      return
    }
    const queryKey = event.query.queryKey
    if (
      queryKey.length === LIVE_SERVERS_KEY.length &&
      queryKey[0] === LIVE_SERVERS_KEY[0] &&
      queryKey[1] === LIVE_SERVERS_KEY[1]
    ) {
      listener(readLiveServers(queryClient))
    }
  })
}
