import type { QueryClient } from '@tanstack/react-query'
import { useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef, useState } from 'react'
import type { ServerMetrics } from './use-servers-ws'

const MAX_BUFFER_SIZE = 200
const TRIM_THRESHOLD = 250

// Persist buffers across unmounts so route switches don't lose data
const bufferCache = new WeakMap<QueryClient, Map<string, { buffer: RealtimeDataPoint[]; lastActive: number }>>()

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

export function useRealtimeMetrics(serverId: string): RealtimeDataPoint[] {
  const queryClient = useQueryClient()
  const bufferRef = useRef<RealtimeDataPoint[]>([])
  const lastActiveRef = useRef<number>(0)
  const [, setTick] = useState(0)

  useEffect(() => {
    const cache = getQueryClientBufferCache(queryClient)
    bufferRef.current = []
    lastActiveRef.current = 0

    const servers = queryClient.getQueryData<ServerMetrics[]>(['servers'])
    const server = servers?.find((s) => s.id === serverId)

    if (server?.online && server.last_active > 0) {
      const cached = cache.get(serverId)
      if (cached && cached.buffer.length > 0) {
        bufferRef.current = cached.buffer
        lastActiveRef.current = cached.lastActive
      } else {
        const point = toRealtimeDataPoint(server)
        bufferRef.current = [point]
        lastActiveRef.current = server.last_active
      }
      setTick((t) => t + 1)
    } else {
      cache.delete(serverId)
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

      if (server.last_active === lastActiveRef.current) {
        return
      }

      lastActiveRef.current = server.last_active
      const point = toRealtimeDataPoint(server)
      bufferRef.current = [...bufferRef.current, point]

      if (bufferRef.current.length > TRIM_THRESHOLD) {
        bufferRef.current = bufferRef.current.slice(-MAX_BUFFER_SIZE)
      }

      setTick((t) => t + 1)
    })

    return () => {
      unsubscribe()
      // Persist buffer so route switches don't lose data
      if (bufferRef.current.length > 0) {
        cache.set(serverId, {
          buffer: bufferRef.current,
          lastActive: lastActiveRef.current
        })
      } else {
        cache.delete(serverId)
      }
    }
  }, [queryClient, serverId])

  return bufferRef.current
}
