import { useCallback, useEffect, useRef, useState } from 'react'
import { WsClient } from '@/lib/ws-client'
import type { DockerLogEntry } from '@/routes/_authed/servers/$serverId/docker/types'

const MAX_LOG_ENTRIES = 1000

interface DockerLogMessage {
  entries: DockerLogEntry[]
  session_id: string
  type: 'docker_log'
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
  start: () => void
  stop: () => void
}

export function useDockerLogs({
  serverId,
  containerId,
  tail = 100,
  follow = true
}: UseDockerLogsOptions): UseDockerLogsResult {
  const [logs, setLogs] = useState<DockerLogEntry[]>([])
  const [isConnected, setIsConnected] = useState(false)
  const wsRef = useRef<WsClient | null>(null)
  const sessionIdRef = useRef<string | null>(null)

  const stop = useCallback(() => {
    if (wsRef.current) {
      if (sessionIdRef.current) {
        wsRef.current.send({ type: 'docker_logs_stop', session_id: sessionIdRef.current })
      }
      wsRef.current.close()
      wsRef.current = null
    }
    sessionIdRef.current = null
    setIsConnected(false)
  }, [])

  const start = useCallback(() => {
    // Clean up any existing connection
    stop()

    const sessionId = `log-${serverId}-${containerId}-${Date.now()}`
    sessionIdRef.current = sessionId

    const ws = new WsClient(`/api/ws/docker/logs/${serverId}`)
    wsRef.current = ws

    ws.onConnectionStateChange((state) => {
      setIsConnected(state === 'connected')
      if (state === 'connected') {
        ws.send({
          type: 'docker_logs_start',
          session_id: sessionId,
          container_id: containerId,
          tail,
          follow
        })
      }
    })

    ws.onMessage((raw) => {
      const msg = raw as DockerLogMessage
      if (msg.type === 'docker_log' && msg.session_id === sessionId) {
        setLogs((prev) => {
          const updated = [...prev, ...msg.entries]
          return updated.length > MAX_LOG_ENTRIES ? updated.slice(-MAX_LOG_ENTRIES) : updated
        })
      }
    })
  }, [serverId, containerId, tail, follow, stop])

  const clearLogs = useCallback(() => {
    setLogs([])
  }, [])

  // Clean up on unmount
  useEffect(() => {
    return () => {
      if (wsRef.current) {
        wsRef.current.close()
        wsRef.current = null
      }
    }
  }, [])

  return { logs, isConnected, start, stop, clearLogs }
}
