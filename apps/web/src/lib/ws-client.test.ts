import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

// Mock WebSocket before importing WsClient
class MockWebSocket {
  static instances: MockWebSocket[] = []
  url: string
  onopen: (() => void) | null = null
  onmessage: ((e: { data: string }) => void) | null = null
  onclose: (() => void) | null = null
  onerror: (() => void) | null = null
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
}

vi.stubGlobal('WebSocket', MockWebSocket)
vi.stubGlobal('window', {
  location: { protocol: 'http:', host: 'localhost:9527' }
})

// Dynamic import AFTER mocks are set up
const { WsClient } = await import('./ws-client')

beforeEach(() => {
  MockWebSocket.instances = []
  vi.useFakeTimers()
})

afterEach(() => {
  vi.useRealTimers()
})

describe('WsClient', () => {
  it('constructs WebSocket with correct URL', () => {
    new WsClient('/api/ws/servers')
    expect(MockWebSocket.instances[0].url).toBe('ws://localhost:9527/api/ws/servers')
  })

  it('delivers parsed JSON to handlers', () => {
    const ws = new WsClient('/api/ws/test')
    const handler = vi.fn()
    ws.onMessage(handler)
    MockWebSocket.instances[0].simulateOpen()
    MockWebSocket.instances[0].simulateMessage({ type: 'update', data: 42 })
    expect(handler).toHaveBeenCalledWith({ type: 'update', data: 42 })
  })

  it('delivers to multiple handlers', () => {
    const ws = new WsClient('/api/ws/test')
    const h1 = vi.fn()
    const h2 = vi.fn()
    ws.onMessage(h1)
    ws.onMessage(h2)
    MockWebSocket.instances[0].simulateOpen()
    MockWebSocket.instances[0].simulateMessage({ x: 1 })
    expect(h1).toHaveBeenCalledOnce()
    expect(h2).toHaveBeenCalledOnce()
  })

  it('unsubscribe removes handler', () => {
    const ws = new WsClient('/api/ws/test')
    const handler = vi.fn()
    const unsub = ws.onMessage(handler)
    unsub()
    MockWebSocket.instances[0].simulateOpen()
    MockWebSocket.instances[0].simulateMessage({ x: 1 })
    expect(handler).not.toHaveBeenCalled()
  })

  it('close() prevents reconnection', () => {
    const ws = new WsClient('/api/ws/test')
    ws.close()
    vi.advanceTimersByTime(60_000)
    // Should only have 1 instance (the initial one), no reconnect
    expect(MockWebSocket.instances.length).toBe(1)
  })

  it('schedules reconnect on close with backoff', () => {
    new WsClient('/api/ws/test')
    const sock = MockWebSocket.instances[0]
    // Simulate server-side close without going through ws.close() so closed flag stays false
    sock.onclose?.()
    expect(MockWebSocket.instances.length).toBe(1) // not yet reconnected
    vi.advanceTimersByTime(1500) // past initial ~1000ms + jitter
    expect(MockWebSocket.instances.length).toBe(2) // reconnected
  })
})
