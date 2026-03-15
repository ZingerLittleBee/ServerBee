import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useServer, useServerRecords } from './use-api'

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

beforeEach(() => {
  vi.spyOn(globalThis, 'fetch')
})

afterEach(() => {
  vi.restoreAllMocks()
})

const mockServer = {
  id: 'srv-1',
  name: 'test-server',
  hidden: false,
  weight: 0,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
  agent_version: '0.1.0',
  billing_cycle: null,
  country_code: 'US',
  cpu_arch: 'x86_64',
  cpu_cores: 4,
  cpu_name: 'Intel Xeon',
  currency: null,
  disk_total: 50_000_000_000,
  expired_at: null,
  group_id: null,
  ipv4: '1.2.3.4',
  ipv6: null,
  kernel_version: '6.1.0',
  mem_total: 8_000_000_000,
  os: 'Linux',
  price: null,
  public_remark: null,
  region: null,
  remark: null,
  swap_total: 2_000_000_000,
  traffic_limit: null,
  traffic_limit_type: null,
  virtualization: 'kvm'
}

const mockRecords = [
  {
    id: 1,
    server_id: 'srv-1',
    cpu: 25.5,
    mem_used: 4_000_000_000,
    swap_used: 0,
    disk_used: 20_000_000_000,
    net_in_speed: 1000,
    net_out_speed: 500,
    net_in_transfer: 100_000,
    net_out_transfer: 50_000,
    load1: 0.5,
    load5: 0.3,
    load15: 0.2,
    tcp_conn: 42,
    udp_conn: 5,
    process_count: 150,
    gpu_usage: null,
    temperature: null,
    time: '2026-01-01T00:00:00Z'
  },
  {
    id: 2,
    server_id: 'srv-1',
    cpu: 30.0,
    mem_used: 4_500_000_000,
    swap_used: 100_000,
    disk_used: 20_100_000_000,
    net_in_speed: 2000,
    net_out_speed: 800,
    net_in_transfer: 200_000,
    net_out_transfer: 100_000,
    load1: 0.8,
    load5: 0.5,
    load15: 0.3,
    tcp_conn: 55,
    udp_conn: 8,
    process_count: 160,
    gpu_usage: null,
    temperature: 45.0,
    time: '2026-01-01T00:05:00Z'
  }
]

describe('useServer', () => {
  it('fetches and returns server detail', async () => {
    vi.mocked(globalThis.fetch).mockResolvedValueOnce(
      new Response(JSON.stringify({ data: mockServer }), {
        status: 200,
        headers: { 'Content-Type': 'application/json' }
      })
    )

    const { result } = renderHook(() => useServer('srv-1'), { wrapper: createWrapper() })

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true)
    })

    expect(result.current.data).toEqual(mockServer)
    expect(result.current.data?.name).toBe('test-server')
    expect(result.current.data?.cpu_cores).toBe(4)
    expect(result.current.data?.virtualization).toBe('kvm')
  })

  it('does not fetch when id is empty', async () => {
    const { result } = renderHook(() => useServer(''), { wrapper: createWrapper() })

    await waitFor(() => {
      expect(result.current.fetchStatus).toBe('idle')
    })

    expect(globalThis.fetch).not.toHaveBeenCalled()
  })
})

describe('useServerRecords', () => {
  it('fetches and returns server records', async () => {
    vi.mocked(globalThis.fetch).mockResolvedValueOnce(
      new Response(JSON.stringify({ data: mockRecords }), {
        status: 200,
        headers: { 'Content-Type': 'application/json' }
      })
    )

    const { result } = renderHook(() => useServerRecords('srv-1', 1, '5m'), {
      wrapper: createWrapper()
    })

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true)
    })

    expect(result.current.data).toHaveLength(2)
    expect(result.current.data?.[0].cpu).toBe(25.5)
    expect(result.current.data?.[1].temperature).toBe(45.0)
  })

  it('does not fetch when id is empty', async () => {
    const { result } = renderHook(() => useServerRecords('', 1, '5m'), {
      wrapper: createWrapper()
    })

    await waitFor(() => {
      expect(result.current.fetchStatus).toBe('idle')
    })

    expect(globalThis.fetch).not.toHaveBeenCalled()
  })

  it('does not fetch when enabled is false', async () => {
    const { result } = renderHook(() => useServerRecords('srv-1', 1, '5m', { enabled: false }), {
      wrapper: createWrapper()
    })

    await waitFor(() => {
      expect(result.current.fetchStatus).toBe('idle')
    })

    expect(globalThis.fetch).not.toHaveBeenCalled()
  })
})
