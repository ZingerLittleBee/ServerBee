import { useCallback, useEffect, useRef, useState } from 'react'

type TerminalStatus = 'closed' | 'connected' | 'connecting' | 'error'

interface TerminalMessage {
  data?: string
  error?: string
  session_id?: string
  type: string
}

export function useTerminalWs(serverId: string) {
  const wsRef = useRef<WebSocket | null>(null)
  const [status, setStatus] = useState<TerminalStatus>('closed')
  const [error, setError] = useState<string | null>(null)
  const onDataRef = useRef<((data: string) => void) | null>(null)

  // Detach all handlers before closing so a stale socket's async onerror/onclose
  // (common under StrictMode double-invoke or rapid reconnects) can no longer
  // mutate state and clobber the live connection's status/error.
  const teardown = useCallback((ws: WebSocket | null) => {
    if (!ws) {
      return
    }
    ws.onopen = null
    ws.onmessage = null
    ws.onerror = null
    ws.onclose = null
    ws.close()
  }, [])

  const connect = useCallback(() => {
    teardown(wsRef.current)

    setStatus('connecting')
    setError(null)

    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
    const url = `${protocol}//${window.location.host}/api/ws/terminal/${serverId}`
    const ws = new WebSocket(url)

    ws.onopen = () => {
      setStatus('connected')
      setError(null)
    }

    ws.onmessage = (event) => {
      let msg: TerminalMessage
      try {
        msg = JSON.parse(event.data as string)
      } catch {
        console.warn('Terminal WS: invalid JSON', event.data)
        return
      }
      switch (msg.type) {
        case 'output':
          if (typeof msg.data === 'string' && onDataRef.current) {
            try {
              const decoded = atob(msg.data)
              onDataRef.current(decoded)
            } catch {
              console.warn('Terminal WS: invalid base64 data')
            }
          }
          break
        case 'started':
          break
        case 'error':
          setError(msg.error ?? 'Unknown error')
          break
        case 'session':
          break
        default:
          break
      }
    }

    ws.onerror = () => {
      setStatus('error')
      setError('WebSocket connection failed')
    }

    ws.onclose = () => {
      setStatus('closed')
    }

    wsRef.current = ws
  }, [serverId, teardown])

  const disconnect = useCallback(() => {
    teardown(wsRef.current)
    wsRef.current = null
    setStatus('closed')
  }, [teardown])

  const sendInput = useCallback((data: string) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      // Encode to base64
      const encoded = btoa(data)
      wsRef.current.send(JSON.stringify({ type: 'input', data: encoded }))
    }
  }, [])

  const sendResize = useCallback((rows: number, cols: number) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify({ type: 'resize', rows, cols }))
    }
  }, [])

  const onData = useCallback((callback: (data: string) => void) => {
    onDataRef.current = callback
  }, [])

  useEffect(() => {
    return () => {
      teardown(wsRef.current)
      wsRef.current = null
    }
  }, [teardown])

  return { connect, disconnect, error, onData, sendInput, sendResize, status }
}
