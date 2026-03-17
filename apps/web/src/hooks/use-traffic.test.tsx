import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useTraffic } from './use-traffic'

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

const mockTrafficData = {
  cycle_start: '2026-03-01',
  cycle_end: '2026-03-31',
  bytes_in: 1_000_000_000,
  bytes_out: 500_000_000,
  bytes_total: 1_500_000_000,
  traffic_limit: 10_000_000_000,
  traffic_limit_type: 'sum',
  usage_percent: 15.0,
  prediction: {
    estimated_total: 4_500_000_000,
    estimated_percent: 45.0,
    will_exceed: false
  },
  daily: [{ date: '2026-03-15', bytes_in: 100_000_000, bytes_out: 50_000_000 }],
  hourly: [{ hour: '2026-03-17T10:00:00Z', bytes_in: 10_000_000, bytes_out: 5_000_000 }]
}

beforeEach(() => {
  vi.spyOn(globalThis, 'fetch')
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('useTraffic', () => {
  it('returns traffic data on success', async () => {
    vi.mocked(fetch).mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({ data: mockTrafficData })
    } as Response)

    const { result } = renderHook(() => useTraffic('srv-1'), { wrapper: createWrapper() })

    await waitFor(() => expect(result.current.isSuccess).toBe(true))

    expect(result.current.data?.bytes_total).toBe(1_500_000_000)
    expect(result.current.data?.cycle_start).toBe('2026-03-01')
    expect(result.current.data?.prediction?.will_exceed).toBe(false)
    expect(result.current.data?.daily).toHaveLength(1)
    expect(result.current.data?.hourly).toHaveLength(1)
  })

  it('uses correct query key', () => {
    vi.mocked(fetch).mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({ data: mockTrafficData })
    } as Response)

    renderHook(() => useTraffic('srv-1'), { wrapper: createWrapper() })

    // Verify the hook was called (fetch should be invoked with the correct URL)
    expect(fetch).toHaveBeenCalledWith('/api/servers/srv-1/traffic', expect.any(Object))
  })

  it('is disabled when serverId is empty', () => {
    const { result } = renderHook(() => useTraffic(''), { wrapper: createWrapper() })

    expect(result.current.fetchStatus).toBe('idle')
    expect(fetch).not.toHaveBeenCalled()
  })
})
