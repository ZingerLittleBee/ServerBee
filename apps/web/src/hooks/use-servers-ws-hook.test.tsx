import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook } from '@testing-library/react'
import type { ReactNode } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'

const { wsInstances, MockWsClient } = vi.hoisted(() => {
  const wsInstances: Array<{
    close: ReturnType<typeof vi.fn>
    onMessage: ReturnType<typeof vi.fn>
    path: string
  }> = []

  class MockWsClient {
    readonly path: string
    close = vi.fn()
    onMessage = vi.fn(() => vi.fn())

    constructor(path: string) {
      this.path = path
      wsInstances.push(this)
    }
  }

  return { wsInstances, MockWsClient }
})

vi.mock('@/lib/ws-client', () => ({
  WsClient: MockWsClient
}))

import { useServersWs } from './use-servers-ws'

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false }
    }
  })

  return function Wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  }
}

afterEach(() => {
  wsInstances.length = 0
  vi.clearAllMocks()
})

describe('useServersWs', () => {
  it('does not connect when disabled', () => {
    renderHook(() => useServersWs(false), { wrapper: createWrapper() })

    expect(wsInstances).toHaveLength(0)
  })

  it('connects when enabled', () => {
    renderHook(() => useServersWs(true), { wrapper: createWrapper() })

    expect(wsInstances).toHaveLength(1)
    expect(wsInstances[0]?.path).toBe('/api/ws/servers')
  })

  it('closes the websocket on unmount', () => {
    const { unmount } = renderHook(() => useServersWs(true), { wrapper: createWrapper() })

    const ws = wsInstances[0]
    expect(ws).toBeDefined()

    unmount()

    expect(ws?.close).toHaveBeenCalledOnce()
  })
})
