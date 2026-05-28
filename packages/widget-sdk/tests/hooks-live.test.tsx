import { renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, it } from 'vitest'
import { useCapability, useMetric, useServers } from '../src/hooks/live'
import { createWidgetRuntime, resetRuntime } from '../src/runtime-context'

describe('live hooks', () => {
  beforeEach(() => {
    resetRuntime()
    createWidgetRuntime({
      apiBaseUrl: '/api',
      queryClient: {} as any,
      serversStore: () => [{ id: 's1', name: 'one', online: true, lastSeen: null, capabilities: 1 | 8 }],
      serverByIdStore: (id) => (id === 's1' ? { id: 's1', cpu: { usage: 42 }, disks: [{ used: 100 }] } : undefined),
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
