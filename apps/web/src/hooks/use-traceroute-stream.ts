import { useEffect, useState } from 'react'
import { subscribeBrowserMessage } from '@/hooks/use-servers-ws'
import type { RecordedProtocol, TracerouteHop } from '@/lib/network-types'

export interface TracerouteStreamState {
  completed: boolean
  error: string | null
  hops: TracerouteHop[]
  protocol: RecordedProtocol
  request_id: string
  round: number
  started_at: number
  target: string
  total_rounds: number
}

export function useTracerouteStream(serverId: string, requestId: string | null): TracerouteStreamState | null {
  const [data, setData] = useState<TracerouteStreamState | null>(null)

  useEffect(() => {
    setData(null)
    if (!requestId) {
      return
    }
    return subscribeBrowserMessage('traceroute_update', (msg: Record<string, unknown>) => {
      if (msg.server_id !== serverId || msg.request_id !== requestId) {
        return
      }
      setData({
        request_id: msg.request_id as string,
        target: msg.target as string,
        protocol: msg.protocol as RecordedProtocol,
        started_at: msg.started_at as number,
        round: msg.round as number,
        total_rounds: msg.total_rounds as number,
        hops: msg.hops as TracerouteHop[],
        completed: msg.completed as boolean,
        error: (msg.error as string | null | undefined) ?? null
      })
    })
  }, [serverId, requestId])

  return data
}
