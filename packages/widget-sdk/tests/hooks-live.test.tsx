import { act, render, renderHook, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it } from 'vitest'
import { useCapability, useMetric, useServers } from '../src/hooks/live'
import { createWidgetRuntime, resetRuntime, type ServerSummary } from '../src/runtime-context'

describe('live hooks', () => {
  beforeEach(() => {
    resetRuntime()
    // Cached references — useSyncExternalStore requires snapshots to be stable
    // across calls when nothing changed; the production runtime backs both
    // stores by React Query, which already memoizes its returns.
    const cachedServers: ServerSummary[] = [
      { id: 's1', name: 'one', online: true, lastSeen: null, capabilities: 1 | 8 }
    ]
    const cachedDetail = { id: 's1', cpu: { usage: 42 }, disks: [{ used: 100 }] }
    createWidgetRuntime({
      apiBaseUrl: '/api',
      queryClient: {} as any,
      serversStore: () => cachedServers,
      serverByIdStore: (id) => (id === 's1' ? cachedDetail : undefined),
      themeStore: () => ({ mode: 'light', cssVar: () => '' }),
      onConfigUpdate: () => {}
    })
  })

  it('useServers returns runtime list', () => {
    const { result } = renderHook(() => useServers())
    expect(result.current).toHaveLength(1)
    expect(result.current[0].id).toBe('s1')
  })

  it('useMetric extracts dot path', () => {
    const { result } = renderHook(() => useMetric('s1', 'cpu.usage'))
    expect(result.current).toBe(42)
  })

  it('useMetric extracts bracket path', () => {
    const { result } = renderHook(() => useMetric('s1', 'disks[0].used'))
    expect(result.current).toBe(100)
  })

  it('useMetric returns undefined when serverId is null', () => {
    const { result } = renderHook(() => useMetric(null, 'cpu.usage'))
    expect(result.current).toBeUndefined()
  })

  it('useCapability checks bitmask', () => {
    const { result: ping } = renderHook(() => useCapability('s1', 'CAP_PING_ICMP'))
    expect(ping.current).toBe(true)
    const { result: term } = renderHook(() => useCapability('s1', 'CAP_TERMINAL'))
    expect(term.current).toBe(true)
    const { result: docker } = renderHook(() => useCapability('s1', 'CAP_DOCKER'))
    expect(docker.current).toBe(false)
  })
})

describe('useServers re-renders on subscription', () => {
  it('rerenders when subscribe callback fires after store mutation', () => {
    resetRuntime()
    let servers: ServerSummary[] = []
    const listeners = new Set<() => void>()
    createWidgetRuntime({
      apiBaseUrl: '/api',
      queryClient: {} as any,
      serversStore: () => servers,
      serverByIdStore: (id) => servers.find((s) => s.id === id),
      subscribeServers: (cb) => {
        listeners.add(cb)
        return () => {
          listeners.delete(cb)
        }
      },
      themeStore: () => ({ mode: 'light', cssVar: () => '' }),
      onConfigUpdate: () => {}
    })

    function Probe() {
      const list = useServers()
      return <div data-testid="count">{list.length}</div>
    }

    render(<Probe />)
    expect(screen.getByTestId('count').textContent).toBe('0')

    act(() => {
      servers = [
        { id: 's1', name: 'one', online: true, lastSeen: null, capabilities: 0 },
        { id: 's2', name: 'two', online: true, lastSeen: null, capabilities: 0 }
      ]
      for (const cb of listeners) {
        cb()
      }
    })

    expect(screen.getByTestId('count').textContent).toBe('2')
  })
})
