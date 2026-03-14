import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

// --- MockWebSocket -----------------------------------------------------------

class MockWebSocket {
  static OPEN = 1
  static instances: MockWebSocket[] = []

  url: string
  readyState = MockWebSocket.OPEN
  onopen: (() => void) | null = null
  onmessage: ((e: { data: string }) => void) | null = null
  onclose: (() => void) | null = null
  onerror: (() => void) | null = null
  send = vi.fn()
  close = vi.fn(() => {
    this.onclose?.()
  })

  constructor(url: string) {
    this.url = url
    MockWebSocket.instances.push(this)
  }

  simulateOpen() {
    this.onopen?.()
  }

  simulateMessage(data: unknown) {
    this.onmessage?.({ data: JSON.stringify(data) })
  }

  simulateClose() {
    this.onclose?.()
  }

  simulateError() {
    this.onerror?.()
  }
}

vi.stubGlobal('WebSocket', MockWebSocket)
vi.stubGlobal('window', {
  location: { protocol: 'http:', host: 'localhost:9527' }
})

// Import hook AFTER globals are stubbed
const { useTerminalWs } = await import('./use-terminal-ws')

// --- Setup / teardown -------------------------------------------------------

beforeEach(() => {
  MockWebSocket.instances = []
})

afterEach(() => {
  vi.restoreAllMocks()
})

// --- Tests ------------------------------------------------------------------

