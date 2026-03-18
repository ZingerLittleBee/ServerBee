import { useCallback, useEffect, useState } from 'react'
import { WsClient } from '@/lib/ws-client'
import type { DockerLogEntry } from '@/routes/_authed/servers/$serverId/docker/types'

const MAX_LOG_ENTRIES = 1000

interface DockerLogSessionMessage {
  session_id: string
  type: 'session'
}

interface DockerLogEntriesMessage {
  entries: DockerLogEntry[]
  type: 'logs'
}

interface UseDockerLogsOptions {
  containerId: string
  follow?: boolean
  serverId: string
  tail?: number
}

interface UseDockerLogsResult {
  clearLogs: () => void
  isConnected: boolean
  logs: DockerLogEntry[]
}

export function useDockerLogs({
  serverId,
  containerId,
  tail = 100,
  follow = true
}: UseDockerLogsOptions): UseDockerLogsResult {
  const [logs, setLogs] = useState<DockerLogEntry[]>([])
  const [isConnected, setIsConnected] = useState(false)

  // Auto-connect on mount, cleanup on unmount
  useEffect(() => {
    const ws = new WsClient(`/api/ws/docker/logs/${serverId}`)

    ws.onConnectionStateChange((state) => {
      setIsConnected(state === 'connected')
    })

    ws.onMessage((raw) => {
      const msg = raw as DockerLogSessionMessage | DockerLogEntriesMessage
      if (msg.type === 'session') {
        ws.send({
          type: 'subscribe',
          container_id: containerId,
          tail,
          follow
        })
      } else if (msg.type === 'logs') {
        const entries = (msg as DockerLogEntriesMessage).entries
        setLogs((prev) => {
          const updated = [...prev, ...entries]
          return updated.length > MAX_LOG_ENTRIES ? updated.slice(-MAX_LOG_ENTRIES) : updated
        })
      }
    })

    return () => {
      ws.close()
    }
  }, [serverId, containerId, tail, follow])

  const clearLogs = useCallback(() => {
    setLogs([])
  }, [])

  return { logs, isConnected, clearLogs }
}
