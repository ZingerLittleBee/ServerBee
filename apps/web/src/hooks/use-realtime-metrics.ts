import type { QueryClient } from '@tanstack/react-query'
import { useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef, useState } from 'react'
import type { ServerMetrics } from './use-servers-ws'

const MAX_BUFFER_SIZE = 200
const TRIM_THRESHOLD = 250
const RENDER_THROTTLE_MS = 2000

// Persist buffers across unmounts so route switches don't lose data
const bufferCache = new WeakMap<QueryClient, Map<string, { buffer: RealtimeDataPoint[]; lastActive: number }>>()

interface RealtimeBufferState {
  buffer: RealtimeDataPoint[]
  lastActive: number
  pendingRender: boolean
  queryClient: QueryClient
  serverId: string
  throttleTimer: ReturnType<typeof setTimeout> | null
}

function getQueryClientBufferCache(
  queryClient: QueryClient
): Map<string, { buffer: RealtimeDataPoint[]; lastActive: number }> {
  let cache = bufferCache.get(queryClient)
  if (!cache) {
    cache = new Map()
    bufferCache.set(queryClient, cache)
  }
  return cache
}

export interface RealtimeDataPoint {
  cpu: number
  disk_pct: number
  load1: number
  load5: number
  load15: number
  memory_pct: number
  net_in_speed: number
  net_in_transfer: number
  net_out_speed: number
  net_out_transfer: number
  timestamp: string
}

export function toRealtimeDataPoint(metrics: ServerMetrics): RealtimeDataPoint {
  return {
    cpu: metrics.cpu,
    disk_pct: metrics.disk_total > 0 ? (metrics.disk_used / metrics.disk_total) * 100 : 0,
    load1: metrics.load1,
    load5: metrics.load5,
    load15: metrics.load15,
    memory_pct: metrics.mem_total > 0 ? (metrics.mem_used / metrics.mem_total) * 100 : 0,
    net_in_speed: metrics.net_in_speed,
    net_in_transfer: metrics.net_in_transfer,
    net_out_speed: metrics.net_out_speed,
    net_out_transfer: metrics.net_out_transfer,
    timestamp: new Date(metrics.last_active * 1000).toISOString()
  }
}

function createRealtimeBufferState(queryClient: QueryClient, serverId: string): RealtimeBufferState {
  const cache = getQueryClientBufferCache(queryClient)
  const servers = queryClient.getQueryData<ServerMetrics[]>(['servers'])
  const server = servers?.find((s) => s.id === serverId)
  const cached = cache.get(serverId)

  if (!server?.online || server.last_active <= 0) {
    return {
      buffer: [],
      lastActive: 0,
      pendingRender: false,
      queryClient,
      serverId,
      throttleTimer: null
    }
  }

  if (cached && cached.buffer.length > 0) {
    return {
      buffer: cached.buffer,
      lastActive: cached.lastActive,
      pendingRender: false,
      queryClient,
      serverId,
      throttleTimer: null
    }
  }

  return {
    buffer: [toRealtimeDataPoint(server)],
    lastActive: server.last_active,
    pendingRender: false,
    queryClient,
    serverId,
    throttleTimer: null
  }
}

export function useRealtimeMetrics(serverId: string): RealtimeDataPoint[] {
  const queryClient = useQueryClient()
  const stateRef = useRef<RealtimeBufferState | null>(null)
  const [, setTick] = useState(0)

  if (!stateRef.current || stateRef.current.serverId !== serverId || stateRef.current.queryClient !== queryClient) {
    stateRef.current = createRealtimeBufferState(queryClient, serverId)
  }

  useEffect(() => {
    const state = stateRef.current
    if (!state) {
      return
    }
    const cache = getQueryClientBufferCache(queryClient)
    const servers = queryClient.getQueryData<ServerMetrics[]>(['servers'])
    const server = servers?.find((s) => s.id === serverId)
    if (!server?.online || server.last_active <= 0) {
      cache.delete(serverId)
    }

    const scheduleRender = () => {
      if (state.throttleTimer) {
        state.pendingRender = true
        return
      }
      setTick((t) => t + 1)
      state.throttleTimer = setTimeout(() => {
        state.throttleTimer = null
        if (state.pendingRender) {
          state.pendingRender = false
          setTick((t) => t + 1)
        }
      }, RENDER_THROTTLE_MS)
    }

    // Subscribe to cache updates
    const unsubscribe = queryClient.getQueryCache().subscribe((event) => {
      if (event.query.queryHash !== '["servers"]') {
        return
      }

      const data = event.query.state.data as ServerMetrics[] | undefined
      if (!data) {
        return
      }

      const server = data.find((s) => s.id === serverId)
      if (!server?.online) {
        return
      }

      if (server.last_active === state.lastActive) {
        return
      }

      state.lastActive = server.last_active
      const point = toRealtimeDataPoint(server)
      state.buffer = [...state.buffer, point]

      if (state.buffer.length > TRIM_THRESHOLD) {
        state.buffer = state.buffer.slice(-MAX_BUFFER_SIZE)
      }

      scheduleRender()
    })

    return () => {
      unsubscribe()
      if (state.throttleTimer) {
        clearTimeout(state.throttleTimer)
        state.throttleTimer = null
      }
      // Persist buffer so route switches don't lose data
      if (state.buffer.length > 0) {
        cache.set(serverId, {
          buffer: state.buffer,
          lastActive: state.lastActive
        })
      } else {
        cache.delete(serverId)
      }
    }
  }, [queryClient, serverId])

  return stateRef.current?.buffer ?? []
}