describe('useTerminalWs', () => {
  describe('WebSocket URL construction', () => {
    it('uses ws: scheme when window.location.protocol is http:', () => {
      vi.stubGlobal('window', { location: { protocol: 'http:', host: 'localhost:9527' } })
      const { result } = renderHook(() => useTerminalWs('server-1'))

      act(() => {
        result.current.connect()
      })

      expect(MockWebSocket.instances[0].url).toBe('ws://localhost:9527/api/ws/terminal/server-1')
    })

    it('uses wss: scheme when window.location.protocol is https:', () => {
      vi.stubGlobal('window', { location: { protocol: 'https:', host: 'example.com' } })
      const { result } = renderHook(() => useTerminalWs('server-42'))

      act(() => {
        result.current.connect()
      })

      expect(MockWebSocket.instances[0].url).toBe('wss://example.com/api/ws/terminal/server-42')

      // Restore for subsequent tests
      vi.stubGlobal('window', { location: { protocol: 'http:', host: 'localhost:9527' } })
    })

    it('embeds serverId in URL path', () => {
      const { result } = renderHook(() => useTerminalWs('abc-123'))

      act(() => {
        result.current.connect()
      })

      expect(MockWebSocket.instances[0].url).toContain('/api/ws/terminal/abc-123')
    })
  })

  describe('status state machine', () => {
    it('starts with status "closed"', () => {
      const { result } = renderHook(() => useTerminalWs('s1'))
      expect(result.current.status).toBe('closed')
    })

    it('transitions to "connecting" when connect() is called', () => {
      const { result } = renderHook(() => useTerminalWs('s1'))

      act(() => {
        result.current.connect()
      })

      expect(result.current.status).toBe('connecting')
    })

    it('transitions to "connected" on WebSocket open', () => {
      const { result } = renderHook(() => useTerminalWs('s1'))

      act(() => {
        result.current.connect()
      })

      act(() => {
        MockWebSocket.instances[0].simulateOpen()
      })

      expect(result.current.status).toBe('connected')
    })

    it('transitions to "closed" on WebSocket close', () => {
      const { result } = renderHook(() => useTerminalWs('s1'))

      act(() => {
        result.current.connect()
      })
      act(() => {
        MockWebSocket.instances[0].simulateOpen()
      })
      act(() => {
        MockWebSocket.instances[0].simulateClose()
      })

      expect(result.current.status).toBe('closed')
    })

    it('transitions to "error" and sets error message on WebSocket error', () => {
      const { result } = renderHook(() => useTerminalWs('s1'))

      act(() => {
        result.current.connect()
      })
      act(() => {
        MockWebSocket.instances[0].simulateError()
      })

      expect(result.current.status).toBe('error')
      expect(result.current.error).toBe('WebSocket connection failed')
    })

    it('clears error when connect() is called again', () => {
      const { result } = renderHook(() => useTerminalWs('s1'))

      act(() => {
        result.current.connect()
      })
      act(() => {
        MockWebSocket.instances[0].simulateError()
      })
      expect(result.current.error).toBe('WebSocket connection failed')

      act(() => {
        result.current.connect()
      })

      expect(result.current.error).toBeNull()
    })
  })

  describe('disconnect()', () => {
    it('sets status to "closed" and closes WebSocket', () => {
      const { result } = renderHook(() => useTerminalWs('s1'))

      act(() => {
        result.current.connect()
      })
      act(() => {
        MockWebSocket.instances[0].simulateOpen()
      })
      act(() => {
        result.current.disconnect()
      })

      expect(result.current.status).toBe('closed')
    })
  })

  describe('sendInput()', () => {
    it('encodes data as base64 and sends input message', () => {
      const { result } = renderHook(() => useTerminalWs('s1'))

      act(() => {
        result.current.connect()
      })
      act(() => {
        MockWebSocket.instances[0].simulateOpen()
      })

      act(() => {
        result.current.sendInput('hello')
      })

      expect(MockWebSocket.instances[0].send).toHaveBeenCalledOnce()
      const sentPayload = JSON.parse(MockWebSocket.instances[0].send.mock.calls[0][0] as string)
      expect(sentPayload.type).toBe('input')
      // 'hello' base64-encoded is 'aGVsbG8='
      expect(sentPayload.data).toBe('aGVsbG8=')
    })

    it('does not send when WebSocket is not open', () => {
      const { result } = renderHook(() => useTerminalWs('s1'))

      // Never called connect() — wsRef is null
      act(() => {
        result.current.sendInput('hello')
      })

      expect(MockWebSocket.instances).toHaveLength(0)
    })

    it('encodes arbitrary binary-safe strings to base64', () => {
      const { result } = renderHook(() => useTerminalWs('s1'))

      act(() => {
        result.current.connect()
      })
      act(() => {
        MockWebSocket.instances[0].simulateOpen()
      })

      const input = 'ls -la\r'
      act(() => {
        result.current.sendInput(input)
      })

      const sentPayload = JSON.parse(MockWebSocket.instances[0].send.mock.calls[0][0] as string)
      // Verify round-trip
      expect(atob(sentPayload.data)).toBe(input)
    })
  })

  describe('sendResize()', () => {
    it('sends resize message with rows and cols', () => {
      const { result } = renderHook(() => useTerminalWs('s1'))

      act(() => {
        result.current.connect()
      })
      act(() => {
        MockWebSocket.instances[0].simulateOpen()
      })

      act(() => {
        result.current.sendResize(24, 80)
      })

      expect(MockWebSocket.instances[0].send).toHaveBeenCalledOnce()
      const sentPayload = JSON.parse(MockWebSocket.instances[0].send.mock.calls[0][0] as string)
      expect(sentPayload).toEqual({ type: 'resize', rows: 24, cols: 80 })
    })

    it('does not send when WebSocket is not open', () => {
      const { result } = renderHook(() => useTerminalWs('s1'))

      act(() => {
        result.current.sendResize(24, 80)
      })

      expect(MockWebSocket.instances).toHaveLength(0)
    })
  })

  describe('onData callback', () => {
    it('decodes base64 output message and passes to callback', () => {
      const { result } = renderHook(() => useTerminalWs('s1'))
      const dataCallback = vi.fn()

      act(() => {
        result.current.onData(dataCallback)
        result.current.connect()
      })
      act(() => {
        MockWebSocket.instances[0].simulateOpen()
      })

      // 'aGVsbG8=' is base64 for 'hello'
      act(() => {
        MockWebSocket.instances[0].simulateMessage({ type: 'output', data: 'aGVsbG8=' })
      })

      expect(dataCallback).toHaveBeenCalledOnce()
      expect(dataCallback).toHaveBeenCalledWith('hello')
    })

    it('sets error state on "error" message type', () => {
      const { result } = renderHook(() => useTerminalWs('s1'))

      act(() => {
        result.current.connect()
      })
      act(() => {
        MockWebSocket.instances[0].simulateOpen()
      })
      act(() => {
        MockWebSocket.instances[0].simulateMessage({ type: 'error', error: 'terminal crashed' })
      })

      expect(result.current.error).toBe('terminal crashed')
    })

    it('uses fallback error text when error message has no error field', () => {
      const { result } = renderHook(() => useTerminalWs('s1'))

      act(() => {
        result.current.connect()
      })
      act(() => {
        MockWebSocket.instances[0].simulateOpen()
      })
      act(() => {
        MockWebSocket.instances[0].simulateMessage({ type: 'error' })
      })

      expect(result.current.error).toBe('Unknown error')
    })

    it('ignores "started" and "session" message types without crashing', () => {
      const { result } = renderHook(() => useTerminalWs('s1'))
      const dataCallback = vi.fn()

      act(() => {
        result.current.onData(dataCallback)
        result.current.connect()
      })
      act(() => {
        MockWebSocket.instances[0].simulateOpen()
      })

      act(() => {
        MockWebSocket.instances[0].simulateMessage({ type: 'started' })
        MockWebSocket.instances[0].simulateMessage({ type: 'session', session_id: 'abc' })
      })

      expect(dataCallback).not.toHaveBeenCalled()
      expect(result.current.error).toBeNull()
    })
  })

  describe('connect() called multiple times', () => {
    it('closes previous WebSocket before opening a new one', () => {
      const { result } = renderHook(() => useTerminalWs('s1'))

      act(() => {
        result.current.connect()
      })
      const firstWs = MockWebSocket.instances[0]

      act(() => {
        result.current.connect()
      })

      expect(firstWs.close).toHaveBeenCalled()
      expect(MockWebSocket.instances).toHaveLength(2)
    })
  })
})
