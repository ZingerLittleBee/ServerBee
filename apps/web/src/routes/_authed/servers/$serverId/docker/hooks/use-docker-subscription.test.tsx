import { renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const mockUseServersWsSend = vi.fn()

vi.mock('@/contexts/servers-ws-context', () => ({
  useServersWsSend: mockUseServersWsSend
}))

const { useDockerSubscription } = await import('./use-docker-subscription')

describe('useDockerSubscription', () => {
  const send = vi.fn()

  beforeEach(() => {
    send.mockReset()
    mockUseServersWsSend.mockReset()
  })

  afterEach(() => {
    vi.clearAllMocks()
  })

  it('waits for an active servers websocket connection before subscribing', () => {
    mockUseServersWsSend.mockReturnValue({
      connectionState: 'disconnected',
      send
    })

    const { rerender, unmount } = renderHook(
      ({ enabled } = { enabled: true }) => useDockerSubscription('srv-1', enabled),
      {
        initialProps: { enabled: true }
      }
    )

    expect(send).not.toHaveBeenCalled()

    mockUseServersWsSend.mockReturnValue({
      connectionState: 'connected',
      send
    })
    rerender()

    expect(send).toHaveBeenCalledTimes(1)
    expect(send).toHaveBeenNthCalledWith(1, {
      type: 'docker_subscribe',
      server_id: 'srv-1'
    })

    unmount()

    expect(send).toHaveBeenCalledTimes(2)
    expect(send).toHaveBeenNthCalledWith(2, {
      type: 'docker_unsubscribe',
      server_id: 'srv-1'
    })
  })

  it('re-subscribes after the shared servers websocket reconnects', () => {
    mockUseServersWsSend.mockReturnValue({
      connectionState: 'connected',
      send
    })

    const { rerender } = renderHook(({ enabled } = { enabled: true }) => useDockerSubscription('srv-1', enabled), {
      initialProps: { enabled: true }
    })

    expect(send).toHaveBeenCalledTimes(1)
    expect(send).toHaveBeenLastCalledWith({
      type: 'docker_subscribe',
      server_id: 'srv-1'
    })

    mockUseServersWsSend.mockReturnValue({
      connectionState: 'disconnected',
      send
    })
    rerender()

    mockUseServersWsSend.mockReturnValue({
      connectionState: 'connected',
      send
    })
    rerender()

    expect(send).toHaveBeenCalledTimes(3)
    expect(send).toHaveBeenNthCalledWith(2, {
      type: 'docker_unsubscribe',
      server_id: 'srv-1'
    })
    expect(send).toHaveBeenLastCalledWith({
      type: 'docker_subscribe',
      server_id: 'srv-1'
    })
  })

  it('does not subscribe while docker is disabled for the page', () => {
    mockUseServersWsSend.mockReturnValue({
      connectionState: 'connected',
      send
    })

    const { rerender, unmount } = renderHook(
      ({ enabled } = { enabled: false }) => useDockerSubscription('srv-1', enabled),
      {
        initialProps: { enabled: false }
      }
    )

    expect(send).not.toHaveBeenCalled()

    rerender({ enabled: true })

    expect(send).toHaveBeenCalledTimes(1)
    expect(send).toHaveBeenNthCalledWith(1, {
      type: 'docker_subscribe',
      server_id: 'srv-1'
    })

    unmount()

    expect(send).toHaveBeenCalledTimes(2)
    expect(send).toHaveBeenNthCalledWith(2, {
      type: 'docker_unsubscribe',
      server_id: 'srv-1'
    })
  })
})
